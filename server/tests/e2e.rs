//! Teste de ponta a ponta: sobe o servidor de verdade, num banco novo, e conversa com ele pelo WebSocket.
//! Precisa do Postgres de desenvolvimento no ar (docker-compose.dev.yml).
use std::{
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use telacast_proto::{C2S, S2C};
use tungstenite::{stream::MaybeTlsStream, Message, WebSocket};

const ADMIN_URL: &str = "postgres://telacast:telacast_dev@127.0.0.1:5432/postgres";

struct Env {
    child: Child,
    port: u16,
    db: String,
    db_url: String,
    files: String,
    rt: tokio::runtime::Runtime,
}

impl Env {
    fn start() -> Env {
        Env::start_with(&[])
    }

    fn start_with(extra: &[(&str, &str)]) -> Env {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let db = format!("telacast_test_{nanos}");
        rt.block_on(async {
            let pool = sqlx::PgPool::connect(ADMIN_URL)
                .await
                .expect("o Postgres de desenvolvimento precisa estar no ar (docker compose -f docker-compose.dev.yml up -d)");
            sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {db}"))).execute(&pool).await.unwrap();
            pool.close().await;
        });
        let db_url = format!("postgres://telacast:telacast_dev@127.0.0.1:5432/{db}");
        let files = format!("/tmp/telacast_test_files_{nanos}");
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let child = Command::new(env!("CARGO_BIN_EXE_telacast-server"))
            .env("DATABASE_URL", &db_url)
            .env("TELACAST_ADDR", format!("127.0.0.1:{port}"))
            .env("RUST_LOG", "warn")
            .env("TELACAST_FILES_DIR", &files)
            .envs(extra.iter().copied())
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let env = Env { child, port, db, db_url, files, rt };
        let start = Instant::now();
        while TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(start.elapsed() < Duration::from_secs(30), "o servidor não subiu");
            std::thread::sleep(Duration::from_millis(100));
        }
        env
    }

    /// Gera um convite de cadastro pela linha de comando, como o operador faria.
    fn invite(&self, uses: u32) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_telacast-server"))
            .args(["invite", "--uses", &uses.to_string()])
            .env("DATABASE_URL", &self.db_url)
            .env("RUST_LOG", "warn")
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        text.lines()
            .find_map(|l| l.strip_prefix("Código de convite: "))
            .unwrap_or_else(|| panic!("saída inesperada do comando invite: {text}"))
            .trim()
            .to_string()
    }

    fn client(&self) -> Client {
        let (ws, _) = tungstenite::connect(format!("ws://127.0.0.1:{}/ws", self.port)).unwrap();
        if let MaybeTlsStream::Plain(s) = ws.get_ref() {
            s.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
        }
        Client { ws }
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.files);
        let db = self.db.clone();
        self.rt.block_on(async {
            if let Ok(pool) = sqlx::PgPool::connect(ADMIN_URL).await {
                let _ = sqlx::query(sqlx::AssertSqlSafe(format!("DROP DATABASE IF EXISTS {db} WITH (FORCE)")))
                    .execute(&pool)
                    .await;
            }
        });
    }
}

struct Client {
    ws: WebSocket<MaybeTlsStream<TcpStream>>,
}

impl Client {
    fn send(&mut self, m: C2S) {
        self.ws.send(Message::Text(serde_json::to_string(&m).unwrap().into())).unwrap();
    }

    /// Lê eventos até achar um que satisfaça `pick` (ou estourar o tempo).
    fn wait<T>(&mut self, what: &str, mut pick: impl FnMut(&S2C) -> Option<T>) -> T {
        let start = Instant::now();
        loop {
            assert!(start.elapsed() < Duration::from_secs(10), "tempo esgotado esperando: {what}");
            match self.ws.read() {
                Ok(Message::Text(t)) => {
                    if let Some(v) = pick(&serde_json::from_str::<S2C>(&t).unwrap()) {
                        return v;
                    }
                }
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                Err(e) => panic!("conexão caiu esperando {what}: {e}"),
            }
        }
    }

    fn error_code(&mut self) -> (telacast_proto::ErrorCode, String) {
        self.wait("um erro com código", |m| if let S2C::Error { code, msg } = m { Some((*code, msg.clone())) } else { None })
    }

    fn error(&mut self) -> String {
        self.wait("um erro", |m| if let S2C::Error { msg, .. } = m { Some(msg.clone()) } else { None })
    }

    /// Cadastra e devolve (token, id).
    fn register(&mut self, name: &str, invite: &str) -> (String, i64) {
        self.send(C2S::Register { username: name.into(), password: "segredo-forte-1".into(), invite: invite.into(), proto: telacast_proto::PROTO_VERSION });
        self.wait("o Auth do cadastro", |m| {
            if let S2C::Auth { token, me } = m {
                Some((token.clone(), me.id))
            } else {
                None
            }
        })
    }
}

