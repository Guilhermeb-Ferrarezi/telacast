//! Contrato do protocolo por fixtures JSON.
//!
//! Para CADA variante de `C2S` e de `S2C` este teste monta um valor realista, serializa com `serde_json`
//! (compacto, sem quebra de linha final) e compara byte a byte com `proto/fixtures/{c2s,s2c}/<Variante>.json`.
//! O cliente Android (Kotlin) lê os MESMOS arquivos: mudou o protocolo, quebram os dois lados.
//!
//! * `UPDATE_FIXTURES=1 cargo test -p telacast-proto -j2` regrava os arquivos em vez de comparar.
//! * Adicionar uma variante a `C2S`/`S2C` quebra a COMPILAÇÃO deste arquivo (os `match` de `variantes!`
//!   são exaustivos, sem `_`) e, depois de listada, o teste exige a fixture `<Variante>.json`.
//! * Variantes com casos diferentes (um `Option` preenchido e vazio, por exemplo) ganham arquivos extras
//!   `<Variante>.<caso>.json`; o prefixo antes do primeiro ponto tem de ser o nome de uma variante.
//! * Cada arquivo é lido de volta (JSON -> tipo Rust -> JSON) e tem de dar o mesmo texto.
//! * Arquivo sem variante correspondente (órfão) falha o teste.
//! * `fixtures/extras/`: ENTRADAS escritas à mão (valores desconhecidos, campos ausentes) que o cliente
//!   tem de tolerar; ver `fixtures/extras/LEIA-ME.md`.
//! * `fixtures/binary/` e `fixtures/http/`: quadros de voz e corpos HTTP.

