//! Telacast: gateway de chat (JSON/WebSocket) + retransmissão de tela (H.264), com PostgreSQL.
mod auth;
mod ids;
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
use p::{C2S, S2C};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    net::SocketAddr,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use store::{RegisterError, Store};
use telacast_proto as p;
use tokio::sync::{
    broadcast,
    mpsc::{unbounded_channel, UnboundedSender},
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
    conns: Mutex<HashMap<i64, Vec<(u64, UnboundedSender<S2C>)>>>,
    rooms: Mutex<HashMap<i64, Room>>,
    next: AtomicU64,
    /// Limita quantos hashes de senha rodam ao mesmo tempo (cada um usa ~19 MiB e CPU).
    hash_sem: Semaphore,
    auth_limit: Limiter,
    limits: Limiter,
    /// Hash de uma senha qualquer, usado para gastar o mesmo tempo quando o usuário não existe.
    dummy_hash: String,
}

const ERR_BUSY: &str = "Muitas ações em pouco tempo. Aguarde um instante.";
const ERR_INTERNAL: &str = "Erro interno do servidor.";

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

    async fn snapshot(&self, me: i64) -> Result<p::Snapshot, sqlx::Error> {
        let online: HashSet<i64> = self.conns.lock().unwrap_or_else(|e| e.into_inner()).keys().copied().collect();
        let live_rooms: Vec<(i64, i64)> = self
            .rooms
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .values()
            .map(|r| (r.channel, r.user))
            .collect();
        let user = |id: i64, name: String| p::User { id, name, online: online.contains(&id) };

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
            .map(|(channel_id, id, name, last_message_id, last_read_id)| p::Dm {
                channel_id,
                user: user(id, name),
                last_message_id,
                last_read_id,
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
        Ok(p::Snapshot { me: user(me, me_name), servers, friends, dms, live })
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

    fn connect(self: &Arc<Self>, uid: i64, cid: u64, tx: UnboundedSender<S2C>) {
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

    fn disconnect(self: &Arc<Self>, uid: i64, cid: u64) {
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
            let app = self.clone();
            tokio::spawn(async move { app.presence_changed(uid).await });
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
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();
    let cli = Cli::parse();

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
        hash_sem: Semaphore::new(permits),
        auth_limit: Limiter::default(),
        limits: Limiter::default(),
        dummy_hash,
    });
    let router = Router::new()
        .route("/ws", get(gateway))
        .route("/publish/{channel}", get(publish))
        .route("/watch/{channel}", get(watch))
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
) -> impl IntoResponse {
    let ip = client_ip(&headers, peer);
    ws.max_message_size(64 << 10).on_upgrade(move |s| run_conn(s, app, ip))
}

struct Session {
    uid: i64,
    token_hash: Vec<u8>,
}

async fn run_conn(mut socket: WebSocket, app: Arc<App>, ip: String) {
    let (tx, mut rx) = unbounded_channel::<S2C>();
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
                let Some(m) = out else { break };
                let Ok(json) = serde_json::to_string(&m) else { continue };
                if socket.send(Message::Text(json.into())).await.is_err() { break }
            }
            inc = socket.recv() => {
                last_seen = tokio::time::Instant::now();
                match inc {
                    Some(Ok(Message::Text(t))) => {
                        if let Ok(m) = serde_json::from_str::<C2S>(&t) {
                            if !handle(&app, &tx, &mut session, cid, &ip, m).await { break }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    _ => {}
                }
            }
        }
    }
    // Entrega o que ficou na fila (por exemplo, a resposta de um Logout).
    while let Ok(m) = rx.try_recv() {
        if let Ok(json) = serde_json::to_string(&m) {
            let _ = socket.send(Message::Text(json.into())).await;
        }
    }
    if let Some(s) = session {
        app.disconnect(s.uid, cid);
    }
}

async fn finish_login(
    app: &Arc<App>,
    tx: &UnboundedSender<S2C>,
    session: &mut Option<Session>,
    cid: u64,
    uid: i64,
    existing_token: Option<String>,
) {
    let token = match existing_token {
        Some(t) => t,
        None => {
            let t = auth::new_token();
            if let Err(e) = app.store.create_session(app.sf.next(), uid, &auth::hash_token(&t)).await {
                error!("create_session: {e}");
                let _ = tx.send(S2C::Error { msg: ERR_INTERNAL.into() });
                return;
            }
            t
        }
    };
    let Ok(Some(name)) = app.store.display_name(uid).await else {
        let _ = tx.send(S2C::Error { msg: ERR_INTERNAL.into() });
        return;
    };
    *session = Some(Session { uid, token_hash: auth::hash_token(&token) });
    let _ = tx.send(S2C::Auth { token, me: p::User { id: uid, name, online: true } });
    app.connect(uid, cid, tx.clone());
    app.push_snapshot(uid).await;
}

/// Devolve `false` quando a conexão deve ser encerrada.
async fn handle(
    app: &Arc<App>,
    tx: &UnboundedSender<S2C>,
    session: &mut Option<Session>,
    cid: u64,
    ip: &str,
    m: C2S,
) -> bool {
    let err = |s: &str| {
        let _ = tx.send(S2C::Error { msg: s.into() });
    };
    match m {
        C2S::Register { username, password, invite } => {
            if session.is_some() {
                return true;
            }
            if !app.auth_limit.allow(&format!("auth:{ip}"), 10, Duration::from_secs(300)) {
                err("Muitas tentativas. Tente de novo em alguns minutos.");
                return true;
            }
            let name = username.trim();
            let valid = (3..=32).contains(&name.chars().count())
                && name.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '-'));
            if !valid {
                err("Nome de usuário: 3 a 32 caracteres (letras, números, _ . -)");
                return true;
            }
            if !(8..=128).contains(&password.chars().count()) {
                err("A senha precisa ter de 8 a 128 caracteres");
                return true;
            }
            let code = auth::normalize_code(&invite);
            if code.is_empty() {
                err("Informe o código de convite");
                return true;
            }
            let Ok(hash) = app.hash_password(password).await else {
                err(ERR_INTERNAL);
                return true;
            };
            match app.store.register(app.sf.next(), name, &hash, &auth::hash_token(&code)).await {
                Ok(id) => finish_login(app, tx, session, cid, id, None).await,
                Err(RegisterError::InvalidInvite) => err("Código de convite inválido, expirado ou já usado"),
                Err(RegisterError::Taken) => err("Esse nome de usuário já está em uso"),
                Err(RegisterError::Db(e)) => {
                    error!("register: {e}");
                    err(ERR_INTERNAL);
                }
            }
            true
        }
        C2S::Login { username, password } => {
            if session.is_some() {
                return true;
            }
            let uname = username.trim().to_lowercase();
            if !app.auth_limit.allow(&format!("auth:{ip}"), 10, Duration::from_secs(300))
                || !app.auth_limit.allow(&format!("login:{uname}"), 10, Duration::from_secs(300))
            {
                err("Muitas tentativas. Tente de novo em alguns minutos.");
                return true;
            }
            let row = match app.store.user_by_username(&uname).await {
                Ok(r) => r,
                Err(e) => {
                    error!("login: {e}");
                    err(ERR_INTERNAL);
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
                Some(id) if ok => finish_login(app, tx, session, cid, id, None).await,
                _ => err("Usuário ou senha incorretos"),
            }
            true
        }
        C2S::Resume { token } => {
            if session.is_some() {
                return true;
            }
            match app.store.session_user(&auth::hash_token(&token)).await {
                Ok(Some(id)) => finish_login(app, tx, session, cid, id, Some(token)).await,
                Ok(None) => err("Sessão expirada"),
                Err(e) => {
                    error!("resume: {e}");
                    err(ERR_INTERNAL);
                }
            }
            true
        }
        other => {
            let Some(s) = session.as_ref() else {
                err("Faça login primeiro");
                return true;
            };
            let (me, token_hash) = (s.uid, s.token_hash.clone());
            handle_authed(app, tx, me, &token_hash, other).await
        }
    }
}