#[test]
fn fluxo_completo() {
    let env = Env::start();

    // --- cadastro fechado: sem convite válido não entra
    let mut c = env.client();
    c.send(C2S::Register { username: "intruso".into(), password: "segredo-forte-1".into(), invite: "NAOEXISTE".into(), proto: telacast_proto::PROTO_VERSION });
    assert!(c.error().contains("convite"), "esperava recusa por convite");

    // --- convite de 2 usos: duas contas entram, a terceira não
    let code = env.invite(2);
    let mut ana = env.client();
    let (ana_token, ana_id) = ana.register("ana", &code);
    let mut beto = env.client();
    let (_, beto_id) = beto.register("beto", &code.to_lowercase()); // minúsculas também valem
    let mut carla = env.client();
    carla.send(C2S::Register { username: "carla".into(), password: "segredo-forte-1".into(), invite: code.clone(), proto: telacast_proto::PROTO_VERSION });
    assert!(carla.error().contains("convite"), "o convite esgotado deveria ser recusado");

    // nome já usado
    let code2 = env.invite(1);
    let mut dup = env.client();
    dup.send(C2S::Register { username: "ANA".into(), password: "segredo-forte-1".into(), invite: code2.clone(), proto: telacast_proto::PROTO_VERSION });
    assert!(dup.error().contains("em uso"));
    // a falha não consumiu o convite: ainda dá para usá-lo
    let mut carla = env.client();
    let (_, carla_id) = carla.register("carla", &code2);
    assert_ne!(carla_id, ana_id);

    // --- login
    let mut bad = env.client();
    bad.send(C2S::Login { username: "ana".into(), password: "errada-errada".into(), proto: telacast_proto::PROTO_VERSION });
    assert!(bad.error().contains("incorretos"));
    let mut bad2 = env.client();
    bad2.send(C2S::Login { username: "ninguem".into(), password: "errada-errada".into(), proto: telacast_proto::PROTO_VERSION });
    assert!(bad2.error().contains("incorretos"), "usuário inexistente deve dar a mesma mensagem");

    // --- servidor, canal e convite de servidor
    ana.send(C2S::CreateServer { name: "Casa".into() });
    // O servidor manda o snapshot antes do ServerJoined, então espero o snapshot que já contém o servidor.
    let snap = ana.wait("snapshot com o servidor", |m| match m {
        S2C::Snapshot(s) if !s.servers.is_empty() => Some(s.clone()),
        _ => None,
    });
    let casa = &snap.servers[0];
    let server_id = casa.id;
    // servidor novo: a categoria "canais de texto" com o canal "geral" dentro
    assert_eq!(casa.channels.len(), 2);
    let cat = casa.channels.iter().find(|c| c.kind == 4).expect("categoria padrão");
    let geral_ch = casa.channels.iter().find(|c| c.kind == 0).expect("canal padrão");
    assert_eq!(cat.name, "canais de texto");
    assert_eq!(geral_ch.name, "geral");
    assert_eq!(geral_ch.parent, Some(cat.id));
    let cat_id = cat.id;
    assert_eq!(casa.invite.len(), 26, "convite de servidor com 128 bits");
    let geral = geral_ch.id;
    let invite = casa.invite.clone();

    beto.send(C2S::JoinServer { code: "CODIGO-INVALIDO".into() });
    assert!(beto.error().contains("Convite"));
    beto.send(C2S::JoinServer { code: invite });
    beto.wait("ServerJoined do beto", |m| matches!(m, S2C::ServerJoined { .. }).then_some(()));

    // só o dono cria canais
    beto.send(C2S::CreateChannel { server_id, name: "Planos de Hoje".into(), kind: 0, parent_id: None });
    assert!(beto.error().contains("dono"));
    ana.send(C2S::CreateChannel { server_id, name: "Planos de Hoje".into(), kind: 0, parent_id: None });
    let snap = ana.wait("snapshot com o novo canal", |m| match m {
        S2C::Snapshot(s) if s.servers.iter().any(|x| x.channels.len() == 3) => Some(s.clone()),
        _ => None,
    });
    // a categoria que não existe é recusada; categoria dentro de outra é ignorada
    ana.send(C2S::CreateChannel { server_id, name: "x".into(), kind: 0, parent_id: Some(geral) });
    assert!(ana.error().contains("Categoria"), "pai precisa ser categoria");
    ana.send(C2S::CreateChannel { server_id, name: "x".into(), kind: 7, parent_id: None });
    assert!(ana.error().contains("Tipo"));
    ana.send(C2S::CreateChannel { server_id, name: "Voz".into(), kind: 0, parent_id: Some(cat_id) });
    let snap = ana.wait("snapshot com o canal dentro da categoria", |m| match m {
        S2C::Snapshot(s) if s.servers.iter().any(|x| x.channels.len() == 4) => Some(s.clone()),
        _ => None,
    });
    let novo = snap.servers[0].channels.iter().find(|c| c.name == "planos-de-hoje").expect("canal novo");
    assert_eq!((novo.kind, novo.parent), (0, None));
    let voz = snap.servers[0].channels.iter().find(|c| c.name == "voz").expect("canal na categoria");
    assert_eq!(voz.parent, Some(cat_id));

    // categoria não recebe mensagem
    ana.send(C2S::Send { channel_id: cat_id, content: "isso não vai".into(), reply_to: None, nonce: 0, attachments: vec![] });

    // --- mensagens em tempo real e histórico
    ana.send(C2S::Send { channel_id: geral, content: "oi beto!".into(), reply_to: None, nonce: 0, attachments: vec![] });
    let msg = beto.wait("a mensagem", |m| if let S2C::Message(m) = m { Some(m.clone()) } else { None });
    assert_eq!(msg.content, "oi beto!");
    assert_eq!(msg.author, "ana");
    assert!(msg.id > 0 && msg.ts > 1_767_225_600_000, "id e hora vêm do snowflake");
    beto.send(C2S::History { channel_id: geral, before: None });
    let hist = beto.wait("o histórico", |m| if let S2C::History { messages, .. } = m { Some(messages.clone()) } else { None });
    assert_eq!(hist.iter().filter(|m| m.kind == 0).count(), 1);
    assert_eq!(hist.iter().find(|m| m.kind == 0).unwrap().id, msg.id);

    // quem não é do servidor não lê nem escreve
    carla.send(C2S::History { channel_id: geral, before: None });
    carla.send(C2S::Send { channel_id: geral, content: "invasão".into(), reply_to: None, nonce: 0, attachments: vec![] });
    std::thread::sleep(Duration::from_millis(400));
    ana.send(C2S::History { channel_id: geral, before: None });
    let hist = ana.wait("o histórico da ana", |m| if let S2C::History { messages, .. } = m { Some(messages.clone()) } else { None });
    assert_eq!(hist.iter().filter(|m| m.kind == 0).count(), 1, "a mensagem da carla não deveria existir");

    // --- amizade e DM
    ana.send(C2S::FriendRequest { username: "carla".into() });
    let snap = carla.wait("pedido recebido", |m| match m {
        S2C::Snapshot(s) if !s.friends.is_empty() => Some(s.clone()),
        _ => None,
    });
    assert_eq!(snap.friends[0].user.id, ana_id);
    carla.send(C2S::FriendAccept { user_id: ana_id });
    carla.send(C2S::OpenDm { user_id: ana_id });
    let dm = carla.wait("DmOpened", |m| if let S2C::DmOpened { channel_id } = m { Some(*channel_id) } else { None });
    carla.send(C2S::Send { channel_id: dm, content: "privado".into(), reply_to: None, nonce: 0, attachments: vec![] });
    let m = ana.wait("DM na ana", |m| if let S2C::Message(m) = m { Some(m.clone()) } else { None });
    assert_eq!(m.content, "privado");
    let _ = beto_id;

    // --- sessão: Resume funciona; Logout revoga o token
    let mut again = env.client();
    again.send(C2S::Resume { token: ana_token.clone(), proto: telacast_proto::PROTO_VERSION });
    again.wait("Auth do Resume", |m| matches!(m, S2C::Auth { .. }).then_some(()));
    again.send(C2S::Logout);
    std::thread::sleep(Duration::from_millis(300));
    let mut after = env.client();
    after.send(C2S::Resume { token: ana_token, proto: telacast_proto::PROTO_VERSION });
    assert!(after.error().contains("expirada"), "o token revogado não deve valer");
}

