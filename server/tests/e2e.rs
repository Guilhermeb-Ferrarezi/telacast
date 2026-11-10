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
            // vazio = push desligado, mesmo que um .env acima do repositório defina a credencial
            .env("TELACAST_FCM_SERVICE_ACCOUNT", "")
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
        Client { ws, bins: Default::default() }
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
    /// Quadros binários (áudio) recebidos e ainda não consumidos.
    bins: std::collections::VecDeque<Vec<u8>>,
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
                Ok(Message::Binary(b)) => self.bins.push_back(b.to_vec()),
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                Err(e) => panic!("conexão caiu esperando {what}: {e}"),
            }
        }
    }

    fn send_bin(&mut self, data: Vec<u8>) {
        self.ws.send(Message::Binary(data.into())).unwrap();
    }

    /// Lê por `dur` e devolve os eventos de texto vistos; os quadros binários ficam na fila.
    fn collect(&mut self, dur: Duration) -> Vec<S2C> {
        let start = Instant::now();
        let mut out = Vec::new();
        while start.elapsed() < dur {
            match self.ws.read() {
                Ok(Message::Text(t)) => out.push(serde_json::from_str::<S2C>(&t).unwrap()),
                Ok(Message::Binary(b)) => self.bins.push_back(b.to_vec()),
                Ok(_) => {}
                Err(tungstenite::Error::Io(e))
                    if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                Err(e) => panic!("conexão caiu durante a coleta: {e}"),
            }
        }
        out
    }

    /// Próximo quadro binário (espera até 10 s).
    fn recv_bin(&mut self) -> Vec<u8> {
        let start = Instant::now();
        loop {
            if let Some(b) = self.bins.pop_front() {
                return b;
            }
            assert!(start.elapsed() < Duration::from_secs(10), "tempo esgotado esperando um quadro de áudio");
            self.collect(Duration::from_millis(100));
        }
    }

    /// Todos os quadros binários que chegarem em `dur`.
    fn bins_in(&mut self, dur: Duration) -> Vec<Vec<u8>> {
        self.collect(dur);
        self.bins.drain(..).collect()
    }

    /// Volta a uma sessão já existente (outra conexão da mesma pessoa).
    fn resume(&mut self, token: &str) {
        self.send(C2S::Resume { token: token.into(), proto: telacast_proto::PROTO_VERSION });
        self.wait("Auth do Resume", |m| matches!(m, S2C::Auth { .. }).then_some(()));
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

/// Foto de mentira com cara de foto (gradiente e formas, sem ruído), em PNG, para as miniaturas.
fn png_grande(w: u32, h: u32) -> Vec<u8> {
    let img = image::ImageBuffer::from_fn(w, h, |x, y| {
        let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
        let sol = (-(((fx - 0.7).powi(2) + (fy - 0.3).powi(2)) * 40.0)).exp();
        image::Rgb([(60.0 + 150.0 * fy + 150.0 * sol).min(255.0) as u8, (90.0 + 90.0 * fy + 120.0 * sol).min(255.0) as u8, (200.0 - 100.0 * fy) as u8])
    });
    let mut v = Vec::new();
    image::DynamicImage::ImageRgb8(img).write_to(&mut std::io::Cursor::new(&mut v), image::ImageFormat::Png).unwrap();
    v
}

#[test]
fn miniaturas_de_imagem() {
    let env = Env::start();
    let code = env.invite(1);
    let mut ana = env.client();
    let (ana_tok, _) = ana.register("ana", &code);
    ana.send(C2S::CreateServer { name: "Casa".into() });
    let snap = ana.wait("snapshot com o servidor", |m| match m {
        S2C::Snapshot(s) if !s.servers.is_empty() => Some(s.clone()),
        _ => None,
    });
    let geral = snap.servers[0].channels.iter().find(|c| c.kind == 0).unwrap().id;
    let code2 = env.invite(1);
    let mut fora = env.client();
    let (fora_tok, _) = fora.register("carla", &code2);

    let up = |name: &str, body: &[u8]| -> (i64, String) {
        let (st, _, b) = http(env.port, "POST", &format!("/upload?token={ana_tok}&channel={geral}&name={name}"), body);
        assert_eq!(st, 200, "upload de {name}");
        let j: serde_json::Value = serde_json::from_slice(&b).unwrap();
        (j["id"].as_i64().unwrap(), j["mime"].as_str().unwrap().to_string())
    };
    let original = png_grande(3000, 2000);
    assert!(original.len() < 10 << 20);
    let (id, mime) = up("foto.png", &original);
    assert_eq!(mime, "image/png");
    let thumbs_dir = std::path::Path::new(&env.files).join("thumbs");
    assert!(!thumbs_dir.exists(), "nada é gerado no upload");

    // sem token: 401; quem não é do canal: 403; sem ?thumb=1 continua o original
    assert_eq!(http(env.port, "GET", &format!("/files/{id}/foto.png?thumb=1"), &[]).0, 401);
    assert_eq!(http(env.port, "GET", &format!("/files/{id}/foto.png?token=invalido&thumb=1"), &[]).0, 401);
    assert_eq!(http(env.port, "GET", &format!("/files/{id}/foto.png?token={fora_tok}&thumb=1"), &[]).0, 403);
    assert!(!thumbs_dir.exists(), "pedido recusado não gera miniatura");
    let (st, head, bytes) = http(env.port, "GET", &format!("/files/{id}/foto.png?token={ana_tok}"), &[]);
    assert_eq!((st, bytes.as_slice()), (200, original.as_slice()));
    assert!(!head.contains("x-telacast-thumb"), "{head}");

    // miniatura: menor, JPEG, lado maior 1024, com o cabeçalho; vários pedidos ao mesmo tempo geram uma vez só
    let url = format!("/files/{id}/foto.png?token={ana_tok}&thumb=1");
    let port = env.port;
    let rs: Vec<_> = (0..4).map(|_| { let u = url.clone(); std::thread::spawn(move || http(port, "GET", &u, &[])) }).collect();
    let rs: Vec<_> = rs.into_iter().map(|t| t.join().unwrap()).collect();
    for (st, head, bytes) in &rs {
        assert_eq!(*st, 200);
        assert!(head.contains("content-type: image/jpeg"), "{head}");
        assert!(head.contains("x-telacast-thumb: 1"), "{head}");
        assert!(head.contains("cache-control: private, max-age=31536000, immutable"), "{head}");
        assert!(head.contains("inline"), "{head}");
        let s = imagesize::blob_size(bytes).unwrap();
        assert_eq!((s.width, s.height), (1024, 683));
        assert!(bytes.len() < original.len() / 2, "miniatura {} B, original {} B", bytes.len(), original.len());
        assert_eq!(bytes, &rs[0].2, "todas as respostas são o mesmo arquivo");
    }
    let files: Vec<_> = std::fs::read_dir(&thumbs_dir).unwrap().map(|e| e.unwrap()).collect();
    assert_eq!(files.len(), 1, "um arquivo só em thumbs/ (sem .tmp sobrando): {files:?}");
    assert!(files[0].file_name().to_string_lossy().ends_with(".jpg"));
    let (mtime, ino) = {
        use std::os::unix::fs::MetadataExt;
        let m = files[0].metadata().unwrap();
        (m.modified().unwrap(), m.ino())
    };

    // segunda chamada vem do cache: o mesmo arquivo em disco, sem regravar
    std::thread::sleep(Duration::from_millis(50));
    let (st, head, again) = http(env.port, "GET", &url, &[]);
    assert_eq!(st, 200);
    assert!(head.contains("x-telacast-thumb: 1"));
    assert_eq!(again, rs[0].2);
    {
        use std::os::unix::fs::MetadataExt;
        let m = std::fs::metadata(files[0].path()).unwrap();
        assert_eq!((m.modified().unwrap(), m.ino()), (mtime, ino), "o cache não foi regravado");
    }
    assert_eq!(std::fs::read_dir(&thumbs_dir).unwrap().count(), 1);

    // cai para o original: imagem pequena, lixo que diz ser PNG (tipo vem do conteúdo, então vira octet-stream) e GIF
    let pequena = png_grande(300, 200);
    let (id_p, _) = up("pequena.png", &pequena);
    let (st, head, bytes) = http(env.port, "GET", &format!("/files/{id_p}/x?token={ana_tok}&thumb=1"), &[]);
    assert_eq!((st, bytes.as_slice()), (200, pequena.as_slice()));
    assert!(!head.contains("x-telacast-thumb"), "{head}");
    let gif: &[u8] = b"GIF89a\x01\x00\x01\x00\x00\x00\x00;";
    let (id_g, mime_g) = up("a.gif", gif);
    assert_eq!(mime_g, "image/gif");
    let (st, head, bytes) = http(env.port, "GET", &format!("/files/{id_g}/x?token={ana_tok}&thumb=1"), &[]);
    assert_eq!((st, bytes.as_slice()), (200, gif));
    assert!(!head.contains("x-telacast-thumb"), "{head}");
    // PNG cortado: o tipo é PNG mas não decodifica; devolve o original e lembra disso
    let cortado = &original[..original.len() / 2];
    let (id_c, mime_c) = up("quebrada.png", cortado);
    assert_eq!(mime_c, "image/png");
    for _ in 0..2 {
        let (st, head, bytes) = http(env.port, "GET", &format!("/files/{id_c}/x?token={ana_tok}&thumb=1"), &[]);
        assert_eq!((st, bytes.as_slice()), (200, cortado));
        assert!(!head.contains("x-telacast-thumb"), "{head}");
    }
    assert!(std::fs::read_dir(&thumbs_dir).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().ends_with(".none")), "marcador do que não tem miniatura");
    assert_eq!(http(env.port, "GET", &format!("/files/999/x?token={ana_tok}&thumb=1"), &[]).0, 404);
}

