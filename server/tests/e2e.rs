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
    rt: tokio::runtime::Runtime,
}

impl Env {
    fn start() -> Env {
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
        let port = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let child = Command::new(env!("CARGO_BIN_EXE_telacast-server"))
            .env("DATABASE_URL", &db_url)
            .env("TELACAST_ADDR", format!("127.0.0.1:{port}"))
            .env("RUST_LOG", "warn")
            .stdout(Stdio::null())
            .spawn()
            .unwrap();
        let env = Env { child, port, db, db_url, rt };
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

    fn error(&mut self) -> String {
        self.wait("um erro", |m| if let S2C::Error { msg } = m { Some(msg.clone()) } else { None })
    }

    /// Cadastra e devolve (token, id).
    fn register(&mut self, name: &str, invite: &str) -> (String, i64) {
        self.send(C2S::Register { username: name.into(), password: "segredo-forte-1".into(), invite: invite.into() });
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
    c.send(C2S::Register { username: "intruso".into(), password: "segredo-forte-1".into(), invite: "NAOEXISTE".into() });
    assert!(c.error().contains("convite"), "esperava recusa por convite");

    // --- convite de 2 usos: duas contas entram, a terceira não
    let code = env.invite(2);
    let mut ana = env.client();
    let (ana_token, ana_id) = ana.register("ana", &code);
    let mut beto = env.client();
    let (_, beto_id) = beto.register("beto", &code.to_lowercase()); // minúsculas também valem
    let mut carla = env.client();
    carla.send(C2S::Register { username: "carla".into(), password: "segredo-forte-1".into(), invite: code.clone() });
    assert!(carla.error().contains("convite"), "o convite esgotado deveria ser recusado");

    // nome já usado
    let code2 = env.invite(1);
    let mut dup = env.client();
    dup.send(C2S::Register { username: "ANA".into(), password: "segredo-forte-1".into(), invite: code2.clone() });
    assert!(dup.error().contains("em uso"));
    // a falha não consumiu o convite: ainda dá para usá-lo
    let mut carla = env.client();
    let (_, carla_id) = carla.register("carla", &code2);
    assert_ne!(carla_id, ana_id);

    // --- login
    let mut bad = env.client();
    bad.send(C2S::Login { username: "ana".into(), password: "errada-errada".into() });
    assert!(bad.error().contains("incorretos"));
    let mut bad2 = env.client();
    bad2.send(C2S::Login { username: "ninguem".into(), password: "errada-errada".into() });
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
    assert_eq!(casa.channels.len(), 1);
    assert_eq!(casa.channels[0].name, "geral");
    assert_eq!(casa.invite.len(), 26, "convite de servidor com 128 bits");
    let geral = casa.channels[0].id;
    let invite = casa.invite.clone();

    beto.send(C2S::JoinServer { code: "CODIGO-INVALIDO".into() });
    assert!(beto.error().contains("Convite"));
    beto.send(C2S::JoinServer { code: invite });
    beto.wait("ServerJoined do beto", |m| matches!(m, S2C::ServerJoined { .. }).then_some(()));

    // só o dono cria canais
    beto.send(C2S::CreateChannel { server_id, name: "Planos de Hoje".into() });
    assert!(beto.error().contains("dono"));
    ana.send(C2S::CreateChannel { server_id, name: "Planos de Hoje".into() });
    let snap = ana.wait("snapshot com o novo canal", |m| match m {
        S2C::Snapshot(s) if s.servers.iter().any(|x| x.channels.len() == 2) => Some(s.clone()),
        _ => None,
    });
    assert_eq!(snap.servers[0].channels[1].name, "planos-de-hoje");

    // --- mensagens em tempo real e histórico
    ana.send(C2S::Send { channel_id: geral, content: "oi beto!".into() });
    let msg = beto.wait("a mensagem", |m| if let S2C::Message(m) = m { Some(m.clone()) } else { None });
    assert_eq!(msg.content, "oi beto!");
    assert_eq!(msg.author, "ana");
    assert!(msg.id > 0 && msg.ts > 1_767_225_600_000, "id e hora vêm do snowflake");
    beto.send(C2S::History { channel_id: geral, before: None });
    let hist = beto.wait("o histórico", |m| if let S2C::History { messages, .. } = m { Some(messages.clone()) } else { None });
    assert_eq!(hist.len(), 1);
    assert_eq!(hist[0].id, msg.id);

    // quem não é do servidor não lê nem escreve
    carla.send(C2S::History { channel_id: geral, before: None });
    carla.send(C2S::Send { channel_id: geral, content: "invasão".into() });
    std::thread::sleep(Duration::from_millis(400));
    ana.send(C2S::History { channel_id: geral, before: None });
    let hist = ana.wait("o histórico da ana", |m| if let S2C::History { messages, .. } = m { Some(messages.clone()) } else { None });
    assert_eq!(hist.len(), 1, "a mensagem da carla não deveria existir");

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
    carla.send(C2S::Send { channel_id: dm, content: "privado".into() });
    let m = ana.wait("DM na ana", |m| if let S2C::Message(m) = m { Some(m.clone()) } else { None });
    assert_eq!(m.content, "privado");
    let _ = beto_id;

    // --- sessão: Resume funciona; Logout revoga o token
    let mut again = env.client();
    again.send(C2S::Resume { token: ana_token.clone() });
    again.wait("Auth do Resume", |m| matches!(m, S2C::Auth { .. }).then_some(()));
    again.send(C2S::Logout);
    std::thread::sleep(Duration::from_millis(300));
    let mut after = env.client();
    after.send(C2S::Resume { token: ana_token });
    assert!(after.error().contains("expirada"), "o token revogado não deve valer");
}

#[test]
fn limite_de_tentativas_de_login() {
    let env = Env::start();
    let mut last = String::new();
    for _ in 0..12 {
        let mut c = env.client();
        c.send(C2S::Login { username: "alguem".into(), password: "errada-errada".into() });
        last = c.error();
    }
    assert!(last.contains("Muitas tentativas"), "depois de várias tentativas deveria bloquear: {last}");
}