#[test]
fn limite_de_tentativas_de_login() {
    let env = Env::start();
    let mut last = String::new();
    for _ in 0..12 {
        let mut c = env.client();
        c.send(C2S::Login { username: "alguem".into(), password: "errada-errada".into(), proto: telacast_proto::PROTO_VERSION });
        last = c.error();
    }
    assert!(last.contains("Muitas tentativas"), "depois de várias tentativas deveria bloquear: {last}");
}

#[test]
fn responder_editar_apagar_e_ler() {
    let env = Env::start();
    let code = env.invite(2);
    let mut ana = env.client();
    let (_, ana_id) = ana.register("ana", &code);
    let mut beto = env.client();
    let (_, _) = beto.register("beto", &code);

    ana.send(C2S::CreateServer { name: "Casa".into() });
    let snap = ana.wait("snapshot com o servidor", |m| match m {
        S2C::Snapshot(s) if !s.servers.is_empty() => Some(s.clone()),
        _ => None,
    });
    let geral = snap.servers[0].channels.iter().find(|c| c.kind == 0).unwrap().id;
    beto.send(C2S::JoinServer { code: snap.servers[0].invite.clone() });
    beto.wait("ServerJoined", |m| matches!(m, S2C::ServerJoined { .. }).then_some(()));

    // --- resposta
    ana.send(C2S::Send { channel_id: geral, content: "primeira".into(), reply_to: None, nonce: 0, attachments: vec![] });
    let m1 = beto.wait("a primeira", |m| if let S2C::Message(m) = m { Some(m.clone()) } else { None });
    assert!(m1.reply.is_none() && !m1.edited);
    beto.send(C2S::Send { channel_id: geral, content: "resposta".into(), reply_to: Some(m1.id), nonce: 0, attachments: vec![] });
    let m2 = ana.wait("a resposta", |m| match m {
        S2C::Message(m) if m.content == "resposta" => Some(m.clone()),
        _ => None,
    });
    let r = m2.reply.expect("a resposta carrega a mensagem respondida");
    assert_eq!((r.id, r.author.as_str(), r.content.as_str()), (m1.id, "ana", "primeira"));
    beto.send(C2S::Send { channel_id: geral, content: "órfã".into(), reply_to: Some(1), nonce: 0, attachments: vec![] });
    assert!(beto.error().contains("respondida"));

    // --- edição: só o autor
    beto.send(C2S::Edit { message_id: m1.id, content: "x".into() });
    assert!(beto.error().contains("autor"));
    ana.send(C2S::Edit { message_id: m1.id, content: "primeira, corrigida".into() });
    let novo = beto.wait("a edição", |m| match m {
        S2C::MessageEdited { message_id, content, .. } if *message_id == m1.id => Some(content.clone()),
        _ => None,
    });
    assert_eq!(novo, "primeira, corrigida");
    beto.send(C2S::History { channel_id: geral, before: None });
    let hist = beto.wait("o histórico", |m| if let S2C::History { messages, .. } = m { Some(messages.clone()) } else { None });
    assert!(hist.iter().find(|m| m.id == m1.id).unwrap().edited, "o histórico marca a mensagem como editada");
    assert_eq!(hist.iter().find(|m| m.id == m2.id).unwrap().reply.as_ref().unwrap().id, m1.id);

    // --- apagar: o autor ou o dono do servidor
    ana.send(C2S::Send { channel_id: geral, content: "da ana".into(), reply_to: None, nonce: 0, attachments: vec![] });
    let da_ana = beto.wait("a mensagem da ana", |m| match m {
        S2C::Message(m) if m.content == "da ana" => Some(m.clone()),
        _ => None,
    });
    beto.send(C2S::Delete { message_id: da_ana.id });
    assert!(beto.error().contains("não pode"), "quem não é autor nem dono não apaga");
    ana.send(C2S::Delete { message_id: m2.id }); // a ana é dona do servidor e apaga a do beto
    let apagada = beto.wait("a exclusão", |m| match m {
        S2C::MessageDeleted { message_id, .. } => Some(*message_id),
        _ => None,
    });
    assert_eq!(apagada, m2.id);

    // --- "digitando"
    ana.send(C2S::Typing { channel_id: geral });
    let quem = beto.wait("o aviso de digitação", |m| if let S2C::Typing { user_id, .. } = m { Some(*user_id) } else { None });
    assert_eq!(quem, ana_id);

    // --- não lidos: o beto só leu quando mandar Ack
    let unread = |env: &Env| -> (i64, i64) {
        let mut c = env.client();
        c.send(C2S::Login { username: "beto".into(), password: "segredo-forte-1".into(), proto: telacast_proto::PROTO_VERSION });
        let s = c.wait("snapshot do login", |m| if let S2C::Snapshot(s) = m { Some(s.clone()) } else { None });
        let ch = s.servers[0].channels.iter().find(|c| c.id == geral).unwrap();
        (ch.last_message_id, ch.last_read_id)
    };
    let (ultima, lida) = unread(&env);
    assert!(ultima > lida, "o beto ainda não leu o canal");
    beto.send(C2S::Ack { channel_id: geral, message_id: ultima });
    std::thread::sleep(Duration::from_millis(300));
    let (ultima2, lida2) = unread(&env);
    assert_eq!(lida2, ultima2, "depois do Ack o canal está lido");
}

