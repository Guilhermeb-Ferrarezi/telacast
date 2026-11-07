//! Telacast: gateway de chat (JSON/WebSocket) + retransmissão de tela (H.264), com PostgreSQL.
mod auth;
mod ids;
mod files;
mod updates;
mod store;

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, Path, Query, State,
    },
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use bytes::Bytes;
use clap::{Parser, Subcommand};
use p::{ErrorCode, Status, C2S, S2C, PROTO_VERSION};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use store::{RegisterError, Store};
use telacast_proto as p;
use tokio::sync::{
    broadcast,
    mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    Semaphore,
};
use tracing::{error, info};

// ------------------------------------------------------------------ limite de taxa

/// Janela deslizante em memória: no máximo `max` eventos por `window` para cada chave.
#[derive(Default)]
struct Limiter(Mutex<HashMap<String, VecDeque<Instant>>>);

impl Limiter {
    fn allow(&self, key: &str, max: usize, window: Duration) -> bool {
        let mut m = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let now = Instant::now();
        let q = m.entry(key.to_string()).or_default();
        while q.front().is_some_and(|t| now.duration_since(*t) > window) {
            q.pop_front();
        }
        if q.len() >= max {
            return false;
        }
        q.push_back(now);
        if m.len() > 20_000 {
            m.retain(|_, q| q.back().is_some_and(|t| now.duration_since(*t) < Duration::from_secs(3600)));
        }
        true
    }
}

// ------------------------------------------------------------------ saída das conexões

/// O que o servidor manda por uma conexão do gateway: um evento JSON do protocolo ou um quadro
/// binário de áudio (já montado). O JSON do protocolo não muda; este tipo é só interno.
enum Out {
    Msg(S2C),
    Bin(Bytes),
}

/// Quadros de áudio na fila de uma conexão (~2 s a 50 quadros/s). Acima disso, descarta: quem não
/// consegue ouvir em tempo real não deve fazer o servidor acumular memória.
const MAX_BIN_QUEUE: usize = 100;

/// Ponta de envio de uma conexão. `send` manda JSON (mesma assinatura de antes); `send_bin` manda
/// áudio com a fila limitada.
#[derive(Clone)]
struct Tx {
    inner: UnboundedSender<Out>,
    bin_pending: Arc<AtomicUsize>,
}

impl Tx {
    fn new() -> (Tx, UnboundedReceiver<Out>) {
        let (inner, rx) = unbounded_channel();
        (Tx { inner, bin_pending: Arc::new(AtomicUsize::new(0)) }, rx)
    }

    fn send(&self, m: S2C) -> Result<(), ()> {
        self.inner.send(Out::Msg(m)).map_err(|_| ())
    }

    fn send_bin(&self, b: Bytes) {
        if self.bin_pending.fetch_add(1, Ordering::Relaxed) >= MAX_BIN_QUEUE
            || self.inner.send(Out::Bin(b)).is_err()
        {
            self.bin_pending.fetch_sub(1, Ordering::Relaxed);
        }
    }

    /// Chamado pela conexão ao tirar um quadro binário da fila.
    fn bin_taken(&self) {
        self.bin_pending.fetch_sub(1, Ordering::Relaxed);
    }
}

// ------------------------------------------------------------------ estado

struct Room {
    tx: broadcast::Sender<Bytes>,
    /// Último quadro-chave: novos espectadores começam por ele.
    key: Option<Bytes>,
    channel: i64,
    user: i64,
}

struct App {
    store: Store,
    sf: ids::Snowflake,
    conns: Mutex<HashMap<i64, Vec<(u64, Tx)>>>,
    rooms: Mutex<HashMap<i64, Room>>,
    next: AtomicU64,
    /// Limita quantos hashes de senha rodam ao mesmo tempo (cada um usa ~19 MiB e CPU).
    hash_sem: Semaphore,
    auth_limit: Limiter,
    limits: Limiter,
    /// Últimos `nonce` aceitos por usuário (evita duplicar mensagem reenviada após reconexão).
    /// Conexões abertas por IP (limite contra um cliente só abrir milhares).
    ip_conns: Mutex<HashMap<String, usize>>,
    max_conns_per_ip: usize,
    /// Conexões marcadas como "paradas" pelo app (ausente automático).
    idle: Mutex<HashSet<u64>>,
    /// Token (hash) da sessão de cada conexão, para avisar quando a sessão for encerrada.
    conn_tokens: Mutex<HashMap<u64, Vec<u8>>>,
    nonces: Mutex<(std::collections::HashSet<(i64, u64)>, std::collections::VecDeque<(i64, u64)>)>,
    /// Hash de uma senha qualquer, usado para gastar o mesmo tempo quando o usuário não existe.
    dummy_hash: String,
    /// Quem está em canal de voz (só em memória): usuário -> estado.
    voice: Mutex<HashMap<i64, VoiceEntry>>,
}

/// Estado de voz de um usuário no servidor, mais o balde de fichas que limita os quadros dele.
struct VoiceEntry {
    /// Conexão que entrou na voz (se a sessão dela for revogada, a pessoa sai da voz).
    cid: u64,
    channel: i64,
    server: i64,
    muted: bool,
    deafened: bool,
    tokens: f64,
    last: Instant,
}

impl VoiceEntry {
    fn state(&self, user_id: i64) -> p::VoiceState {
        p::VoiceState { user_id, channel_id: self.channel, muted: self.muted, deafened: self.deafened }
    }
}

/// Limite de quadros por usuário: 100/s sustentados, rajada de até 100 (20 ms = 50/s no uso normal).
const VOICE_RATE: f64 = 100.0;
const VOICE_BURST: f64 = 100.0;

const ERR_BUSY: &str = "Muitas ações em pouco tempo. Aguarde um instante.";
const ERR_INTERNAL: &str = "Erro interno do servidor.";
const ERR_VERSION: &str = "Versão do app incompatível com o servidor. Atualize o Telacast.";

impl App {
    async fn hash_password(&self, pw: String) -> Result<String, String> {
        let _permit = self.hash_sem.acquire().await.map_err(|e| e.to_string())?;
        tokio::task::spawn_blocking(move || auth::hash_password(&pw)).await.map_err(|e| e.to_string())?
    }

    async fn verify_password(&self, pw: String, hash: String) -> bool {
        let Ok(_permit) = self.hash_sem.acquire().await else { return false };
        tokio::task::spawn_blocking(move || auth::verify_password(&pw, &hash)).await.unwrap_or(false)
    }

    fn send_to(&self, uid: i64, m: &S2C) {
        if let Some(v) = self.conns.lock().unwrap_or_else(|e| e.into_inner()).get(&uid) {
            for (_, tx) in v {
                let _ = tx.send(m.clone());
            }
        }
    }

    /// Quem está em voz nos servidores dados, ordenado por canal e usuário.
    fn voice_in(&self, servers: &HashSet<i64>) -> Vec<p::VoiceState> {
        let v = self.voice.lock().unwrap_or_else(|e| e.into_inner());
        let mut out: Vec<p::VoiceState> =
            v.iter().filter(|(_, e)| servers.contains(&e.server)).map(|(u, e)| e.state(*u)).collect();
        out.sort_by_key(|s| (s.channel_id, s.user_id));
        out
    }

    /// Manda a todos os membros online de um servidor.
    async fn broadcast_server(&self, server: i64, m: &S2C) {
        match self.store.server_member_ids(server).await {
            Ok(ids) => {
                for u in ids {
                    self.send_to(u, m);
                }
            }
            Err(e) => error!("server_member_ids: {e}"),
        }
    }

    /// Tira da voz (síncrono, para a saída ficar valendo já) e devolve onde estava.
    fn voice_take(&self, uid: i64) -> Option<VoiceEntry> {
        self.voice.lock().unwrap_or_else(|e| e.into_inner()).remove(&uid)
    }

