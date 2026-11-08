//! Notificações push (FCM): quem deve ser avisado, o texto do aviso, o anti-spam e o envio.
//!
//! O arquivo tem quatro partes independentes:
//! 1. `decide`: função PURA que diz se (e com que tipo) uma pessoa recebe o push de um evento.
//! 2. Corpo do aviso: texto simples a partir do Markdown, título, truncamento.
//! 3. `Throttle`: agrupa por (usuário, canal) em janelas de 3 s e limita a 20 por minuto por usuário.
//! 4. `Fcm` + `PushService`: credencial OAuth2 (JWT RS256), envio HTTP v1 e a fila em tarefa própria.
//!
//! A cola com o resto do servidor (`App::notify_message`, `App::notify_friend_request`) fica no fim.
use crate::{store::Store, App};
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use ring::{rand::SystemRandom, signature};
use serde::Deserialize;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    io::Read,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use telacast_proto::{
    PushData, PUSH_BODY_MAX_CHARS, PUSH_KIND_DM, PUSH_KIND_FRIEND_REQUEST, PUSH_KIND_MENTION, PUSH_KIND_MESSAGE,
    PUSH_PLATFORM_FCM, PUSH_TOKEN_MAX_BYTES,
};
use tracing::{debug, error, info, warn};

// =================================================================== 1. quem é notificado

/// Tipo do aviso (`PushData.kind`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Dm,
    Mention,
    /// Reservado: só sai quando `ChannelNotify::AllMessages` for configurável por canal.
    Message,
    FriendRequest,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Dm => PUSH_KIND_DM,
            Kind::Mention => PUSH_KIND_MENTION,
            Kind::Message => PUSH_KIND_MESSAGE,
            Kind::FriendRequest => PUSH_KIND_FRIEND_REQUEST,
        }
    }
}

/// O que a pessoa quer receber de um canal de servidor.
///
/// PONTO DE EXTENSÃO: hoje não há tabela de preferências e o chamador sempre usa `MentionsOnly`. Quando
/// existirem preferências por canal/servidor (e "silenciar"), basta preencher `Recipient::channel_pref` a
/// partir delas: `decide` já trata os três casos e é testada para eles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ChannelNotify {
    /// Só menções (o padrão do Discord para servidores grandes).
    #[default]
    MentionsOnly,
    /// Toda mensagem do canal (vira `Kind::Message` quando não houver menção).
    AllMessages,
    /// Silenciado: nem menção avisa.
    Nothing,
}

/// De onde vem o evento.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    /// Mensagem numa DM.
    Dm,
    /// Mensagem num canal de servidor.
    Channel,
    /// Pedido de amizade novo.
    FriendRequest,
}

/// O evento, igual para todos os destinatários.
#[derive(Clone, Debug)]
pub struct Event<'a> {
    pub kind: EventKind,
    pub author: i64,
    /// Ids citados com `<@id>`.
    pub mentioned: &'a [i64],
    pub everyone: bool,
    pub here: bool,
    /// Autor da mensagem a que esta responde.
    pub reply_to_author: Option<i64>,
}

/// A situação de um possível destinatário (já levantada pelo chamador).
#[derive(Clone, Debug)]
pub struct Recipient {
    pub id: i64,
    pub has_token: bool,
    /// Tem alguma conexão aberta (o app em segundo plano mantém o socket por alguns minutos).
    pub online: bool,
    /// Alguma conexão está com `SetFocus{true}`: a pessoa está olhando o app.
    pub focused: bool,
    /// Status "Não perturbe".
    pub dnd: bool,
    /// A pessoa bloqueou o autor.
    pub blocked_author: bool,
    pub channel_pref: ChannelNotify,
}

/// Decide se `r` recebe push do evento `ev` e de que tipo. Função pura.
pub fn decide(ev: &Event, r: &Recipient) -> Option<Kind> {
    if r.id == ev.author || !r.has_token || r.focused || r.dnd || r.blocked_author {
        return None;
    }
    match ev.kind {
        EventKind::FriendRequest => Some(Kind::FriendRequest),
        EventKind::Dm => Some(Kind::Dm),
        EventKind::Channel => {
            if r.channel_pref == ChannelNotify::Nothing {
                return None;
            }
            let mention = ev.mentioned.contains(&r.id)
                || ev.reply_to_author == Some(r.id)
                || ev.everyone
                || (ev.here && r.online);
            if mention {
                Some(Kind::Mention)
            } else if r.channel_pref == ChannelNotify::AllMessages {
                Some(Kind::Message)
            } else {
                None
            }
        }
    }
}

/// `(@everyone, @here)` no texto. Quem puder ver o canal recebe o primeiro; só quem está online, o segundo.
pub fn broadcast_flags(content: &str) -> (bool, bool) {
    (content.contains("@everyone"), content.contains("@here"))
}

// =================================================================== 2. corpo do aviso

/// Texto simples de uma mensagem para o corpo do push: sem Markdown, spoiler escondido, menções com o nome,
/// anexos como `📎 nome`, espaços normalizados e no máximo `PUSH_BODY_MAX_CHARS` caracteres.
///
/// `names`: nome de exibição de cada id citado com `<@id>` (id desconhecido vira `@usuário`).
pub fn push_body(content: &str, names: &HashMap<i64, String>, attachments: &[String]) -> String {
    let mut text = String::new();
    for b in telacast_md::parse(content) {
        block_text(&b, names, &mut text);
        text.push(' ');
    }
    let mut out = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if !attachments.is_empty() {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str("📎 ");
        out.push_str(&attachments[0]);
        if attachments.len() > 1 {
            out.push_str(&format!(" (+{})", attachments.len() - 1));
        }
    }
    truncate_chars(&out, PUSH_BODY_MAX_CHARS)
}