#[test]
fn versao_do_protocolo_e_codigos_de_erro() {
    use telacast_proto::ErrorCode;
    let env = Env::start();
    // versão errada (ou ausente: cliente antigo manda 0) é recusada com código estável
    let mut old = env.client();
    old.send(C2S::Login { username: "ana".into(), password: "segredo-forte-1".into(), proto: 0 });
    assert_eq!(old.error_code().0, ErrorCode::VersionMismatch);
    let mut newer = env.client();
    newer.send(C2S::Resume { token: "x".into(), proto: telacast_proto::PROTO_VERSION + 1 });
    assert_eq!(newer.error_code().0, ErrorCode::VersionMismatch);
    // códigos dos erros mais comuns
    let mut c = env.client();
    c.send(C2S::Login { username: "ninguem".into(), password: "errada-errada".into(), proto: telacast_proto::PROTO_VERSION });
    assert_eq!(c.error_code().0, ErrorCode::InvalidCredentials);
    let mut r = env.client();
    r.send(C2S::Register { username: "ana".into(), password: "segredo-forte-1".into(), invite: "NAOEXISTE".into(), proto: telacast_proto::PROTO_VERSION });
    assert_eq!(r.error_code().0, ErrorCode::InvalidInvite);
    let mut s = env.client();
    s.send(C2S::Resume { token: "sessao-que-nao-existe".into(), proto: telacast_proto::PROTO_VERSION });
    assert_eq!(s.error_code().0, ErrorCode::SessionExpired);
    let mut n = env.client();
    n.send(C2S::CreateServer { name: "sem login".into() });
    assert_eq!(n.error_code().0, ErrorCode::NotAuthenticated);
}

#[test]
fn nonce_nao_duplica_mensagem() {
    let env = Env::start();
    let code = env.invite(2);
    let mut ana = env.client();
    ana.register("ana", &code);
    let mut beto = env.client();
    beto.register("beto", &code);
    ana.send(C2S::CreateServer { name: "Casa".into() });
    let snap = ana.wait("snapshot com o servidor", |m| match m {
        S2C::Snapshot(s) if !s.servers.is_empty() => Some(s.clone()),
        _ => None,
    });
    let geral = snap.servers[0].channels.iter().find(|c| c.kind == 0).unwrap().id;
    beto.send(C2S::JoinServer { code: snap.servers[0].invite.clone() });
    beto.wait("ServerJoined", |m| matches!(m, S2C::ServerJoined { .. }).then_some(()));

    // o mesmo nonce duas vezes (reenvio depois de reconectar) vira UMA mensagem, que devolve o nonce
    ana.send(C2S::Send { channel_id: geral, content: "oi".into(), reply_to: None, nonce: 777, attachments: vec![] });
    ana.send(C2S::Send { channel_id: geral, content: "oi".into(), reply_to: None, nonce: 777, attachments: vec![] });
    ana.send(C2S::Send { channel_id: geral, content: "tchau".into(), reply_to: None, nonce: 778, attachments: vec![] });
    let primeira = beto.wait("a primeira", |m| if let S2C::Message(m) = m { Some(m.clone()) } else { None });
    assert_eq!((primeira.content.as_str(), primeira.nonce), ("oi", 777));
    let segunda = beto.wait("a segunda", |m| if let S2C::Message(m) = m { Some(m.clone()) } else { None });
    assert_eq!((segunda.content.as_str(), segunda.nonce), ("tchau", 778), "a repetição foi descartada");
    beto.send(C2S::History { channel_id: geral, before: None });
    let hist = beto.wait("o histórico", |m| if let S2C::History { messages, .. } = m { Some(messages.clone()) } else { None });
    assert_eq!(hist.iter().filter(|m| m.kind == 0).count(), 2);
}

#[test]
fn status_manual_e_ausente_automatico() {
    use telacast_proto::Status;
    let env = Env::start();
    let code = env.invite(2);
    let mut ana = env.client();
    let (_, ana_id) = ana.register("ana", &code);
    let mut beto = env.client();
    beto.register("beto", &code);
    ana.send(C2S::CreateServer { name: "Casa".into() });
    let snap = ana.wait("snapshot com o servidor", |m| match m {
        S2C::Snapshot(s) if !s.servers.is_empty() => Some(s.clone()),
        _ => None,
    });
    beto.send(C2S::JoinServer { code: snap.servers[0].invite.clone() });
    beto.wait("ServerJoined", |m| matches!(m, S2C::ServerJoined { .. }).then_some(()));

    // como o beto vê a ana
    let vista_do_beto = |beto: &mut Client, want: Status| {
        beto.wait("o status da ana", |m| match m {
            S2C::Snapshot(s) => s
                .servers
                .first()
                .and_then(|sv| sv.members.iter().find(|u| u.id == ana_id))
                .filter(|u| u.status == want)
                .map(|u| (u.status, u.online)),
            _ => None,
        })
    };
    ana.send(C2S::SetStatus { status: Status::Dnd });
    assert_eq!(vista_do_beto(&mut beto, Status::Dnd), (Status::Dnd, true), "não perturbe continua online");
    ana.send(C2S::SetStatus { status: Status::Invisible });
    assert_eq!(vista_do_beto(&mut beto, Status::Offline), (Status::Offline, false), "invisível aparece offline para os outros");
    // para ela mesma, continua invisível
    let propria = ana.wait("o próprio status", |m| match m {
        S2C::Snapshot(s) if s.me.status == Status::Invisible => Some(s.me.status),
        _ => None,
    });
    assert_eq!(propria, Status::Invisible);
    ana.send(C2S::SetStatus { status: Status::Online });
    assert_eq!(vista_do_beto(&mut beto, Status::Online).0, Status::Online);
    // ausente automático: a única conexão da ana fica parada
    ana.send(C2S::SetIdle { idle: true });
    assert_eq!(vista_do_beto(&mut beto, Status::Idle).0, Status::Idle);
    ana.send(C2S::SetIdle { idle: false });
    assert_eq!(vista_do_beto(&mut beto, Status::Online).0, Status::Online);
    // Offline não pode ser escolhido
    ana.send(C2S::SetStatus { status: Status::Offline });
    assert_eq!(ana.error_code().0, telacast_proto::ErrorCode::InvalidInput);
}