    async fn voice_left(&self, uid: i64, old: &VoiceEntry) {
        self.broadcast_server(old.server, &S2C::VoiceLeft { user_id: uid, channel_id: old.channel }).await;
    }

    /// Repassa um quadro de áudio de `uid` aos outros participantes do canal dele.
    /// Descarta em silêncio o que for inválido, fora de voz, de quem está mudo ou acima do limite.
    fn voice_relay(&self, uid: i64, data: &[u8]) {
        // [VOICE_FRAME][seq u32][opus]: pelo menos 1 byte de áudio, no máximo VOICE_MAX_PACKET
        if data.len() < 6 || data[0] != p::VOICE_FRAME || data.len() - 5 > p::VOICE_MAX_PACKET {
            return;
        }
        let targets: Vec<i64> = {
            let mut v = self.voice.lock().unwrap_or_else(|e| e.into_inner());
            let Some(me) = v.get_mut(&uid) else { return };
            if me.muted || me.deafened {
                return;
            }
            let now = Instant::now();
            me.tokens = (me.tokens + now.duration_since(me.last).as_secs_f64() * VOICE_RATE).min(VOICE_BURST);
            me.last = now;
            if me.tokens < 1.0 {
                return;
            }
            me.tokens -= 1.0;
            let channel = me.channel;
            v.iter().filter(|(u, e)| **u != uid && e.channel == channel && !e.deafened).map(|(u, _)| *u).collect()
        };
        if targets.is_empty() {
            return;
        }
        let mut frame = Vec::with_capacity(data.len() + 8);
        frame.push(p::VOICE_FRAME);
        frame.extend_from_slice(&uid.to_be_bytes());
        frame.extend_from_slice(&data[1..]);
        let frame = Bytes::from(frame);
        let conns = self.conns.lock().unwrap_or_else(|e| e.into_inner());
        for t in targets {
            for (_, tx) in conns.get(&t).into_iter().flatten() {
                tx.send_bin(frame.clone());
            }
        }
    }

    async fn snapshot(&self, me: i64) -> Result<p::Snapshot, sqlx::Error> {
        let online: HashSet<i64> = self.conns.lock().unwrap_or_else(|e| e.into_inner()).keys().copied().collect();
        let live_rooms: Vec<(i64, i64)> = self
            .rooms
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .map(|r| (r.channel, r.user))
            .collect();
        // status escolhido por quem está conectado; "ausente automático" = todas as conexões da pessoa paradas
        let connected: Vec<i64> = online.iter().copied().collect();
        let manual: HashMap<i64, i16> = self.store.manual_statuses(&connected).await?.into_iter().collect();
        let all_idle: HashSet<i64> = {
            let conns = self.conns.lock().unwrap_or_else(|e| e.into_inner());
            let idle = self.idle.lock().unwrap_or_else(|e| e.into_inner());
            conns.iter().filter(|(_, v)| !v.is_empty() && v.iter().all(|(cid, _)| idle.contains(cid))).map(|(u, _)| *u).collect()
        };
        let user = |id: i64, name: String| {
            let status = if !online.contains(&id) {
                Status::Offline
            } else {
                match manual.get(&id).copied().unwrap_or(0) {
                    1 => Status::Idle,
                    2 => Status::Dnd,
                    // invisível: só a própria pessoa sabe; para os outros é offline
                    3 if id == me => Status::Invisible,
                    3 => Status::Offline,
                    _ if all_idle.contains(&id) => Status::Idle,
                    _ => Status::Online,
                }
            };
            p::User { id, name, online: !matches!(status, Status::Offline | Status::Invisible), status }
        };

        let raw = self.store.servers_of(me).await?;
        let ids: Vec<i64> = raw.iter().map(|s| s.0).collect();
        let chans = self.store.channels_in(&ids, me).await?;
        let mems = self.store.members_in(&ids).await?;
        let servers = raw
            .into_iter()
            .map(|(id, name, owner, invite)| p::ServerInfo {
                id,
                name,
                owner,
                invite,
                channels: chans
                    .iter()
                    .filter(|c| c.0 == id)
                    .map(|c| p::Channel {
                        id: c.1,
                        name: c.2.clone(),
                        kind: c.3 as u8,
                        parent: c.4,
                        position: c.5,
                        last_message_id: c.6,
                        last_read_id: c.7,
                        mentions: c.8.max(0) as u32,
                    })
                    .collect(),
                members: mems.iter().filter(|m| m.0 == id).map(|m| user(m.1, m.2.clone())).collect(),
            })
            .collect();

        let friends = self
            .store
            .friends_of(me)
            .await?
            .into_iter()
            .map(|(id, name, accepted, by_me)| p::Friend {
                user: user(id, name),
                status: match (accepted, by_me) {
                    (true, _) => p::FriendStatus::Friend,
                    (false, true) => p::FriendStatus::Outgoing,
                    (false, false) => p::FriendStatus::Incoming,
                },
            })
            .collect();
        let dms = self
            .store
            .dms_of(me)
            .await?
            .into_iter()
            .map(|(channel_id, id, name, last_message_id, last_read_id, mentions)| p::Dm {
                channel_id,
                user: user(id, name),
                last_message_id,
                last_read_id,
                mentions: mentions.max(0) as u32,
            })
            .collect();

        let live_ch: Vec<i64> = live_rooms.iter().map(|r| r.0).collect();
        let allowed: HashSet<i64> = if live_ch.is_empty() {
            HashSet::new()
        } else {
            self.store.accessible_channels(&live_ch, me).await?.into_iter().collect()
        };
        let live = live_rooms
            .into_iter()
            .filter(|(c, _)| allowed.contains(c))
            .map(|(channel_id, user_id)| p::Live { channel_id, user_id })
            .collect();

        let me_name = self.store.display_name(me).await?.unwrap_or_default();
        let blocked = self.store.blocked_by(me).await?.into_iter().map(|(id, name)| user(id, name)).collect();
        let voice = self.voice_in(&ids.iter().copied().collect());
        Ok(p::Snapshot { me: user(me, me_name), servers, friends, dms, live, blocked, voice })
    }

    async fn push_snapshot(&self, uid: i64) {
        match self.snapshot(uid).await {
            Ok(s) => self.send_to(uid, &S2C::Snapshot(s)),
            Err(e) => error!("snapshot de {uid}: {e}"),
        }
    }

    async fn push_many(&self, ids: &[i64]) {
        for id in ids {
            self.push_snapshot(*id).await;
        }
    }

    async fn presence_changed(&self, uid: i64) {
        match self.store.related_users(uid).await {
            Ok(rel) => self.push_many(&rel).await,
            Err(e) => error!("related_users: {e}"),
        }
    }

    fn connect(self: &Arc<Self>, uid: i64, cid: u64, tx: Tx) {
        let first = {
            let mut c = self.conns.lock().unwrap_or_else(|e| e.into_inner());
            let v = c.entry(uid).or_default();
            v.push((cid, tx));
            v.len() == 1
        };
        if first {
            let app = self.clone();
            tokio::spawn(async move { app.presence_changed(uid).await });
        }
    }