fn block_text(b: &telacast_md::Block, names: &HashMap<i64, String>, out: &mut String) {
    use telacast_md::Block;
    match b {
        Block::Paragraph(k) | Block::Heading(_, k) | Block::Subtext(k) => inline_text(k, names, out),
        Block::Quote(bs) => {
            for b in bs {
                block_text(b, names, out);
                out.push(' ');
            }
        }
        Block::List { items, .. } => {
            for it in items {
                inline_text(&it.kids, names, out);
                out.push(' ');
            }
        }
        Block::Code { text, .. } => out.push_str(text),
    }
}

fn inline_text(kids: &[telacast_md::Inline], names: &HashMap<i64, String>, out: &mut String) {
    use telacast_md::Inline;
    for i in kids {
        match i {
            Inline::Text(t) | Inline::Code(t) => out.push_str(t),
            Inline::Strong(k) | Inline::Em(k) | Inline::Under(k) | Inline::Strike(k) => inline_text(k, names, out),
            Inline::Spoiler { .. } => out.push_str("spoiler oculto"),
            Inline::Link { kids, url } => {
                if kids.is_empty() {
                    out.push_str(url);
                } else {
                    inline_text(kids, names, out);
                }
            }
            Inline::User(id) => {
                out.push('@');
                out.push_str(names.get(id).map(String::as_str).unwrap_or("usuário"));
            }
            Inline::Channel(_) => out.push_str("#canal"),
            Inline::Time { .. } => out.push_str("horário"),
            Inline::Broadcast(s) => out.push_str(s),
        }
    }
}

/// Corta em `max` caracteres (não bytes: nunca parte um caractere UTF-8). Se cortou, o último é `…`.
pub fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    while out.ends_with(char::is_whitespace) {
        out.pop();
    }
    out.push('…');
    out
}

/// Título do aviso: autor na DM; "autor em #canal (servidor)" nos canais.
pub fn push_title(kind: EventKind, author: &str, channel: &str, server: Option<&str>) -> String {
    match kind {
        EventKind::Dm => author.to_string(),
        EventKind::Channel => match server {
            Some(s) => format!("{author} em #{channel} ({s})"),
            None => format!("{author} em #{channel}"),
        },
        EventKind::FriendRequest => "Pedido de amizade".to_string(),
    }
}

pub fn friend_request_data(author_id: i64, author: &str, ts: i64) -> PushData {
    PushData {
        kind: Kind::FriendRequest.as_str().into(),
        channel_id: String::new(),
        message_id: String::new(),
        server_id: String::new(),
        title: push_title(EventKind::FriendRequest, author, "", None),
        body: format!("{author} quer ser seu amigo"),
        author_id: author_id.to_string(),
        ts: ts.to_string(),
    }
}

/// Confere o que o cliente mandou em `PushRegister`. Devolve o token sem espaços nas pontas.
pub fn validate_registration<'a>(token: &'a str, platform: &str) -> Result<&'a str, &'static str> {
    if platform != PUSH_PLATFORM_FCM {
        return Err("Plataforma de push desconhecida (use \"fcm\")");
    }
    if token.len() > PUSH_TOKEN_MAX_BYTES {
        return Err("Token de push grande demais (máximo 4096 bytes)");
    }
    let token = token.trim();
    if token.is_empty() {
        return Err("Token de push vazio");
    }
    if token.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return Err("Token de push inválido");
    }
    Ok(token)
}

// =================================================================== 3. anti-spam

/// Agrupa avisos em janelas e limita a frequência por pessoa. Puro: o relógio vem de fora.
///
/// * Por (usuário, chave): o primeiro aviso abre uma janela de `window` (3 s); os que chegam dentro dela
///   substituem o conteúdo (o último vence) e a janela fecha com UM envio.
/// * Por usuário: no máximo `per_minute` envios em 60 s; o excesso some em silêncio.
/// * No máximo `cap` janelas abertas ao mesmo tempo; ao encher, a mais antiga é descartada.
pub struct Throttle<T> {
    window: Duration,
    per_minute: usize,
    cap: usize,
    pending: HashMap<(i64, i64), Pending<T>>,
    sent: HashMap<i64, VecDeque<Instant>>,
}

struct Pending<T> {
    deadline: Instant,
    item: T,
}

impl<T> Throttle<T> {
    pub fn new(window: Duration, per_minute: usize, cap: usize) -> Self {
        Throttle { window, per_minute, cap, pending: HashMap::new(), sent: HashMap::new() }
    }

    /// Entrega um aviso. A chave de uma DM/canal é o id do canal; a de um pedido de amizade, `-id do autor`.
    pub fn offer(&mut self, now: Instant, user: i64, key: i64, item: T) {
        if let Some(p) = self.pending.get_mut(&(user, key)) {
            p.item = item; // o último vence; o prazo da janela não muda
            return;
        }
        if self.pending.len() >= self.cap {
            if let Some(oldest) = self.pending.iter().min_by_key(|(_, p)| p.deadline).map(|(k, _)| *k) {
                self.pending.remove(&oldest);
            }
        }
        self.pending.insert((user, key), Pending { deadline: now + self.window, item });
    }

    /// Janelas que fecharam até `now`, já filtradas pelo limite por minuto, em ordem de prazo.
    pub fn due(&mut self, now: Instant) -> Vec<(i64, T)> {
        let mut keys: Vec<((i64, i64), Instant)> =
            self.pending.iter().filter(|(_, p)| p.deadline <= now).map(|(k, p)| (*k, p.deadline)).collect();
        keys.sort_by_key(|(_, d)| *d);
        let mut out = Vec::new();
        for (k, _) in keys {
            let Some(p) = self.pending.remove(&k) else { continue };
            let q = self.sent.entry(k.0).or_default();
            while q.front().is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(60)) {
                q.pop_front();
            }
            if q.len() >= self.per_minute {
                continue;
            }
            q.push_back(now);
            out.push((k.0, p.item));
        }
        if self.sent.len() > 10_000 {
            self.sent.retain(|_, q| q.back().is_some_and(|t| now.duration_since(*t) < Duration::from_secs(60)));
        }
        out
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.pending.values().map(|p| p.deadline).min()
    }

    #[cfg(test)]
    pub fn pending_len(&self) -> usize {
        self.pending.len()
    }
}