#[test]
fn mencoes_contam_ate_ler() {
    let env = Env::start();
    let code = env.invite(3);
    let mut ana = env.client();
    ana.register("ana", &code);
    let mut beto = env.client();
    let (_, beto_id) = beto.register("beto", &code);
    let mut carla = env.client();
    carla.register("carla", &code);
    ana.send(C2S::CreateServer { name: "Casa".into() });
    let snap = ana.wait("snapshot com o servidor", |m| match m {
        S2C::Snapshot(s) if !s.servers.is_empty() => Some(s.clone()),
        _ => None,
    });
    let geral = snap.servers[0].channels.iter().find(|c| c.kind == 0).unwrap().id;
    for c in [&mut beto, &mut carla] {
        c.send(C2S::JoinServer { code: snap.servers[0].invite.clone() });
        c.wait("ServerJoined", |m| matches!(m, S2C::ServerJoined { .. }).then_some(()));
    }
    let mencoes = |env: &Env, user: &str| -> u32 {
        let mut c = env.client();
        c.send(C2S::Login { username: user.into(), password: "segredo-forte-1".into(), proto: telacast_proto::PROTO_VERSION });
        let s = c.wait("snapshot do login", |m| if let S2C::Snapshot(s) = m { Some(s.clone()) } else { None });
        s.servers[0].channels.iter().find(|c| c.id == geral).unwrap().mentions
    };
    ana.send(C2S::Send { channel_id: geral, content: format!("oi <@{beto_id}>"), reply_to: None, nonce: 0, attachments: vec![] });
    ana.send(C2S::Send { channel_id: geral, content: "mensagem sem menção".into(), reply_to: None, nonce: 0, attachments: vec![] });
    ana.send(C2S::Send { channel_id: geral, content: format!("de novo <@!{beto_id}>"), reply_to: None, nonce: 0, attachments: vec![] });
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(mencoes(&env, "beto"), 2, "o beto foi citado duas vezes");
    assert_eq!(mencoes(&env, "carla"), 0, "a carla não foi citada");
    ana.send(C2S::Send { channel_id: geral, content: "@everyone atenção".into(), reply_to: None, nonce: 0, attachments: vec![] });
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(mencoes(&env, "carla"), 1, "@everyone conta para todos");
    assert_eq!(mencoes(&env, "beto"), 3);
    assert_eq!(mencoes(&env, "ana"), 0, "o autor não conta a própria menção");
    // ler zera
    beto.send(C2S::Ack { channel_id: geral, message_id: i64::MAX / 2 });
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(mencoes(&env, "beto"), 0);
}

#[test]
fn busca_de_mensagens() {
    let env = Env::start();
    let code = env.invite(2);
    let mut ana = env.client();
    let (_, ana_id) = ana.register("ana", &code);
    let mut beto = env.client();
    beto.register("beto", &code);
    ana.send(C2S::CreateServer { name: "Casa".into() });
    let snap = ana.wait("snapshot com o servidor", |m| match m {
        S2C::Snapshot(s) if !s.servers.is_empty() => Some(s.clone()),
        _ => None,
    });
    let server_id = snap.servers[0].id;
    let geral = snap.servers[0].channels.iter().find(|c| c.kind == 0).unwrap().id;
    beto.send(C2S::JoinServer { code: snap.servers[0].invite.clone() });
    beto.wait("ServerJoined", |m| matches!(m, S2C::ServerJoined { .. }).then_some(()));
    for (quem, t) in [
        (0, "vamos jogar futebol amanhã"),
        (1, "eu adoro jogar futebol"),
        (0, "e depois um cinema"),
        (1, "a transmissão do jogo começa às nove"),
    ] {
        let c = if quem == 0 { &mut ana } else { &mut beto };
        c.send(C2S::Send { channel_id: geral, content: t.into(), reply_to: None, nonce: 0, attachments: vec![] });
        std::thread::sleep(Duration::from_millis(80));
    }
    std::thread::sleep(Duration::from_millis(300));
    let buscar = |c: &mut Client, nonce: u64, text: &str, from: Option<i64>, before: Option<i64>| {
        c.send(C2S::Search { nonce, channel_id: None, server_id: Some(server_id), from, text: text.into(), before });
        c.wait("resultado da busca", |m| match m {
            S2C::SearchResults { nonce: n, hits, more } if *n == nonce => Some((hits.clone(), *more)),
            _ => None,
        })
    };
    // radical: "jogar" acha "jogo" e "jogar" (português)
    let (h, more) = buscar(&mut beto, 1, "jogar", None, None);
    assert!(!more);
    let textos: Vec<&str> = h.iter().map(|x| x.message.content.as_str()).collect();
    assert_eq!(textos, vec!["a transmissão do jogo começa às nove", "eu adoro jogar futebol", "vamos jogar futebol amanhã"], "mais novo primeiro");
    assert_eq!(h[0].channel_name, "geral");
    // só do autor
    let (h, _) = buscar(&mut beto, 2, "futebol", Some(ana_id), None);
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].message.author, "ana");
    // só o filtro de autor, sem texto
    let (h, _) = buscar(&mut beto, 3, "", Some(ana_id), None);
    assert_eq!(h.len(), 2);
    // paginação: antes do id mais novo
    let (h, _) = buscar(&mut beto, 4, "jogar", None, Some(h[0].message.id));
    assert!(h.iter().all(|x| x.message.content != "a transmissão do jogo começa às nove"));
    // sem resultados
    let (h, _) = buscar(&mut beto, 5, "xyzinexistente", None, None);
    assert!(h.is_empty());
    // quem não é do servidor não busca nele
    let mut fora = env.client();
    let code2 = env.invite(1);
    fora.register("carla", &code2);
    fora.send(C2S::Search { nonce: 9, channel_id: None, server_id: Some(server_id), from: None, text: "jogar".into(), before: None });
    assert_eq!(fora.error_code().0, telacast_proto::ErrorCode::Forbidden);
    // sem escopo
    fora.send(C2S::Search { nonce: 10, channel_id: None, server_id: None, from: None, text: "x".into(), before: None });
    assert_eq!(fora.error_code().0, telacast_proto::ErrorCode::InvalidInput);
}

