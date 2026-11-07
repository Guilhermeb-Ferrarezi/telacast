//! Telacast: gateway de chat (JSON/WebSocket) + retransmissão de tela (H.264).
mod db;

use argon2::{
    password_hash::{phc::PasswordHash, PasswordHasher, PasswordVerifier},
    Argon2,
};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, Query, State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use bytes::Bytes;
use db::Db;
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};
use telacast_proto as p;
use p::{C2S, S2C};
use tokio::sync::{broadcast, mpsc::{unbounded_channel, UnboundedSender}};

struct Room {
    tx: broadcast::Sender<Bytes>,
    /// Último quadro-chave: novos espectadores começam por ele.
    key: Option<Bytes>,
    channel: i64,
    user: i64,
}

struct App {
    db: Mutex<Db>,
    conns: Mutex<HashMap<i64, Vec<(u64, UnboundedSender<S2C>)>>>,
    rooms: Mutex<HashMap<i64, Room>>,
    next: AtomicU64,
}

// Ordem de locks (nunca ao contrário): db -> conns -> rooms.
impl App {
    fn online(&self, id: i64) -> bool {
        self.conns.lock().unwrap().get(&id).is_some_and(|v| !v.is_empty())
    }

    fn send_to(&self, uid: i64, m: &S2C) {
        if let Some(v) = self.conns.lock().unwrap().get(&uid) {
            for (_, tx) in v {
                let _ = tx.send(m.clone());
            }
        }
    }

    fn snapshot(&self, me: i64) -> db::R<p::Snapshot> {
        let live_rooms: Vec<(i64, i64)> = self
            .rooms
            .lock()
            .unwrap()
            .values()
            .map(|r| (r.channel, r.user))
            .collect();
        let db = self.db.lock().unwrap();
        let user = |id: i64, name: String| p::User { id, name, online: self.online(id) };
        let mut servers = Vec::new();
        for (id, name, owner, invite) in db.servers_of(me)? {
            servers.push(p::ServerInfo {
                id,
                name,
                owner,
                invite,
                channels: db.channels_of(id)?,
                members: db.members_of(id)?.into_iter().map(|(i, n)| user(i, n)).collect(),
            });
        }
        let friends = db
            .friends_of(me)?
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
        let dms = db
            .dms_of(me)?
            .into_iter()
            .map(|(channel_id, id, name)| p::Dm { channel_id, user: user(id, name) })
            .collect();
        let mut live = Vec::new();
        for (channel_id, user_id) in live_rooms {
            if db.can_access(channel_id, me)? {
                live.push(p::Live { channel_id, user_id });
            }
        }
        let me_name = db.user_name(me)?.unwrap_or_default();
        Ok(p::Snapshot { me: user(me, me_name), servers, friends, dms, live })
    }

    fn push_snapshot(&self, uid: i64) {
        if let Ok(s) = self.snapshot(uid) {
            self.send_to(uid, &S2C::Snapshot(s));
        }
    }

    fn push_many(&self, ids: &[i64]) {
        for id in ids {
            self.push_snapshot(*id);
        }
    }

    fn presence_changed(&self, uid: i64) {
        let rel = self.db.lock().unwrap().related_users(uid).unwrap_or_default();
        self.push_many(&rel);
    }

    fn connect(&self, uid: i64, cid: u64, tx: UnboundedSender<S2C>) {
        let first = {
            let mut c = self.conns.lock().unwrap();
            let v = c.entry(uid).or_default();
            v.push((cid, tx));
            v.len() == 1
        };
        if first {
            self.presence_changed(uid);
        }
    }

    fn disconnect(&self, uid: i64, cid: u64) {
        let last = {
            let mut c = self.conns.lock().unwrap();
            if let Some(v) = c.get_mut(&uid) {
                v.retain(|(i, _)| *i != cid);
                if v.is_empty() {
                    c.remove(&uid);
                    true
                } else {
                    false
                }
            } else {
                false
            }
        };
        if last {
            self.presence_changed(uid);
        }
    }