// =================================================================== 4. FCM

const FCM_SCOPE: &str = "https://www.googleapis.com/auth/firebase.messaging";
const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const DEFAULT_ENDPOINT: &str = "https://fcm.googleapis.com/v1/projects/{project_id}/messages:send";
/// Fila de envio: acima disso o mais antigo sai.
const OUT_QUEUE_MAX: usize = 1000;
/// Janelas abertas ao mesmo tempo no anti-spam.
const PENDING_MAX: usize = 5000;
const SEND_CONCURRENCY: usize = 4;

/// Chave da conta de serviço do Firebase (o JSON baixado do console). Nunca é logada.
struct ServiceAccount {
    project_id: String,
    client_email: String,
    token_uri: Option<String>,
    key: signature::RsaKeyPair,
}

impl std::fmt::Debug for ServiceAccount {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServiceAccount").field("project_id", &self.project_id).finish_non_exhaustive()
    }
}

#[derive(Deserialize)]
struct ServiceAccountFile {
    project_id: String,
    client_email: String,
    private_key: String,
    #[serde(default)]
    token_uri: Option<String>,
}

impl ServiceAccount {
    fn parse(json: &str) -> Result<Self, String> {
        let f: ServiceAccountFile =
            serde_json::from_str(json).map_err(|_| "o arquivo não é um JSON de conta de serviço (faltam project_id, client_email ou private_key)".to_string())?;
        if f.project_id.trim().is_empty() || f.client_email.trim().is_empty() {
            return Err("project_id ou client_email vazio".into());
        }
        let key = parse_private_key(&f.private_key)?;
        Ok(ServiceAccount { project_id: f.project_id, client_email: f.client_email, token_uri: f.token_uri.filter(|u| !u.is_empty()), key })
    }
}

/// PEM (PKCS#8 `PRIVATE KEY`, como o Google entrega, ou PKCS#1 `RSA PRIVATE KEY`) para a chave do `ring`.
fn parse_private_key(pem: &str) -> Result<signature::RsaKeyPair, String> {
    let pkcs1 = pem.contains("BEGIN RSA PRIVATE KEY");
    let b64: String = pem.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with("-----")).collect();
    let der = STANDARD.decode(b64.as_bytes()).map_err(|_| "private_key não é um PEM válido".to_string())?;
    let key = if pkcs1 { signature::RsaKeyPair::from_der(&der) } else { signature::RsaKeyPair::from_pkcs8(&der) };
    key.map_err(|_| "private_key rejeitada (esperava uma chave RSA de 2048 bits ou mais em PKCS#8)".to_string())
}

/// JWT RS256 para trocar por um token de acesso (`aud` = endereço de troca).
fn sign_jwt(sa: &ServiceAccount, aud: &str, iat: u64) -> Result<String, String> {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
    let claims = serde_json::json!({
        "iss": sa.client_email,
        "scope": FCM_SCOPE,
        "aud": aud,
        "iat": iat,
        "exp": iat + 3600,
    });
    let claims = URL_SAFE_NO_PAD.encode(claims.to_string());
    let input = format!("{header}.{claims}");
    let mut sig = vec![0u8; sa.key.public().modulus_len()];
    sa.key
        .sign(&signature::RSA_PKCS1_SHA256, &SystemRandom::new(), input.as_bytes(), &mut sig)
        .map_err(|_| "falha ao assinar o JWT".to_string())?;
    Ok(format!("{input}.{}", URL_SAFE_NO_PAD.encode(sig)))
}

/// O que fazer com a resposta de erro do FCM.
#[derive(Debug, PartialEq, Eq)]
enum Reply {
    /// O token não existe mais: apagar do banco.
    InvalidToken,
    /// O token de acesso foi recusado: renovar e tentar de novo.
    Unauthorized,
    /// Falha passageira (429, 5xx): esperar e tentar uma vez mais.
    Retry,
    /// Qualquer outra recusa: descartar este aviso.
    Drop,
}

fn classify(status: u16, body: &str) -> Reply {
    let lower = body.to_ascii_lowercase();
    match status {
        404 => Reply::InvalidToken,
        _ if lower.contains("unregistered") => Reply::InvalidToken,
        // INVALID_ARGUMENT também acontece por erro NOSSO no corpo; só apaga quando o problema é o token.
        400 if lower.contains("invalid_argument") && lower.contains("token") => Reply::InvalidToken,
        401 => Reply::Unauthorized,
        429 | 500..=599 => Reply::Retry,
        _ => Reply::Drop,
    }
}

#[derive(Debug, PartialEq, Eq)]
enum Sent {
    Ok,
    InvalidToken,
    Dropped(String),
}

struct Fcm {
    sa: ServiceAccount,
    endpoint: String,
    token_uri: String,
    retry_delay: Duration,
    agent: ureq::Agent,
    access: Mutex<Option<(String, Instant)>>,
}

impl Fcm {
    fn new(sa: ServiceAccount) -> Fcm {
        let env = |k: &str| std::env::var(k).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let endpoint = env("TELACAST_FCM_ENDPOINT").unwrap_or_else(|| DEFAULT_ENDPOINT.to_string()).replace("{project_id}", &sa.project_id);
        let token_uri = env("TELACAST_FCM_TOKEN_URI").or_else(|| sa.token_uri.clone()).unwrap_or_else(|| DEFAULT_TOKEN_URI.to_string());
        let retry_delay = env("TELACAST_FCM_RETRY_MS").and_then(|v| v.parse().ok()).map(Duration::from_millis).unwrap_or(Duration::from_secs(2));
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .user_agent("telacast-server")
            .build();
        Fcm { sa, endpoint, token_uri, retry_delay, agent, access: Mutex::new(None) }
    }