#[test]
fn reacoes_com_emoji() {
    let env = Env::start();
    let code = env.invite(2);
    let mut ana = env.client();
    let (_, ana_id) = ana.register("ana", &code);
    let mut beto = env.client();
    let (_, beto_id) = beto.register("beto", &code);
    ana.send(C2S::CreateServer { name: "Casa".into() });
    let snap = ana.wait("snapshot com o servidor", |m| match m {
        S2C::Snapshot(s) if !s.servers.is_empty() => Some(s.clone()),
        _ => None,
    });
    let geral = snap.servers[0].channels.iter().find(|c| c.kind == 0).unwrap().id;
    beto.send(C2S::JoinServer { code: snap.servers[0].invite.clone() });
    beto.wait("ServerJoined", |m| matches!(m, S2C::ServerJoined { .. }).then_some(()));
    ana.send(C2S::Send { channel_id: geral, content: "reaja a isto".into(), reply_to: None, nonce: 0, attachments: vec![] });
    let msg = beto.wait("a mensagem", |m| if let S2C::Message(m) = m { Some(m.clone()) } else { None });

    // o beto reage; a ana recebe o evento
    beto.send(C2S::React { message_id: msg.id, emoji: "👍".into(), add: true });
    let ev = ana.wait("o evento de reação", |m| match m {
        S2C::Reaction { message_id, emoji, user_id, add, .. } => Some((*message_id, emoji.clone(), *user_id, *add)),
        _ => None,
    });
    assert_eq!(ev, (msg.id, "👍".to_string(), beto_id, true));
    // repetir não gera outro evento; a ana reage com o mesmo e com outro
    beto.send(C2S::React { message_id: msg.id, emoji: "👍".into(), add: true });
    ana.send(C2S::React { message_id: msg.id, emoji: "👍".into(), add: true });
    ana.send(C2S::React { message_id: msg.id, emoji: "🎉".into(), add: true });
    std::thread::sleep(Duration::from_millis(400));
    // o histórico agrega: 👍 x2 (e a ana está entre eles), 🎉 x1
    ana.send(C2S::History { channel_id: geral, before: None });
    let hist = ana.wait("o histórico", |m| if let S2C::History { messages, .. } = m { Some(messages.clone()) } else { None });
    let r = &hist.iter().find(|m| m.kind == 0).unwrap().reactions;
    assert_eq!(r.len(), 2, "dois emojis diferentes: {r:?}");
    assert_eq!((r[0].emoji.as_str(), r[0].count, r[0].me), ("👍", 2, true));
    assert_eq!((r[1].emoji.as_str(), r[1].count, r[1].me), ("🎉", 1, true));
    // para o beto, só o 👍 é dele
    beto.send(C2S::History { channel_id: geral, before: None });
    let h2 = beto.wait("o histórico do beto", |m| if let S2C::History { messages, .. } = m { Some(messages.clone()) } else { None });
    let r2 = &h2.iter().find(|m| m.kind == 0).unwrap().reactions;
    assert_eq!((r2[0].me, r2[1].me), (true, false));
    // tirar a reação
    ana.send(C2S::React { message_id: msg.id, emoji: "🎉".into(), add: false });
    let ev = beto.wait("a remoção", |m| match m {
        S2C::Reaction { user_id, add: false, emoji, .. } => Some((*user_id, emoji.clone())),
        _ => None,
    });
    assert_eq!(ev, (ana_id, "🎉".to_string()));
    // emoji inválido e mensagem de fora
    ana.send(C2S::React { message_id: msg.id, emoji: "a".into(), add: true });
    assert_eq!(ana.error_code().0, telacast_proto::ErrorCode::InvalidInput);
    let code2 = env.invite(1);
    let mut fora = env.client();
    fora.register("carla", &code2);
    fora.send(C2S::React { message_id: msg.id, emoji: "👍".into(), add: true });
    assert_eq!(fora.error_code().0, telacast_proto::ErrorCode::NotFound);
}

#[test]
fn fixadas_e_avisos_de_sistema() {
    let env = Env::start();
    let code = env.invite(2);
    let mut ana = env.client();
    let (_, ana_id) = ana.register("ana", &code);
    let mut beto = env.client();
    let (_, beto_id) = beto.register("beto", &code);
    ana.send(C2S::CreateServer { name: "Casa".into() });
    let snap = ana.wait("snapshot com o servidor", |m| match m {
        S2C::Snapshot(s) if !s.servers.is_empty() => Some(s.clone()),
        _ => None,
    });
    let geral = snap.servers[0].channels.iter().find(|c| c.kind == 0).unwrap().id;
    // quem entra gera um aviso de sistema (kind 2) no primeiro canal
    beto.send(C2S::JoinServer { code: snap.servers[0].invite.clone() });
    let aviso = ana.wait("o aviso de entrada", |m| match m {
        S2C::Message(m) if m.kind == 2 => Some(m.clone()),
        _ => None,
    });
    assert_eq!((aviso.author_id, aviso.channel_id), (beto_id, geral));

    ana.send(C2S::Send { channel_id: geral, content: "regras do canal".into(), reply_to: None, nonce: 0, attachments: vec![] });
    let msg = beto.wait("a mensagem", |m| match m {
        S2C::Message(m) if m.kind == 0 => Some(m.clone()),
        _ => None,
    });
    // o beto não é dono: não fixa
    beto.send(C2S::Pin { message_id: msg.id, pin: true });
    assert_eq!(beto.error_code().0, telacast_proto::ErrorCode::Forbidden);
    // a ana é dona: fixa; todos recebem o evento e o aviso "fixou" (kind 3, content = id da fixada)
    ana.send(C2S::Pin { message_id: msg.id, pin: true });
    let ev = beto.wait("o evento de fixar", |m| match m {
        S2C::Pinned { message_id, pinned, .. } => Some((*message_id, *pinned)),
        _ => None,
    });
    assert_eq!(ev, (msg.id, true));
    let fixou = beto.wait("o aviso de fixar", |m| match m {
        S2C::Message(m) if m.kind == 3 => Some(m.clone()),
        _ => None,
    });
    assert_eq!((fixou.author_id, fixou.content.as_str()), (ana_id, msg.id.to_string().as_str()));
    // fixar de novo não repete
    ana.send(C2S::Pin { message_id: msg.id, pin: true });
    // lista de fixadas e marca no histórico
    beto.send(C2S::Pins { channel_id: geral });
    let pins = beto.wait("as fixadas", |m| if let S2C::Pins { messages, .. } = m { Some(messages.clone()) } else { None });
    assert_eq!(pins.len(), 1);
    assert_eq!(pins[0].id, msg.id);
    beto.send(C2S::History { channel_id: geral, before: None });
    let hist = beto.wait("o histórico", |m| if let S2C::History { messages, .. } = m { Some(messages.clone()) } else { None });
    assert!(hist.iter().find(|m| m.id == msg.id).unwrap().pinned);
    assert!(hist.iter().any(|m| m.kind == 2) && hist.iter().filter(|m| m.kind == 3).count() == 1, "um aviso de entrada e um só de fixar");
    // desafixar
    ana.send(C2S::Pin { message_id: msg.id, pin: false });
    let ev = beto.wait("o evento de desafixar", |m| match m {
        S2C::Pinned { pinned: false, message_id, .. } => Some(*message_id),
        _ => None,
    });
    assert_eq!(ev, msg.id);
    beto.send(C2S::Pins { channel_id: geral });
    let pins = beto.wait("as fixadas", |m| if let S2C::Pins { messages, .. } = m { Some(messages.clone()) } else { None });
    assert!(pins.is_empty());
    // aviso de sistema não se fixa nem se reage
    ana.send(C2S::Pin { message_id: aviso.id, pin: true });
    std::thread::sleep(Duration::from_millis(300));
    beto.send(C2S::Pins { channel_id: geral });
    let pins = beto.wait("as fixadas", |m| if let S2C::Pins { messages, .. } = m { Some(messages.clone()) } else { None });
    assert!(pins.is_empty(), "mensagem de sistema não fixa");
}