    fn auth_stream(&self, token: &str, channel: i64) -> Option<i64> {
        let db = self.db.lock().unwrap();
        let uid = db.user_by_token(token).ok().flatten()?;
        db.can_access(channel, uid).ok()?.then_some(uid)
    }

    fn participants(&self, channel: i64) -> Vec<i64> {
        self.db.lock().unwrap().channel_participants(channel).unwrap_or_default()
    }
}

#[tokio::main]
async fn main() {
    let addr = std::env::var("TELACAST_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".into());
    let path = std::env::var("TELACAST_DB").unwrap_or_else(|_| "telacast.db".into());
    let app = Arc::new(App {
        db: Mutex::new(Db::open(&path).expect("abrir banco")),
        conns: Default::default(),
        rooms: Default::default(),
        next: AtomicU64::new(1),
    });
    let router = Router::new()
        .route("/ws", get(gateway))
        .route("/publish/{channel}", get(publish))
        .route("/watch/{channel}", get(watch))
        .route("/health", get(|| async { "ok" }))
        .with_state(app);
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    println!("telacast-server em ws://{addr} (banco: {path})");
    axum::serve(listener, router).await.unwrap();
}

// ------------------------------------------------------------------ gateway

async fn gateway(ws: WebSocketUpgrade, State(app): State<Arc<App>>) -> impl IntoResponse {
    ws.max_message_size(64 << 10).on_upgrade(move |s| run_conn(s, app))
}

async fn run_conn(mut socket: WebSocket, app: Arc<App>) {
    let (tx, mut rx) = unbounded_channel::<S2C>();
    let cid = app.next.fetch_add(1, Ordering::Relaxed);
    let mut user: Option<i64> = None;
    loop {
        tokio::select! {
            out = rx.recv() => {
                let Some(m) = out else { break };
                let Ok(json) = serde_json::to_string(&m) else { continue };
                if socket.send(Message::Text(json.into())).await.is_err() { break }
            }
            inc = socket.recv() => match inc {
                Some(Ok(Message::Text(t))) => {
                    if let Ok(m) = serde_json::from_str::<C2S>(&t) {
                        handle(&app, &tx, &mut user, cid, m);
                    }
                }
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                _ => {}
            }
        }
    }
    if let Some(u) = user {
        app.disconnect(u, cid);
    }
}

fn login(app: &Arc<App>, tx: &UnboundedSender<S2C>, user: &mut Option<i64>, cid: u64, id: i64, token: Option<String>) {
    let (token, name) = {
        let db = app.db.lock().unwrap();
        (token.or_else(|| db.create_token(id).ok()), db.user_name(id).ok().flatten())
    };
    let (Some(token), Some(name)) = (token, name) else {
        let _ = tx.send(S2C::Error { msg: "Erro interno ao entrar".into() });
        return;
    };
    *user = Some(id);
    let _ = tx.send(S2C::Auth { token, me: p::User { id, name, online: true } });
    app.connect(id, cid, tx.clone());
    app.push_snapshot(id);
}

