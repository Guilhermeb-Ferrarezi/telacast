//! Protocolo do gateway (JSON sobre WebSocket) entre cliente e servidor.
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub online: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Channel {
    pub id: i64,
    pub name: String,
    /// 0 texto, 4 categoria (1 voz e 2 anúncio reservados).
    pub kind: u8,
    /// Categoria a que o canal pertence; `None` fica solto no topo.
    pub parent: Option<i64>,
    pub position: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ServerInfo {
    pub id: i64,
    pub name: String,
    pub owner: i64,
    pub invite: String,
    pub channels: Vec<Channel>,
    pub members: Vec<User>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum FriendStatus {
    Friend,
    /// Pedido recebido, aguardando você aceitar.
    Incoming,
    /// Pedido enviado por você.
    Outgoing,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Friend {
    pub user: User,
    pub status: FriendStatus,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Dm {
    pub channel_id: i64,
    pub user: User,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Live {
    pub channel_id: i64,
    pub user_id: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Snapshot {
    pub me: User,
    pub servers: Vec<ServerInfo>,
    pub friends: Vec<Friend>,
    pub dms: Vec<Dm>,
    pub live: Vec<Live>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Message {
    pub id: i64,
    pub channel_id: i64,
    pub author_id: i64,
    pub author: String,
    pub content: String,
    /// Milissegundos desde a época Unix (UTC).
    pub ts: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum C2S {
    /// `invite`: código de convite da instância (cadastro fechado).
    Register { username: String, password: String, invite: String },
    Login { username: String, password: String },
    Resume { token: String },
    /// Encerra a sessão (revoga o token) e fecha a conexão.
    Logout,
    CreateServer { name: String },
    JoinServer { code: String },
    /// `kind`: 0 texto ou 4 categoria. `parent_id`: categoria do novo canal (ignorado em categorias).
    CreateChannel { server_id: i64, name: String, kind: u8, parent_id: Option<i64> },
    Send { channel_id: i64, content: String },
    History { channel_id: i64, before: Option<i64> },
    FriendRequest { username: String },
    FriendAccept { user_id: i64 },
    /// Remove amigo, recusa ou cancela pedido.
    FriendRemove { user_id: i64 },
    OpenDm { user_id: i64 },
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum S2C {
    Auth { token: String, me: User },
    Error { msg: String },
    Snapshot(Snapshot),
    Message(Message),
    /// Mensagens em ordem cronológica.
    History { channel_id: i64, messages: Vec<Message> },
    DmOpened { channel_id: i64 },
    ServerJoined { server_id: i64 },
}