#[test]
fn bloquear_usuario() {
    let env = Env::start();
    let code = env.invite(2);
    let mut ana = env.client();
    let (_, ana_id) = ana.register("ana", &code);
    let mut beto = env.client();
    let (_, beto_id) = beto.register("beto", &code);
    ana.send(C2S::CreateServer { name: "Casa".into() });
    let snap = ana.wait("snapshot com o servidor", |m| match m {
        S2C::Snapshot(s) if !s.servers.is_empty() => Some(s.clone()),
        _ => None,
    });
    beto.send(C2S::JoinServer { code: snap.servers[0].invite.clone() });
    beto.wait("ServerJoined", |m| matches!(m, S2C::ServerJoined { .. }).then_some(()));
    // viram amigos e abrem uma DM
    ana.send(C2S::FriendRequest { username: "beto".into() });
    beto.wait("o pedido", |m| match m {
        S2C::Snapshot(s) if !s.friends.is_empty() => Some(()),
        _ => None,
    });
    beto.send(C2S::FriendAccept { user_id: ana_id });
    ana.send(C2S::OpenDm { user_id: beto_id });
    let dm = ana.wait("DmOpened", |m| if let S2C::DmOpened { channel_id } = m { Some(*channel_id) } else { None });
    ana.send(C2S::Send { channel_id: dm, content: "oi".into(), reply_to: None, nonce: 0, attachments: vec![] });
    beto.wait("a DM", |m| matches!(m, S2C::Message(m) if m.content == "oi").then_some(()));

    // a ana bloqueia o beto: a amizade some e ele aparece na lista de bloqueados dela
    ana.send(C2S::Block { user_id: beto_id });
    let s = ana.wait("snapshot com o bloqueio", |m| match m {
        S2C::Snapshot(s) if !s.blocked.is_empty() => Some(s.clone()),
        _ => None,
    });
    assert_eq!(s.blocked[0].id, beto_id);
    assert!(s.friends.is_empty(), "a amizade é desfeita");
    // o beto não manda mais mensagem na DM existente, não abre DM nova e não pede amizade
    beto.send(C2S::Send { channel_id: dm, content: "ainda aí?".into(), reply_to: None, nonce: 0, attachments: vec![] });
    assert_eq!(beto.error_code().0, telacast_proto::ErrorCode::Forbidden);
    beto.send(C2S::OpenDm { user_id: ana_id });
    assert_eq!(beto.error_code().0, telacast_proto::ErrorCode::Forbidden);
    beto.send(C2S::FriendRequest { username: "ana".into() });
    assert_eq!(beto.error_code().0, telacast_proto::ErrorCode::NotFound, "quem foi bloqueado não descobre o bloqueio");
    // a ana também não manda nem pede amizade enquanto bloqueia
    ana.send(C2S::Send { channel_id: dm, content: "e agora?".into(), reply_to: None, nonce: 0, attachments: vec![] });
    assert_eq!(ana.error_code().0, telacast_proto::ErrorCode::Forbidden);
    ana.send(C2S::FriendRequest { username: "beto".into() });
    assert_eq!(ana.error_code().0, telacast_proto::ErrorCode::InvalidInput);
    ana.send(C2S::Block { user_id: ana_id });
    assert_eq!(ana.error_code().0, telacast_proto::ErrorCode::InvalidInput, "não bloqueia a si mesmo");
    // desbloquear libera a conversa
    ana.send(C2S::Unblock { user_id: beto_id });
    ana.wait("snapshot sem bloqueios", |m| match m {
        S2C::Snapshot(s) if s.blocked.is_empty() => Some(()),
        _ => None,
    });
    beto.send(C2S::Send { channel_id: dm, content: "voltei".into(), reply_to: None, nonce: 0, attachments: vec![] });
    ana.wait("a DM do beto", |m| matches!(m, S2C::Message(m) if m.content == "voltei").then_some(()));
}

#[test]
fn limite_de_conexoes_por_ip() {
    let env = Env::start_with(&[("TELACAST_MAX_CONNS_PER_IP", "3")]);
    let url = format!("ws://127.0.0.1:{}/ws", env.port);
    let a = tungstenite::connect(&url).expect("1ª conexão");
    let b = tungstenite::connect(&url).expect("2ª conexão");
    let c = tungstenite::connect(&url).expect("3ª conexão");
    assert!(tungstenite::connect(&url).is_err(), "a 4ª conexão do mesmo IP é recusada");
    // fechar uma libera a vaga
    drop(a);
    std::thread::sleep(Duration::from_millis(500));
    let d = tungstenite::connect(&url);
    assert!(d.is_ok(), "com uma vaga livre a conexão passa de novo");
    drop((b, c, d));
}