    /// Token de acesso OAuth2, do cache até 60 s antes de vencer.
    fn access_token(&self, force: bool) -> Result<String, String> {
        let mut cache = self.access.lock().unwrap_or_else(|e| e.into_inner());
        if force {
            *cache = None;
        }
        if let Some((t, exp)) = cache.as_ref() {
            if Instant::now() + Duration::from_secs(60) < *exp {
                return Ok(t.clone());
            }
        }
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let jwt = sign_jwt(&self.sa, &self.token_uri, now)?;
        let resp = self
            .agent
            .post(&self.token_uri)
            .send_form(&[("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"), ("assertion", &jwt)])
            .map_err(|e| match e {
                ureq::Error::Status(code, _) => format!("a troca de credencial foi recusada (HTTP {code})"),
                ureq::Error::Transport(t) => format!("sem resposta na troca de credencial ({})", t.kind()),
            })?;
        #[derive(Deserialize)]
        struct Tok {
            access_token: String,
            #[serde(default)]
            expires_in: u64,
        }
        let tok: Tok = resp.into_json().map_err(|_| "resposta inesperada na troca de credencial".to_string())?;
        let ttl = if tok.expires_in == 0 { 3000 } else { tok.expires_in };
        *cache = Some((tok.access_token.clone(), Instant::now() + Duration::from_secs(ttl)));
        Ok(tok.access_token)
    }

    /// Envia uma mensagem de dados a um token. Bloqueante (rode em `spawn_blocking`).
    fn send(&self, token: &str, data: &PushData) -> Sent {
        let body = serde_json::json!({
            "message": {
                "token": token,
                "data": data,
                "android": { "priority": "HIGH", "ttl": "3600s", "collapse_key": collapse_key(data) },
            }
        });
        let mut last = String::new();
        for attempt in 0..2 {
            let at = match self.access_token(false) {
                Ok(t) => t,
                Err(e) => {
                    last = e;
                    if attempt == 0 {
                        std::thread::sleep(self.retry_delay);
                        continue;
                    }
                    break;
                }
            };
            let res = self.agent.post(&self.endpoint).set("Authorization", &format!("Bearer {at}")).send_json(&body);
            match res {
                Ok(_) => return Sent::Ok,
                Err(ureq::Error::Status(code, resp)) => {
                    let retry_after = resp.header("retry-after").and_then(|v| v.trim().parse::<u64>().ok());
                    let mut text = String::new();
                    let _ = resp.into_reader().take(8192).read_to_string(&mut text);
                    match classify(code, &text) {
                        Reply::InvalidToken => return Sent::InvalidToken,
                        Reply::Unauthorized if attempt == 0 => {
                            let _ = self.access_token(true);
                            continue;
                        }
                        Reply::Retry if attempt == 0 => {
                            let wait = retry_after.map(Duration::from_secs).unwrap_or(self.retry_delay).min(Duration::from_secs(30));
                            std::thread::sleep(wait);
                            continue;
                        }
                        _ => return Sent::Dropped(format!("FCM respondeu HTTP {code}")),
                    }
                }
                Err(ureq::Error::Transport(t)) => {
                    last = format!("falha de rede ({})", t.kind());
                    if attempt == 0 {
                        std::thread::sleep(self.retry_delay);
                        continue;
                    }
                }
            }
        }
        Sent::Dropped(last)
    }
}

/// Mensagens do mesmo canal (ou do mesmo pedido de amizade) se substituem no aparelho que ainda não as recebeu.
fn collapse_key(d: &PushData) -> String {
    if d.channel_id.is_empty() {
        format!("friend-{}", d.author_id)
    } else {
        format!("ch-{}", d.channel_id)
    }
}

// ------------------------------------------------------------------ o serviço (fila em tarefa própria)

/// Um aviso a enviar: a pessoa, todos os aparelhos dela e os dados.
struct Job {
    tokens: Vec<String>,
    data: PushData,
}

struct State {
    throttle: Throttle<Job>,
    out: VecDeque<Job>,
}

struct Shared {
    state: Mutex<State>,
    wake: tokio::sync::Notify,
}

/// Ponta do serviço de push que o gateway usa. Clonar é barato. Só existe se a credencial foi carregada.
#[derive(Clone)]
pub struct PushService {
    shared: Arc<Shared>,
}

impl PushService {
    /// Lê `TELACAST_FCM_SERVICE_ACCOUNT` e liga o serviço. Sem a variável (ou com credencial ruim) devolve
    /// `None` e o servidor segue sem push; o aviso no log nunca traz o conteúdo da credencial.
    pub fn from_env(store: Store) -> Option<PushService> {
        let path = std::env::var("TELACAST_FCM_SERVICE_ACCOUNT").ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let Some(path) = path else {
            warn!("push desligado: defina TELACAST_FCM_SERVICE_ACCOUNT com o JSON da conta de serviço do Firebase (docs/DEPLOY.md)");
            return None;
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(m) = std::fs::metadata(&path) {
                if m.permissions().mode() & 0o077 != 0 {
                    warn!("a credencial do FCM em {path} está legível por outros usuários; use chmod 600");
                }
            }
        }
        let json = match std::fs::read_to_string(&path) {
            Ok(j) => j,
            Err(e) => {
                error!("push desligado: não consegui ler {path} ({})", e.kind());
                return None;
            }
        };
        let sa = match ServiceAccount::parse(&json) {
            Ok(sa) => sa,
            Err(e) => {
                error!("push desligado: credencial do FCM inválida em {path}: {e}");
                return None;
            }
        };
        let window = std::env::var("TELACAST_PUSH_COALESCE_MS").ok().and_then(|v| v.parse().ok()).map(Duration::from_millis).unwrap_or(Duration::from_secs(3));
        info!("push ligado (FCM, projeto {})", sa.project_id);
        let fcm = Arc::new(Fcm::new(sa));
        let shared = Arc::new(Shared {
            state: Mutex::new(State { throttle: Throttle::new(window, 20, PENDING_MAX), out: VecDeque::new() }),
            wake: tokio::sync::Notify::new(),
        });
        tokio::spawn(worker(shared.clone(), fcm, store));
        Some(PushService { shared })
    }