#[test]
fn trocar_senha_e_encerrar_sessoes() {
    use telacast_proto::ErrorCode;
    let env = Env::start();
    let code = env.invite(2);
    let mut a1 = env.client();
    let (_, ana_id) = a1.register("ana", &code);
    let mut beto = env.client();
    beto.register("beto", &code);

    let login = |c: &mut Client, pw: &str| {
        c.send(C2S::Login { username: "ana".into(), password: pw.into(), proto: telacast_proto::PROTO_VERSION });
    };
    // segunda sessão da ana (outro aparelho)
    let mut a2 = env.client();
    login(&mut a2, "segredo-forte-1");
    let tok2 = a2.wait("Auth da 2ª sessão", |m| if let S2C::Auth { token, .. } = m { Some(token.clone()) } else { None });

    // a lista mostra as duas, e só uma é a atual
    a1.send(C2S::ListSessions);
    let list = a1.wait("lista de sessões", |m| if let S2C::Sessions { list } = m { Some(list.clone()) } else { None });
    assert_eq!(list.len(), 2);
    assert_eq!(list.iter().filter(|s| s.current).count(), 1);
    assert!(list.iter().all(|s| s.device == "127.0.0.1" && s.created > 0 && s.last_used >= s.created));

    // senha atual errada e senha nova curta são recusadas
    a1.send(C2S::ChangePassword { old: "errada-errada".into(), new: "outra-senha-2".into() });
    assert_eq!(a1.error_code().0, ErrorCode::WrongPassword);
    a1.send(C2S::ChangePassword { old: "segredo-forte-1".into(), new: "curta".into() });
    assert_eq!(a1.error_code().0, ErrorCode::InvalidInput);

    // a troca vale: a sessão atual fica, a outra é avisada e some
    a1.send(C2S::ChangePassword { old: "segredo-forte-1".into(), new: "outra-senha-2".into() });
    a1.wait("PasswordChanged", |m| matches!(m, S2C::PasswordChanged).then_some(()));
    let list = a1.wait("lista após a troca", |m| if let S2C::Sessions { list } = m { Some(list.clone()) } else { None });
    assert_eq!(list.len(), 1);
    assert!(list[0].current);
    assert_eq!(a2.error_code().0, ErrorCode::SessionExpired);
    let mut a3 = env.client();
    a3.send(C2S::Resume { token: tok2, proto: telacast_proto::PROTO_VERSION });
    assert_eq!(a3.error_code().0, ErrorCode::SessionExpired);

    // senha antiga não entra mais; a nova entra
    let mut a4 = env.client();
    login(&mut a4, "segredo-forte-1");
    assert_eq!(a4.error_code().0, ErrorCode::InvalidCredentials);
    login(&mut a4, "outra-senha-2");
    a4.wait("Auth com a senha nova", |m| matches!(m, S2C::Auth { .. }).then_some(()));

    // encerrar uma sessão específica; não dá para encerrar a de outra pessoa
    a1.send(C2S::ListSessions);
    let list = a1.wait("lista com 2", |m| match m {
        S2C::Sessions { list } if list.len() == 2 => Some(list.clone()),
        _ => None,
    });
    let outra = list.iter().find(|s| !s.current).unwrap().id;
    beto.send(C2S::RevokeSession { id: outra });
    assert_eq!(beto.error_code().0, ErrorCode::NotFound);
    a1.send(C2S::RevokeSession { id: outra });
    let list = a1.wait("lista sem a outra", |m| match m {
        S2C::Sessions { list } if list.len() == 1 => Some(list.clone()),
        _ => None,
    });
    assert!(list[0].current);
    assert_eq!(a4.error_code().0, ErrorCode::SessionExpired);

    // encerrar todas as outras
    let mut a5 = env.client();
    login(&mut a5, "outra-senha-2");
    a5.wait("Auth", |m| matches!(m, S2C::Auth { .. }).then_some(()));
    a1.send(C2S::RevokeOtherSessions);
    a1.wait("só a atual", |m| match m {
        S2C::Sessions { list } if list.len() == 1 => Some(()),
        _ => None,
    });
    assert_eq!(a5.error_code().0, ErrorCode::SessionExpired);
    let _ = ana_id;
}

#[test]
fn entrar_duas_vezes_no_mesmo_servidor() {
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
    let invite = snap.servers[0].invite.clone();
    let channel = snap.servers[0].channels.iter().find(|c| c.kind == 0).unwrap().id;

    beto.send(C2S::JoinServer { code: invite.clone() });
    beto.wait("ServerJoined", |m| matches!(m, S2C::ServerJoined { .. }).then_some(()));
    // a 2ª vez só reabre o servidor: sem novo aviso "entrou" e sem duplicar o membro
    beto.send(C2S::JoinServer { code: invite.clone() });
    beto.wait("ServerJoined de novo", |m| matches!(m, S2C::ServerJoined { .. }).then_some(()));
    // o dono também usa o próprio convite sem efeito colateral
    ana.send(C2S::JoinServer { code: invite });
    ana.wait("ServerJoined do dono", |m| matches!(m, S2C::ServerJoined { .. }).then_some(()));

    ana.send(C2S::Send { channel_id: channel, content: "marcador".into(), reply_to: None, nonce: 0, attachments: vec![] });
    // o que chega depois da 2ª entrada tem de ser o marcador (nenhum "beto entrou" repetido no meio)
    let mut entrou = 0;
    beto.wait("o marcador", |m| match m {
        S2C::Message(msg) if msg.kind == 2 => {
            entrou += 1;
            None
        }
        S2C::Message(msg) if msg.content == "marcador" => Some(()),
        _ => None,
    });
    assert!(entrou <= 1, "o aviso de entrada apareceu {entrou} vezes");
}

// ------------------------------------------------------------------ voz

use telacast_proto::{ErrorCode, VoiceState, VOICE_FRAME, VOICE_MAX_PACKET};

/// Servidor "Casa" do dono com o canal de texto padrão e dois canais de voz.
struct Casa {
    server_id: i64,
    categoria: i64,
    geral: i64,
    sala: i64,
    sala2: i64,
}