fn handle(app: &Arc<App>, tx: &UnboundedSender<S2C>, user: &mut Option<i64>, cid: u64, m: C2S) {
    let err = |s: &str| {
        let _ = tx.send(S2C::Error { msg: s.into() });
    };
    match m {
        C2S::Register { username, password } => {
            if user.is_some() {
                return;
            }
            let name = username.trim();
            let valid = (3..=32).contains(&name.chars().count())
                && name.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '-'));
            if !valid {
                return err("Nome de usuário: 3 a 32 caracteres (letras, números, _ . -)");
            }
            if password.len() < 6 {
                return err("Senha muito curta (mínimo 6 caracteres)");
            }
            let Ok(hash) = Argon2::default().hash_password(password.as_bytes()).map(|h| h.to_string()) else {
                return err("Erro ao processar a senha");
            };
            let created = app.db.lock().unwrap().create_user(name, &hash);
            match created {
                Ok(id) => login(app, tx, user, cid, id, None),
                Err(_) => err("Esse nome de usuário já está em uso"),
            }
        }
        C2S::Login { username, password } => {
            if user.is_some() {
                return;
            }
            let row = app.db.lock().unwrap().user_by_name(username.trim()).ok().flatten();
            let ok = row.as_ref().is_some_and(|(_, h, _)| {
                PasswordHash::new(h)
                    .is_ok_and(|p| Argon2::default().verify_password(password.as_bytes(), &p).is_ok())
            });
            match row {
                Some((id, _, _)) if ok => login(app, tx, user, cid, id, None),
                _ => err("Usuário ou senha incorretos"),
            }
        }
        C2S::Resume { token } => {
            if user.is_some() {
                return;
            }
            let id = app.db.lock().unwrap().user_by_token(&token).ok().flatten();
            match id {
                Some(id) => login(app, tx, user, cid, id, Some(token)),
                None => err("Sessão expirada"),
            }
        }
        other => {
            let Some(me) = *user else {
                return err("Faça login primeiro");
            };
            handle_authed(app, tx, me, other);
        }
    }
}

fn handle_authed(app: &Arc<App>, tx: &UnboundedSender<S2C>, me: i64, m: C2S) {
    let err = |s: &str| {
        let _ = tx.send(S2C::Error { msg: s.into() });
    };
    match m {
        C2S::CreateServer { name } => {
            let name = name.trim();
            if name.is_empty() || name.chars().count() > 50 {
                return err("Nome do servidor: 1 a 50 caracteres");
            }
            let sid = app.db.lock().unwrap().create_server(me, name);
            if let Ok(sid) = sid {
                app.push_snapshot(me);
                let _ = tx.send(S2C::ServerJoined { server_id: sid });
            }
        }
        C2S::JoinServer { code } => {
            let sid = app.db.lock().unwrap().server_by_invite(code.trim()).ok().flatten();
            let Some(sid) = sid else {
                return err("Convite inválido");
            };
            let members = {
                let db = app.db.lock().unwrap();
                let _ = db.add_member(sid, me);
                db.members_of(sid).unwrap_or_default()
            };
            let ids: Vec<i64> = members.into_iter().map(|(i, _)| i).collect();
            app.push_many(&ids);
            let _ = tx.send(S2C::ServerJoined { server_id: sid });
        }
        C2S::CreateChannel { server_id, name } => {
            let name: String = name
                .trim()
                .to_lowercase()
                .chars()
                .map(|c| if c.is_whitespace() { '-' } else { c })
                .collect();
            if name.is_empty() || name.chars().count() > 32 {
                return err("Nome do canal: 1 a 32 caracteres");
            }
            let members = {
                let db = app.db.lock().unwrap();
                if db.server_owner(server_id).ok().flatten() != Some(me) {
                    drop(db);
                    return err("Só o dono do servidor cria canais");
                }
                let _ = db.create_channel(server_id, &name);
                db.members_of(server_id).unwrap_or_default()
            };
            let ids: Vec<i64> = members.into_iter().map(|(i, _)| i).collect();
            app.push_many(&ids);
        }
        C2S::Send { channel_id, content } => {
            let content = content.trim();
            if content.is_empty() || content.chars().count() > 4000 {
                return;
            }
            let (msg, to) = {
                let db = app.db.lock().unwrap();
                if !db.can_access(channel_id, me).unwrap_or(false) {
                    return;
                }
                let Ok(msg) = db.insert_message(channel_id, me, content) else { return };
                (msg, db.channel_participants(channel_id).unwrap_or_default())
            };
            let out = S2C::Message(msg);
            for u in to {
                app.send_to(u, &out);
            }
        }
        C2S::History { channel_id, before } => {
            let messages = {
                let db = app.db.lock().unwrap();
                if !db.can_access(channel_id, me).unwrap_or(false) {
                    return;
                }
                db.history(channel_id, before).unwrap_or_default()
            };
            let _ = tx.send(S2C::History { channel_id, messages });
        }
        C2S::FriendRequest { username } => {
            let other = app.db.lock().unwrap().user_by_name(username.trim()).ok().flatten();
            let Some((other, _, _)) = other else {
                return err("Usuário não encontrado");
            };
            if other == me {
                return err("Você não pode adicionar a si mesmo");
            }
            if app.db.lock().unwrap().are_friends(me, other).unwrap_or(false) {
                return err("Vocês já são amigos");
            }
            let _ = app.db.lock().unwrap().friend_request(me, other);
            app.push_many(&[me, other]);
        }
        C2S::FriendAccept { user_id } => {
            let _ = app.db.lock().unwrap().friend_accept(me, user_id);
            app.push_many(&[me, user_id]);
        }
        C2S::FriendRemove { user_id } => {
            let _ = app.db.lock().unwrap().friend_remove(me, user_id);
            app.push_many(&[me, user_id]);
        }
        C2S::OpenDm { user_id } => {
            let allowed = {
                let db = app.db.lock().unwrap();
                db.related_users(me).unwrap_or_default().contains(&user_id)
            };
            if !allowed {
                return err("Você só pode conversar com amigos ou membros dos seus servidores");
            }
            let ch = app.db.lock().unwrap().open_dm(me, user_id);
            if let Ok(channel_id) = ch {
                app.push_many(&[me, user_id]);
                let _ = tx.send(S2C::DmOpened { channel_id });
            }
        }
        // Tratados em `handle`.
        C2S::Register { .. } | C2S::Login { .. } | C2S::Resume { .. } => {}
    }
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
    let Some(uid) = app.auth_stream(&q.token, channel) else {
        return StatusCode::FORBIDDEN.into_response();
    };
    ws.max_message_size(16 << 20)
        .on_upgrade(move |s| run_publisher(s, channel, uid, app))
}

