//! Protocolo do gateway (JSON sobre WebSocket) entre cliente e servidor.
use serde::{Deserialize, Serialize};

/// Versão do protocolo. Sobe quando uma mudança quebra clientes antigos; o servidor recusa outra versão
/// no login com `ErrorCode::VersionMismatch`.
pub const PROTO_VERSION: u32 = 1;

/// Motivo estável de um erro, para o cliente decidir o que fazer sem ler o texto.
/// Um código desconhecido (servidor mais novo) vira `Unknown`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ErrorCode {
    InvalidCredentials,
    InvalidInvite,
    UsernameTaken,
    SessionExpired,
    NotAuthenticated,
    RateLimited,
    Forbidden,
    NotFound,
    InvalidInput,
    VersionMismatch,
    Internal,
    #[default]
    #[serde(other)]
    Unknown,
}

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
    /// Id da última mensagem do canal (0 se vazio) e da última que `me` leu (0 se nunca leu).
    pub last_message_id: i64,
    pub last_read_id: i64,
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
    pub last_message_id: i64,
    pub last_read_id: i64,
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
pub struct Reply {
    pub id: i64,
    pub author: String,
    /// Começo do texto respondido (até 120 caracteres).
    pub content: String,
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
    pub edited: bool,
    pub reply: Option<Reply>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum C2S {
    /// `invite`: código de convite da instância (cadastro fechado).
    Register { username: String, password: String, invite: String, #[serde(default)] proto: u32 },
    Login { username: String, password: String, #[serde(default)] proto: u32 },
    Resume { token: String, #[serde(default)] proto: u32 },
    /// Encerra a sessão (revoga o token) e fecha a conexão.
    Logout,
    CreateServer { name: String },
    JoinServer { code: String },
    /// `kind`: 0 texto ou 4 categoria. `parent_id`: categoria do novo canal (ignorado em categorias).
    CreateChannel { server_id: i64, name: String, kind: u8, parent_id: Option<i64> },
    Send { channel_id: i64, content: String, reply_to: Option<i64> },
    /// Só o autor edita.
    Edit { message_id: i64, content: String },
    /// O autor ou o dono do servidor apaga.
    Delete { message_id: i64 },
    /// Marca o canal como lido até `message_id`.
    Ack { channel_id: i64, message_id: i64 },
    /// "Está digitando…" (reenviar a cada poucos segundos).
    Typing { channel_id: i64 },
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
    Error {
        #[serde(default)]
        code: ErrorCode,
        msg: String,
    },
    Snapshot(Snapshot),
    Message(Message),
    /// Mensagens em ordem cronológica.
    History { channel_id: i64, messages: Vec<Message> },
    MessageEdited { channel_id: i64, message_id: i64, content: String },
    MessageDeleted { channel_id: i64, message_id: i64 },
    Typing { channel_id: i64, user_id: i64 },
    DmOpened { channel_id: i64 },
    ServerJoined { server_id: i64 },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codigo_desconhecido_vira_unknown() {
        let c: ErrorCode = serde_json::from_str("\"CodigoDoFuturo\"").unwrap();
        assert_eq!(c, ErrorCode::Unknown);
        let e: S2C = serde_json::from_str(r#"{"Error":{"msg":"x"}}"#).unwrap();
        assert!(matches!(e, S2C::Error { code: ErrorCode::Unknown, .. }), "servidor antigo sem código também lê");
    }

    #[test]
    fn login_antigo_sem_versao_le_como_zero() {
        let m: C2S = serde_json::from_str(r#"{"Login":{"username":"a","password":"b"}}"#).unwrap();
        assert!(matches!(m, C2S::Login { proto: 0, .. }));
    }
}