/// O dono cria o servidor e os canais de voz; `membros` entram pelo convite.
fn montar_casa(dono: &mut Client, membros: &mut [&mut Client]) -> Casa {
    dono.send(C2S::CreateServer { name: "Casa".into() });
    let snap = dono.wait("snapshot com o servidor", |m| match m {
        S2C::Snapshot(s) if !s.servers.is_empty() => Some(s.clone()),
        _ => None,
    });
    let server_id = snap.servers[0].id;
    let geral = snap.servers[0].channels.iter().find(|c| c.kind == 0).unwrap().id;
    let categoria = snap.servers[0].channels.iter().find(|c| c.kind == 4).unwrap().id;
    for nome in ["sala", "sala 2"] {
        dono.send(C2S::CreateChannel { server_id, name: nome.into(), kind: 1, parent_id: None });
    }
    let snap = dono.wait("snapshot com os canais de voz", |m| match m {
        S2C::Snapshot(s) if s.servers[0].channels.iter().filter(|c| c.kind == 1).count() == 2 => Some(s.clone()),
        _ => None,
    });
    let voz = |nome: &str| snap.servers[0].channels.iter().find(|c| c.kind == 1 && c.name == nome).unwrap().id;
    let (sala, sala2) = (voz("sala"), voz("sala-2"));
    for m in membros.iter_mut() {
        m.send(C2S::JoinServer { code: snap.servers[0].invite.clone() });
        m.wait("ServerJoined", |e| matches!(e, S2C::ServerJoined { .. }).then_some(()));
    }
    Casa { server_id, categoria, geral, sala, sala2 }
}

fn estado(user_id: i64, channel_id: i64, muted: bool, deafened: bool) -> VoiceState {
    VoiceState { user_id, channel_id, muted, deafened }
}

fn voice_update(c: &mut Client, user: i64) -> VoiceState {
    c.wait("VoiceUpdate", |m| match m {
        S2C::VoiceUpdate(v) if v.user_id == user => Some(v.clone()),
        _ => None,
    })
}

fn voice_left(c: &mut Client, user: i64) -> i64 {
    c.wait("VoiceLeft", |m| match m {
        S2C::VoiceLeft { user_id, channel_id } if *user_id == user => Some(*channel_id),
        _ => None,
    })
}

fn nenhum_evento_de_voz(eventos: &[S2C]) -> bool {
    !eventos.iter().any(|e| matches!(e, S2C::VoiceUpdate(_) | S2C::VoiceLeft { .. }))
}

#[test]
fn voz_entrar_mover_sair_e_cair() {
    let env = Env::start();
    let code = env.invite(3);
    let mut ana = env.client();
    let (_, ana_id) = ana.register("ana", &code);
    let mut beto = env.client();
    let (beto_tok, beto_id) = beto.register("beto", &code);
    let mut fora = env.client();
    fora.register("fora", &code);
    let casa = montar_casa(&mut ana, &mut [&mut beto]);
    // quem está fora do servidor não vê nada: precisa de um servidor próprio para ser "online" em outro lugar
    fora.send(C2S::CreateServer { name: "Outra".into() });
    fora.wait("snapshot próprio", |m| matches!(m, S2C::Snapshot(s) if !s.servers.is_empty()).then_some(()));

    // recusas: canal de texto, canal inexistente e não-membro
    beto.send(C2S::VoiceJoin { channel_id: casa.geral });
    assert_eq!(beto.error_code().0, ErrorCode::InvalidInput);
    beto.send(C2S::VoiceJoin { channel_id: 12345 });
    assert_eq!(beto.error_code().0, ErrorCode::NotFound);
    fora.send(C2S::VoiceJoin { channel_id: casa.sala });
    assert_eq!(fora.error_code().0, ErrorCode::Forbidden);
    let _ = ana.collect(Duration::from_millis(200));

    // entrar: o próprio beto e a ana (que está no servidor, mas fora do canal) recebem; quem é de fora, não
    beto.send(C2S::VoiceJoin { channel_id: casa.sala });
    assert_eq!(voice_update(&mut beto, beto_id), estado(beto_id, casa.sala, false, false));
    assert_eq!(voice_update(&mut ana, beto_id), estado(beto_id, casa.sala, false, false));
    assert!(nenhum_evento_de_voz(&fora.collect(Duration::from_millis(300))), "quem não é do servidor não pode ver a voz");

    // o snapshot de uma conexão nova já traz quem está na voz
    let mut ana2 = env.client();
    ana2.send(C2S::Login { username: "ana".into(), password: "segredo-forte-1".into(), proto: telacast_proto::PROTO_VERSION });
    let snap = ana2.wait("snapshot com voz", |m| match m {
        S2C::Snapshot(s) => Some(s.clone()),
        _ => None,
    });
    assert_eq!(snap.voice, vec![estado(beto_id, casa.sala, false, false)]);

    // surdo implica mudo
    beto.send(C2S::VoiceSet { muted: false, deafened: true });
    assert_eq!(voice_update(&mut ana, beto_id), estado(beto_id, casa.sala, true, true));
    // mover de canal: avisa a saída do antigo e a entrada no novo, mantendo mudo/surdo
    beto.send(C2S::VoiceJoin { channel_id: casa.sala2 });
    assert_eq!(voice_left(&mut ana, beto_id), casa.sala);
    assert_eq!(voice_update(&mut ana, beto_id), estado(beto_id, casa.sala2, true, true));
    beto.send(C2S::VoiceSet { muted: false, deafened: false });
    assert_eq!(voice_update(&mut ana, beto_id), estado(beto_id, casa.sala2, false, false));

    // a ana também entra: o estado completo tem os dois, em ordem
    ana.send(C2S::VoiceJoin { channel_id: casa.sala });
    assert_eq!(voice_update(&mut ana, ana_id), estado(ana_id, casa.sala, false, false));
    ana2.collect(Duration::from_millis(100));
    let mut ana3 = env.client();
    ana3.send(C2S::Login { username: "ana".into(), password: "segredo-forte-1".into(), proto: telacast_proto::PROTO_VERSION });
    let snap = ana3.wait("snapshot com os dois na voz", |m| match m {
        S2C::Snapshot(s) => Some(s.clone()),
        _ => None,
    });
    assert_eq!(snap.voice, vec![estado(ana_id, casa.sala, false, false), estado(beto_id, casa.sala2, false, false)]);

    // sair avisa todos (inclusive quem saiu); sair sem estar na voz não faz nada
    beto.send(C2S::VoiceLeave);
    // (o beto ainda tem na fila o VoiceLeft da sala antiga, da mudança de canal)
    beto.wait("VoiceLeft da sala 2", |m| matches!(m, S2C::VoiceLeft { user_id, channel_id } if *user_id == beto_id && *channel_id == casa.sala2).then_some(()));
    assert_eq!(voice_left(&mut ana, beto_id), casa.sala2);
    beto.send(C2S::VoiceLeave);
    assert!(nenhum_evento_de_voz(&beto.collect(Duration::from_millis(300))));

    // cair a conexão: só some da voz quando cai a ÚLTIMA conexão da pessoa
    let mut beto2 = env.client();
    beto2.resume(&beto_tok);
    beto.send(C2S::VoiceJoin { channel_id: casa.sala });
    voice_update(&mut ana, beto_id);
    drop(beto);
    assert!(nenhum_evento_de_voz(&ana.collect(Duration::from_millis(400))), "ainda há outra conexão do beto");
    drop(beto2);
    assert_eq!(voice_left(&mut ana, beto_id), casa.sala);
}