async fn run_publisher(mut socket: WebSocket, channel: i64, uid: i64, app: Arc<App>) {
    let tx = {
        let mut rooms = app.rooms.lock().unwrap();
        if rooms.contains_key(&channel) {
            None
        } else {
            let (tx, _) = broadcast::channel(32);
            rooms.insert(channel, Room { tx: tx.clone(), key: None, channel, user: uid });
            Some(tx)
        }
    };
    let Some(tx) = tx else {
        let _ = socket.send(Message::Text("já há uma transmissão neste canal".into())).await;
        return;
    };
    let people = app.participants(channel);
    app.push_many(&people);
    println!("[canal {channel}] transmissão iniciada por {uid}");

    while let Some(Ok(msg)) = socket.recv().await {
        if let Message::Binary(data) = msg {
            if data.len() < 2 {
                continue;
            }
            if data[0] == 1 {
                if let Some(room) = app.rooms.lock().unwrap().get_mut(&channel) {
                    room.key = Some(data.clone());
                }
            }
            let _ = tx.send(data); // sem espectadores: erro ignorado
        }
    }
    app.rooms.lock().unwrap().remove(&channel); // derruba o canal e encerra os espectadores
    app.push_many(&people);
    println!("[canal {channel}] transmissão encerrada");
}

async fn watch(
    ws: WebSocketUpgrade,
    Path(channel): Path<i64>,
    Query(q): Query<Q>,
    State(app): State<Arc<App>>,
) -> Response {
    if app.auth_stream(&q.token, channel).is_none() {
        return StatusCode::FORBIDDEN.into_response();
    }
    ws.on_upgrade(move |s| run_viewer(s, channel, app))
}

async fn run_viewer(mut socket: WebSocket, channel: i64, app: Arc<App>) {
    let found = {
        let rooms = app.rooms.lock().unwrap();
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
