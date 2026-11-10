//! Protocolo do gateway (JSON sobre WebSocket) entre cliente e servidor.
use serde::{Deserialize, Serialize};

/// Versão do protocolo. Sobe quando uma mudança quebra clientes antigos; o servidor recusa outra versão
/// no login com `ErrorCode::VersionMismatch`.
pub const PROTO_VERSION: u32 = 2;

/// Pacote binário enviado pelos WebSockets `/publish` e `/watch`.
pub const MEDIA_HEADER_LEN: usize = 9;
pub const MEDIA_MAX_PACKET: usize = 4 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MediaKind {
    H264Delta = 0,
    H264Keyframe = 1,
    SystemAudioOpus = 2,
}

impl TryFrom<u8> for MediaKind {
    type Error = MediaPacketError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::H264Delta),
            1 => Ok(Self::H264Keyframe),
            2 => Ok(Self::SystemAudioOpus),
            other => Err(MediaPacketError::UnknownKind(other)),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MediaPacketRef<'a> {
    pub kind: MediaKind,
    pub pts_us: u64,
    pub payload: &'a [u8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaPacketError {
    TooShort,
    UnknownKind(u8),
    EmptyPayload,
    TooLarge,
}

impl std::fmt::Display for MediaPacketError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooShort => f.write_str("pacote de mídia incompleto"),
            Self::UnknownKind(k) => write!(f, "tipo de pacote de mídia desconhecido: {k}"),
            Self::EmptyPayload => f.write_str("pacote de mídia sem conteúdo"),
            Self::TooLarge => f.write_str("pacote de mídia excede 4 MiB"),
        }
    }
}

impl std::error::Error for MediaPacketError {}

pub fn parse_media_packet(data: &[u8]) -> Result<MediaPacketRef<'_>, MediaPacketError> {
    if data.len() < MEDIA_HEADER_LEN {
        return Err(MediaPacketError::TooShort);
    }
    if data.len() > MEDIA_MAX_PACKET {
        return Err(MediaPacketError::TooLarge);
    }
    let kind = MediaKind::try_from(data[0])?;
    let payload = &data[MEDIA_HEADER_LEN..];
    if payload.is_empty() {
        return Err(MediaPacketError::EmptyPayload);
    }
    Ok(MediaPacketRef { kind, pts_us: u64::from_be_bytes(data[1..9].try_into().unwrap()), payload })
}

pub fn encode_media_packet(kind: MediaKind, pts_us: u64, payload: &[u8]) -> Result<Vec<u8>, MediaPacketError> {
    if payload.is_empty() {
        return Err(MediaPacketError::EmptyPayload);
    }
    if payload.len() > MEDIA_MAX_PACKET - MEDIA_HEADER_LEN {
        return Err(MediaPacketError::TooLarge);
    }
    let mut packet = Vec::with_capacity(MEDIA_HEADER_LEN + payload.len());
    packet.push(kind as u8);
    packet.extend_from_slice(&pts_us.to_be_bytes());
    packet.extend_from_slice(payload);
    Ok(packet)
}

/// Motivo estável de um erro, para o cliente decidir o que fazer sem ler o texto.
/// Um código desconhecido (servidor mais novo) vira `Unknown`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ErrorCode {
    InvalidCredentials,
    /// Senha atual errada ao trocar a senha.
    WrongPassword,
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
    /// 0 texto, 1 voz, 4 categoria (2 anúncio reservado).
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
    /// Monotonic revision for live-share state; clients ignore older snapshots that arrive late.
    #[serde(default)]
    pub live_revision: u64,
    /// Quem você bloqueou.
    #[serde(default)]
    pub blocked: Vec<User>,
    /// Quem está em cada canal de voz dos seus servidores (estado completo; depois vêm `VoiceUpdate`/`VoiceLeft`).
    #[serde(default)]
    pub voice: Vec<VoiceState>,
}

/// Quadro de áudio no WebSocket do gateway (mensagem **binária**; o texto continua sendo JSON).
///
/// Cliente → servidor: `[VOICE_FRAME][seq: u32 big-endian][pacote Opus]`.
/// Servidor → clientes do mesmo canal de voz: `[VOICE_FRAME][remetente: i64 big-endian][seq: u32 big-endian][pacote Opus]`.
/// Opus mono, 48 kHz, quadros de 20 ms (um pacote por quadro); pacote de até 400 bytes. Quem está em silêncio
/// (VAD) não manda nada: quem recebe considera "falando" quem mandou pacote nos últimos ~250 ms.
/// O valor 1 identifica áudio; outros ficam livres (o vídeo usa conexão própria).
pub const VOICE_FRAME: u8 = 1;
pub const VOICE_MAX_PACKET: usize = 400;

/// Quem está num canal de voz e com que estado. Mudo/surdo são escolha da própria pessoa.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct VoiceState {
    pub user_id: i64,
    pub channel_id: i64,
    pub muted: bool,
    /// Surdo: não ouve ninguém (e, como no Discord, também fica mudo).
    pub deafened: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Reply {
    pub id: i64,
    pub author: String,
    /// Começo do texto respondido (até 120 caracteres).
    pub content: String,
}

/// Arquivo anexado a uma mensagem. O download é por HTTP: `GET /files/{id}/{name}?token=...`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Attachment {
    pub id: i64,
    pub name: String,
    pub mime: String,
    pub size: u64,
    /// 0 se não for imagem (ou se as dimensões não puderam ser lidas).
    pub width: u32,
    pub height: u32,
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
    #[serde(default)]
    pub attachments: Vec<Attachment>,
}

/// Um resultado de busca: a mensagem e onde ela está.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchHit {
    pub message: Message,
    pub channel_name: String,
}