/// Requisição HTTP mínima (sem dependência): devolve (status, cabeçalhos em minúsculas, corpo).
fn http(port: u16, method: &str, path: &str, body: &[u8]) -> (u16, String, Vec<u8>) {
    use std::io::{Read, Write};
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(s, "{method} {path} HTTP/1.1\r\nHost: x\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len()).unwrap();
    s.write_all(body).unwrap();
    let mut raw = Vec::new();
    let _ = s.read_to_end(&mut raw);
    let cut = raw.windows(4).position(|w| w == b"\r\n\r\n").expect("resposta HTTP");
    let head = String::from_utf8_lossy(&raw[..cut]).to_lowercase();
    let status: u16 = head.split_whitespace().nth(1).unwrap().parse().unwrap();
    (status, head, raw[cut + 4..].to_vec())
}

/// PNG de 1x1 pixel.
const PNG_1X1: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00,
    0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4, 0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63,
    0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0D, 0x0A, 0x2D, 0xB4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

#[test]
fn anexos() {
    let env = Env::start_with(&[("TELACAST_MAX_UPLOAD_MB", "1")]);
    let code = env.invite(2);
    let mut ana = env.client();
    let (ana_tok, _) = ana.register("ana", &code);
    let mut beto = env.client();
    let (beto_tok, _) = beto.register("beto", &code);
    ana.send(C2S::CreateServer { name: "Casa".into() });
    let snap = ana.wait("snapshot com o servidor", |m| match m {
        S2C::Snapshot(s) if !s.servers.is_empty() => Some(s.clone()),
        _ => None,
    });
    let geral = snap.servers[0].channels.iter().find(|c| c.kind == 0).unwrap().id;
    let code2 = env.invite(1);
    let mut fora = env.client();
    let (fora_tok, _) = fora.register("carla", &code2);
    beto.send(C2S::JoinServer { code: snap.servers[0].invite.clone() });
    beto.wait("ServerJoined", |m| matches!(m, S2C::ServerJoined { .. }).then_some(()));

    let up = |tok: &str, ch: i64, name: &str, body: &[u8]| http(env.port, "POST", &format!("/upload?token={tok}&channel={ch}&name={name}"), body);
    // sem token, de fora do canal e grande demais
    assert_eq!(up("invalido", geral, "a.png", PNG_1X1).0, 401);
    assert_eq!(up(&fora_tok, geral, "a.png", PNG_1X1).0, 403, "quem não é do servidor não envia");
    assert_eq!(up(&ana_tok, geral, "grande.bin", &vec![7u8; (1 << 20) + 10]).0, 413, "acima de 1 MiB neste teste");
    assert_eq!(up(&ana_tok, geral, "vazio.bin", &[]).0, 400);
    // envio normal: o tipo e as dimensões vêm do conteúdo (o nome "../foto.txt" é limpo)
    let (st, _, body) = up(&ana_tok, geral, "..%2Ffoto.png", PNG_1X1);
    assert_eq!(st, 200, "upload do PNG");
    let j: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let (id, mime, w) = (j["id"].as_i64().unwrap(), j["mime"].as_str().unwrap().to_string(), j["width"].as_u64().unwrap());
    assert_eq!((mime.as_str(), w, j["height"].as_u64().unwrap()), ("image/png", 1, 1));
    assert_eq!(j["name"].as_str().unwrap(), "foto.png", "sem pastas no nome");
    // um arquivo qualquer não é imagem, mesmo que diga ser
    let (_, _, body2) = up(&ana_tok, geral, "nota.png", b"isto e texto puro");
    let j2: serde_json::Value = serde_json::from_slice(&body2).unwrap();
    assert_eq!(j2["mime"].as_str().unwrap(), "application/octet-stream");
    let id2 = j2["id"].as_i64().unwrap();

    // mensagem só com anexos (texto vazio); o beto recebe com os metadados
    ana.send(C2S::Send { channel_id: geral, content: "".into(), reply_to: None, nonce: 0, attachments: vec![id, id2] });
    let msg = beto.wait("a mensagem com anexos", |m| match m {
        S2C::Message(m) if m.kind == 0 => Some(m.clone()),
        _ => None,
    });
    assert_eq!(msg.attachments.len(), 2);
    assert_eq!((msg.attachments[0].width, msg.attachments[0].height, msg.attachments[0].mime.as_str()), (1, 1, "image/png"));
    // o histórico também traz
    beto.send(C2S::History { channel_id: geral, before: None });
    let hist = beto.wait("o histórico", |m| if let S2C::History { messages, .. } = m { Some(messages.clone()) } else { None });
    assert_eq!(hist.iter().find(|m| m.kind == 0).unwrap().attachments.len(), 2);
    // anexo já usado ou de outra pessoa é recusado
    ana.send(C2S::Send { channel_id: geral, content: "de novo".into(), reply_to: None, nonce: 0, attachments: vec![id] });
    assert_eq!(ana.error_code().0, telacast_proto::ErrorCode::InvalidInput);
    // download: imagem inline, outro tipo como anexo; sem token ou de fora, recusado
    let (st, head, bytes) = http(env.port, "GET", &format!("/files/{id}/foto.png?token={beto_tok}"), &[]);
    assert_eq!(st, 200);
    assert_eq!(bytes, PNG_1X1);
    assert!(head.contains("content-type: image/png") && head.contains("inline") && head.contains("nosniff"), "{head}");
    let (st, head, _) = http(env.port, "GET", &format!("/files/{id2}/nota.png?token={beto_tok}"), &[]);
    assert_eq!(st, 200);
    assert!(head.contains("application/octet-stream") && head.contains("attachment"), "{head}");
    assert_eq!(http(env.port, "GET", &format!("/files/{id}/x?token="), &[]).0, 401);
    assert_eq!(http(env.port, "GET", &format!("/files/{id}/x?token={fora_tok}"), &[]).0, 403);
    assert_eq!(http(env.port, "GET", &format!("/files/999/x?token={beto_tok}"), &[]).0, 404);
    // mensagem vazia e sem anexos é ignorada
    ana.send(C2S::Send { channel_id: geral, content: "  ".into(), reply_to: None, nonce: 0, attachments: vec![] });
    ana.send(C2S::Send { channel_id: geral, content: "fim".into(), reply_to: None, nonce: 0, attachments: vec![] });
    let m = beto.wait("a próxima mensagem", |m| match m {
        S2C::Message(m) if m.kind == 0 => Some(m.content.clone()),
        _ => None,
    });
    assert_eq!(m, "fim");
}