async fn handle_authed(
    app: &Arc<App>,
    tx: &UnboundedSender<S2C>,
    me: i64,
    token_hash: &[u8],
    m: C2S,
) -> bool {
    let err = |s: &str| {
        let _ = tx.send(S2C::Error { msg: s.into() });
    };
    let busy = |key: &str, max: usize, secs: u64| !app.limits.allow(&format!("{key}:{me}"), max, Duration::from_secs(secs));
    let db_err = |what: &str, e: sqlx::Error| {
        error!("{what}: {e}");
        let _ = tx.send(S2C::Error { msg: ERR_INTERNAL.into() });
    };
    match m {
        C2S::Logout => {
            let _ = app.store.revoke_session(token_hash).await;
            return false;
        }
        C2S::CreateServer { name } => {
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 50 {
                err("Nome do servidor: 1 a 50 caracteres");
            } else if busy("mkserver", 5, 3600) {
                err(ERR_BUSY);
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
                err(ERR_BUSY);
            } else {
                match app.store.join_by_invite(&auth::normalize_code(&code), me).await {
                    Ok(Some(sid)) => {
                        let ids = app.store.server_member_ids(sid).await.unwrap_or_default();
                        app.push_many(&ids).await;
                        let _ = tx.send(S2C::ServerJoined { server_id: sid });
                    }
                    Ok(None) => err("Convite inválido, expirado ou esgotado"),
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
            if kind != 0 && kind != 4 {
                err("Tipo de canal inválido");
            } else if name.is_empty() || name.chars().count() > 32 {
                err("Nome do canal: 1 a 32 caracteres");
            } else if busy("mkchan", 20, 3600) {
                err(ERR_BUSY);
            } else if app.store.server_owner(server_id).await.ok().flatten() != Some(me) {
                err("Só o dono do servidor cria canais");
            } else {
                let parent = if kind == 4 { None } else { parent_id };
                match app.store.create_channel(app.sf.next(), server_id, &name, kind as i16, parent).await {
                    Ok(true) => {
                        let ids = app.store.server_member_ids(server_id).await.unwrap_or_default();
                        app.push_many(&ids).await;
                    }
                    Ok(false) => err("Categoria não encontrada"),
                    Err(e) => db_err("create_channel", e),
                }
            }
        }
        C2S::Send { channel_id, content, reply_to } => {
            let content = content.trim();
            if content.is_empty() || content.chars().count() > 4000 {
                return true;
            }
            if busy("send", 12, 10) {
                err(ERR_BUSY);
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
            match app.store.insert_message(app.sf.next(), channel_id, me, content, reply_to).await {
                Ok(Some(msg)) => {
                    let id = msg.id;
                    let to = app.store.participants(channel_id).await.unwrap_or_default();
                    let out = S2C::Message(msg);
                    for u in to {
                        app.send_to(u, &out);
                    }
                    // quem escreve já leu a própria mensagem
                    let _ = app.store.ack(me, channel_id, id).await;
                }
                Ok(None) => err("A mensagem respondida não existe mais"),
                Err(e) => db_err("insert_message", e),
            }
        }
        C2S::Edit { message_id, content } => {
            let content = content.trim();
            if content.is_empty() || content.chars().count() > 4000 {
                err("A mensagem precisa ter de 1 a 4000 caracteres");
            } else if busy("edit", 20, 30) {
                err(ERR_BUSY);
            } else {
                match app.store.edit_message(message_id, me, content).await {
                    Ok(Some(channel_id)) => {
                        let out = S2C::MessageEdited { channel_id, message_id, content: content.to_string() };
                        for u in app.store.participants(channel_id).await.unwrap_or_default() {
                            app.send_to(u, &out);
                        }
                    }
                    Ok(None) => err("Só o autor edita a mensagem"),
                    Err(e) => db_err("edit_message", e),
                }
            }
        }
        C2S::Delete { message_id } => {
            if busy("delete", 30, 30) {
                err(ERR_BUSY);
            } else {
                match app.store.delete_message(message_id, me).await {
                    Ok(Some(channel_id)) => {
                        let out = S2C::MessageDeleted { channel_id, message_id };
                        for u in app.store.participants(channel_id).await.unwrap_or_default() {
                            app.send_to(u, &out);
                        }
                    }
                    Ok(None) => err("Você não pode apagar esta mensagem"),
                    Err(e) => db_err("delete_message", e),
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
            Ok(true) => match app.store.history(channel_id, before).await {
                Ok(messages) => {
                    let _ = tx.send(S2C::History { channel_id, messages });
                }
                Err(e) => db_err("history", e),
            },
            Ok(false) => {}
            Err(e) => db_err("can_access", e),
        },
        C2S::FriendRequest { username } => {
            if busy("friend", 20, 3600) {
                err(ERR_BUSY);
                return true;
            }
            let Ok(Some(other)) = app.store.find_user_id(username.trim()).await else {
                err("Usuário não encontrado");
                return true;
            };
            if other == me {
                err("Você não pode adicionar a si mesmo");
            } else if app.store.are_friends(me, other).await.unwrap_or(false) {
                err("Vocês já são amigos");
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
                err("Você só pode conversar com amigos ou membros dos seus servidores");
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
        C2S::Register { .. } | C2S::Login { .. } | C2S::Resume { .. } => {}
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