/// Uma sessão (login) da conta. `device` é de onde ela foi aberta (endereço IP).
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SessionInfo {
    pub id: i64,
    pub device: String,
    /// Milissegundos desde 1970.
    pub created: i64,
    pub last_used: i64,
    /// É a sessão desta conexão.
    pub current: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum C2S {
    /// `invite`: código de convite da instância (cadastro fechado).
    Register { username: String, password: String, invite: String, #[serde(default)] proto: u32 },
    Login { username: String, password: String, #[serde(default)] proto: u32 },
    Resume { token: String, #[serde(default)] proto: u32 },
    /// Encerra a sessão (revoga o token) e fecha a conexão.
    Logout,
    /// Entra num canal de voz (sai do que estava; só um por vez). Depois disso valem os quadros binários.
    VoiceJoin { channel_id: i64 },
    VoiceLeave,
    /// Mudo/surdo próprios.
    VoiceSet { muted: bool, deafened: bool },
    /// Troca a senha. Encerra todas as outras sessões (a atual continua).
    ChangePassword { old: String, new: String },
    /// Pede a lista de sessões da conta (resposta: `Sessions`).
    ListSessions,
    /// Encerra uma sessão pelo id (resposta: `Sessions` atualizada).
    RevokeSession { id: i64 },
    /// Encerra todas as sessões, menos a atual.
    RevokeOtherSessions,
    CreateServer { name: String },
    JoinServer { code: String },
    /// `kind`: 0 texto ou 4 categoria. `parent_id`: categoria do novo canal (ignorado em categorias).
    CreateChannel { server_id: i64, name: String, kind: u8, parent_id: Option<i64> },
    /// `nonce`: número escolhido pelo cliente (0 = sem). Repetir o mesmo `nonce` não duplica a mensagem.
    Send {
        channel_id: i64,
        content: String,
        reply_to: Option<i64>,
        #[serde(default)]
        nonce: u64,
        /// Ids de arquivos já enviados por HTTP (`POST /upload`); a mensagem pode ter só anexos.
        #[serde(default)]
        attachments: Vec<i64>,
    },
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
    /// Bloqueia: some a amizade, nenhum dos dois abre DM nem manda mensagem privada ao outro, e o outro
    /// não consegue mais te pedir amizade. As mensagens dele em servidores continuam existindo (o app as esconde).
    Block { user_id: i64 },
    Unblock { user_id: i64 },
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
    /// Quem está assistindo à transmissão do canal (vai ao transmissor e aos espectadores; lista vazia = ninguém).
    Viewers { channel_id: i64, users: Vec<i64> },
    /// Alguém entrou num canal de voz ou mudou de estado (inclui você).
    VoiceUpdate(VoiceState),
    /// Alguém saiu do canal de voz (ou a conexão dele caiu).
    VoiceLeft { user_id: i64, channel_id: i64 },
    Sessions { list: Vec<SessionInfo> },
    /// A senha foi trocada (as outras sessões foram encerradas).
    PasswordChanged,
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

// ------------------------------------------------------------------ atualização do app

/// Manifesto de atualização: o servidor publica, o app confere (hash e, se houver chave, assinatura).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct UpdateManifest {
    /// Versão do app, por exemplo `0.2.0`.
    pub version: String,
    /// `linux-x86_64` (sistema e arquitetura, como em `std::env::consts`).
    pub platform: String,
    /// SHA-256 do executável, em hexadecimal minúsculo (64 caracteres).
    pub sha256: String,
    pub size: u64,
    #[serde(default)]
    pub notes: String,
    /// Assinatura Ed25519 (hex) de [`update_signing_message`]. Vazia = sem assinatura.
    #[serde(default)]
    pub signature: String,
}

/// Bytes que a assinatura cobre: amarra versão, plataforma, hash e tamanho (trocar qualquer um invalida).
pub fn update_signing_message(version: &str, platform: &str, sha256: &str, size: u64) -> Vec<u8> {
    format!("telacast-update-v1\n{version}\n{platform}\n{sha256}\n{size}\n").into_bytes()
}

/// `candidate` é uma versão mais nova que `current`? Compara `maior.menor.correção` numericamente
/// (ignora sufixo depois de `-` ou `+`); o que não for número vale 0.
pub fn version_is_newer(candidate: &str, current: &str) -> bool {
    fn parts(v: &str) -> [u64; 3] {
        let core = v.trim().trim_start_matches('v').split(['-', '+']).next().unwrap_or("");
        let mut out = [0u64; 3];
        for (i, p) in core.split('.').take(3).enumerate() {
            out[i] = p.parse().unwrap_or(0);
        }
        out
    }
    parts(candidate) > parts(current)
}

#[cfg(test)]
mod update_tests {
    use super::*;

    #[test]
    fn compara_versoes() {
        assert!(version_is_newer("0.2.0", "0.1.9"));
        assert!(version_is_newer("0.10.0", "0.9.5"));
        assert!(version_is_newer("1.0.0", "0.99.99"));
        assert!(!version_is_newer("0.1.0", "0.1.0"));
        assert!(!version_is_newer("0.1.0", "0.1.1"));
        assert!(version_is_newer("v0.2.0-beta", "0.1.0"));
        assert!(!version_is_newer("lixo", "0.0.1"));
    }

    #[test]
    fn mensagem_de_assinatura_amarra_os_campos() {
        let a = update_signing_message("0.2.0", "linux-x86_64", "ab", 10);
        assert_ne!(a, update_signing_message("0.2.1", "linux-x86_64", "ab", 10));
        assert_ne!(a, update_signing_message("0.2.0", "linux-aarch64", "ab", 10));
        assert_ne!(a, update_signing_message("0.2.0", "linux-x86_64", "ac", 10));
        assert_ne!(a, update_signing_message("0.2.0", "linux-x86_64", "ab", 11));
    }
}