    /// Avisa as conexões que usam estas sessões (já apagadas) que elas expiraram.
    fn expire_connections(self: &Arc<Self>, uid: i64, hashes: &[Vec<u8>]) {
        let cids: Vec<u64> = {
            let t = self.conn_tokens.lock().unwrap_or_else(|e| e.into_inner());
            t.iter().filter(|(_, h)| hashes.contains(h)).map(|(c, _)| *c).collect()
        };
        let all_expired = {
            let conns = self.conns.lock().unwrap_or_else(|e| e.into_inner());
            match conns.get(&uid) {
                Some(v) => {
                    for (cid, tx) in v {
                        if cids.contains(cid) {
                            let _ = tx.send(S2C::Error { code: ErrorCode::SessionExpired, msg: "Essa sessão foi encerrada".into() });
                        }
                    }
                    v.iter().all(|(cid, _)| cids.contains(cid))
                }
                None => false,
            }
        };
        // sem nenhuma conexão válida sobrando, ou se a que entrou na voz foi expirada, a pessoa sai da voz
        let old = {
            let mut v = self.voice.lock().unwrap_or_else(|e| e.into_inner());
            if all_expired || v.get(&uid).is_some_and(|e| cids.contains(&e.cid)) {
                v.remove(&uid)
            } else {
                None
            }
        };
        if let Some(old) = old {
            let app = self.clone();
            tokio::spawn(async move { app.voice_left(uid, &old).await });
        }
    }

    fn disconnect(self: &Arc<Self>, uid: i64, cid: u64) {
        self.conn_tokens.lock().unwrap_or_else(|e| e.into_inner()).remove(&cid);
        self.idle.lock().unwrap_or_else(|e| e.into_inner()).remove(&cid);
        let last = {
            let mut c = self.conns.lock().unwrap_or_else(|e| e.into_inner());
            match c.get_mut(&uid) {
                Some(v) => {
                    v.retain(|(i, _)| *i != cid);
                    if v.is_empty() {
                        c.remove(&uid);
                        true
                    } else {
                        false
                    }
                }
                None => false,
            }
        };
        if last {
            let old = self.voice_take(uid);
            let app = self.clone();
            tokio::spawn(async move {
                if let Some(old) = old {
                    app.voice_left(uid, &old).await;
                }
                app.presence_changed(uid).await;
            });
        }
    }

    async fn auth_stream(&self, token: &str, channel: i64) -> Option<i64> {
        let uid = self.store.session_user(&auth::hash_token(token)).await.ok().flatten()?;
        self.store.can_access(channel, uid).await.ok()?.then_some(uid)
    }
}

// ------------------------------------------------------------------ linha de comando

#[derive(Parser)]
#[command(name = "telacast-server", about = "Servidor do Telacast")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Inicia o servidor (padrão).
    Serve,
    /// Cria um código de convite de cadastro. O código só aparece agora; no banco fica só o hash.
    Invite {
        /// Quantas pessoas podem usar o código.
        #[arg(long, default_value_t = 1)]
        uses: i32,
        /// Validade em dias (sem isso, não expira).
        #[arg(long)]
        days: Option<i32>,
        /// Anotação para você lembrar de quem é.
        #[arg(long)]
        label: Option<String>,
    },
    /// Cria a chave que assina as atualizações (guarde o arquivo; a chave pública vai embutida no app).
    UpdateKeygen {
        /// Onde gravar a chave secreta (não pode existir).
        #[arg(long)]
        out: std::path::PathBuf,
    },
    /// Publica um executável do app para atualização automática (assina se `TELACAST_UPDATE_KEY_FILE` existir).
    PublishUpdate {
        #[arg(long)]
        file: std::path::PathBuf,
        #[arg(long)]
        version: String,
        /// Padrão: a plataforma desta máquina (por exemplo linux-x86_64).
        #[arg(long)]
        platform: Option<String>,
        #[arg(long, default_value = "")]
        notes: String,
    },
    /// Define uma senha nova (gerada aqui) para um usuário e encerra todas as sessões dele.
    /// Recuperação de conta: o cadastro não tem e-mail. A senha só aparece agora.
    ResetPassword {
        /// Nome de usuário.
        #[arg(long)]
        user: String,
    },
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let cli = Cli::parse();

    // comandos de atualização não precisam do banco
    match &cli.cmd {
        Some(Cmd::UpdateKeygen { out }) => {
            match updates::keygen(out) {
                Ok(public) => {
                    println!("Chave secreta gravada em {} (modo 600). Guarde com cuidado e NÃO a coloque no repositório.", out.display());
                    println!("Chave pública (compile o app com TELACAST_UPDATE_PUBKEY={public}):");
                    println!("{public}");
                }
                Err(e) => {
                    eprintln!("Erro: {e}");
                    std::process::exit(1);
                }
            }
            return;
        }
        Some(Cmd::PublishUpdate { file, version, platform, notes }) => {
            let platform = platform.clone().unwrap_or_else(|| format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH));
            match updates::publish(file, version, &platform, notes) {
                Ok(m) => {
                    println!("Publicado {} para {}: sha256 {} ({} bytes)", m.version, m.platform, m.sha256, m.size);
                    if m.signature.is_empty() {
                        println!("Aviso: sem assinatura (defina TELACAST_UPDATE_KEY_FILE com uma chave de update-keygen).");
                    } else {
                        println!("Assinado com Ed25519.");
                    }
                }
                Err(e) => {
                    eprintln!("Erro: {e}");
                    std::process::exit(1);
                }
            }
            return;
        }
        _ => {}
    }

    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        eprintln!("Defina DATABASE_URL (veja .env.example).");
        std::process::exit(1)
    });
    let store = Store::connect(&url).await.unwrap_or_else(|e| {
        eprintln!("Não consegui conectar ao banco ou aplicar as migrações: {e}");
        std::process::exit(1)
    });
    let node: u16 = std::env::var("TELACAST_NODE").ok().and_then(|v| v.parse().ok()).unwrap_or(0);
    let sf = ids::Snowflake::new(node);

    match cli.cmd.unwrap_or(Cmd::Serve) {
        Cmd::Invite { uses, days, label } => {
            let code = auth::new_code();
            let hash = auth::hash_token(&code);
            if let Err(e) = store.create_instance_invite(sf.next(), &hash, label.as_deref(), uses.max(1), days).await {
                eprintln!("Erro ao criar o convite: {e}");
                std::process::exit(1);
            }
            println!("Código de convite: {code}");
            println!(
                "  usos: {}   validade: {}",
                uses.max(1),
                days.map(|d| format!("{d} dias")).unwrap_or_else(|| "sem expiração".into())
            );
            println!("  Guarde agora: o código não pode ser consultado depois.");
        }
        Cmd::UpdateKeygen { .. } | Cmd::PublishUpdate { .. } => unreachable!("tratados antes de abrir o banco"),
        Cmd::ResetPassword { user } => {
            let Ok(Some(uid)) = store.find_user_id(user.trim()).await else {
                eprintln!("Usuário não encontrado: {user}");
                std::process::exit(1);
            };
            let pw: String = auth::new_code().to_lowercase().chars().take(16).collect();
            let hash = auth::hash_password(&pw).expect("argon2");
            if let Err(e) = store.set_password_hash(uid, &hash).await {
                eprintln!("Erro ao gravar a senha: {e}");
                std::process::exit(1);
            }
            let _ = store.revoke_other_sessions(uid, &[]).await;
            println!("Nova senha de {user}: {pw}");
            println!("  Todas as sessões foram encerradas. Entre e troque a senha em Minha conta.");
        }
        Cmd::Serve => serve(store, sf).await,
    }
}