use serde::{de::DeserializeOwned, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
use telacast_proto::*;

// ------------------------------------------------------------------ infraestrutura

fn raiz() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn atualizar() -> bool {
    std::env::var("UPDATE_FIXTURES").is_ok_and(|v| v == "1")
}

/// Compara (ou, com `UPDATE_FIXTURES=1`, grava) um arquivo de texto.
fn confere_arquivo(caminho: &Path, esperado: &str) {
    if atualizar() {
        fs::create_dir_all(caminho.parent().unwrap()).unwrap();
        fs::write(caminho, esperado).unwrap();
        return;
    }
    let lido = fs::read_to_string(caminho).unwrap_or_else(|e| {
        panic!("fixture {} não existe ({e}); rode com UPDATE_FIXTURES=1 para gravá-la", caminho.display())
    });
    assert_eq!(
        lido,
        esperado,
        "fixture {} desatualizada: o protocolo mudou. Rode com UPDATE_FIXTURES=1, confira o diff e avise o lado Kotlin",
        caminho.display()
    );
}

/// Garante que a pasta `sub` só tem os arquivos `itens` (com a extensão dada), e que cada um bate com o
/// valor, volta do JSON para o tipo Rust e reserializa igual.
fn confere_pasta<T: Serialize + DeserializeOwned>(sub: &str, itens: Vec<(&'static str, T)>) {
    let pasta = raiz().join(sub);
    let mut esperados = BTreeSet::new();
    for (nome, valor) in &itens {
        let json = serde_json::to_string(valor).unwrap();
        let arquivo = format!("{nome}.json");
        assert!(esperados.insert(arquivo.clone()), "fixture repetida: {arquivo}");
        let caminho = pasta.join(&arquivo);
        confere_arquivo(&caminho, &json);

        // ida e volta a partir do ARQUIVO (é isso que o Kotlin lê)
        let do_arquivo = fs::read_to_string(&caminho).unwrap();
        let volta: T = serde_json::from_str(&do_arquivo)
            .unwrap_or_else(|e| panic!("{} não deserializa: {e}", caminho.display()));
        assert_eq!(serde_json::to_string(&volta).unwrap(), do_arquivo, "ida e volta de {} mudou o texto", caminho.display());
    }
    // órfãos
    if let Ok(rd) = fs::read_dir(&pasta) {
        for e in rd {
            let nome = e.unwrap().file_name().to_string_lossy().to_string();
            assert!(
                esperados.contains(&nome),
                "fixture órfã {sub}/{nome}: nenhuma variante a gera mais. Apague o arquivo (ou renomeie a variante)"
            );
        }
    }
}

/// Define, a partir da lista de padrões, a função que dá o nome de uma variante e a lista de todos os
/// nomes. O `match` não tem `_`: variante nova no enum deixa de compilar até ser listada aqui.
macro_rules! variantes {
    ($fn_nome:ident, $todas:ident, $ty:ty; $($pat:pat => $nome:literal),+ $(,)?) => {
        fn $fn_nome(v: &$ty) -> &'static str {
            match v { $($pat => $nome),+ }
        }
        const $todas: &[&str] = &[$($nome),+];
    };
}

variantes!(nome_c2s, TODAS_C2S, C2S;
    C2S::Register { .. } => "Register",
    C2S::Login { .. } => "Login",
    C2S::Resume { .. } => "Resume",
    C2S::Logout => "Logout",
    C2S::VoiceJoin { .. } => "VoiceJoin",
    C2S::VoiceLeave => "VoiceLeave",
    C2S::VoiceSet { .. } => "VoiceSet",
    C2S::ChangePassword { .. } => "ChangePassword",
    C2S::ListSessions => "ListSessions",
    C2S::RevokeSession { .. } => "RevokeSession",
    C2S::RevokeOtherSessions => "RevokeOtherSessions",
    C2S::CreateServer { .. } => "CreateServer",
    C2S::JoinServer { .. } => "JoinServer",
    C2S::CreateChannel { .. } => "CreateChannel",
    C2S::Send { .. } => "Send",
    C2S::Edit { .. } => "Edit",
    C2S::Delete { .. } => "Delete",
    C2S::SetStatus { .. } => "SetStatus",
    C2S::SetIdle { .. } => "SetIdle",
    C2S::Search { .. } => "Search",
    C2S::Block { .. } => "Block",
    C2S::Unblock { .. } => "Unblock",
    C2S::Pin { .. } => "Pin",
    C2S::Pins { .. } => "Pins",
    C2S::React { .. } => "React",
    C2S::Ack { .. } => "Ack",
    C2S::Typing { .. } => "Typing",
    C2S::History { .. } => "History",
    C2S::FriendRequest { .. } => "FriendRequest",
    C2S::FriendAccept { .. } => "FriendAccept",
    C2S::FriendRemove { .. } => "FriendRemove",
    C2S::OpenDm { .. } => "OpenDm",
);

variantes!(nome_s2c, TODAS_S2C, S2C;
    S2C::Auth { .. } => "Auth",
    S2C::Viewers { .. } => "Viewers",
    S2C::VoiceUpdate(_) => "VoiceUpdate",
    S2C::VoiceLeft { .. } => "VoiceLeft",
    S2C::Sessions { .. } => "Sessions",
    S2C::PasswordChanged => "PasswordChanged",
    S2C::Error { .. } => "Error",
    S2C::Snapshot(_) => "Snapshot",
    S2C::Message(_) => "Message",
    S2C::History { .. } => "History",
    S2C::SearchResults { .. } => "SearchResults",
    S2C::Pinned { .. } => "Pinned",
    S2C::Pins { .. } => "Pins",
    S2C::Reaction { .. } => "Reaction",
    S2C::MessageEdited { .. } => "MessageEdited",
    S2C::MessageDeleted { .. } => "MessageDeleted",
    S2C::Typing { .. } => "Typing",
    S2C::DmOpened { .. } => "DmOpened",
    S2C::ServerJoined { .. } => "ServerJoined",
);

/// Todas as variantes têm fixture principal `<Variante>.json`, e todo arquivo `<Variante>.<caso>.json`
/// pertence a uma variante do enum.
fn confere_cobertura<T>(itens: &[(&'static str, T)], todas: &[&str], nome: fn(&T) -> &'static str) {
    let mut vistas = BTreeSet::new();
    for (arquivo, valor) in itens {
        let variante = nome(valor);
        assert_eq!(
            arquivo.split('.').next().unwrap(),
            variante,
            "a fixture {arquivo} guarda uma variante {variante}: o nome do arquivo tem de começar por ela"
        );
        if *arquivo == variante {
            vistas.insert(variante);
        }
    }
    for v in todas {
        assert!(vistas.contains(v), "falta a fixture principal da variante {v}");
    }
    assert_eq!(vistas.len(), todas.len());
}

// ------------------------------------------------------------------ dados de exemplo
// Ids estilo snowflake: bem acima de 2^53 (9007199254740992). Quem lê com Double/JS perde precisão;
// o Kotlin tem de usar Long.

const GUILHERME: i64 = 112623612862660608;
const MARINA: i64 = 112623612862660617;
const JOAO: i64 = 112623612862660633;
const ANA: i64 = 112623612862660641;
const SERVIDOR: i64 = 112623700000000001;
const CAT_TEXTO: i64 = 112623700000000010;
const CAT_VOZ: i64 = 112623700000000011;
const GERAL: i64 = 112623700000000012;
const MEMES: i64 = 112623700000000013;
const SALA_VOZ: i64 = 112623700000000014;
const DM_MARINA: i64 = 112623800000000001;
const DM_JOAO: i64 = 112623800000000002;
const MSG1: i64 = 112624001234567890;
const MSG2: i64 = 112624001234567891;
const MSG3: i64 = 112624001234567905;
const ANEXO1: i64 = 112624005550000001;
const SESSAO1: i64 = 112625000000000001;
const SESSAO2: i64 = 112625000000000002;
const TS: i64 = 1762512345678; // 2025-11-07 em milissegundos

fn user(id: i64, name: &str, online: bool, status: Status) -> User {
    User { id, name: name.into(), online, status }
}

fn guilherme() -> User {
    user(GUILHERME, "guilherme", true, Status::Online)
}
fn marina() -> User {
    user(MARINA, "marina.souza", true, Status::Idle)
}
fn joao() -> User {
    user(JOAO, "joao_pedro", true, Status::Dnd)
}
fn ana() -> User {
    user(ANA, "ana-clara", false, Status::Offline)
}

fn imagem() -> Attachment {
    Attachment { id: ANEXO1, name: "foto da praia.png".into(), mime: "image/png".into(), size: 482_913, width: 1280, height: 720 }
}

fn pdf() -> Attachment {
    Attachment { id: ANEXO1 + 1, name: "relatório-final.pdf".into(), mime: "application/octet-stream".into(), size: 1_048_576, width: 0, height: 0 }
}

fn mensagem_simples(id: i64, canal: i64, autor: &User, texto: &str, ts: i64) -> Message {
    Message {
        id,
        channel_id: canal,
        author_id: autor.id,
        author: autor.name.clone(),
        content: texto.into(),
        ts,
        edited: false,
        reply: None,
        nonce: 0,
        reactions: vec![],
        kind: 0,
        pinned: false,
        attachments: vec![],
    }
}

/// Mensagem com tudo preenchido: resposta, reações, anexos, editada, fixada, nonce.
fn mensagem_completa() -> Message {
    Message {
        id: MSG3,
        channel_id: GERAL,
        author_id: MARINA,
        author: "marina.souza".into(),
        content: "Bom dia, pessoal! Segue a foto da viagem e o relatório. **Valeu** @guilherme 🎉".into(),
        ts: TS + 90_000,
        edited: true,
        reply: Some(Reply { id: MSG1, author: "guilherme".into(), content: "Alguém já terminou o relatório de ontem?".into() }),
        nonce: 1_730_000_000_123_456,
        reactions: vec![
            Reaction { emoji: "👍".into(), count: 3, me: true },
            Reaction { emoji: "🎉".into(), count: 1, me: false },
        ],
        kind: 0,
        pinned: true,
        attachments: vec![imagem(), pdf()],
    }
}

fn mensagens_historico(canal: i64) -> Vec<Message> {
    let mut a = mensagem_simples(MSG1, canal, &guilherme(), "Alguém já terminou o relatório de ontem?", TS);
    a.reactions = vec![Reaction { emoji: "👀".into(), count: 2, me: false }];
    let mut b = mensagem_simples(MSG2, canal, &joao(), "Quase! Falta só a parte das conclusões.", TS + 30_000);
    b.reply = Some(Reply { id: MSG1, author: "guilherme".into(), content: "Alguém já terminou o relatório de ontem?".into() });
    let mut c = mensagem_completa();
    c.channel_id = canal;
    c.nonce = 0; // histórico não devolve nonce
    vec![a, b, c]
}

fn voz_marina() -> VoiceState {
    VoiceState { user_id: MARINA, channel_id: SALA_VOZ, muted: true, deafened: false }
}

fn canais() -> Vec<Channel> {
    vec![
        Channel { id: CAT_TEXTO, name: "canais de texto".into(), kind: 4, parent: None, position: 0, last_message_id: 0, last_read_id: 0, mentions: 0 },
        Channel { id: GERAL, name: "geral".into(), kind: 0, parent: Some(CAT_TEXTO), position: 0, last_message_id: MSG3, last_read_id: MSG1, mentions: 2 },
        Channel { id: MEMES, name: "memes-e-risadas".into(), kind: 0, parent: Some(CAT_TEXTO), position: 1, last_message_id: MSG2, last_read_id: MSG2, mentions: 0 },
        Channel { id: CAT_VOZ, name: "canais de voz".into(), kind: 4, parent: None, position: 1, last_message_id: 0, last_read_id: 0, mentions: 0 },
        Channel { id: SALA_VOZ, name: "sala de reunião".into(), kind: 1, parent: Some(CAT_VOZ), position: 0, last_message_id: 0, last_read_id: 0, mentions: 0 },
    ]
}

fn snapshot_cheio() -> Snapshot {
    Snapshot {
        me: guilherme(),
        servers: vec![ServerInfo {
            id: SERVIDOR,
            name: "Turma do Telacast".into(),
            owner: GUILHERME,
            invite: "B4NZ-7TQK".into(),
            channels: canais(),
            members: vec![guilherme(), marina(), joao(), ana()],
        }],
        friends: vec![
            Friend { user: marina(), status: FriendStatus::Friend },
            Friend { user: ana(), status: FriendStatus::Incoming },
            Friend { user: joao(), status: FriendStatus::Outgoing },
        ],
        dms: vec![
            Dm { channel_id: DM_MARINA, user: marina(), last_message_id: MSG3, last_read_id: MSG2, mentions: 1 },
            Dm { channel_id: DM_JOAO, user: joao(), last_message_id: 0, last_read_id: 0, mentions: 0 },
        ],
        live: vec![Live { channel_id: SALA_VOZ, user_id: JOAO }],
        blocked: vec![user(ANA + 8, "spammer.123", false, Status::Offline)],
        voice: vec![voz_marina(), VoiceState { user_id: JOAO, channel_id: SALA_VOZ, muted: false, deafened: false }],
    }
}

// ------------------------------------------------------------------ amostras

fn amostras_c2s() -> Vec<(&'static str, C2S)> {
    let token = "q3Vb9Zt1mXw0L5nY8rJc2HdKpEoAaSuIvGf7TzBN4kM".to_string();
    vec![
        ("Register", C2S::Register { username: "marina.souza".into(), password: "senha-super-secreta-123".into(), invite: "K7QX-M2PD-93FA".into(), proto: PROTO_VERSION }),
        ("Login", C2S::Login { username: "guilherme".into(), password: "correct horse battery".into(), proto: PROTO_VERSION }),
        ("Resume", C2S::Resume { token, proto: PROTO_VERSION }),
        ("Logout", C2S::Logout),
        ("VoiceJoin", C2S::VoiceJoin { channel_id: SALA_VOZ }),
        ("VoiceLeave", C2S::VoiceLeave),
        ("VoiceSet", C2S::VoiceSet { muted: true, deafened: false }),
        ("ChangePassword", C2S::ChangePassword { old: "senha-antiga-123".into(), new: "senha-nova-456".into() }),
        ("ListSessions", C2S::ListSessions),
        ("RevokeSession", C2S::RevokeSession { id: SESSAO2 }),
        ("RevokeOtherSessions", C2S::RevokeOtherSessions),
        ("CreateServer", C2S::CreateServer { name: "Turma do Telacast".into() }),
        ("JoinServer", C2S::JoinServer { code: "B4NZ-7TQK".into() }),
        ("CreateChannel", C2S::CreateChannel { server_id: SERVIDOR, name: "memes e risadas".into(), kind: 0, parent_id: Some(CAT_TEXTO) }),
        ("CreateChannel.categoria", C2S::CreateChannel { server_id: SERVIDOR, name: "canais de voz".into(), kind: 4, parent_id: None }),
        ("Send", C2S::Send {
            channel_id: GERAL,
            content: "Segue a foto e o relatório, valeu @guilherme!".into(),
            reply_to: Some(MSG1),
            nonce: 1_730_000_000_123_456,
            attachments: vec![ANEXO1, ANEXO1 + 1],
        }),
        ("Send.simples", C2S::Send { channel_id: GERAL, content: "Bom dia!".into(), reply_to: None, nonce: 0, attachments: vec![] }),
        ("Edit", C2S::Edit { message_id: MSG3, content: "Bom dia, pessoal! (corrigido)".into() }),
        ("Delete", C2S::Delete { message_id: MSG3 }),
        ("SetStatus", C2S::SetStatus { status: Status::Invisible }),
        ("SetIdle", C2S::SetIdle { idle: true }),
        ("Search", C2S::Search { nonce: 77, channel_id: Some(GERAL), server_id: None, from: None, text: "relatório".into(), before: Some(MSG3) }),
        ("Search.servidor", C2S::Search { nonce: 78, channel_id: None, server_id: Some(SERVIDOR), from: Some(MARINA), text: String::new(), before: None }),
        ("Block", C2S::Block { user_id: ANA + 8 }),
        ("Unblock", C2S::Unblock { user_id: ANA + 8 }),
        ("Pin", C2S::Pin { message_id: MSG3, pin: true }),
        ("Pins", C2S::Pins { channel_id: GERAL }),
        ("React", C2S::React { message_id: MSG3, emoji: "👍".into(), add: true }),
        ("Ack", C2S::Ack { channel_id: GERAL, message_id: MSG3 }),
        ("Typing", C2S::Typing { channel_id: GERAL }),
        ("History", C2S::History { channel_id: GERAL, before: Some(MSG2) }),
        ("History.inicial", C2S::History { channel_id: GERAL, before: None }),
        ("FriendRequest", C2S::FriendRequest { username: "ana-clara".into() }),
        ("FriendAccept", C2S::FriendAccept { user_id: ANA }),
        ("FriendRemove", C2S::FriendRemove { user_id: ANA }),
        ("OpenDm", C2S::OpenDm { user_id: MARINA }),
    ]
}

fn amostras_s2c() -> Vec<(&'static str, S2C)> {
    let token = "q3Vb9Zt1mXw0L5nY8rJc2HdKpEoAaSuIvGf7TzBN4kM".to_string();
    let vazio = Snapshot {
        me: guilherme(),
        servers: vec![],
        friends: vec![],
        dms: vec![],
        live: vec![],
        blocked: vec![],
        voice: vec![],
    };
    let mut sistema = mensagem_simples(MSG2, GERAL, &joao(), "entrou", TS + 5_000);
    sistema.kind = 2;
    let mut fixou = mensagem_simples(MSG2 + 1, GERAL, &marina(), &MSG1.to_string(), TS + 6_000);
    fixou.kind = 3;
    vec![
        ("Auth", S2C::Auth { token, me: guilherme() }),
        ("Viewers", S2C::Viewers { channel_id: SALA_VOZ, users: vec![MARINA, JOAO] }),
        ("Viewers.ninguem", S2C::Viewers { channel_id: SALA_VOZ, users: vec![] }),
        ("VoiceUpdate", S2C::VoiceUpdate(voz_marina())),
        ("VoiceLeft", S2C::VoiceLeft { user_id: MARINA, channel_id: SALA_VOZ }),
        ("Sessions", S2C::Sessions {
            list: vec![
                SessionInfo { id: SESSAO1, device: "203.0.113.42".into(), created: TS - 86_400_000, last_used: TS, current: true },
                SessionInfo { id: SESSAO2, device: "198.51.100.7".into(), created: TS - 3_600_000, last_used: TS - 60_000, current: false },
            ],
        }),
        ("PasswordChanged", S2C::PasswordChanged),
        ("Error", S2C::Error { code: ErrorCode::InvalidCredentials, msg: "Usuário ou senha incorretos".into() }),
        ("Error.limite", S2C::Error { code: ErrorCode::RateLimited, msg: "Muitas ações em pouco tempo. Aguarde um instante.".into() }),
        ("Snapshot", S2C::Snapshot(snapshot_cheio())),
        ("Snapshot.vazio", S2C::Snapshot(vazio)),
        ("Message", S2C::Message(mensagem_completa())),
        ("Message.simples", S2C::Message(mensagem_simples(MSG1, GERAL, &guilherme(), "Alguém já terminou o relatório de ontem?", TS))),
        ("Message.entrou", S2C::Message(sistema)),
        ("Message.fixou", S2C::Message(fixou)),
        ("History", S2C::History { channel_id: GERAL, messages: mensagens_historico(GERAL) }),
        ("History.vazio", S2C::History { channel_id: MEMES, messages: vec![] }),
        ("SearchResults", S2C::SearchResults {
            nonce: 77,
            hits: vec![
                SearchHit { message: mensagem_completa(), channel_name: "geral".into() },
                SearchHit { message: mensagem_simples(MSG2, MEMES, &joao(), "O relatório ficou ótimo", TS + 30_000), channel_name: "memes-e-risadas".into() },
            ],
            more: true,
        }),
        ("SearchResults.vazio", S2C::SearchResults { nonce: 78, hits: vec![], more: false }),
        ("Pinned", S2C::Pinned { channel_id: GERAL, message_id: MSG3, pinned: true }),
        ("Pins", S2C::Pins { channel_id: GERAL, messages: vec![mensagem_completa()] }),
        ("Reaction", S2C::Reaction { channel_id: GERAL, message_id: MSG3, emoji: "🎉".into(), user_id: JOAO, add: true }),
        ("MessageEdited", S2C::MessageEdited { channel_id: GERAL, message_id: MSG3, content: "Bom dia, pessoal! (corrigido)".into() }),
        ("MessageDeleted", S2C::MessageDeleted { channel_id: GERAL, message_id: MSG3 }),
        ("Typing", S2C::Typing { channel_id: GERAL, user_id: MARINA }),
        ("DmOpened", S2C::DmOpened { channel_id: DM_MARINA }),
        ("ServerJoined", S2C::ServerJoined { server_id: SERVIDOR }),
    ]
}

fn manifesto() -> UpdateManifest {
    UpdateManifest {
        version: "0.2.0".into(),
        platform: "linux-x86_64".into(),
        sha256: "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08".into(),
        size: 48_213_504,
        notes: "Correções na lista de mensagens e voz mais estável.".into(),
        signature: "ab".repeat(64),
    }
}

// ------------------------------------------------------------------ testes

#[test]
fn fixtures_c2s() {
    let itens = amostras_c2s();
    confere_cobertura(&itens, TODAS_C2S, nome_c2s);
    confere_pasta("c2s", itens);
}

#[test]
fn fixtures_s2c() {
    let itens = amostras_s2c();
    confere_cobertura(&itens, TODAS_S2C, nome_s2c);
    confere_pasta("s2c", itens);
}

/// Respostas HTTP: `POST /upload` devolve exatamente um `Attachment`; `/update/manifest` um `UpdateManifest`.
#[test]
fn fixtures_http() {
    confere_pasta("http/upload", vec![("UploadResponse", imagem())]);
    confere_pasta("http/update", vec![("UpdateManifest", manifesto())]);
}

/// Os ids grandes precisam sobreviver na fixture como números inteiros (o Kotlin lê como Long).
#[test]
fn ids_grandes_nas_fixtures() {
    let ids = [GUILHERME, SERVIDOR, MSG3];
    for (nome, valor) in amostras_s2c() {
        if nome == "Snapshot" || nome == "Message" {
            let json = serde_json::to_string(&valor).unwrap();
            for id in ids {
                if nome == "Snapshot" || id == MSG3 {
                    assert!(json.contains(&id.to_string()), "{nome} sem o id {id}");
                }
            }
        }
    }
    assert!(GUILHERME > (1 << 53));
}

/// Quadros binários de voz (formato descrito em `VOICE_FRAME`), em hexadecimal minúsculo numa linha.
#[test]
fn fixtures_binarias_de_voz() {
    let opus: [u8; 6] = [0xf8, 0xff, 0xfe, 0x01, 0x02, 0x03]; // bytes de exemplo, não é áudio de verdade
    let seq: u32 = 123_456;

    let mut cliente = vec![VOICE_FRAME];
    cliente.extend_from_slice(&seq.to_be_bytes());
    cliente.extend_from_slice(&opus);

    let mut servidor = vec![VOICE_FRAME];
    servidor.extend_from_slice(&MARINA.to_be_bytes());
    servidor.extend_from_slice(&seq.to_be_bytes());
    servidor.extend_from_slice(&opus);

    assert_eq!(cliente.len(), 1 + 4 + opus.len());
    assert_eq!(servidor.len(), 1 + 8 + 4 + opus.len());
    assert!(opus.len() <= VOICE_MAX_PACKET);

    let hex = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
    confere_arquivo(&raiz().join("binary/voz_cliente_para_servidor.hex"), &hex(&cliente));
    confere_arquivo(&raiz().join("binary/voz_servidor_para_cliente.hex"), &hex(&servidor));

    let esperados: BTreeSet<&str> = ["voz_cliente_para_servidor.hex", "voz_servidor_para_cliente.hex"].into();
    for e in fs::read_dir(raiz().join("binary")).unwrap() {
        let n = e.unwrap().file_name().to_string_lossy().to_string();
        assert!(esperados.contains(n.as_str()), "arquivo órfão em binary/: {n}");
    }
}

// ------------------------------------------------------------------ extras: o que o cliente tem de tolerar

fn extra(nome: &str) -> String {
    let c = raiz().join("extras").join(format!("{nome}.json"));
    fs::read_to_string(&c).unwrap_or_else(|e| panic!("falta a entrada {}: {e}", c.display()))
}

/// Lê uma entrada escrita à mão, deserializa e reserializa; o texto normalizado (com os `default`
/// preenchidos e os campos novos descartados) é o `<caso>.esperado.json`.
fn normaliza<T: Serialize + DeserializeOwned>(nome: &str) -> T {
    let valor: T = serde_json::from_str(&extra(nome)).unwrap_or_else(|e| panic!("extras/{nome}.json não deserializa: {e}"));
    confere_arquivo(&raiz().join("extras").join(format!("{nome}.esperado.json")), &serde_json::to_string(&valor).unwrap());
    valor
}

#[test]
fn extras_error_com_codigo_desconhecido_vira_unknown() {
    match normaliza::<S2C>("error-codigo-desconhecido") {
        S2C::Error { code, msg } => {
            assert_eq!(code, ErrorCode::Unknown);
            assert_eq!(msg, "Isto é uma falha que só o servidor do futuro conhece.");
        }
        outro => panic!("esperava Error, veio {outro:?}"),
    }
}

#[test]
fn extras_error_sem_code_vira_unknown() {
    assert!(matches!(normaliza::<S2C>("error-sem-code"), S2C::Error { code: ErrorCode::Unknown, .. }));
}

#[test]
fn extras_status_desconhecido_vira_offline() {
    match normaliza::<S2C>("status-desconhecido") {
        S2C::Auth { me, .. } => assert_eq!(me.status, Status::Offline),
        outro => panic!("esperava Auth, veio {outro:?}"),
    }
}

#[test]
fn extras_usuario_sem_status_vira_offline() {
    match normaliza::<S2C>("usuario-sem-status") {
        S2C::Auth { me, .. } => {
            assert_eq!(me.status, Status::Offline);
            assert!(me.online);
        }
        outro => panic!("esperava Auth, veio {outro:?}"),
    }
}

#[test]
fn extras_campos_novos_sao_ignorados() {
    match normaliza::<S2C>("campos-novos-ignorados") {
        S2C::Message(m) => {
            assert_eq!(m.id, 112624001234567890);
            assert_eq!(m.reply.as_ref().map(|r| r.id), Some(112624001234567800));
            assert_eq!(m.attachments.len(), 1);
            assert_eq!(m.attachments[0].name, "mapa.png");
        }
        outro => panic!("esperava Message, veio {outro:?}"),
    }
}

#[test]
fn extras_mensagem_minima_usa_defaults() {
    match normaliza::<S2C>("mensagem-minima") {
        S2C::Message(m) => {
            assert!(m.reply.is_none());
            assert_eq!(m.nonce, 0);
            assert!(m.reactions.is_empty());
            assert_eq!(m.kind, 0);
            assert!(!m.pinned);
            assert!(m.attachments.is_empty());
        }
        outro => panic!("esperava Message, veio {outro:?}"),
    }
}

#[test]
fn extras_snapshot_minimo_usa_defaults() {
    match normaliza::<S2C>("snapshot-minimo") {
        S2C::Snapshot(s) => {
            assert!(s.blocked.is_empty());
            assert!(s.voice.is_empty());
            assert_eq!(s.me.status, Status::Offline);
            assert_eq!(s.servers[0].channels[0].mentions, 0);
            assert_eq!(s.servers[0].channels[0].parent, None, "Option ausente vira null");
            assert_eq!(s.dms[0].mentions, 0);
        }
        outro => panic!("esperava Snapshot, veio {outro:?}"),
    }
}

#[test]
fn extras_login_sem_proto_vira_zero() {
    assert!(matches!(normaliza::<C2S>("login-sem-proto"), C2S::Login { proto: 0, .. }));
}

#[test]
fn extras_send_minimo_usa_defaults() {
    match normaliza::<C2S>("send-minimo") {
        C2S::Send { reply_to, nonce, attachments, .. } => {
            assert_eq!(reply_to, None);
            assert_eq!(nonce, 0);
            assert!(attachments.is_empty());
        }
        outro => panic!("esperava Send, veio {outro:?}"),
    }
}

#[test]
fn extras_manifesto_minimo_usa_defaults() {
    let m = normaliza::<UpdateManifest>("manifesto-minimo");
    assert_eq!(m.notes, "");
    assert_eq!(m.signature, "");
}

/// Variante de `S2C` que este cliente não conhece: a deserialização FALHA (não há `serde(other)` no enum
/// de mensagens). O cliente tem de descartar o quadro e seguir em frente, sem derrubar a conexão.
/// Não há `.esperado.json` para este caso.
#[test]
fn extras_variante_s2c_desconhecida_falha_e_deve_ser_ignorada() {
    assert!(serde_json::from_str::<S2C>(&extra("s2c-variante-desconhecida")).is_err());
}

/// Valor desconhecido num enum SEM `serde(other)` (`FriendStatus`): o `Snapshot` inteiro falha.
/// Documenta uma fragilidade conhecida; ver o relatório e `docs/PROTOCOL.md`.
#[test]
fn extras_friend_status_desconhecido_derruba_o_snapshot() {
    assert!(serde_json::from_str::<S2C>(&extra("friend-status-desconhecido")).is_err());
}

/// Cada arquivo de `extras/` é uma entrada de um caso, o `.esperado.json` dele (só dos casos que
/// deserializam) ou o `LEIA-ME.md`; o resto é órfão.
#[test]
fn extras_sem_orfaos() {
    const COM_ESPERADO: &[&str] = &[
        "error-codigo-desconhecido",
        "error-sem-code",
        "status-desconhecido",
        "usuario-sem-status",
        "campos-novos-ignorados",
        "mensagem-minima",
        "snapshot-minimo",
        "login-sem-proto",
        "send-minimo",
        "manifesto-minimo",
    ];
    const SO_ENTRADA: &[&str] = &["s2c-variante-desconhecida", "friend-status-desconhecido"];
    let mut ok: BTreeSet<String> = BTreeSet::new();
    ok.insert("LEIA-ME.md".into());
    for n in COM_ESPERADO {
        ok.insert(format!("{n}.json"));
        ok.insert(format!("{n}.esperado.json"));
    }
    for n in SO_ENTRADA {
        ok.insert(format!("{n}.json"));
    }
    let achados: BTreeSet<String> =
        fs::read_dir(raiz().join("extras")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
    for a in &achados {
        assert!(ok.contains(a), "arquivo órfão em extras/: {a}");
    }
    for o in &ok {
        assert!(achados.contains(o), "falta extras/{o} (rode os testes de extras com UPDATE_FIXTURES=1 para gerar os .esperado.json)");
    }
}