#[test]
fn voz_logout_e_sessao_revogada() {
    let env = Env::start();
    let code = env.invite(3);
    let mut ana = env.client();
    ana.register("ana", &code);
    let mut beto = env.client();
    let (_, beto_id) = beto.register("beto", &code);
    let mut carla = env.client();
    let (_, carla_id) = carla.register("carla", &code);
    let casa = montar_casa(&mut ana, &mut [&mut beto, &mut carla]);

    // Logout tira da voz
    beto.send(C2S::VoiceJoin { channel_id: casa.sala });
    voice_update(&mut ana, beto_id);
    beto.send(C2S::Logout);
    assert_eq!(voice_left(&mut ana, beto_id), casa.sala);

    // sessão revogada por outra sessão da mesma pessoa: a conexão em voz é expirada e some da voz
    let mut c2 = env.client();
    c2.send(C2S::Login { username: "carla".into(), password: "segredo-forte-1".into(), proto: telacast_proto::PROTO_VERSION });
    c2.wait("Auth da 2ª sessão", |m| matches!(m, S2C::Auth { .. }).then_some(()));
    c2.send(C2S::VoiceJoin { channel_id: casa.sala2 });
    assert_eq!(voice_update(&mut ana, carla_id).channel_id, casa.sala2);
    carla.send(C2S::RevokeOtherSessions);
    assert_eq!(c2.error_code().0, ErrorCode::SessionExpired);
    assert_eq!(voice_left(&mut ana, carla_id), casa.sala2);
}

#[test]
fn voz_criar_canal_de_voz() {
    let env = Env::start();
    let code = env.invite(2);
    let mut ana = env.client();
    ana.register("ana", &code);
    let mut beto = env.client();
    let (_, beto_id) = beto.register("beto", &code);
    let casa = montar_casa(&mut ana, &mut [&mut beto]);

    // só o dono cria; o tipo 2 (anúncio) ainda não existe
    beto.send(C2S::CreateChannel { server_id: casa.server_id, name: "intruso".into(), kind: 1, parent_id: None });
    assert_eq!(beto.error_code().0, ErrorCode::Forbidden);
    ana.send(C2S::CreateChannel { server_id: casa.server_id, name: "anuncios".into(), kind: 2, parent_id: None });
    assert_eq!(ana.error_code().0, ErrorCode::InvalidInput);

    // voz pode ficar dentro de uma categoria; um canal de texto não serve de pai
    ana.send(C2S::CreateChannel { server_id: casa.server_id, name: "Sala Na Categoria".into(), kind: 1, parent_id: Some(casa.geral) });
    assert_eq!(ana.error_code().0, ErrorCode::NotFound);
    ana.send(C2S::CreateChannel { server_id: casa.server_id, name: "Sala Na Categoria".into(), kind: 1, parent_id: Some(casa.categoria) });
    let snap = beto.wait("snapshot do membro com o canal novo", |m| match m {
        S2C::Snapshot(s) if s.servers[0].channels.iter().any(|c| c.name == "sala-na-categoria") => Some(s.clone()),
        _ => None,
    });
    let nova = snap.servers[0].channels.iter().find(|c| c.name == "sala-na-categoria").unwrap();
    assert_eq!((nova.kind, nova.parent), (1, Some(casa.categoria)));
    // e dá para entrar nela
    beto.send(C2S::VoiceJoin { channel_id: nova.id });
    assert_eq!(voice_update(&mut ana, beto_id).channel_id, nova.id);
}

#[test]
fn voz_rele_de_quadros() {
    let env = Env::start();
    let code = env.invite(4);
    let mut ana = env.client();
    let (_, ana_id) = ana.register("ana", &code);
    let mut beto = env.client();
    let (beto_tok, beto_id) = beto.register("beto", &code);
    let mut carla = env.client();
    let (_, carla_id) = carla.register("carla", &code);
    let mut dora = env.client();
    dora.register("dora", &code);
    let casa = montar_casa(&mut ana, &mut [&mut beto, &mut carla, &mut dora]);
    let mut beto2 = env.client();
    beto2.resume(&beto_tok);

    for (c, id) in [(&mut ana, ana_id), (&mut beto, beto_id), (&mut carla, carla_id)] {
        c.send(C2S::VoiceJoin { channel_id: casa.sala });
        voice_update(c, id);
    }
    // a dora não entra na voz; espera todos assentarem
    let frame = |seq: u32, opus: &[u8]| {
        let mut f = vec![VOICE_FRAME];
        f.extend_from_slice(&seq.to_be_bytes());
        f.extend_from_slice(opus);
        f
    };
    let esperado = |de: i64, seq: u32, opus: &[u8]| {
        let mut f = vec![VOICE_FRAME];
        f.extend_from_slice(&de.to_be_bytes());
        f.extend_from_slice(&seq.to_be_bytes());
        f.extend_from_slice(opus);
        f
    };
    let dre = Duration::from_millis(300);

    // a ana fala: beto (nas duas conexões) e carla recebem o conteúdo certo; ela e a dora, nada
    ana.send_bin(frame(7, b"ola-opus"));
    assert_eq!(beto.recv_bin(), esperado(ana_id, 7, b"ola-opus"));
    assert_eq!(beto2.recv_bin(), esperado(ana_id, 7, b"ola-opus"));
    assert_eq!(carla.recv_bin(), esperado(ana_id, 7, b"ola-opus"));
    assert!(ana.bins_in(dre).is_empty(), "o remetente não recebe o próprio áudio");
    assert!(dora.bins_in(Duration::from_millis(50)).is_empty());

    // quem não está em voz não é repassado
    dora.send_bin(frame(1, b"intruso"));
    assert!(ana.bins_in(dre).is_empty() && beto.bins_in(Duration::from_millis(50)).is_empty());

    // mudo não repassa
    ana.send(C2S::VoiceSet { muted: true, deafened: false });
    voice_update(&mut ana, ana_id);
    ana.send_bin(frame(8, b"mudo"));
    assert!(beto.bins_in(dre).is_empty() && carla.bins_in(Duration::from_millis(50)).is_empty());
    ana.send(C2S::VoiceSet { muted: false, deafened: false });
    voice_update(&mut ana, ana_id);

    // surdo não recebe e também não é ouvido; os outros continuam recebendo
    carla.send(C2S::VoiceSet { muted: false, deafened: true });
    voice_update(&mut carla, carla_id);
    ana.send_bin(frame(9, b"para-beto"));
    assert_eq!(beto.recv_bin(), esperado(ana_id, 9, b"para-beto"));
    assert!(carla.bins_in(dre).is_empty(), "surdo não recebe");
    carla.send_bin(frame(1, b"surdo-fala"));
    assert!(ana.bins_in(dre).is_empty() && beto.bins_in(Duration::from_millis(50)).is_empty(), "surdo também fica mudo");
    carla.send(C2S::VoiceSet { muted: false, deafened: false });
    voice_update(&mut carla, carla_id);

    // inválidos são descartados sem derrubar a conexão: curto, tipo errado, pacote grande e quadro enorme
    ana.send_bin(vec![VOICE_FRAME, 0, 0, 0, 1]); // só o cabeçalho, sem Opus
    ana.send_bin(vec![]);
    ana.send_bin(vec![VOICE_FRAME]);
    let mut errado = frame(10, b"x");
    errado[0] = 99;
    ana.send_bin(errado);
    ana.send_bin(frame(11, &vec![1u8; VOICE_MAX_PACKET + 1]));
    ana.send_bin(frame(12, &vec![1u8; 5000]));
    assert!(beto.bins_in(dre).is_empty(), "nada disso deveria ser repassado");
    // no limite exato passa, e a conexão continua viva (texto e áudio)
    ana.send_bin(frame(13, &vec![2u8; VOICE_MAX_PACKET]));
    assert_eq!(beto.recv_bin(), esperado(ana_id, 13, &vec![2u8; VOICE_MAX_PACKET]));
    ana.send(C2S::Send { channel_id: casa.geral, content: "ainda aqui".into(), reply_to: None, nonce: 0, attachments: vec![] });
    ana.wait("a própria mensagem", |m| matches!(m, S2C::Message(x) if x.content == "ainda aqui").then_some(()));

    // limite de taxa: uma rajada de 400 quadros não passa inteira (rajada de ~100 + 100/s)
    for i in 0..400u32 {
        ana.send_bin(frame(100 + i, b"rajada"));
    }
    let recebidos = beto.bins_in(Duration::from_secs(1)).len();
    assert!((50..=250).contains(&recebidos), "a rajada de 400 deveria ser limitada, chegaram {recebidos}");
}