async fn serve(store: Store, sf: ids::Snowflake) {
    let addr = std::env::var("TELACAST_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let permits = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2).clamp(1, 4);
    let dummy_hash = auth::hash_password("senha-falsa-para-igualar-o-tempo").expect("argon2");
    let app = Arc::new(App {
        store,
        sf,
        conns: Default::default(),
        rooms: Default::default(),
        next: AtomicU64::new(1),
        conn_tokens: Mutex::new(HashMap::new()),
        hash_sem: Semaphore::new(permits),
        auth_limit: Limiter::default(),
        limits: Limiter::default(),
        ip_conns: Mutex::new(HashMap::new()),
        max_conns_per_ip: std::env::var("TELACAST_MAX_CONNS_PER_IP").ok().and_then(|v| v.parse().ok()).unwrap_or(30),
        idle: Mutex::new(HashSet::new()),
        nonces: Mutex::new(Default::default()),
        dummy_hash,
        voice: Mutex::new(HashMap::new()),
    });
    let router = Router::new()
        .route("/ws", get(gateway))
        .route("/publish/{channel}", get(publish))
        .route("/watch/{channel}", get(watch))
        .route("/upload", axum::routing::post(files::upload).layer(axum::extract::DefaultBodyLimit::max(files::max_upload())))
        .route("/files/{id}/{name}", get(files::download))
        .route("/update/manifest", get(updates::manifest))
        .route("/update/file/{hash}", get(updates::file))
        .route("/health", get(|| async { "ok" }))
        .with_state(app);
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    info!("telacast-server em ws://{addr}");
    axum::serve(listener, router.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap();
}

/// IP do cliente. Atrás do proxy local (Caddy) usa o cabeçalho X-Forwarded-For; senão, o IP da conexão.
fn client_ip(headers: &HeaderMap, peer: SocketAddr) -> String {
    if peer.ip().is_loopback() {
        if let Some(v) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if let Some(first) = v.split(',').next() {
                let first = first.trim();
                if !first.is_empty() {
                    return first.to_string();
                }
            }
        }
    }
    peer.ip().to_string()
}

// ------------------------------------------------------------------ gateway

async fn gateway(
    ws: WebSocketUpgrade,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    State(app): State<Arc<App>>,
) -> axum::response::Response {
    let ip = client_ip(&headers, peer);
    {
        let mut m = app.ip_conns.lock().unwrap_or_else(|e| e.into_inner());
        let n = m.entry(ip.clone()).or_insert(0);
        if *n >= app.max_conns_per_ip {
            return axum::http::StatusCode::TOO_MANY_REQUESTS.into_response();
        }
        *n += 1;
    }
    ws.max_message_size(64 << 10).on_upgrade(move |s| async move {
        run_conn(s, app.clone(), ip.clone()).await;
        let mut m = app.ip_conns.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(n) = m.get_mut(&ip) {
            *n = n.saturating_sub(1);
            if *n == 0 {
                m.remove(&ip);
            }
        }
    })
}

struct Session {
    uid: i64,
    token_hash: Vec<u8>,
}

async fn run_conn(mut socket: WebSocket, app: Arc<App>, ip: String) {
    let (tx, mut rx) = Tx::new();
    let cid = app.next.fetch_add(1, Ordering::Relaxed);
    let mut session: Option<Session> = None;
    // Heartbeat: ping a cada 20 s; qualquer quadro recebido (inclusive o pong) conta como vivo.
    // Sem nada por 60 s a conexão é considerada morta (derruba o "online" falso).
    let mut beat = tokio::time::interval(Duration::from_secs(20));
    beat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut last_seen = tokio::time::Instant::now();
    beat.tick().await; // o primeiro tick é imediato; o primeiro ping só sai aos 20 s
    loop {
        tokio::select! {
            _ = beat.tick() => {
                if last_seen.elapsed() > Duration::from_secs(60) { break }
                if socket.send(Message::Ping(Default::default())).await.is_err() { break }
            }
            out = rx.recv() => {
                match out {
                    None => break,
                    Some(Out::Msg(m)) => {
                        let Ok(json) = serde_json::to_string(&m) else { continue };
                        if socket.send(Message::Text(json.into())).await.is_err() { break }
                    }
                    Some(Out::Bin(b)) => {
                        tx.bin_taken();
                        if socket.send(Message::Binary(b)).await.is_err() { break }
                    }
                }
            }
            inc = socket.recv() => {
                last_seen = tokio::time::Instant::now();
                match inc {
                    Some(Ok(Message::Text(t))) => {
                        if let Ok(m) = serde_json::from_str::<C2S>(&t) {
                            if !handle(&app, &tx, &mut session, cid, &ip, m).await { break }
                        }
                    }
                    // áudio: nunca derruba a conexão; o que for inválido é descartado em silêncio
                    Some(Ok(Message::Binary(b))) => {
                        if let Some(s) = session.as_ref() {
                            app.voice_relay(s.uid, &b);
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
    // Entrega o que ficou na fila (por exemplo, a resposta de um Logout).
    while let Ok(out) = rx.try_recv() {
        if let Out::Msg(m) = out {
            if let Ok(json) = serde_json::to_string(&m) {
                let _ = socket.send(Message::Text(json.into())).await;
            }
        }
    }
    if let Some(s) = session {
        app.disconnect(s.uid, cid);
    }
}

async fn finish_login(
    app: &Arc<App>,
    tx: &Tx,
    session: &mut Option<Session>,
    cid: u64,
    ip: &str,
    uid: i64,
    existing_token: Option<String>,
) {
    let token = match existing_token {
        Some(t) => t,
        None => {
            let t = auth::new_token();
            if let Err(e) = app.store.create_session(app.sf.next(), uid, &auth::hash_token(&t), ip).await {
                error!("create_session: {e}");
                let _ = tx.send(S2C::Error { code: ErrorCode::Internal, msg: ERR_INTERNAL.into() });
                return;
            }
            t
        }
    };
    let Ok(Some(name)) = app.store.display_name(uid).await else {
        let _ = tx.send(S2C::Error { code: ErrorCode::Internal, msg: ERR_INTERNAL.into() });
        return;
    };
    let token_hash = auth::hash_token(&token);
    app.conn_tokens.lock().unwrap_or_else(|e| e.into_inner()).insert(cid, token_hash.clone());
    *session = Some(Session { uid, token_hash });
    let _ = tx.send(S2C::Auth { token, me: p::User { id: uid, name, online: true, status: Status::Online } });
    app.connect(uid, cid, tx.clone());
    app.push_snapshot(uid).await;
}

/// Devolve `false` quando a conexão deve ser encerrada.
async fn handle(
    app: &Arc<App>,
    tx: &Tx,
    session: &mut Option<Session>,
    cid: u64,
    ip: &str,
    m: C2S,
) -> bool {
    let err = |code: ErrorCode, s: &str| {
        let _ = tx.send(S2C::Error { code, msg: s.into() });
    };
    match m {
        C2S::Register { username, password, invite, proto } => {
            if session.is_some() {
                return true;
            }
            if proto != PROTO_VERSION {
                err(ErrorCode::VersionMismatch, ERR_VERSION);
                return true;
            }
            if !app.auth_limit.allow(&format!("auth:{ip}"), 10, Duration::from_secs(300)) {
                err(ErrorCode::RateLimited, "Muitas tentativas. Tente de novo em alguns minutos.");
                return true;
            }
            let name = username.trim();
            let valid = (3..=32).contains(&name.chars().count())
                && name.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '-'));
            if !valid {
                err(ErrorCode::InvalidInput, "Nome de usuário: 3 a 32 caracteres (letras, números, _ . -)");
                return true;
            }
            if !(8..=128).contains(&password.chars().count()) {
                err(ErrorCode::InvalidInput, "A senha precisa ter de 8 a 128 caracteres");
                return true;
            }
            let code = auth::normalize_code(&invite);
            if code.is_empty() {
                err(ErrorCode::InvalidInvite, "Informe o código de convite");
                return true;
            }
            let Ok(hash) = app.hash_password(password).await else {
                err(ErrorCode::Internal, ERR_INTERNAL);
                return true;
            };
            match app.store.register(app.sf.next(), name, &hash, &auth::hash_token(&code)).await {
                Ok(id) => finish_login(app, tx, session, cid, ip, id, None).await,
                Err(RegisterError::InvalidInvite) => err(ErrorCode::InvalidInvite, "Código de convite inválido, expirado ou já usado"),
                Err(RegisterError::Taken) => err(ErrorCode::UsernameTaken, "Esse nome de usuário já está em uso"),
                Err(RegisterError::Db(e)) => {
                    error!("register: {e}");
                    err(ErrorCode::Internal, ERR_INTERNAL);
                }
            }
            true
        }
        C2S::Login { username, password, proto } => {
            if session.is_some() {
                return true;
            }
            if proto != PROTO_VERSION {
                err(ErrorCode::VersionMismatch, ERR_VERSION);
                return true;
            }
            let uname = username.trim().to_lowercase();
            if !app.auth_limit.allow(&format!("auth:{ip}"), 10, Duration::from_secs(300))
                || !app.auth_limit.allow(&format!("login:{uname}"), 10, Duration::from_secs(300))
            {
                err(ErrorCode::RateLimited, "Muitas tentativas. Tente de novo em alguns minutos.");
                return true;
            }
            let row = match app.store.user_by_username(&uname).await {
                Ok(r) => r,
                Err(e) => {
                    error!("login: {e}");
                    err(ErrorCode::Internal, ERR_INTERNAL);
                    return true;
                }
            };
            // Usuário inexistente também gasta o tempo de um hash (não revela quem existe).
            let (hash, id) = match &row {
                Some((id, h, _)) => (h.clone(), Some(*id)),
                None => (app.dummy_hash.clone(), None),
            };
            let ok = app.verify_password(password, hash).await;
            match id {
                Some(id) if ok => finish_login(app, tx, session, cid, ip, id, None).await,
                _ => err(ErrorCode::InvalidCredentials, "Usuário ou senha incorretos"),
            }
            true
        }
        C2S::Resume { token, proto } => {
            if session.is_some() {
                return true;
            }
            if proto != PROTO_VERSION {
                err(ErrorCode::VersionMismatch, ERR_VERSION);
                return true;
            }
            match app.store.session_user(&auth::hash_token(&token)).await {
                Ok(Some(id)) => finish_login(app, tx, session, cid, ip, id, Some(token)).await,
                Ok(None) => err(ErrorCode::SessionExpired, "Sessão expirada"),
                Err(e) => {
                    error!("resume: {e}");
                    err(ErrorCode::Internal, ERR_INTERNAL);
                }
            }
            true
        }
        C2S::SetIdle { idle } => {
            if let Some(s) = session.as_ref() {
                let uid = s.uid;
                let changed = {
                    let mut set = app.idle.lock().unwrap_or_else(|e| e.into_inner());
                    if idle { set.insert(cid) } else { set.remove(&cid) }
                };
                if changed {
                    app.push_snapshot(uid).await;
                    app.presence_changed(uid).await;
                }
            }
            true
        }
        other => {
            let Some(s) = session.as_ref() else {
                err(ErrorCode::NotAuthenticated, "Faça login primeiro");
                return true;
            };
            let (me, token_hash) = (s.uid, s.token_hash.clone());
            handle_authed(app, tx, me, cid, &token_hash, other).await
        }
    }
}

async fn send_sessions(app: &Arc<App>, tx: &Tx, me: i64, token_hash: &[u8]) {
    match app.store.list_sessions(me, token_hash).await {
        Ok(rows) => {
            let list = rows
                .into_iter()
                .map(|(id, device, created, last_used, current)| p::SessionInfo { id, device, created, last_used, current })
                .collect();
            let _ = tx.send(S2C::Sessions { list });
        }
        Err(e) => {
            error!("list_sessions: {e}");
            let _ = tx.send(S2C::Error { code: ErrorCode::Internal, msg: ERR_INTERNAL.into() });
        }
    }
}

async fn handle_authed(
    app: &Arc<App>,
    tx: &Tx,
    me: i64,
    cid: u64,
    token_hash: &[u8],
    m: C2S,
) -> bool {
    let err = |code: ErrorCode, s: &str| {
        let _ = tx.send(S2C::Error { code, msg: s.into() });
    };
    let busy = |key: &str, max: usize, secs: u64| !app.limits.allow(&format!("{key}:{me}"), max, Duration::from_secs(secs));
    let db_err = |what: &str, e: sqlx::Error| {
        error!("{what}: {e}");
        let _ = tx.send(S2C::Error { code: ErrorCode::Internal, msg: ERR_INTERNAL.into() });
    };
    match m {
        C2S::Logout => {
            let _ = app.store.revoke_session(token_hash).await;
            if let Some(old) = app.voice_take(me) {
                app.voice_left(me, &old).await;
            }
            return false;
        }
        C2S::VoiceJoin { channel_id } => {
            if busy("voice", 30, 10) {
                err(ErrorCode::RateLimited, ERR_BUSY);
                return true;
            }
            let (server, kind) = match app.store.channel_server_kind(channel_id).await {
                Ok(Some(x)) => x,
                Ok(None) => {
                    err(ErrorCode::NotFound, "Canal de voz não encontrado");
                    return true;
                }
                Err(e) => {
                    db_err("channel_server_kind", e);
                    return true;
                }
            };
            match app.store.is_member(server, me).await {
                Ok(true) => {}
                Ok(false) => {
                    err(ErrorCode::Forbidden, "Você não é membro deste servidor");
                    return true;
                }
                Err(e) => {
                    db_err("is_member", e);
                    return true;
                }
            }
            if kind != 1 {
                err(ErrorCode::InvalidInput, "Este canal não é de voz");
                return true;
            }
            // um canal por vez: entrar noutro move (mantém mudo/surdo). Entrar no mesmo só reenvia o estado.
            let (old, state) = {
                let mut v = app.voice.lock().unwrap_or_else(|e| e.into_inner());
                let old = v.get(&me).map(|e| (e.channel, e.server));
                if old.is_none_or(|(c, _)| c != channel_id) {
                    let (muted, deafened) = v.get(&me).map(|e| (e.muted, e.deafened)).unwrap_or((false, false));
                    v.insert(me, VoiceEntry { cid, channel: channel_id, server, muted, deafened, tokens: VOICE_BURST, last: Instant::now() });
                }
                (old, v.get(&me).map(|e| e.state(me)))
            };
            let Some(state) = state else { return true };
            if let Some((old_channel, old_server)) = old.filter(|(c, _)| *c != channel_id) {
                app.broadcast_server(old_server, &S2C::VoiceLeft { user_id: me, channel_id: old_channel }).await;
            }
            app.broadcast_server(server, &S2C::VoiceUpdate(state)).await;
        }
        C2S::VoiceLeave => {
            if let Some(old) = app.voice_take(me) {
                app.voice_left(me, &old).await;
            }
        }
        C2S::VoiceSet { muted, deafened } => {
            if busy("voice", 30, 10) {
                err(ErrorCode::RateLimited, ERR_BUSY);
                return true;
            }
            // surdo implica mudo
            let muted = muted || deafened;
            let changed = {
                let mut v = app.voice.lock().unwrap_or_else(|e| e.into_inner());
                match v.get_mut(&me) {
                    Some(e) if (e.muted, e.deafened) != (muted, deafened) => {
                        e.muted = muted;
                        e.deafened = deafened;
                        Some((e.server, e.state(me)))
                    }
                    _ => None,
                }
            };
            if let Some((server, state)) = changed {
                app.broadcast_server(server, &S2C::VoiceUpdate(state)).await;
            }
        }
        C2S::ListSessions => send_sessions(app, tx, me, token_hash).await,
        C2S::RevokeSession { id } => {
            match app.store.revoke_session_by_id(me, id).await {
                Ok(Some(h)) => {
                    app.expire_connections(me, &[h]);
                    send_sessions(app, tx, me, token_hash).await;
                }
                Ok(None) => err(ErrorCode::NotFound, "Sessão não encontrada"),
                Err(e) => db_err("revoke_session", e),
            }
        }
        C2S::RevokeOtherSessions => {
            match app.store.revoke_other_sessions(me, token_hash).await {
                Ok(h) => {
                    app.expire_connections(me, &h);
                    send_sessions(app, tx, me, token_hash).await;
                }
                Err(e) => db_err("revoke_other_sessions", e),
            }
        }
        C2S::ChangePassword { old, new } => {
            if !app.auth_limit.allow(&format!("pw:{me}"), 5, Duration::from_secs(300)) {
                err(ErrorCode::RateLimited, "Muitas tentativas. Tente de novo em alguns minutos.");
                return true;
            }
            if !(8..=128).contains(&new.chars().count()) {
                err(ErrorCode::InvalidInput, "A senha precisa ter de 8 a 128 caracteres");
                return true;
            }
            let current = match app.store.password_hash(me).await {
                Ok(Some(h)) => h,
                Ok(None) => return false,
                Err(e) => {
                    db_err("password_hash", e);
                    return true;
                }
            };
            if !app.verify_password(old, current).await {
                err(ErrorCode::WrongPassword, "A senha atual está incorreta");
                return true;
            }
            let Ok(hash) = app.hash_password(new).await else {
                err(ErrorCode::Internal, ERR_INTERNAL);
                return true;
            };
            if let Err(e) = app.store.set_password_hash(me, &hash).await {
                db_err("set_password_hash", e);
                return true;
            }
            match app.store.revoke_other_sessions(me, token_hash).await {
                Ok(h) => app.expire_connections(me, &h),
                Err(e) => error!("revoke_other_sessions: {e}"),
            }
            let _ = tx.send(S2C::PasswordChanged);
            send_sessions(app, tx, me, token_hash).await;
        }
        C2S::CreateServer { name } => {
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 50 {
                err(ErrorCode::InvalidInput, "Nome do servidor: 1 a 50 caracteres");
            } else if busy("mkserver", 5, 3600) {
                err(ErrorCode::RateLimited, ERR_BUSY);
            } else if app.store.owned_servers(me).await.unwrap_or(0) >= 20 {
                err(ErrorCode::InvalidInput, "Você já tem 20 servidores");
            } else {
                let sid = app.sf.next();
                match app.store.create_server(sid, app.sf.next(), app.sf.next(), me, name, &auth::new_code()).await {
                    Ok(()) => {
                        app.push_snapshot(me).await;
                        let _ = tx.send(S2C::ServerJoined { server_id: sid });
                    }
                    Err(e) => db_err("create_server", e),
                }
            }
        }
        C2S::JoinServer { code } => {
            if busy("join", 10, 600) {
                err(ErrorCode::RateLimited, ERR_BUSY);
            } else {
                match app.store.join_by_invite(&auth::normalize_code(&code), me).await {
                    Ok(Some((sid, false))) => {
                        // já era membro: só abre o servidor, sem aviso nem novo uso do convite
                        let _ = tx.send(S2C::ServerJoined { server_id: sid });
                    }
                    Ok(Some((sid, true))) => {
                        let ids = app.store.server_member_ids(sid).await.unwrap_or_default();
                        app.push_many(&ids).await;
                        // aviso "fulano entrou" no primeiro canal de texto
                        if let Ok(Some(ch)) = app.store.system_channel(sid).await {
                            if let Ok(msg) = app.store.insert_system(app.sf.next(), ch, me, 2, "entrou").await {
                                let out = S2C::Message(msg);
                                for u in &ids {
                                    app.send_to(*u, &out);
                                }
                            }
                        }
                        let _ = tx.send(S2C::ServerJoined { server_id: sid });
                    }
                    Ok(None) => err(ErrorCode::InvalidInvite, "Convite inválido, expirado ou esgotado"),
                    Err(e) => db_err("join_by_invite", e),
                }
            }
        }
        C2S::CreateChannel { server_id, name, kind, parent_id } => {
            let name: String = name
                .trim()
                .to_lowercase()
                .chars()
                .map(|c| if c.is_whitespace() { '-' } else { c })
                .collect();
            if !matches!(kind, 0 | 1 | 4) {
                err(ErrorCode::InvalidInput, "Tipo de canal inválido");
            } else if name.is_empty() || name.chars().count() > 32 {
                err(ErrorCode::InvalidInput, "Nome do canal: 1 a 32 caracteres");
            } else if busy("mkchan", 20, 3600) {
                err(ErrorCode::RateLimited, ERR_BUSY);
            } else if app.store.server_owner(server_id).await.ok().flatten() != Some(me) {
                err(ErrorCode::Forbidden, "Só o dono do servidor cria canais");
            } else if app.store.channel_count(server_id).await.unwrap_or(0) >= 200 {
                err(ErrorCode::InvalidInput, "Este servidor já tem 200 canais");
            } else {
                let parent = if kind == 4 { None } else { parent_id };
                match app.store.create_channel(app.sf.next(), server_id, &name, kind as i16, parent).await {
                    Ok(true) => {
                        let ids = app.store.server_member_ids(server_id).await.unwrap_or_default();
                        app.push_many(&ids).await;
                    }
                    Ok(false) => err(ErrorCode::NotFound, "Categoria não encontrada"),
                    Err(e) => db_err("create_channel", e),
                }
            }
        }
        C2S::Send { channel_id, content, reply_to, nonce, attachments } => {
            let content = content.trim();
            if (content.is_empty() && attachments.is_empty()) || content.chars().count() > 4000 {
                return true;
            }
            if attachments.len() > 10 {
                err(ErrorCode::InvalidInput, "No máximo 10 anexos por mensagem");
                return true;
            }
            if busy("send", 12, 10) {
                err(ErrorCode::RateLimited, ERR_BUSY);
                return true;
            }
            match app.store.can_access(channel_id, me).await {
                Ok(true) => {}
                Ok(false) => return true,
                Err(e) => {
                    db_err("can_access", e);
                    return true;
                }
            }
            if app.store.dm_blocked(channel_id, me).await.unwrap_or(false) {
                err(ErrorCode::Forbidden, "Você não pode enviar mensagens para esta pessoa");
                return true;
            }
            if !attachments.is_empty() && !app.store.pending_ok(&attachments, me, channel_id).await.unwrap_or(false) {
                err(ErrorCode::InvalidInput, "Anexo inválido ou já usado");
                return true;
            }
            // reenvio com o mesmo nonce (cliente reconectou sem saber se a primeira chegou): ignora
            if nonce != 0 {
                let mut n = app.nonces.lock().unwrap_or_else(|e| e.into_inner());
                if !n.0.insert((me, nonce)) {
                    return true;
                }
                n.1.push_back((me, nonce));
                if n.1.len() > 4096 {
                    if let Some(old) = n.1.pop_front() {
                        n.0.remove(&old);
                    }
                }
            }
            match app.store.insert_message(app.sf.next(), channel_id, me, content, reply_to).await {
                Ok(Some(mut msg)) => {
                    msg.nonce = nonce;
                    if !attachments.is_empty() {
                        if let Err(e) = app.store.link_attachments(msg.id, &attachments).await {
                            error!("link_attachments: {e}");
                        }
                        let _ = app.store.attach_files(std::slice::from_mut(&mut msg)).await;
                    }
                    let id = msg.id;
                    if let Err(e) = app.store.add_mentions(channel_id, me, &msg.content).await {
                        error!("add_mentions: {e}");
                    }
                    let to = app.store.participants(channel_id).await.unwrap_or_default();
                    let out = S2C::Message(msg);
                    for u in to {
                        app.send_to(u, &out);
                    }
                    // quem escreve já leu a própria mensagem
                    let _ = app.store.ack(me, channel_id, id).await;
                }
                Ok(None) => err(ErrorCode::NotFound, "A mensagem respondida não existe mais"),
                Err(e) => db_err("insert_message", e),
            }
        }
        C2S::Edit { message_id, content } => {
            let content = content.trim();
            if content.is_empty() || content.chars().count() > 4000 {
                err(ErrorCode::InvalidInput, "A mensagem precisa ter de 1 a 4000 caracteres");
            } else if busy("edit", 20, 30) {
                err(ErrorCode::RateLimited, ERR_BUSY);
            } else {
                match app.store.edit_message(message_id, me, content).await {
                    Ok(Some(channel_id)) => {
                        let out = S2C::MessageEdited { channel_id, message_id, content: content.to_string() };
                        for u in app.store.participants(channel_id).await.unwrap_or_default() {
                            app.send_to(u, &out);
                        }
                    }
                    Ok(None) => err(ErrorCode::Forbidden, "Só o autor edita a mensagem"),
                    Err(e) => db_err("edit_message", e),
                }
            }
        }
        C2S::Delete { message_id } => {
            if busy("delete", 30, 30) {
                err(ErrorCode::RateLimited, ERR_BUSY);
            } else {
                match app.store.delete_message(message_id, me).await {
                    Ok(Some(channel_id)) => {
                        let out = S2C::MessageDeleted { channel_id, message_id };
                        for u in app.store.participants(channel_id).await.unwrap_or_default() {
                            app.send_to(u, &out);
                        }
                    }
                    Ok(None) => err(ErrorCode::Forbidden, "Você não pode apagar esta mensagem"),
                    Err(e) => db_err("delete_message", e),
                }
            }
        }
        C2S::Search { nonce, channel_id, server_id, from, text, before } => {
            let text = text.trim();
            if text.chars().count() > 200 {
                err(ErrorCode::InvalidInput, "A busca aceita até 200 caracteres");
                return true;
            }
            if channel_id.is_none() && server_id.is_none() {
                err(ErrorCode::InvalidInput, "Escolha onde buscar");
                return true;
            }
            if busy("search", 20, 10) {
                err(ErrorCode::RateLimited, ERR_BUSY);
                return true;
            }
            // só busca onde a pessoa pode ler
            let allowed = match (channel_id, server_id) {
                (Some(c), _) => app.store.can_access(c, me).await.unwrap_or(false),
                (None, Some(s)) => app.store.is_member(s, me).await.unwrap_or(false),
                _ => false,
            };
            if !allowed {
                err(ErrorCode::Forbidden, "Você não tem acesso a este lugar");
                return true;
            }
            let scope_channel = channel_id;
            let scope_server = if channel_id.is_some() { None } else { server_id };
            match app.store.search(scope_channel, scope_server, from, text, before, me).await {
                Ok(mut rows) => {
                    let more = rows.len() > 25;
                    rows.truncate(25);
                    let hits = rows.into_iter().map(|(message, channel_name)| p::SearchHit { message, channel_name }).collect();
                    let _ = tx.send(S2C::SearchResults { nonce, hits, more });
                }
                Err(e) => db_err("search", e),
            }
        }
        C2S::Pin { message_id, pin } => {
            if busy("pin", 20, 30) {
                err(ErrorCode::RateLimited, ERR_BUSY);
            } else {
                match app.store.message_channel(message_id).await {
                    Ok(Some(channel_id)) if app.store.can_access(channel_id, me).await.unwrap_or(false) => {
                        if !app.store.can_pin(channel_id, me).await.unwrap_or(false) {
                            err(ErrorCode::Forbidden, "Só o dono do servidor fixa mensagens");
                        } else {
                            match app.store.set_pinned(message_id, channel_id, pin).await {
                                Ok(Some(true)) => {
                                    let to = app.store.participants(channel_id).await.unwrap_or_default();
                                    let ev = S2C::Pinned { channel_id, message_id, pinned: pin };
                                    for u in &to {
                                        app.send_to(*u, &ev);
                                    }
                                    if pin {
                                        // aviso "fulano fixou uma mensagem" (o conteúdo é o id da fixada)
                                        if let Ok(msg) = app.store.insert_system(app.sf.next(), channel_id, me, 3, &message_id.to_string()).await {
                                            let out = S2C::Message(msg);
                                            for u in &to {
                                                app.send_to(*u, &out);
                                            }
                                        }
                                    }
                                }
                                Ok(Some(false)) => {}
                                Ok(None) => err(ErrorCode::InvalidInput, "Este canal já tem 50 mensagens fixadas"),
                                Err(e) => db_err("set_pinned", e),
                            }
                        }
                    }
                    Ok(_) => err(ErrorCode::NotFound, "Mensagem não encontrada"),
                    Err(e) => db_err("message_channel", e),
                }
            }
        }
        C2S::Pins { channel_id } => match app.store.can_access(channel_id, me).await {
            Ok(true) => match app.store.pins(channel_id, me).await {
                Ok(messages) => {
                    let _ = tx.send(S2C::Pins { channel_id, messages });
                }
                Err(e) => db_err("pins", e),
            },
            Ok(false) => {}
            Err(e) => db_err("can_access", e),
        },
        C2S::React { message_id, emoji, add } => {
            // emoji de verdade: sem letras/números/espaços ASCII, curto (sequências com ZWJ cabem)
            let valid = !emoji.is_empty()
                && emoji.chars().count() <= 8
                && emoji.len() <= 32
                && !emoji.chars().any(|c| c.is_ascii_alphanumeric() || c.is_whitespace() || c.is_control());
            if !valid {
                err(ErrorCode::InvalidInput, "Emoji inválido");
            } else if busy("react", 30, 10) {
                err(ErrorCode::RateLimited, ERR_BUSY);
            } else {
                match app.store.message_channel(message_id).await {
                    Ok(Some(channel_id)) if app.store.can_access(channel_id, me).await.unwrap_or(false) => {
                        match app.store.set_reaction(message_id, me, &emoji, add).await {
                            Ok(true) => {
                                let out = S2C::Reaction { channel_id, message_id, emoji, user_id: me, add };
                                for u in app.store.participants(channel_id).await.unwrap_or_default() {
                                    app.send_to(u, &out);
                                }
                            }
                            Ok(false) => {}
                            Err(e) => db_err("set_reaction", e),
                        }
                    }
                    Ok(_) => err(ErrorCode::NotFound, "Mensagem não encontrada"),
                    Err(e) => db_err("message_channel", e),
                }
            }
        }
        C2S::SetStatus { status } => {
            let code = match status {
                Status::Online => 0,
                Status::Idle => 1,
                Status::Dnd => 2,
                Status::Invisible => 3,
                Status::Offline => {
                    err(ErrorCode::InvalidInput, "Use Invisível no lugar de Offline");
                    return true;
                }
            };
            if busy("status", 30, 60) {
                err(ErrorCode::RateLimited, ERR_BUSY);
            } else {
                match app.store.set_status(me, code).await {
                    Ok(()) => {
                        app.push_snapshot(me).await;
                        app.presence_changed(me).await;
                    }
                    Err(e) => db_err("set_status", e),
                }
            }
        }
        C2S::Ack { channel_id, message_id } => {
            if !busy("ack", 120, 10) && app.store.can_access(channel_id, me).await.unwrap_or(false) {
                if let Err(e) = app.store.ack(me, channel_id, message_id).await {
                    db_err("ack", e);
                }
            }
        }
        C2S::Typing { channel_id } => {
            if !busy("typing", 20, 10) && app.store.can_access(channel_id, me).await.unwrap_or(false) {
                let out = S2C::Typing { channel_id, user_id: me };
                for u in app.store.participants(channel_id).await.unwrap_or_default() {
                    if u != me {
                        app.send_to(u, &out);
                    }
                }
            }
        }
        C2S::History { channel_id, before } => match app.store.can_access(channel_id, me).await {
            Ok(true) => match app.store.history(channel_id, before, me).await {
                Ok(messages) => {
                    let _ = tx.send(S2C::History { channel_id, messages });
                }
                Err(e) => db_err("history", e),
            },
            Ok(false) => {}
            Err(e) => db_err("can_access", e),
        },
        C2S::Block { user_id } => {
            if user_id == me {
                err(ErrorCode::InvalidInput, "Você não pode bloquear a si mesmo");
            } else if app.store.display_name(user_id).await.ok().flatten().is_none() {
                err(ErrorCode::NotFound, "Usuário não encontrado");
            } else if busy("block", 20, 60) {
                err(ErrorCode::RateLimited, ERR_BUSY);
            } else {
                match app.store.block(me, user_id).await {
                    Ok(()) => app.push_many(&[me, user_id]).await,
                    Err(e) => db_err("block", e),
                }
            }
        }
        C2S::Unblock { user_id } => match app.store.unblock(me, user_id).await {
            Ok(()) => app.push_many(&[me, user_id]).await,
            Err(e) => db_err("unblock", e),
        },
        C2S::FriendRequest { username } => {
            if busy("friend", 20, 3600) {
                err(ErrorCode::RateLimited, ERR_BUSY);
                return true;
            }
            let Ok(Some(other)) = app.store.find_user_id(username.trim()).await else {
                err(ErrorCode::NotFound, "Usuário não encontrado");
                return true;
            };
            if other == me {
                err(ErrorCode::InvalidInput, "Você não pode adicionar a si mesmo");
            } else if app.store.is_blocking(me, other).await.unwrap_or(false) {
                err(ErrorCode::InvalidInput, "Você bloqueou esta pessoa. Desbloqueie para adicionar.");
            } else if app.store.is_blocking(other, me).await.unwrap_or(false) {
                // quem foi bloqueado não descobre isso: parece que a pessoa não existe
                err(ErrorCode::NotFound, "Usuário não encontrado");
            } else if app.store.are_friends(me, other).await.unwrap_or(false) {
                err(ErrorCode::InvalidInput, "Vocês já são amigos");
            } else if let Err(e) = app.store.friend_request(me, other).await {
                db_err("friend_request", e);
            } else {
                app.push_many(&[me, other]).await;
            }
        }
        C2S::FriendAccept { user_id } => {
            if let Err(e) = app.store.friend_accept(me, user_id).await {
                db_err("friend_accept", e);
            } else {
                app.push_many(&[me, user_id]).await;
            }
        }
        C2S::FriendRemove { user_id } => {
            if let Err(e) = app.store.friend_remove(me, user_id).await {
                db_err("friend_remove", e);
            } else {
                app.push_many(&[me, user_id]).await;
            }
        }
        C2S::OpenDm { user_id } => {
            let allowed = app.store.related_users(me).await.unwrap_or_default().contains(&user_id);
            if !allowed {
                err(ErrorCode::Forbidden, "Você só pode conversar com amigos ou membros dos seus servidores");
            } else if app.store.is_blocking(me, user_id).await.unwrap_or(false) || app.store.is_blocking(user_id, me).await.unwrap_or(false) {
                err(ErrorCode::Forbidden, "Você não pode conversar com esta pessoa");
            } else {
                match app.store.open_dm(app.sf.next(), me, user_id).await {
                    Ok(channel_id) => {
                        app.push_many(&[me, user_id]).await;
                        let _ = tx.send(S2C::DmOpened { channel_id });
                    }
                    Err(e) => db_err("open_dm", e),
                }
            }
        }
        // Tratados em `handle`.
        C2S::Register { .. } | C2S::Login { .. } | C2S::Resume { .. } | C2S::SetIdle { .. } => {}
    }
    true
}

// ------------------------------------------------------------------ vídeo

#[derive(Deserialize)]
struct Q {
    token: String,
}

async fn publish(
    ws: WebSocketUpgrade,
    Path(channel): Path<i64>,
    Query(q): Query<Q>,
    State(app): State<Arc<App>>,
) -> Response {
    let Some(uid) = app.auth_stream(&q.token, channel).await else {
        return StatusCode::FORBIDDEN.into_response();
    };
    // 4 MiB por quadro é folga de sobra para H.264 de tela; limita o uso de memória por sala.
    ws.max_message_size(4 << 20).on_upgrade(move |s| run_publisher(s, channel, uid, app))
}

async fn run_publisher(mut socket: WebSocket, channel: i64, uid: i64, app: Arc<App>) {
    let tx = {
        let mut rooms = app.rooms.lock().unwrap_or_else(|e| e.into_inner());
        if rooms.contains_key(&channel) {
            None
        } else {
            let (tx, _) = broadcast::channel(16);
            rooms.insert(channel, Room { tx: tx.clone(), key: None, channel, user: uid });
            Some(tx)
        }
    };
    let Some(tx) = tx else {
        let _ = socket.send(Message::Text("já há uma transmissão neste canal".into())).await;
        return;
    };
    let people = app.store.participants(channel).await.unwrap_or_default();
    app.push_many(&people).await;
    info!("[canal {channel}] transmissão iniciada por {uid}");

    while let Some(Ok(msg)) = socket.recv().await {
        if let Message::Binary(data) = msg {
            if data.len() < 2 {
                continue;
            }
            if data[0] == 1 {
                if let Some(room) = app.rooms.lock().unwrap_or_else(|e| e.into_inner()).get_mut(&channel) {
                    room.key = Some(data.clone());
                }
            }
            let _ = tx.send(data); // sem espectadores: erro ignorado
        }
    }
    app.rooms.lock().unwrap_or_else(|e| e.into_inner()).remove(&channel); // encerra os espectadores
    app.push_many(&people).await;
    info!("[canal {channel}] transmissão encerrada");
}

async fn watch(
    ws: WebSocketUpgrade,
    Path(channel): Path<i64>,
    Query(q): Query<Q>,
    State(app): State<Arc<App>>,
) -> Response {
    if app.auth_stream(&q.token, channel).await.is_none() {
        return StatusCode::FORBIDDEN.into_response();
    }
    ws.on_upgrade(move |s| run_viewer(s, channel, app))
}

async fn run_viewer(mut socket: WebSocket, channel: i64, app: Arc<App>) {
    let found = {
        let rooms = app.rooms.lock().unwrap_or_else(|e| e.into_inner());
        rooms.get(&channel).map(|r| (r.tx.subscribe(), r.key.clone()))
    };
    let Some((mut rx, key)) = found else {
        let _ = socket.send(Message::Text("ninguém está transmitindo neste canal".into())).await;
        return;
    };
    let mut need_key = key.is_none();
    if let Some(k) = key {
        if socket.send(Message::Binary(k)).await.is_err() {
            return;
        }
    }
    loop {
        tokio::select! {
            frame = rx.recv() => match frame {
                Ok(data) => {
                    if need_key {
                        if data[0] != 1 { continue; }
                        need_key = false;
                    }
                    if socket.send(Message::Binary(data)).await.is_err() { return; }
                }
                // Espectador lento: descarta até o próximo quadro-chave.
                Err(broadcast::error::RecvError::Lagged(_)) => need_key = true,
                Err(broadcast::error::RecvError::Closed) => return,
            },
            incoming = socket.recv() => {
                if !matches!(incoming, Some(Ok(_))) { return; }
            }
        }
    }
}