    /// Entra no anti-spam; o envio acontece quando a janela de 3 s fecha.
    fn enqueue(&self, user: i64, key: i64, job: Job) {
        self.shared.state.lock().unwrap_or_else(|e| e.into_inner()).throttle.offer(Instant::now(), user, key, job);
        self.shared.wake.notify_one();
    }
}

/// A tarefa que fecha as janelas do anti-spam e envia, com no máximo `SEND_CONCURRENCY` envios ao mesmo tempo.
async fn worker(shared: Arc<Shared>, fcm: Arc<Fcm>, store: Store) {
    let sem = Arc::new(tokio::sync::Semaphore::new(SEND_CONCURRENCY));
    loop {
        let (job, next) = {
            let mut st = shared.state.lock().unwrap_or_else(|e| e.into_inner());
            let due = st.throttle.due(Instant::now());
            for (_, job) in due {
                if st.out.len() >= OUT_QUEUE_MAX {
                    st.out.pop_front();
                    warn!("fila de push cheia: descartei o aviso mais antigo");
                }
                st.out.push_back(job);
            }
            (st.out.pop_front(), st.throttle.next_deadline())
        };
        if let Some(job) = job {
            let Ok(permit) = sem.clone().acquire_owned().await else { return };
            let (fcm, store) = (fcm.clone(), store.clone());
            tokio::spawn(async move {
                deliver(&fcm, &store, job).await;
                drop(permit);
            });
            continue;
        }
        match next {
            Some(deadline) => {
                tokio::select! {
                    _ = tokio::time::sleep(deadline.saturating_duration_since(Instant::now())) => {}
                    _ = shared.wake.notified() => {}
                }
            }
            None => shared.wake.notified().await,
        }
    }
}

async fn deliver(fcm: &Arc<Fcm>, store: &Store, job: Job) {
    for token in job.tokens {
        let (f, d, t) = (fcm.clone(), job.data.clone(), token.clone());
        let res = tokio::task::spawn_blocking(move || f.send(&t, &d)).await;
        match res {
            Ok(Sent::Ok) => debug!("push {} enviado", job.data.kind),
            Ok(Sent::InvalidToken) => {
                debug!("token de push inválido: apagando");
                if let Err(e) = store.push_delete_token(&token).await {
                    error!("push_delete_token: {e}");
                }
            }
            Ok(Sent::Dropped(why)) => warn!("push descartado: {why}"),
            Err(_) => warn!("push descartado: a tarefa de envio falhou"),
        }
    }
}

// =================================================================== cola com o servidor

impl App {
    /// Estado de conexão de cada usuário: (online, em primeiro plano).
    fn presence_for_push(&self, users: &[i64]) -> HashMap<i64, (bool, bool)> {
        let conns = self.conns.lock().unwrap_or_else(|e| e.into_inner());
        let focused = self.focused.lock().unwrap_or_else(|e| e.into_inner());
        users
            .iter()
            .map(|u| {
                let c = conns.get(u);
                let online = c.is_some_and(|v| !v.is_empty());
                let foc = c.is_some_and(|v| v.iter().any(|(cid, _)| focused.contains(cid)));
                (*u, (online, foc))
            })
            .collect()
    }

    /// Avisa por push quem deve saber desta mensagem nova. Não espera: roda em tarefa própria.
    pub fn notify_message(self: &Arc<Self>, msg: telacast_proto::Message) {
        if self.push.is_none() || msg.kind != 0 {
            return;
        }
        let app = self.clone();
        tokio::spawn(async move {
            if let Err(e) = app.notify_message_inner(&msg).await {
                error!("notify_message: {e}");
            }
        });
    }

    async fn notify_message_inner(&self, msg: &telacast_proto::Message) -> Result<(), sqlx::Error> {
        let Some(push) = &self.push else { return Ok(()) };
        let Some((kind, server_id, channel_name, server_name)) = self.store.push_channel_info(msg.channel_id).await? else { return Ok(()) };
        let is_dm = kind == 10;
        let candidates: Vec<i64> = self.store.participants(msg.channel_id).await?.into_iter().filter(|u| *u != msg.author_id).collect();
        if candidates.is_empty() {
            return Ok(());
        }
        let mut tokens: HashMap<i64, Vec<String>> = HashMap::new();
        for (u, t) in self.store.push_tokens_for(&candidates).await? {
            tokens.entry(u).or_default().push(t);
        }
        if tokens.is_empty() {
            return Ok(());
        }
        let with_tokens: Vec<i64> = tokens.keys().copied().collect();
        let dnd: HashSet<i64> = self.store.manual_statuses(&with_tokens).await?.into_iter().filter(|(_, s)| *s == 2).map(|(u, _)| u).collect();
        let blockers: HashSet<i64> = self.store.blockers_among(msg.author_id, &with_tokens).await?.into_iter().collect();
        let presence = self.presence_for_push(&with_tokens);
        let (mentioned, _) = crate::store::mention_targets(&msg.content);
        let (everyone, here) = broadcast_flags(&msg.content);
        let reply_to_author = match &msg.reply {
            Some(r) => self.store.message_author(r.id).await?,
            None => None,
        };
        let ev = Event {
            kind: if is_dm { EventKind::Dm } else { EventKind::Channel },
            author: msg.author_id,
            mentioned: &mentioned,
            everyone,
            here,
            reply_to_author,
        };
        let mut chosen: Vec<(i64, Kind)> = Vec::new();
        for u in &with_tokens {
            let (online, focused) = presence.get(u).copied().unwrap_or((false, false));
            let r = Recipient {
                id: *u,
                has_token: true,
                online,
                focused,
                dnd: dnd.contains(u),
                blocked_author: blockers.contains(u),
                channel_pref: ChannelNotify::default(),
            };
            if let Some(k) = decide(&ev, &r) {
                chosen.push((*u, k));
            }
        }
        if chosen.is_empty() {
            return Ok(());
        }
        let names: HashMap<i64, String> = if mentioned.is_empty() { HashMap::new() } else { self.store.display_names(&mentioned).await?.into_iter().collect() };
        let files: Vec<String> = msg.attachments.iter().map(|a| a.name.clone()).collect();
        let body = push_body(&msg.content, &names, &files);
        let title = push_title(ev.kind, &msg.author, &channel_name, server_name.as_deref());
        for (u, k) in chosen {
            let data = PushData {
                kind: k.as_str().into(),
                channel_id: msg.channel_id.to_string(),
                message_id: msg.id.to_string(),
                server_id: server_id.map(|s| s.to_string()).unwrap_or_default(),
                title: title.clone(),
                body: body.clone(),
                author_id: msg.author_id.to_string(),
                ts: msg.ts.to_string(),
            };
            push.enqueue(u, msg.channel_id, Job { tokens: tokens.remove(&u).unwrap_or_default(), data });
        }
        Ok(())
    }