#[test]
fn atualizacao_publicar_e_baixar() {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = format!("/tmp/telacast_test_updates_{nanos}");
    std::fs::create_dir_all(&dir).unwrap();
    let key = format!("{dir}/chave");
    let bin = env!("CARGO_BIN_EXE_telacast-server");
    // chave de assinatura: 600, e a chave pública sai no texto
    let out = Command::new(bin).args(["update-keygen", "--out", &key]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let public = String::from_utf8_lossy(&out.stdout).lines().last().unwrap().trim().to_string();
    assert_eq!(public.len(), 64);
    // não sobrescreve uma chave existente
    assert!(!Command::new(bin).args(["update-keygen", "--out", &key]).output().unwrap().status.success());

    let exe = format!("{dir}/app.bin");
    std::fs::write(&exe, b"binario falso do app").unwrap();
    let srv = format!("{dir}/srv");
    let out = Command::new(bin)
        .args(["publish-update", "--file", &exe, "--version", "0.2.0", "--platform", "linux-x86_64", "--notes", "novidades"])
        .env("TELACAST_UPDATES_DIR", &srv)
        .env("TELACAST_UPDATE_KEY_FILE", &key)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

    let env = Env::start_with(&[("TELACAST_UPDATES_DIR", &srv)]);
    let (st, _, body) = http(env.port, "GET", "/update/manifest?platform=linux-x86_64", b"");
    assert_eq!(st, 200);
    let m: telacast_proto::UpdateManifest = serde_json::from_slice(&body).unwrap();
    assert_eq!(m.version, "0.2.0");
    assert_eq!(m.size, 20);
    assert_eq!(m.signature.len(), 128, "assinatura Ed25519 em hex");
    // o arquivo vem pelo hash e confere
    let (st, _, file) = http(env.port, "GET", &format!("/update/file/{}", m.sha256), b"");
    assert_eq!(st, 200);
    assert_eq!(file, b"binario falso do app");
    // link de download amigável: mesmo arquivo, com nome e tipo certos
    let plat = m.platform.clone();
    let (st, head, baixado) = http(env.port, "GET", &format!("/download/{plat}"), b"");
    assert_eq!(st, 200);
    assert_eq!(baixado, b"binario falso do app");
    assert!(head.contains(&format!("filename=\"telacast-{}-{plat}.", m.version)), "{head}");
    assert_eq!(http(env.port, "GET", "/download/android-arm64", b"").0, 404, "sem publicação");
    // barra no fim, página com os links e apelidos curtos
    assert_eq!(http(env.port, "GET", &format!("/download/{plat}/"), b"").0, 200);
    let (st, _, pagina) = http(env.port, "GET", "/download", b"");
    assert_eq!(st, 200);
    assert!(String::from_utf8_lossy(&pagina).contains(&format!("/download/{plat}")), "a página lista a plataforma publicada");
    assert_eq!(http(env.port, "GET", "/baixar", b"").0, 200);
    assert_eq!(http(env.port, "GET", "/download/..%2Fx", b"").0, 400);
    // plataforma sem publicação, plataforma e hash malformados
    assert_eq!(http(env.port, "GET", "/update/manifest?platform=linux-aarch64", b"").0, 404);
    assert_eq!(http(env.port, "GET", "/update/manifest?platform=..%2Fx", b"").0, 400);
    assert_eq!(http(env.port, "GET", "/update/file/../../etc/passwd", b"").0, 404);
    assert_eq!(http(env.port, "GET", &format!("/update/file/{}", "0".repeat(64)), b"").0, 404);
    assert_eq!(http(env.port, "GET", "/update/file/XYZ", b"").0, 400);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn transmissao_espectadores() {
    let env = Env::start();
    let code = env.invite(2);
    let mut ana = env.client();
    let (ana_token, ana_id) = ana.register("ana", &code);
    let mut beto = env.client();
    let (beto_token, beto_id) = beto.register("beto", &code);
    let casa = montar_casa(&mut ana, &mut [&mut beto]);
    // transmitir e assistir exigem estar na chamada do canal de voz agora
    let channel = casa.sala;
    ana.send(C2S::VoiceJoin { channel_id: channel });
    voice_update(&mut ana, ana_id);
    beto.send(C2S::VoiceJoin { channel_id: channel });
    voice_update(&mut beto, beto_id);

    // a ana transmite (um quadro-chave falso basta para o servidor)
    let (mut pubws, _) = tungstenite::connect(format!("ws://127.0.0.1:{}/publish/{channel}?token={ana_token}", env.port)).unwrap();
    let keyframe = telacast_proto::encode_media_packet(telacast_proto::MediaKind::H264Keyframe, 0, &[0, 0, 0, 1, 0x65]).unwrap();
    pubws.send(tungstenite::Message::Binary(keyframe.clone().into())).unwrap();
    beto.wait("a transmissão aparece", |m| match m {
        S2C::Snapshot(s) if !s.live.is_empty() => Some(()),
        _ => None,
    });

    // o beto assiste: a ana e ele recebem a lista com o beto
    let (mut view, _) = tungstenite::connect(format!("ws://127.0.0.1:{}/watch/{channel}?token={beto_token}", env.port)).unwrap();
    let users = ana.wait("espectadores da ana", |m| match m {
        S2C::Viewers { channel_id, users } if *channel_id == channel && !users.is_empty() => Some(users.clone()),
        _ => None,
    });
    assert_eq!(users, vec![beto_id]);
    beto.wait("espectadores do beto", |m| matches!(m, S2C::Viewers { users, .. } if users == &vec![beto_id]).then_some(()));
    // o relé funciona: o espectador recebe o quadro-chave
    let got = loop {
        match view.read().unwrap() {
            tungstenite::Message::Binary(b) => break b,
            _ => continue,
        }
    };
    assert_eq!(got.as_ref(), &keyframe[..], "o relé entrega o pacote de mídia intacto");

    // o beto sai: a lista da ana esvazia
    view.close(None).ok();
    drop(view);
    ana.wait("lista vazia", |m| matches!(m, S2C::Viewers { users, .. } if users.is_empty()).then_some(()));
    let _ = (ana_id, pubws.close(None));
}


// ------------------------------------------------------------------ notificações push (FCM)

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

/// Uma requisição que o servidor falso recebeu.
#[derive(Clone, Debug)]
struct Req {
    path: String,
    auth: String,
    body: String,
}

/// Servidor HTTP local que faz o papel do Google: `POST /token` (OAuth2) e `POST /v1/projects/*/messages:send` (FCM).
struct Fake {
    port: u16,
    reqs: Arc<Mutex<Vec<Req>>>,
    /// Status das próximas respostas do FCM; esgotado, responde 200.
    script: Arc<Mutex<VecDeque<u16>>>,
    stop: Arc<AtomicBool>,
}

impl Fake {
    fn start() -> Fake {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let (reqs, script, stop) = (Arc::new(Mutex::new(Vec::new())), Arc::new(Mutex::new(VecDeque::new())), Arc::new(AtomicBool::new(false)));
        let (r2, s2, st2) = (reqs.clone(), script.clone(), stop.clone());
        std::thread::spawn(move || {
            while !st2.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((conn, _)) => {
                        let (r, s) = (r2.clone(), s2.clone());
                        std::thread::spawn(move || Fake::serve(conn, r, s));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(10)),
                }
            }
        });
        Fake { port, reqs, script, stop }
    }

    fn serve(mut conn: TcpStream, reqs: Arc<Mutex<Vec<Req>>>, script: Arc<Mutex<VecDeque<u16>>>) {
        conn.set_nonblocking(false).unwrap();
        conn.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut raw = Vec::new();
        let mut buf = [0u8; 4096];
        let cut = loop {
            if let Some(i) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                break i;
            }
            match conn.read(&mut buf) {
                Ok(0) | Err(_) => return,
                Ok(n) => raw.extend_from_slice(&buf[..n]),
            }
        };
        let head = String::from_utf8_lossy(&raw[..cut]).to_string();
        let header = |name: &str| {
            head.lines().find_map(|l| {
                let (k, v) = l.split_once(':')?;
                k.eq_ignore_ascii_case(name).then(|| v.trim().to_string())
            })
        };
        let len: usize = header("content-length").and_then(|v| v.parse().ok()).unwrap_or(0);
        let mut body = raw[cut + 4..].to_vec();
        while body.len() < len {
            match conn.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => body.extend_from_slice(&buf[..n]),
            }
        }
        let path = head.split_whitespace().nth(1).unwrap_or("").to_string();
        reqs.lock().unwrap().push(Req { path: path.clone(), auth: header("authorization").unwrap_or_default(), body: String::from_utf8_lossy(&body).to_string() });
        let (status, text) = if path.contains(":send") {
            match script.lock().unwrap().pop_front().unwrap_or(200) {
                200 => (200, r#"{"name":"projects/telacast-test/messages/0:1"}"#.to_string()),
                404 => (404, r#"{"error":{"code":404,"status":"NOT_FOUND","details":[{"errorCode":"UNREGISTERED"}]}}"#.to_string()),
                n => (n, r#"{"error":{"status":"UNAVAILABLE"}}"#.to_string()),
            }
        } else {
            (200, r#"{"access_token":"fake-access-token","expires_in":3600,"token_type":"Bearer"}"#.to_string())
        };
        let _ = write!(conn, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}", text.len());
    }

    fn sends(&self) -> Vec<Req> {
        self.reqs.lock().unwrap().iter().filter(|r| r.path.contains(":send")).cloned().collect()
    }

    fn token_reqs(&self) -> Vec<Req> {
        self.reqs.lock().unwrap().iter().filter(|r| r.path == "/token").cloned().collect()
    }

    /// Espera até haver `n` envios (e devolve todos).
    fn wait_sends(&self, n: usize) -> Vec<Req> {
        let start = Instant::now();
        loop {
            let s = self.sends();
            if s.len() >= n {
                return s;
            }
            assert!(start.elapsed() < Duration::from_secs(12), "esperava {n} envios ao FCM, houve {}", s.len());
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Confere que nada novo é enviado durante `dur` (as janelas de teste são de 400 ms).
    fn still_at(&self, n: usize, dur: Duration) {
        std::thread::sleep(dur);
        assert_eq!(self.sends().len(), n, "não devia haver envio novo: {:?}", self.sends().last());
    }
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Conta de serviço de mentira: par RSA gerado agora (openssl) só para o teste.
struct Cred {
    dir: std::path::PathBuf,
    json: String,
    /// Chave pública em DER (RSAPublicKey), para conferir a assinatura do JWT.
    public: Vec<u8>,
}

impl Cred {
    fn new() -> Cred {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::path::PathBuf::from(format!("/tmp/telacast_test_fcm_{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let key = dir.join("key.pem");
        let ok = |st: std::io::Result<std::process::ExitStatus>| st.expect("o teste precisa do comando openssl").success();
        assert!(ok(Command::new("openssl").args(["genpkey", "-algorithm", "RSA", "-pkeyopt", "rsa_keygen_bits:2048", "-out"]).arg(&key).status()));
        let pubder = dir.join("pub.der");
        assert!(ok(Command::new("openssl").arg("rsa").arg("-in").arg(&key).args(["-RSAPublicKey_out", "-outform", "DER", "-out"]).arg(&pubder).stderr(Stdio::null()).status()));
        let sa = serde_json::json!({
            "type": "service_account",
            "project_id": "telacast-test",
            "client_email": "push@telacast-test.iam.gserviceaccount.com",
            "private_key": std::fs::read_to_string(&key).unwrap(),
            "token_uri": "https://oauth2.googleapis.com/token",
        });
        let json = dir.join("service-account.json");
        std::fs::write(&json, sa.to_string()).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&json, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        let public = std::fs::read(&pubder).unwrap();
        std::fs::remove_file(&key).unwrap();
        Cred { json: json.to_string_lossy().to_string(), dir, public }
    }
}

impl Drop for Cred {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Sobe o servidor de verdade apontando para o Google falso. `coalesce_ms` é a janela do anti-spam.
fn start_push(fake: &Fake, cred: &Cred, coalesce_ms: &str) -> Env {
    let endpoint = format!("http://127.0.0.1:{}/v1/projects/{{project_id}}/messages:send", fake.port);
    let token_uri = format!("http://127.0.0.1:{}/token", fake.port);
    Env::start_with(&[
        ("TELACAST_FCM_SERVICE_ACCOUNT", &cred.json),
        ("TELACAST_FCM_ENDPOINT", &endpoint),
        ("TELACAST_FCM_TOKEN_URI", &token_uri),
        ("TELACAST_FCM_RETRY_MS", "100"),
        ("TELACAST_PUSH_COALESCE_MS", coalesce_ms),
    ])
}

impl Env {
    fn tokens(&self, user_id: i64) -> Vec<String> {
        self.rt.block_on(async {
            let pool = sqlx::PgPool::connect(&self.db_url).await.unwrap();
            let rows: Vec<(String,)> = sqlx::query_as("SELECT token FROM push_tokens WHERE user_id = $1 ORDER BY last_seen")
                .bind(user_id)
                .fetch_all(&pool)
                .await
                .unwrap();
            pool.close().await;
            rows.into_iter().map(|r| r.0).collect()
        })
    }
}

impl Client {
    /// Manda `m` e espera uma resposta qualquer do servidor sobre ela (`ListSessions` volta `Sessions`):
    /// as mensagens de uma conexão são tratadas em ordem, então tudo o que veio antes já foi aplicado.
    fn sync(&mut self) {
        self.send(C2S::ListSessions);
        self.wait("Sessions", |m| matches!(m, S2C::Sessions { .. }).then_some(()));
    }

    fn push_register(&mut self, token: &str) {
        self.send(C2S::PushRegister { token: token.into(), platform: "fcm".into() });
        self.sync();
    }

    fn texto(&mut self, canal: i64, texto: &str) -> i64 {
        self.send(C2S::Send { channel_id: canal, content: texto.into(), reply_to: None, nonce: 0, attachments: vec![] });
        let t = texto.to_string();
        self.wait("a própria mensagem", |m| match m {
            S2C::Message(x) if x.content == t => Some(x.id),
            _ => None,
        })
    }
}

/// Ana (dona), Beto e Carla no servidor "Casa"; Ana e Beto já têm uma DM.
struct Cena {
    ana: Client,
    beto: Client,
    carla: Client,
    ana_id: i64,
    beto_id: i64,
    carla_id: i64,
    server_id: i64,
    geral: i64,
    dm: i64,
}

fn cena(env: &Env) -> Cena {
    let code = env.invite(3);
    let (mut ana, mut beto, mut carla) = (env.client(), env.client(), env.client());
    let (_, ana_id) = ana.register("ana", &code);
    let (_, beto_id) = beto.register("beto", &code);
    let (_, carla_id) = carla.register("carla", &code);
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
    ana.send(C2S::OpenDm { user_id: beto_id });
    let dm = ana.wait("DmOpened", |m| if let S2C::DmOpened { channel_id } = m { Some(*channel_id) } else { None });
    Cena { ana, beto, carla, ana_id, beto_id, carla_id, server_id: snap.servers[0].id, geral, dm }
}

fn json(r: &Req) -> serde_json::Value {
    serde_json::from_str(&r.body).unwrap_or_else(|e| panic!("corpo não é JSON ({e}): {}", r.body))
}

/// Confere a assinatura RS256 do JWT enviado na troca de credencial e devolve os campos (`iss`, `aud`...).
fn confere_jwt(req: &Req, cred: &Cred, aud: &str) {
    let (grant, assertion) = req.body.split_once("&assertion=").expect("corpo form com grant_type e assertion");
    assert_eq!(grant, "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer");
    let parts: Vec<&str> = assertion.split('.').collect();
    assert_eq!(parts.len(), 3, "JWT tem 3 partes");
    let header: serde_json::Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[0]).unwrap()).unwrap();
    assert_eq!(header, serde_json::json!({"alg": "RS256", "typ": "JWT"}));
    let claims: serde_json::Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(parts[1]).unwrap()).unwrap();
    assert_eq!(claims["iss"], "push@telacast-test.iam.gserviceaccount.com");
    assert_eq!(claims["scope"], "https://www.googleapis.com/auth/firebase.messaging");
    assert_eq!(claims["aud"], aud);
    let (iat, exp) = (claims["iat"].as_u64().unwrap(), claims["exp"].as_u64().unwrap());
    assert_eq!(exp - iat, 3600);
    let agora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    assert!(agora.abs_diff(iat) < 30, "iat é de agora");
    let sig = URL_SAFE_NO_PAD.decode(parts[2]).unwrap();
    let signed = format!("{}.{}", parts[0], parts[1]);
    ring::signature::UnparsedPublicKey::new(&ring::signature::RSA_PKCS1_2048_8192_SHA256, &cred.public)
        .verify(signed.as_bytes(), &sig)
        .expect("a assinatura RS256 do JWT confere com a chave pública");
}

#[test]
fn push_sem_credencial_nada_e_enviado() {
    let fake = Fake::start();
    let endpoint = format!("http://127.0.0.1:{}/v1/projects/{{project_id}}/messages:send", fake.port);
    // endpoints configurados, mas SEM TELACAST_FCM_SERVICE_ACCOUNT: push desligado
    let env = Env::start_with(&[("TELACAST_FCM_ENDPOINT", &endpoint), ("TELACAST_PUSH_COALESCE_MS", "300")]);
    let mut c = cena(&env);
    c.beto.push_register("tok-beto");
    assert_eq!(env.tokens(c.beto_id), vec!["tok-beto"], "o registro funciona mesmo com o push desligado");
    c.ana.texto(c.dm, "oi beto");
    c.ana.texto(c.geral, &format!("oi <@{}>", c.beto_id));
    c.ana.send(C2S::FriendRequest { username: "beto".into() });
    c.ana.sync();
    std::thread::sleep(Duration::from_millis(1500));
    assert!(fake.reqs.lock().unwrap().is_empty(), "nada pode ser enviado nem pedido ao Google sem credencial");
}

#[test]
fn push_registro_de_tokens_e_limites() {
    let env = Env::start();
    let mut c = cena(&env);
    let tok = |i: usize| format!("tok-{i:02}");
    for i in 0..=10 {
        c.beto.send(C2S::PushRegister { token: tok(i), platform: "fcm".into() });
    }
    c.beto.sync();
    let t = env.tokens(c.beto_id);
    assert_eq!(t.len(), 10, "no máximo 10 tokens por conta");
    assert!(!t.contains(&tok(0)), "o mais antigo saiu");
    assert!(t.contains(&tok(10)));
    // repetir é idempotente e renova: o 1 vira o mais novo e o 2 é quem sai quando entra um novo
    c.beto.push_register(&tok(1));
    assert_eq!(env.tokens(c.beto_id).len(), 10);
    c.beto.push_register(&tok(11));
    let t = env.tokens(c.beto_id);
    assert!(t.contains(&tok(1)) && !t.contains(&tok(2)) && t.contains(&tok(11)), "{t:?}");
    // um token pertence a uma conta: se a Ana registra o do Beto, ele muda de dono
    c.ana.push_register(&tok(10));
    assert_eq!(env.tokens(c.ana_id), vec![tok(10)]);
    assert!(!env.tokens(c.beto_id).contains(&tok(10)));
    // desregistrar só apaga token da própria conta; token desconhecido não é erro
    c.ana.send(C2S::PushUnregister { token: tok(3) });
    c.ana.send(C2S::PushUnregister { token: "nao-existe".into() });
    c.ana.sync();
    assert!(env.tokens(c.beto_id).contains(&tok(3)), "a Ana não apaga token do Beto");
    c.beto.send(C2S::PushUnregister { token: tok(3) });
    c.beto.sync();
    assert!(!env.tokens(c.beto_id).contains(&tok(3)));
    // entradas inválidas
    for (token, platform) in [(String::new(), "fcm"), ("   ".into(), "fcm"), ("x".repeat(4097), "fcm"), ("abc".into(), "apns"), ("a b".into(), "fcm")] {
        c.beto.send(C2S::PushRegister { token, platform: platform.into() });
        assert_eq!(c.beto.error_code().0, ErrorCode::InvalidInput);
    }
    c.beto.push_register(&"y".repeat(4096));
    assert!(env.tokens(c.beto_id).iter().any(|t| t.len() == 4096));
    // sem login, nada disso vale
    let mut anon = env.client();
    anon.send(C2S::PushRegister { token: "x".into(), platform: "fcm".into() });
    assert_eq!(anon.error_code().0, ErrorCode::NotAuthenticated);
}

#[test]
fn push_dm_foco_e_token_em_cache() {
    let fake = Fake::start();
    let cred = Cred::new();
    let env = start_push(&fake, &cred, "400");
    let mut c = cena(&env);
    c.beto.push_register("tok-beto-1");

    // 1) o Beto está conectado mas em segundo plano (padrão focused=false): recebe
    let texto = format!("**oi** ||segredo|| <@{}>", c.beto_id);
    let id = c.ana.texto(c.dm, &texto);
    let msg_ts = c.beto.wait("a DM", |m| match m {
        S2C::Message(x) if x.id == id => Some(x.ts),
        _ => None,
    });
    let envios = fake.wait_sends(1);
    let r = &envios[0];
    assert_eq!(r.path, "/v1/projects/telacast-test/messages:send");
    assert_eq!(r.auth, "Bearer fake-access-token");
    let b = json(r);
    assert_eq!(
        b["message"]["data"],
        serde_json::json!({
            "kind": "dm",
            "channel_id": c.dm.to_string(),
            "message_id": id.to_string(),
            "server_id": "",
            "title": "ana",
            "body": "oi spoiler oculto @beto",
            "author_id": c.ana_id.to_string(),
            "ts": msg_ts.to_string(),
        })
    );
    assert_eq!(b["message"]["token"], "tok-beto-1");
    assert_eq!(b["message"]["android"], serde_json::json!({"priority": "HIGH", "ttl": "3600s", "collapse_key": format!("ch-{}", c.dm)}));
    assert!(b["message"].get("notification").is_none(), "só mensagem de dados");
    // o JWT trocado pelo token de acesso está assinado de verdade
    let trocas = fake.token_reqs();
    assert_eq!(trocas.len(), 1);
    confere_jwt(&trocas[0], &cred, &format!("http://127.0.0.1:{}/token", fake.port));

    // 2) com o app em primeiro plano o Beto não recebe
    c.beto.send(C2S::SetFocus { focused: true });
    c.beto.sync();
    c.ana.texto(c.dm, "segunda");
    fake.still_at(1, Duration::from_millis(1500));

    // 3) voltou para o segundo plano: recebe de novo, e o token de acesso veio do cache
    c.beto.send(C2S::SetFocus { focused: false });
    c.beto.sync();
    c.ana.texto(c.dm, "terceira");
    let envios = fake.wait_sends(2);
    assert_eq!(json(&envios[1])["message"]["data"]["body"], "terceira");
    assert_eq!(fake.token_reqs().len(), 1, "o token de acesso fica em cache");

    // 4) foco é por conexão: a conexão em foco cai e o estado some com ela
    let mut outra = env.client();
    outra.resume(&{
        // sessão nova do Beto
        c.beto.send(C2S::Logout);
        let mut x = env.client();
        x.send(C2S::Login { username: "beto".into(), password: "segredo-forte-1".into(), proto: telacast_proto::PROTO_VERSION });
        x.wait("Auth", |m| if let S2C::Auth { token, .. } = m { Some(token.clone()) } else { None })
    });
    outra.send(C2S::SetFocus { focused: true });
    outra.sync();
    c.ana.texto(c.dm, "quarta");
    fake.still_at(2, Duration::from_millis(1500));
    drop(outra); // a conexão fecha: o foco some
    std::thread::sleep(Duration::from_millis(300));
    // 5) offline (sem conexão nenhuma) também recebe
    c.ana.texto(c.dm, "quinta");
    let envios = fake.wait_sends(3);
    assert_eq!(json(&envios[2])["message"]["data"]["body"], "quinta");
}

#[test]
fn push_regras_de_quem_e_avisado() {
    let fake = Fake::start();
    let cred = Cred::new();
    let env = start_push(&fake, &cred, "400");
    let mut c = cena(&env);
    c.beto.push_register("tok-beto");
    let beto = c.beto_id;
    let janela = Duration::from_millis(1400);
    let ultimo = |n: usize| json(&fake.wait_sends(n)[n - 1])["message"]["data"].clone();

    // canal comum sem menção: ninguém é avisado
    c.ana.texto(c.geral, "bom dia pessoal");
    fake.still_at(0, janela);

    // menção direta
    let id = c.ana.texto(c.geral, &format!("oi <@{beto}>, olha **isso**"));
    let d = ultimo(1);
    assert_eq!(d["kind"], "mention");
    assert_eq!(d["title"], "ana em #geral (Casa)");
    assert_eq!(d["body"], "oi @beto, olha isso");
    assert_eq!(d["channel_id"], c.geral.to_string());
    assert_eq!(d["server_id"], c.server_id.to_string());
    assert_eq!(d["message_id"], id.to_string());

    // @everyone e @here (o Beto está conectado, então os dois valem)
    c.ana.texto(c.geral, "@everyone reunião agora");
    assert_eq!(ultimo(2)["kind"], "mention");
    c.ana.texto(c.geral, "@here quem está aí?");
    assert_eq!(ultimo(3)["body"], "@here quem está aí?");

    // resposta a uma mensagem do Beto
    let pergunta = c.beto.texto(c.geral, "alguém viu o relatório?");
    c.ana.wait("a pergunta do Beto", |m| matches!(m, S2C::Message(x) if x.id == pergunta).then_some(()));
    fake.still_at(3, Duration::from_millis(500));
    c.ana.send(C2S::Send { channel_id: c.geral, content: "vi sim".into(), reply_to: Some(pergunta), nonce: 0, attachments: vec![] });
    let d = ultimo(4);
    assert_eq!((d["kind"].as_str(), d["body"].as_str()), (Some("mention"), Some("vi sim")));

    // o autor nunca é avisado da própria menção
    c.beto.texto(c.geral, &format!("eu mesmo <@{beto}>"));
    fake.still_at(4, janela);

    // não perturbe
    c.beto.send(C2S::SetStatus { status: telacast_proto::Status::Dnd });
    c.beto.wait("status DND", |m| matches!(m, S2C::Snapshot(s) if s.me.status == telacast_proto::Status::Dnd).then_some(()));
    c.ana.texto(c.dm, "dm com DND");
    c.ana.texto(c.geral, &format!("<@{beto}> com DND"));
    fake.still_at(4, janela);
    c.beto.send(C2S::SetStatus { status: telacast_proto::Status::Online });
    c.beto.wait("status online", |m| matches!(m, S2C::Snapshot(s) if s.me.status == telacast_proto::Status::Online).then_some(()));

    // bloqueio: quem o Beto bloqueou não o faz receber push, nem citando
    c.beto.send(C2S::Block { user_id: c.carla_id });
    c.beto.wait("bloqueio", |m| matches!(m, S2C::Snapshot(s) if !s.blocked.is_empty()).then_some(()));
    c.carla.texto(c.geral, &format!("<@{beto}> ei"));
    fake.still_at(4, janela);
    c.beto.send(C2S::Unblock { user_id: c.carla_id });
    c.beto.wait("desbloqueio", |m| matches!(m, S2C::Snapshot(s) if s.blocked.is_empty()).then_some(()));
    c.carla.texto(c.geral, &format!("<@{beto}> agora vai"));
    assert_eq!(ultimo(5)["title"], "carla em #geral (Casa)");

    // pedido de amizade
    c.ana.send(C2S::FriendRequest { username: "beto".into() });
    let d = ultimo(6);
    assert_eq!(d["kind"], "friend_request");
    assert_eq!(d["title"], "Pedido de amizade");
    assert_eq!(d["body"], "ana quer ser seu amigo");
    assert_eq!(d["author_id"], c.ana_id.to_string());
    assert_eq!((d["channel_id"].as_str(), d["message_id"].as_str(), d["server_id"].as_str()), (Some(""), Some(""), Some("")));
    // aceitar o pedido (ou pedir de volta) não gera aviso novo para quem pediu: ela não tem token
    c.beto.send(C2S::FriendAccept { user_id: c.ana_id });
    c.beto.sync();
    fake.still_at(6, Duration::from_millis(800));
}

#[test]
fn push_token_invalido_e_apagado_e_tentativas() {
    let fake = Fake::start();
    let cred = Cred::new();
    let env = start_push(&fake, &cred, "300");
    let mut c = cena(&env);
    c.beto.push_register("tok-morto");
    fake.script.lock().unwrap().push_back(404);
    c.ana.texto(c.dm, "um");
    fake.wait_sends(1);
    let t = Instant::now();
    while !env.tokens(c.beto_id).is_empty() {
        assert!(t.elapsed() < Duration::from_secs(5), "o token inválido devia ter sido apagado");
        std::thread::sleep(Duration::from_millis(50));
    }
    assert_eq!(fake.sends().len(), 1, "404 não é repetido");

    // 503 uma vez: tenta de novo e o token fica
    c.beto.push_register("tok-bom");
    fake.script.lock().unwrap().extend([503, 200]);
    c.ana.texto(c.dm, "dois");
    fake.wait_sends(3);
    fake.still_at(3, Duration::from_millis(800));
    assert_eq!(env.tokens(c.beto_id), vec!["tok-bom"]);

    // 429 duas vezes: uma nova tentativa só, depois descarta (e o token continua)
    fake.script.lock().unwrap().extend([429, 429]);
    c.ana.texto(c.dm, "tres");
    fake.wait_sends(5);
    fake.still_at(5, Duration::from_millis(1000));
    assert_eq!(env.tokens(c.beto_id), vec!["tok-bom"]);
}

#[test]
fn push_duas_mensagens_em_3s_viram_um_envio() {
    let fake = Fake::start();
    let cred = Cred::new();
    let env = start_push(&fake, &cred, "3000"); // a janela padrão do contrato
    let mut c = cena(&env);
    c.beto.push_register("tok-beto");
    let t0 = Instant::now();
    c.ana.texto(c.dm, "primeira");
    c.ana.texto(c.dm, "segunda");
    // outro canal não entra no mesmo grupo
    c.ana.texto(c.geral, &format!("<@{}> aqui", c.beto_id));
    let envios = fake.wait_sends(2);
    assert!(t0.elapsed() >= Duration::from_millis(2900), "a janela de 3 s segura o envio");
    fake.still_at(2, Duration::from_millis(1200));
    let bodies: Vec<String> = fake.sends().iter().map(|r| json(r)["message"]["data"]["body"].as_str().unwrap().to_string()).collect();
    assert!(bodies.contains(&"segunda".to_string()), "o último vence: {bodies:?}");
    assert!(!bodies.contains(&"primeira".to_string()), "{bodies:?}");
    assert_eq!(envios.len(), 2);
}
