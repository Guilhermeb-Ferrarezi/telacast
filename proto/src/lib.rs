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

/// Presença de uma pessoa. `Invisible` só aparece para a própria pessoa (os outros a veem `Offline`).
/// Valor desconhecido (servidor mais novo) vira `Offline`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Status {
    Online,
    Idle,
    Dnd,
    Invisible,
    #[default]
    #[serde(other)]
    Offline,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct User {
    pub id: i64,
    pub name: String,
    /// Conectada e visível (online, ausente ou não perturbe).
    pub online: bool,
    #[serde(default)]
    pub status: Status,
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
    /// Menções a você ainda não lidas neste canal.
    #[serde(default)]
    pub mentions: u32,
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
    #[serde(default)]
    pub mentions: u32,
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

/// Reações de um emoji numa mensagem: quantas pessoas e se você é uma delas.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Reaction {
    pub emoji: String,
    pub count: u32,
    pub me: bool,
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
    /// Devolve o `nonce` que o autor mandou (0 se não houve ou se vem do histórico): o cliente troca
    /// a mensagem provisória pela definitiva.
    #[serde(default)]
    pub nonce: u64,
    /// Só vem preenchido no histórico e na busca; reações novas chegam por `S2C::Reaction`.
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    /// 0 normal, 2 alguém entrou no servidor, 3 alguém fixou uma mensagem (`content` = id da fixada).
    #[serde(default)]
    pub kind: u8,
    #[serde(default)]
    pub pinned: bool,
}

/// Um resultado de busca: a mensagem e onde ela está.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchHit {
    pub message: Message,
    pub channel_name: String,
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
    /// `nonce`: número escolhido pelo cliente (0 = sem). Repetir o mesmo `nonce` não duplica a mensagem.
    Send { channel_id: i64, content: String, reply_to: Option<i64>, #[serde(default)] nonce: u64 },
    /// Só o autor edita.
    Edit { message_id: i64, content: String },
    /// O autor ou o dono do servidor apaga.
    Delete { message_id: i64 },
    /// Status escolhido (fica salvo na conta). `Offline` não é aceito: use `Invisible`.
    SetStatus { status: Status },
    /// O app avisa que ficou parado (ou voltou): "ausente" automático. Vale só para esta conexão.
    SetIdle { idle: bool },
    /// Busca mensagens. Escopo: `channel_id` (um canal ou DM) ou `server_id` (todos os canais de texto do
    /// servidor). `from`: só deste autor. `text` vazio + `from` lista as mensagens da pessoa.
    /// `before`: id da última mensagem da página anterior (paginação). `nonce` volta na resposta.
    Search {
        nonce: u64,
        channel_id: Option<i64>,
        server_id: Option<i64>,
        from: Option<i64>,
        text: String,
        before: Option<i64>,
    },
    /// Fixa ou desafixa uma mensagem (dono do servidor, ou qualquer participante de uma DM). Máx. 50 por canal.
    Pin { message_id: i64, pin: bool },
    /// Pede as mensagens fixadas do canal (resposta: `S2C::Pins`).
    Pins { channel_id: i64 },
    /// Reage (ou tira a reação) com um emoji. Repetir não muda nada.
    React { message_id: i64, emoji: String, add: bool },
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
    /// Resultado de `Search` (mais novos primeiro); `more` indica que há outra página.
    SearchResults { nonce: u64, hits: Vec<SearchHit>, more: bool },
    /// A mensagem foi fixada ou desafixada.
    Pinned { channel_id: i64, message_id: i64, pinned: bool },
    /// Fixadas do canal, a mais recente primeiro.
    Pins { channel_id: i64, messages: Vec<Message> },
    /// Alguém reagiu (`add`) ou tirou a reação.
    Reaction { channel_id: i64, message_id: i64, emoji: String, user_id: i64, add: bool },
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