    /// Avisa `to` de um pedido de amizade novo vindo de `from`.
    pub fn notify_friend_request(self: &Arc<Self>, from: i64, to: i64) {
        if self.push.is_none() {
            return;
        }
        let app = self.clone();
        tokio::spawn(async move {
            if let Err(e) = app.notify_friend_request_inner(from, to).await {
                error!("notify_friend_request: {e}");
            }
        });
    }

    async fn notify_friend_request_inner(&self, from: i64, to: i64) -> Result<(), sqlx::Error> {
        let Some(push) = &self.push else { return Ok(()) };
        let tokens: Vec<String> = self.store.push_tokens_for(&[to]).await?.into_iter().map(|x| x.1).collect();
        if tokens.is_empty() {
            return Ok(());
        }
        let dnd = self.store.manual_statuses(&[to]).await?.into_iter().any(|(_, s)| s == 2);
        let blocked = self.store.blockers_among(from, &[to]).await?.contains(&to);
        let (online, focused) = self.presence_for_push(&[to]).get(&to).copied().unwrap_or((false, false));
        let ev = Event { kind: EventKind::FriendRequest, author: from, mentioned: &[], everyone: false, here: false, reply_to_author: None };
        let r = Recipient { id: to, has_token: true, online, focused, dnd, blocked_author: blocked, channel_pref: ChannelNotify::default() };
        if decide(&ev, &r).is_none() {
            return Ok(());
        }
        let Some(name) = self.store.display_name(from).await? else { return Ok(()) };
        let ts = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
        push.enqueue(to, -from, Job { tokens, data: friend_request_data(from, &name, ts) });
        Ok(())
    }
}

// =================================================================== testes

#[cfg(test)]
mod tests {
    use super::*;

    const AUTOR: i64 = 1;
    const ANA: i64 = 2;

    fn rec(id: i64) -> Recipient {
        Recipient { id, has_token: true, online: false, focused: false, dnd: false, blocked_author: false, channel_pref: ChannelNotify::MentionsOnly }
    }

    fn canal<'a>(mentioned: &'a [i64]) -> Event<'a> {
        Event { kind: EventKind::Channel, author: AUTOR, mentioned, everyone: false, here: false, reply_to_author: None }
    }

    fn dm() -> Event<'static> {
        Event { kind: EventKind::Dm, author: AUTOR, mentioned: &[], everyone: false, here: false, reply_to_author: None }
    }

    // ---------------------------------------------------------------- decide

    #[test]
    fn dm_sempre_avisa() {
        assert_eq!(decide(&dm(), &rec(ANA)), Some(Kind::Dm));
        // online só em segundo plano também recebe
        assert_eq!(decide(&dm(), &Recipient { online: true, ..rec(ANA) }), Some(Kind::Dm));
    }

    #[test]
    fn dm_nao_avisa_bloqueado_nem_dnd_nem_sem_token_nem_autor() {
        assert_eq!(decide(&dm(), &Recipient { blocked_author: true, ..rec(ANA) }), None);
        assert_eq!(decide(&dm(), &Recipient { dnd: true, ..rec(ANA) }), None);
        assert_eq!(decide(&dm(), &Recipient { has_token: false, ..rec(ANA) }), None);
        assert_eq!(decide(&dm(), &rec(AUTOR)), None, "ninguém é avisado da própria mensagem");
    }

    #[test]
    fn quem_esta_olhando_o_app_nao_recebe() {
        let f = Recipient { focused: true, online: true, ..rec(ANA) };
        assert_eq!(decide(&dm(), &f), None);
        assert_eq!(decide(&canal(&[ANA]), &f), None);
        let amizade = Event { kind: EventKind::FriendRequest, ..dm() };
        assert_eq!(decide(&amizade, &f), None);
    }

    #[test]
    fn canal_comum_sem_mencao_nao_avisa() {
        assert_eq!(decide(&canal(&[]), &rec(ANA)), None);
        assert_eq!(decide(&canal(&[99]), &rec(ANA)), None);
    }

    #[test]
    fn mencao_direta_avisa() {
        assert_eq!(decide(&canal(&[ANA]), &rec(ANA)), Some(Kind::Mention));
        assert_eq!(decide(&canal(&[ANA]), &Recipient { online: true, ..rec(ANA) }), Some(Kind::Mention));
    }

    #[test]
    fn auto_mencao_nao_avisa() {
        assert_eq!(decide(&canal(&[AUTOR]), &rec(AUTOR)), None);
        let todos = Event { everyone: true, ..canal(&[]) };
        assert_eq!(decide(&todos, &rec(AUTOR)), None);
    }

    #[test]
    fn resposta_a_mensagem_dele_avisa() {
        let ev = Event { reply_to_author: Some(ANA), ..canal(&[]) };
        assert_eq!(decide(&ev, &rec(ANA)), Some(Kind::Mention));
        assert_eq!(decide(&ev, &rec(3)), None);
    }

    #[test]
    fn everyone_avisa_todos_e_here_so_online() {
        let todos = Event { everyone: true, ..canal(&[]) };
        assert_eq!(decide(&todos, &rec(ANA)), Some(Kind::Mention));
        assert_eq!(decide(&todos, &Recipient { online: true, ..rec(ANA) }), Some(Kind::Mention));
        let aqui = Event { here: true, ..canal(&[]) };
        assert_eq!(decide(&aqui, &rec(ANA)), None, "@here não acorda quem está offline");
        assert_eq!(decide(&aqui, &Recipient { online: true, ..rec(ANA) }), Some(Kind::Mention));
    }

    #[test]
    fn mencao_respeita_bloqueio_dnd_e_token() {
        let ev = canal(&[ANA]);
        assert_eq!(decide(&ev, &Recipient { blocked_author: true, ..rec(ANA) }), None);
        assert_eq!(decide(&ev, &Recipient { dnd: true, ..rec(ANA) }), None);
        assert_eq!(decide(&ev, &Recipient { has_token: false, ..rec(ANA) }), None);
    }

    #[test]
    fn pedido_de_amizade() {
        let ev = Event { kind: EventKind::FriendRequest, ..dm() };
        assert_eq!(decide(&ev, &rec(ANA)), Some(Kind::FriendRequest));
        assert_eq!(decide(&ev, &Recipient { dnd: true, ..rec(ANA) }), None);
        assert_eq!(decide(&ev, &Recipient { has_token: false, ..rec(ANA) }), None);
        assert_eq!(decide(&ev, &Recipient { blocked_author: true, ..rec(ANA) }), None);
    }

    #[test]
    fn ponto_de_extensao_de_preferencias_de_canal() {
        let tudo = Recipient { channel_pref: ChannelNotify::AllMessages, ..rec(ANA) };
        assert_eq!(decide(&canal(&[]), &tudo), Some(Kind::Message));
        assert_eq!(decide(&canal(&[ANA]), &tudo), Some(Kind::Mention), "menção ganha de 'toda mensagem'");
        let mudo = Recipient { channel_pref: ChannelNotify::Nothing, ..rec(ANA) };
        assert_eq!(decide(&canal(&[ANA]), &mudo), None);
        assert_eq!(decide(&dm(), &mudo), Some(Kind::Dm), "a preferência de canal não vale para DM");
        assert_eq!(Kind::Message.as_str(), "message");
    }

    #[test]
    fn broadcast_no_texto() {
        assert_eq!(broadcast_flags("oi @everyone"), (true, false));
        assert_eq!(broadcast_flags("oi @here"), (false, true));
        assert_eq!(broadcast_flags("oi"), (false, false));
    }

    // ---------------------------------------------------------------- corpo

    fn nomes() -> HashMap<i64, String> {
        HashMap::from([(7, "marina".to_string())])
    }

    #[test]
    fn corpo_sem_markdown() {
        assert_eq!(push_body("**negrito** _itálico_ ~~tachado~~ `código` __sub__", &nomes(), &[]), "negrito itálico tachado código sub");
        assert_eq!(push_body("# Título\ntexto", &nomes(), &[]), "Título texto");
        assert_eq!(push_body("> citação\nresposta", &nomes(), &[]), "citação resposta");
        assert_eq!(push_body("[veja](https://exemplo.com/x) agora", &nomes(), &[]), "veja agora");
        assert_eq!(push_body("- um\n- dois", &nomes(), &[]), "um dois");
        assert_eq!(push_body("```rs\nlet x = 1;\n```", &nomes(), &[]), "let x = 1;");
        assert_eq!(push_body("linha 1\n\n\nlinha   2", &nomes(), &[]), "linha 1 linha 2");
    }

    #[test]
    fn corpo_esconde_spoiler() {
        assert_eq!(push_body("o assassino é ||o mordomo||!", &nomes(), &[]), "o assassino é spoiler oculto!");
        assert_eq!(push_body("||**tudo** isso||", &nomes(), &[]), "spoiler oculto");
    }

    #[test]
    fn corpo_troca_mencoes_por_nome() {
        assert_eq!(push_body("oi <@7> e <@!7>, ok <@99>", &nomes(), &[]), "oi @marina e @marina, ok @usuário");
        assert_eq!(push_body("@everyone reunião", &nomes(), &[]), "@everyone reunião");
    }

    #[test]
    fn corpo_com_anexos() {
        let um = vec!["foto.png".to_string()];
        assert_eq!(push_body("olha", &nomes(), &um), "olha 📎 foto.png");
        assert_eq!(push_body("", &nomes(), &um), "📎 foto.png");
        let tres = vec!["a.png".to_string(), "b.pdf".to_string(), "c.txt".to_string()];
        assert_eq!(push_body("", &nomes(), &tres), "📎 a.png (+2)");
    }

    #[test]
    fn corpo_trunca_em_180_caracteres_sem_partir_utf8() {
        let longo = "ação ".repeat(100);
        let b = push_body(&longo, &nomes(), &[]);
        assert_eq!(b.chars().count(), 180);
        assert!(b.ends_with('…'));
        assert!(b.starts_with("ação ação"));
        // emoji de 4 bytes e acentos na fronteira: nenhum corte pode quebrar o UTF-8 (String já garante, o
        // teste garante que o contador é de caracteres e não de bytes)
        let emojis = "🎉".repeat(300);
        let b = push_body(&emojis, &nomes(), &[]);
        assert_eq!(b.chars().count(), 180);
        assert_eq!(b.chars().filter(|c| *c == '🎉').count(), 179);
        // exatamente no limite não corta
        let exato = "é".repeat(180);
        assert_eq!(push_body(&exato, &nomes(), &[]), exato);
        assert_eq!(truncate_chars("abc", 3), "abc");
        assert_eq!(truncate_chars("abcd", 3), "ab…");
        assert_eq!(truncate_chars("ab cd", 4), "ab…", "não deixa espaço antes das reticências");
    }

    #[test]
    fn titulos() {
        assert_eq!(push_title(EventKind::Dm, "marina", "", None), "marina");
        assert_eq!(push_title(EventKind::Channel, "marina", "geral", Some("Turma")), "marina em #geral (Turma)");
        assert_eq!(push_title(EventKind::FriendRequest, "ana", "", None), "Pedido de amizade");
        let d = friend_request_data(5, "ana", 1000);
        assert_eq!(d.body, "ana quer ser seu amigo");
        assert_eq!(d.kind, "friend_request");
        assert_eq!((d.channel_id.as_str(), d.message_id.as_str(), d.server_id.as_str()), ("", "", ""));
    }

    // ---------------------------------------------------------------- registro

    #[test]
    fn valida_registro() {
        assert_eq!(validate_registration(" abc:DEF-1_2 ", "fcm"), Ok("abc:DEF-1_2"));
        assert!(validate_registration("", "fcm").is_err());
        assert!(validate_registration("   ", "fcm").is_err());
        assert!(validate_registration("abc", "apns").is_err());
        assert!(validate_registration("a b", "fcm").is_err());
        assert!(validate_registration("a\nb", "fcm").is_err());
        assert!(validate_registration(&"x".repeat(4096), "fcm").is_ok());
        assert!(validate_registration(&"x".repeat(4097), "fcm").is_err());
        assert!(validate_registration(&"é".repeat(2049), "fcm").is_err(), "o limite é em bytes");
    }

    // ---------------------------------------------------------------- anti-spam

    fn t0() -> Instant {
        Instant::now()
    }

    fn th() -> Throttle<&'static str> {
        Throttle::new(Duration::from_secs(3), 20, 100)
    }

    #[test]
    fn agrupa_na_janela_e_o_ultimo_vence() {
        let (mut t, a) = (th(), t0());
        t.offer(a, 1, 10, "primeira");
        t.offer(a + Duration::from_secs(1), 1, 10, "segunda");
        t.offer(a + Duration::from_secs(2), 1, 10, "terceira");
        assert!(t.due(a + Duration::from_millis(2999)).is_empty(), "a janela é contada do primeiro aviso");
        assert_eq!(t.due(a + Duration::from_secs(3)), vec![(1, "terceira")]);
        assert_eq!(t.pending_len(), 0);
        assert!(t.due(a + Duration::from_secs(10)).is_empty());
    }

    #[test]
    fn canais_e_usuarios_diferentes_nao_se_misturam() {
        let (mut t, a) = (th(), t0());
        t.offer(a, 1, 10, "u1c10");
        t.offer(a, 1, 11, "u1c11");
        t.offer(a, 2, 10, "u2c10");
        let mut v = t.due(a + Duration::from_secs(3));
        v.sort();
        assert_eq!(v, vec![(1, "u1c10"), (1, "u1c11"), (2, "u2c10")]);
    }

    #[test]
    fn depois_de_fechar_abre_nova_janela() {
        let (mut t, a) = (th(), t0());
        t.offer(a, 1, 10, "a");
        assert_eq!(t.due(a + Duration::from_secs(3)).len(), 1);
        t.offer(a + Duration::from_secs(4), 1, 10, "b");
        assert_eq!(t.next_deadline(), Some(a + Duration::from_secs(7)));
        assert_eq!(t.due(a + Duration::from_secs(7)), vec![(1, "b")]);
    }

    #[test]
    fn vinte_por_minuto_por_usuario() {
        let (mut t, a) = (th(), t0());
        let mut enviados = 0;
        // 30 canais diferentes, todos de uma vez: só 20 saem
        for c in 0..30 {
            t.offer(a, 1, c, "x");
        }
        enviados += t.due(a + Duration::from_secs(3)).len();
        assert_eq!(enviados, 20);
        // outro usuário não é afetado
        t.offer(a + Duration::from_secs(4), 2, 0, "y");
        assert_eq!(t.due(a + Duration::from_secs(7)).len(), 1);
        // depois de 60 s o usuário 1 volta a receber
        t.offer(a + Duration::from_secs(61), 1, 0, "z");
        assert_eq!(t.due(a + Duration::from_secs(64)), vec![(1, "z")]);
    }

    #[test]
    fn janelas_abertas_tem_teto() {
        let mut t: Throttle<u32> = Throttle::new(Duration::from_secs(3), 20, 3);
        let a = t0();
        for i in 0..5u32 {
            t.offer(a + Duration::from_millis(i as u64), i as i64, 0, i);
        }
        assert_eq!(t.pending_len(), 3);
        let mut v: Vec<u32> = t.due(a + Duration::from_secs(4)).into_iter().map(|x| x.1).collect();
        v.sort();
        assert_eq!(v, vec![2, 3, 4], "descartou as mais antigas");
    }

    // ---------------------------------------------------------------- FCM

    #[test]
    fn classifica_respostas_do_fcm() {
        assert_eq!(classify(404, ""), Reply::InvalidToken);
        assert_eq!(classify(404, r#"{"error":{"status":"NOT_FOUND","details":[{"errorCode":"UNREGISTERED"}]}}"#), Reply::InvalidToken);
        assert_eq!(classify(400, r#"{"error":{"status":"INVALID_ARGUMENT","message":"The registration token is not a valid FCM registration token"}}"#), Reply::InvalidToken);
        assert_eq!(classify(400, r#"{"error":{"status":"INVALID_ARGUMENT","message":"Invalid value at 'message.data'"}}"#), Reply::Drop, "erro nosso não apaga token");
        assert_eq!(classify(401, ""), Reply::Unauthorized);
        assert_eq!(classify(429, ""), Reply::Retry);
        assert_eq!(classify(503, ""), Reply::Retry);
        assert_eq!(classify(403, ""), Reply::Drop);
    }

    #[test]
    fn credencial_ruim_nao_vaza_conteudo() {
        let e = ServiceAccount::parse(r#"{"project_id":"p","client_email":"a@b","private_key":"SEGREDO-NAO-PODE-APARECER"}"#).unwrap_err();
        assert!(!e.contains("SEGREDO"));
        assert!(ServiceAccount::parse("lixo").is_err());
    }
}
