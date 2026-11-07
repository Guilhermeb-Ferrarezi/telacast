mod gateway;
mod files;
mod gpu;
mod md_view;
mod net;
mod publisher;
mod quick;
mod single;
mod theme;
mod viewer;

use eframe::egui::{self, Align, Align2, Color32, Layout, RichText};
use gateway::{Event, Gateway};
use std::{
    collections::{HashMap, HashSet},
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use telacast_proto as p;
use p::{C2S, S2C};
use theme::*;

fn main() -> eframe::Result {
    if std::env::args().any(|a| a == "--bench") {
        if let Err(e) = publisher::bench(144, 20000) {
            eprintln!("erro: {e:#}");
        }
        return Ok(());
    }
    if !single::acquire() {
        return Ok(());
    }
    eframe::run_native(
        "Telacast",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([1280.0, 760.0])
                .with_min_inner_size([900.0, 520.0]),
            ..Default::default()
        },
        Box::new(|cc| {
            apply_theme(&cc.egui_ctx);
            single::serve(cc.egui_ctx.clone());
            Ok(Box::new(App::new(cc)))
        }),
    )
}

/// "agora", "há 5 min", "há 3 h", "há 2 dias" a partir de milissegundos.
fn ago(now_ms: i64, then_ms: i64) -> String {
    let m = (now_ms - then_ms).max(0) / 60_000;
    match m {
        0 => "agora".into(),
        1..=59 => format!("há {m} min"),
        60..=1439 => format!("há {} h", m / 60),
        _ => format!("há {} dias", m / 1440),
    }
}

// ------------------------------------------------------------------ estado

#[derive(Clone)]
enum View {
    Home,
    Dm(i64),
    Chan { server: i64, channel: i64 },
}

#[derive(PartialEq, Clone, Copy)]
enum HomeTab {
    Online,
    All,
    Pending,
    Blocked,
    Add,
}

enum Modal {
    None,
    AddServer { create: bool, name: String, code: String },
    NewChannel { server: i64, name: String, kind: u8, parent: Option<i64> },
    Invite(String),
    GoLive,
    DeleteMsg { id: i64, preview: String },
    ConfirmBlock { user_id: i64, name: String },
    Account { old: String, new: String, again: String },
    Preview(p::Attachment),
}

enum Act {
    Open(View),
    OpenServer(i64),
    Send(C2S),
    Modal(Modal),
    ToggleCategory(i64),
    Copy(String),
    Reply(i64, p::Reply),
    CancelReply,
    JumpTo(i64),
    SendMsg { ch: i64, content: String, reply: Option<p::Reply>, attachments: Vec<p::Attachment> },
    PickFile,
    AddFile(std::path::PathBuf),
    RemoveUpload(u64),
    Resend(u64),
    Discard(u64),
    ToggleSpoiler(i64, u32),
    SetStatus(p::Status),
    Search(bool),
    React { message: i64, emoji: String, add: bool },
    OpenPicker(i64, egui::Pos2),
    Pin { message: i64, pin: bool },
    OpenPins,
    ClosePins,
    Goto { channel: i64, id: i64 },
    CloseSearch,
    StartEdit(i64, String),
    CancelEdit,
    SaveEdit(i64, String),
    AskDelete(i64, String),
    Logout,
    OpenAccount,
    Watch(i64),
    StopWatch,
    Publish(i64),
    StopPublish,
    Fullscreen,
}

/// Estado da ligação com o servidor depois do login.
enum Link {
    Up,
    /// Caiu; tenta de novo em `retry_at`.
    Down { attempt: u32, retry_at: Instant, why: String },
    /// Conexão reaberta, esperando o servidor aceitar a sessão.
    Resuming { attempt: u32 },
}

struct Session {
    server: String,
    ctx: egui::Context,
    link: Link,
    gw: Gateway,
    token: String,
    me: p::User,
    snap: p::Snapshot,
    view: View,
    tab: HomeTab,
    msgs: HashMap<i64, Vec<p::Message>>,
    requested: HashSet<i64>,
    unread: HashSet<i64>,
    input: String,
    add_friend: String,
    modal: Modal,
    sessions: Vec<p::SessionInfo>,
    toast: Option<(String, Instant)>,
    last_channel: HashMap<i64, i64>,
    view_since: Instant,
    seen: HashMap<i64, Instant>,
    collapsed: HashSet<i64>,
    replying: Option<(i64, p::Reply)>,
    editing: Option<(i64, String)>,
    typing: HashMap<(i64, i64), Instant>,
    last_typing: Instant,
    focus_composer: bool,
    jump: Option<Jump>,
    pending: Vec<Pending>,
    nonce_seq: u64,
    /// Spoilers que o usuário abriu (mensagem, número do spoiler); só em memória, como no Discord.
    revealed: HashSet<(i64, u32)>,
    /// Já avisei o servidor que estou parado?
    idle_sent: bool,
    /// Menções ainda não lidas por canal (do snapshot, mais as que chegam com o canal fechado).
    mentions: HashMap<i64, u32>,
    /// Destinos visitados: (0 servidor, 1 canal, 2 usuário, id) -> vezes. Reforça a nota no Ctrl+K.
    visits: HashMap<(u8, i64), u32>,
    /// Canais e DMs visitados, o mais recente primeiro.
    recent: Vec<i64>,
    search: Option<SearchState>,
    /// Seletor de emoji aberto para uma mensagem: (id, posição na tela).
    picker: Option<(i64, egui::Pos2)>,
    /// Servidor HTTP dos anexos (o mesmo endereço do WebSocket, com http/https).
    http_base: String,
    images: files::Images,
    /// Arquivos sendo enviados para o campo de mensagem (por canal) e o canal por onde chegam os resultados.
    uploads: Vec<files::Upload>,
    upload_key: u64,
    upload_tx: std::sync::mpsc::Sender<(u64, Result<p::Attachment, String>)>,
    upload_rx: std::sync::mpsc::Receiver<(u64, Result<p::Attachment, String>)>,
    /// Caminhos escolhidos no seletor de arquivos (a janela roda em outra thread).
    picked_tx: std::sync::mpsc::Sender<std::path::PathBuf>,
    picked_rx: std::sync::mpsc::Receiver<std::path::PathBuf>,
    /// Painel de fixadas aberto e a lista do canal atual.
    pins_open: bool,
    pins: Vec<p::Message>,
    /// Ir até uma mensagem assim que o histórico do canal carregar: (canal, id).
    pending_goto: Option<(i64, i64)>,
    /// Divisor "novas mensagens": (canal, último id lido ao abrir, já rolei até ele).
    marker: Option<(i64, i64, bool)>,
    /// Canais com uma página de mensagens antigas a caminho.
    older: HashSet<i64>,
    /// Canais que ainda têm mensagens mais antigas no servidor (a última página veio cheia).
    has_more: HashMap<i64, bool>,
    /// Saltando até uma mensagem antiga: (canal, id procurado, páginas que ainda posso buscar).
    jump_wanted: Option<(i64, i64, u8)>,
    /// (canal, id e posição na tela da primeira mensagem da lista), para manter a rolagem ao carregar antigas.
    anchor: Option<(i64, i64, f32)>,
}

/// Painel de busca de mensagens (Ctrl+F).
struct SearchState {
    query: String,
    hits: Vec<p::SearchHit>,
    more: bool,
    loading: bool,
    appending: bool,
    nonce: u64,
    focus: bool,
    /// Já buscou ao menos uma vez (para distinguir "sem resultados" de "ainda não buscou").
    searched: bool,
    /// Buscar só no canal atual (em vez do servidor inteiro).
    channel_scope: bool,
}

/// Mensagem enviada que o servidor ainda não confirmou (aparece apagada na lista).
struct Pending {
    ch: i64,
    nonce: u64,
    content: String,
    reply_to: Option<i64>,
    reply: Option<p::Reply>,
    attachments: Vec<p::Attachment>,
    ts: i64,
    since: Instant,
}

impl Pending {
    fn provisional(&self, me: &p::User) -> p::Message {
        p::Message {
            id: -(self.nonce as i64),
            channel_id: self.ch,
            author_id: me.id,
            author: me.name.clone(),
            content: self.content.clone(),
            ts: self.ts,
            edited: false,
            reply: self.reply.clone(),
            nonce: self.nonce,
            reactions: Vec::new(),
            kind: 0,
            pinned: false,
            attachments: self.attachments.clone(),
        }
    }
}

/// Destaque temporário de uma mensagem para a qual o usuário saltou.
struct Jump {
    id: i64,
    since: Instant,
    scrolled: bool,
}

impl Session {
    fn new(gw: Gateway, server: String, ctx: egui::Context, token: String, me: p::User) -> Self {
        let http_base = files::http_base(&server);
        let (upload_tx, upload_rx) = std::sync::mpsc::channel();
        let (picked_tx, picked_rx) = std::sync::mpsc::channel();
        Self {
            server,
            ctx,
            link: Link::Up,
            gw,
            token,
            snap: p::Snapshot { me: me.clone(), servers: vec![], friends: vec![], dms: vec![], live: vec![], blocked: vec![] },
            me,
            view: View::Home,
            tab: HomeTab::Online,
            msgs: HashMap::new(),
            requested: HashSet::new(),
            unread: HashSet::new(),
            input: String::new(),
            add_friend: String::new(),
            modal: Modal::None,
            sessions: Vec::new(),
            toast: None,
            last_channel: HashMap::new(),
            view_since: Instant::now(),
            seen: HashMap::new(),
            collapsed: HashSet::new(),
            replying: None,
            editing: None,
            typing: HashMap::new(),
            last_typing: Instant::now() - Duration::from_secs(60),
            focus_composer: false,
            jump: None,
            pending: Vec::new(),
            nonce_seq: 0,
            revealed: HashSet::new(),
            idle_sent: false,
            mentions: HashMap::new(),
            visits: HashMap::new(),
            recent: Vec::new(),
            search: None,
            http_base,
            images: files::Images::new(),
            uploads: Vec::new(),
            upload_key: 0,
            upload_tx,
            upload_rx,
            picked_tx,
            picked_rx,
            picker: None,
            pins_open: false,
            pins: Vec::new(),
            pending_goto: None,
            marker: None,
            older: HashSet::new(),
            has_more: HashMap::new(),
            jump_wanted: None,
            anchor: None,
        }
    }

    fn current_channel(&self) -> Option<i64> {
        match self.view {
            View::Home => None,
            View::Dm(c) => Some(c),
            View::Chan { channel, .. } => Some(channel),
        }
    }

    fn open(&mut self, v: View) {
        if let View::Chan { server, channel } = &v {
            self.last_channel.insert(*server, *channel);
        }
        let ch = match &v {
            View::Home => None,
            View::Dm(c) => Some(*c),
            View::Chan { channel, .. } => Some(*channel),
        };
        if let Some(ch) = ch {
            // onde eu tinha parado de ler (para o divisor), antes de marcar como lido
            let (last, read) = self
                .snap
                .servers
                .iter()
                .flat_map(|s| s.channels.iter())
                .find(|c| c.id == ch)
                .map(|c| (c.last_message_id, c.last_read_id))
                .or_else(|| self.snap.dms.iter().find(|d| d.channel_id == ch).map(|d| (d.last_message_id, d.last_read_id)))
                .unwrap_or((0, 0));
            self.marker = (last > read).then_some((ch, read, false));
            // histórico e frequência para o Ctrl+K
            self.recent.retain(|c| *c != ch);
            self.recent.insert(0, ch);
            self.recent.truncate(20);
            match &v {
                View::Chan { server, channel } => {
                    *self.visits.entry((1, *channel)).or_insert(0) += 1;
                    *self.visits.entry((0, *server)).or_insert(0) += 1;
                }
                View::Dm(c) => {
                    if let Some(d) = self.snap.dms.iter().find(|d| d.channel_id == *c) {
                        *self.visits.entry((2, d.user.id)).or_insert(0) += 1;
                    }
                }
                View::Home => {}
            }
            self.unread.remove(&ch);
            self.mentions.remove(&ch);
            if self.pins_open {
                self.pins.clear();
                self.gw.send(C2S::Pins { channel_id: ch });
            }
            self.ack(ch);
            if self.requested.insert(ch) {
                self.gw.send(C2S::History { channel_id: ch, before: None });
            }
        }
        self.editing = None;
        self.view = v;
        self.view_since = Instant::now();
    }

    /// Id da última mensagem conhecida do canal (a do snapshot ou a mais nova já carregada).
    fn latest_id(&self, ch: i64) -> i64 {
        let snap = self
            .snap
            .servers
            .iter()
            .flat_map(|s| s.channels.iter())
            .find(|c| c.id == ch)
            .map(|c| c.last_message_id)
            .or_else(|| self.snap.dms.iter().find(|d| d.channel_id == ch).map(|d| d.last_message_id))
            .unwrap_or(0);
        let loaded = self.msgs.get(&ch).and_then(|v| v.last()).map(|m| m.id).unwrap_or(0);
        snap.max(loaded)
    }

    /// Mostra a mensagem na hora (provisória) e manda ao servidor; se a conexão estiver fora, fica na fila.
    fn send_message(&mut self, ch: i64, content: String, reply: Option<p::Reply>, attachments: Vec<p::Attachment>) {
        // "@nome" de quem está neste canal vira a menção `<@id>` (o chat mostra o nome atual)
        let users: Vec<&p::User> = match self.snap.servers.iter().find(|s| s.channels.iter().any(|c| c.id == ch)) {
            Some(sv) => sv.members.iter().collect(),
            None => self.snap.dms.iter().filter(|d| d.channel_id == ch).map(|d| &d.user).collect(),
        };
        let content = mention_tokens(&content, &users);
        self.nonce_seq += 1;
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(1);
        // nonce positivo e menor que 2^62; o id provisório é o nonce negativo
        let nonce = (((now_ms & 0xFFFF_FFFF_FFFF) << 8) | (self.nonce_seq & 0xFF)).max(1);
        let reply_to = reply.as_ref().map(|r| r.id);
        let pend = Pending { ch, nonce, content: content.clone(), reply_to, reply, attachments, ts: now_ms as i64, since: Instant::now() };
        if self.requested.contains(&ch) {
            let prov = pend.provisional(&self.me);
            self.msgs.entry(ch).or_default().push(prov);
        }
        if matches!(self.link, Link::Up) {
            let ids = pend.attachments.iter().map(|a| a.id).collect();
            self.gw.send(C2S::Send { channel_id: ch, content, reply_to, nonce, attachments: ids });
        }
        self.pending.push(pend);
    }

    /// Começa a enviar um arquivo para o canal aberto; o resultado volta por `upload_rx`.
    fn start_upload(&mut self, path: std::path::PathBuf) {
        let Some(channel) = self.current_channel() else { return };
        self.upload_key += 1;
        let key = self.upload_key;
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "arquivo".into());
        self.uploads.push(files::Upload { key, channel, name, path: path.clone(), state: files::UpState::Sending });
        let (base, token, tx, ctx) = (self.http_base.clone(), self.token.clone(), self.upload_tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send((key, files::upload(&base, &token, channel, &path)));
            ctx.request_repaint();
        });
    }

    /// Rola até a mensagem `id` do canal aberto; se não estiver carregada, busca páginas antigas.
    fn jump_to(&mut self, id: i64) {
        let found = self.current_channel().and_then(|c| self.msgs.get(&c)).is_some_and(|v| v.iter().any(|m| m.id == id));
        if found {
            self.jump = Some(Jump { id, since: Instant::now(), scrolled: false });
        } else if let Some(ch) = self.current_channel().filter(|c| self.has_more.get(c) == Some(&true)) {
            // vai buscando páginas antigas até achar (no máximo 10, ou 500 mensagens)
            self.jump_wanted = Some((ch, id, 10));
            self.load_older(ch);
        } else {
            self.toast = Some(("A mensagem não está mais disponível".into(), Instant::now()));
        }
    }

    /// Pede a página de mensagens anteriores à mais antiga já carregada.
    fn load_older(&mut self, ch: i64) {
        if self.older.contains(&ch) || self.has_more.get(&ch) != Some(&true) {
            return;
        }
        let Some(oldest) = self.msgs.get(&ch).and_then(|v| v.iter().find(|m| m.id > 0)).map(|m| m.id) else { return };
        self.older.insert(ch);
        self.gw.send(C2S::History { channel_id: ch, before: Some(oldest) });
    }

    fn resend_pending(&mut self) {
        for p in &mut self.pending {
            p.since = Instant::now();
            let ids = p.attachments.iter().map(|a| a.id).collect();
            self.gw.send(C2S::Send { channel_id: p.ch, content: p.content.clone(), reply_to: p.reply_to, nonce: p.nonce, attachments: ids });
        }
    }

    /// Avisa o servidor que o canal foi lido até a última mensagem conhecida.
    fn ack(&mut self, ch: i64) {
        let id = self.latest_id(ch);
        if id > 0 {
            self.gw.send(C2S::Ack { channel_id: ch, message_id: id });
        }
    }

    fn open_server(&mut self, id: i64) {
        let Some(sv) = self.snap.servers.iter().find(|s| s.id == id) else { return };
        let wanted = self.last_channel.get(&id).copied();
        let ch = sv
            .channels
            .iter()
            .find(|c| c.kind == 0 && Some(c.id) == wanted)
            .or(sv.channels.iter().find(|c| c.kind == 0))
            .map(|c| c.id);
        if let Some(channel) = ch {
            self.open(View::Chan { server: id, channel });
        }
    }

    fn fix_view(&mut self) {
        let ok = match &self.view {
            View::Home => true,
            View::Dm(c) => self.snap.dms.iter().any(|d| d.channel_id == *c),
            View::Chan { server, channel } => self
                .snap
                .servers
                .iter()
                .any(|s| s.id == *server && s.channels.iter().any(|c| c.id == *channel)),
        };
        if !ok {
            self.view = View::Home;
        }
    }

    fn name_of(&self, id: i64) -> String {
        if id == self.me.id {
            return self.me.name.clone();
        }
        self.snap
            .servers
            .iter()
            .flat_map(|s| s.members.iter())
            .chain(self.snap.friends.iter().map(|f| &f.user))
            .chain(self.snap.dms.iter().map(|d| &d.user))
            .find(|u| u.id == id)
            .map(|u| u.name.clone())
            .unwrap_or_else(|| "alguém".into())
    }

    /// Processa eventos do servidor. Devolve o motivo se a conexão caiu.
    fn pump(&mut self) -> Option<String> {
        // hora de tentar de novo?
        if let Link::Down { attempt, retry_at, .. } = &self.link {
            if Instant::now() >= *retry_at {
                let attempt = *attempt;
                self.gw = gateway::open(&self.server, self.ctx.clone());
                self.gw.send(C2S::Resume { token: self.token.clone(), proto: p::PROTO_VERSION });
                self.link = Link::Resuming { attempt };
            } else {
                self.ctx.request_repaint_after(Duration::from_millis(500));
            }
        }
        while let Ok((key, res)) = self.upload_rx.try_recv() {
            if let Some(u) = self.uploads.iter_mut().find(|u| u.key == key) {
                u.state = match res {
                    Ok(a) => files::UpState::Done(a),
                    Err(e) => files::UpState::Failed(e),
                };
            }
        }
        while let Ok(path) = self.picked_rx.try_recv() {
            self.start_upload(path);
        }
        while let Ok(ev) = self.gw.rx.try_recv() {
            match ev {
                Event::Closed(why) => {
                    let attempt = match &self.link {
                        Link::Up => 1,
                        Link::Down { attempt, .. } | Link::Resuming { attempt } => attempt + 1,
                    };
                    // 1, 2, 4, 8 e no máximo 15 segundos entre tentativas
                    let wait = (1u64 << (attempt - 1).min(4)).min(15);
                    self.link = Link::Down { attempt, retry_at: Instant::now() + Duration::from_secs(wait), why };
                    self.ctx.request_repaint_after(Duration::from_millis(500));
                    break;
                }
                Event::Connected => {}
                Event::Msg(S2C::Error { code, msg }) if matches!(self.link, Link::Resuming { .. }) => {
                    use p::ErrorCode::*;
                    if matches!(code, SessionExpired | NotAuthenticated | VersionMismatch | InvalidCredentials) {
                        // a sessão não vale mais (ou o app ficou velho demais): volta para o login
                        return Some(msg);
                    }
                    // erro passageiro (limite, falha interna): tenta de novo mais tarde
                    let attempt = match &self.link {
                        Link::Resuming { attempt } => attempt + 1,
                        _ => 1,
                    };
                    let wait = (1u64 << (attempt - 1).min(4)).min(15);
                    self.link = Link::Down { attempt, retry_at: Instant::now() + Duration::from_secs(wait), why: msg };
                    break;
                }
                Event::Msg(S2C::Auth { .. }) if matches!(self.link, Link::Resuming { .. }) => {
                    self.link = Link::Up;
                    // recarrega o canal aberto: pode ter perdido mensagens enquanto estava fora
                    let cur = self.current_channel();
                    self.requested.clear();
                    if let Some(ch) = cur {
                        self.requested.insert(ch);
                        self.gw.send(C2S::History { channel_id: ch, before: None });
                    }
                    // o servidor descarta o que já recebeu (mesmo nonce); o resto sai agora
                    self.resend_pending();
                    self.idle_sent = false; // a conexão nova começa "ativa" no servidor
                }
                Event::Msg(m) => match m {
                    S2C::Snapshot(sn) => {
                        self.me = sn.me.clone();
                        // não lido = há mensagem mais nova que a última lida (fora o canal aberto)
                        let cur = self.current_channel();
                        self.unread = sn
                            .servers
                            .iter()
                            .flat_map(|s| s.channels.iter())
                            .filter(|c| c.kind == 0)
                            .map(|c| (c.id, c.last_message_id, c.last_read_id))
                            .chain(sn.dms.iter().map(|d| (d.channel_id, d.last_message_id, d.last_read_id)))
                            .filter(|(id, last, read)| last > read && Some(*id) != cur)
                            .map(|(id, _, _)| id)
                            .collect();
                        self.mentions = sn
                            .servers
                            .iter()
                            .flat_map(|s| s.channels.iter().map(|c| (c.id, c.mentions)))
                            .chain(sn.dms.iter().map(|d| (d.channel_id, d.mentions)))
                            .filter(|(id, n)| *n > 0 && Some(*id) != cur)
                            .collect();
                        self.snap = sn;
                        self.fix_view();
                    }
                    S2C::Message(m) => {
                        let ch = m.channel_id;
                        let mine = m.author_id == self.me.id;
                        let (id, author) = (m.id, m.author_id);
                        let content_for_mention = m.content.clone();
                        if mine && m.nonce != 0 {
                            self.pending.retain(|p| p.nonce != m.nonce);
                            if let Some(v) = self.msgs.get_mut(&ch) {
                                v.retain(|x| x.id != -(m.nonce as i64));
                            }
                        }
                        if self.requested.contains(&ch) {
                            self.msgs.entry(ch).or_default().push(m);
                        }
                        self.typing.remove(&(ch, author));
                        if !mine {
                            if self.current_channel() == Some(ch) {
                                self.gw.send(C2S::Ack { channel_id: ch, message_id: id });
                            } else {
                                self.unread.insert(ch);
                                if mentions_me(&content_for_mention, self.me.id) {
                                    *self.mentions.entry(ch).or_insert(0) += 1;
                                }
                            }
                        }
                    }
                    S2C::SearchResults { nonce, hits, more } => {
                        if let Some(st) = &mut self.search {
                            if st.nonce == nonce {
                                if st.appending {
                                    st.hits.extend(hits);
                                } else {
                                    st.hits = hits;
                                }
                                st.more = more;
                                st.loading = false;
                                st.searched = true;
                            }
                        }
                    }
                    S2C::Pinned { channel_id, message_id, pinned } => {
                        if let Some(msg) = self.msgs.get_mut(&channel_id).and_then(|v| v.iter_mut().find(|m| m.id == message_id)) {
                            msg.pinned = pinned;
                        }
                        if self.pins_open && self.current_channel() == Some(channel_id) {
                            self.gw.send(C2S::Pins { channel_id });
                        }
                    }
                    S2C::Pins { channel_id, messages } => {
                        if self.current_channel() == Some(channel_id) {
                            self.pins = messages;
                        }
                    }
                    S2C::Reaction { channel_id, message_id, emoji, user_id, add } => {
                        let mine = user_id == self.me.id;
                        if let Some(msg) = self.msgs.get_mut(&channel_id).and_then(|v| v.iter_mut().find(|m| m.id == message_id)) {
                            match msg.reactions.iter().position(|r| r.emoji == emoji) {
                                Some(i) => {
                                    let r = &mut msg.reactions[i];
                                    if add {
                                        r.count += 1;
                                        r.me |= mine;
                                    } else {
                                        r.count = r.count.saturating_sub(1);
                                        if mine {
                                            r.me = false;
                                        }
                                        if r.count == 0 {
                                            msg.reactions.remove(i);
                                        }
                                    }
                                }
                                None if add => msg.reactions.push(p::Reaction { emoji, count: 1, me: mine }),
                                None => {}
                            }
                        }
                    }
                    S2C::MessageEdited { channel_id, message_id, content } => {
                        if let Some(m) = self.msgs.get_mut(&channel_id).and_then(|v| v.iter_mut().find(|m| m.id == message_id)) {
                            m.content = content;
                            m.edited = true;
                        }
                    }
                    S2C::MessageDeleted { channel_id, message_id } => {
                        self.pins.retain(|m| m.id != message_id);
                        if let Some(v) = self.msgs.get_mut(&channel_id) {
                            v.retain(|m| m.id != message_id);
                        }
                    }
                    S2C::Typing { channel_id, user_id } => {
                        self.typing.insert((channel_id, user_id), Instant::now());
                    }
                    S2C::History { channel_id, messages } if self.older.remove(&channel_id) => {
                        // página mais antiga: entra na frente, sem repetir ids
                        let page = messages.len();
                        let cur = self.msgs.entry(channel_id).or_default();
                        let have: HashSet<i64> = cur.iter().map(|m| m.id).collect();
                        let mut front: Vec<p::Message> = messages.into_iter().filter(|m| !have.contains(&m.id)).collect();
                        front.append(cur);
                        *cur = front;
                        self.has_more.insert(channel_id, page >= 50);
                        // estava procurando uma mensagem antiga (resposta a algo fora da tela)?
                        if let Some((jch, id, pages)) = self.jump_wanted {
                            if jch == channel_id {
                                let found = self.msgs.get(&jch).is_some_and(|v| v.iter().any(|m| m.id == id));
                                if found {
                                    self.jump = Some(Jump { id, since: Instant::now(), scrolled: false });
                                    self.jump_wanted = None;
                                } else if pages > 0 && page >= 50 {
                                    self.jump_wanted = Some((jch, id, pages - 1));
                                    self.load_older(jch);
                                } else {
                                    self.jump_wanted = None;
                                    self.toast = Some(("Não achei a mensagem original; ela é muito antiga".into(), Instant::now()));
                                }
                            }
                        }
                    }
                    S2C::History { channel_id, messages } => {
                        self.has_more.insert(channel_id, messages.len() >= 50);
                        let mut v = messages;
                        let me = self.me.clone();
                        for p in self.pending.iter().filter(|p| p.ch == channel_id) {
                            if !v.iter().any(|m| m.author_id == me.id && m.nonce == p.nonce) {
                                v.push(p.provisional(&me));
                            }
                        }
                        self.msgs.insert(channel_id, v);
                        // abri o canal vindo da busca: agora que há mensagens, vai até a escolhida
                        if let Some((gch, gid)) = self.pending_goto {
                            if gch == channel_id {
                                self.pending_goto = None;
                                self.jump_to(gid);
                            }
                        }
                    }
                    S2C::DmOpened { channel_id } => self.open(View::Dm(channel_id)),
                    S2C::ServerJoined { server_id } => self.open_server(server_id),
                    S2C::Error { msg, .. } => self.toast = Some((msg, Instant::now())),
                    S2C::Sessions { list } => self.sessions = list,
                    S2C::PasswordChanged => {
                        if matches!(self.modal, Modal::Account { .. }) {
                            self.modal = Modal::Account { old: String::new(), new: String::new(), again: String::new() };
                        }
                        self.toast = Some(("Senha alterada. As outras sessões foram encerradas.".into(), Instant::now()));
                    }
                    S2C::Auth { .. } => {}
                },
            }
        }
        None
    }
}

struct Login {
    server: String,
    user: String,
    pass: String,
    invite: String,
    register: bool,
    error: String,
    gw: Option<Gateway>,
    pending: Option<C2S>,
    resuming: bool,
}

struct App {
    login: Login,
    session: Option<Session>,
    token_saved: String,
    fps: u32,
    kbps: u32,
    monitors: Vec<String>,
    monitor: usize,
    publishing: Option<(i64, publisher::Handle)>,
    watching: Option<(i64, viewer::Handle)>,
    texture: Option<egui::TextureHandle>,
    fullscreen: bool,
    /// Última atividade do usuário (relógio de parede: suspender o PC conta como tempo parado).
    qs: Option<quick::Qs>,
    last_input: std::time::SystemTime,
    /// Tempo parado até virar "ausente" (10 min; `TELACAST_IDLE_SECS` encurta para testar).
    idle_after: Duration,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let get = |k: &str| cc.storage.and_then(|s| s.get_string(k)).unwrap_or_default();
        let mut login = Login {
            server: {
                let s = get("server");
                if s.is_empty() { "ws://127.0.0.1:8080".into() } else { s }
            },
            user: get("user"),
            pass: String::new(),
            invite: String::new(),
            register: false,
            error: String::new(),
            gw: None,
            pending: None,
            resuming: false,
        };
        let token = get("token");
        if !token.is_empty() && net::insecure_remote(&login.server) {
            login.error = "Servidor remoto precisa de wss:// (conexão criptografada).".into();
        } else if !token.is_empty() {
            login.gw = Some(gateway::open(&login.server, cc.egui_ctx.clone()));
            login.pending = Some(C2S::Resume { token: token.clone(), proto: p::PROTO_VERSION });
            login.resuming = true;
        }
        Self {
            login,
            session: None,
            token_saved: token,
            fps: get("fps").parse().unwrap_or(60),
            kbps: get("kbps").parse().unwrap_or(8000),
            monitors: publisher::monitor_names(),
            monitor: 0,
            publishing: None,
            watching: None,
            texture: None,
            fullscreen: false,
            qs: None,
            last_input: std::time::SystemTime::now(),
            idle_after: Duration::from_secs(
                std::env::var("TELACAST_IDLE_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(600),
            ),
        }
    }

    fn start_login(&mut self, ctx: &egui::Context) {
        let l = &mut self.login;
        l.error.clear();
        if net::insecure_remote(&l.server) {
            l.error = "Para um servidor fora da sua rede use wss:// (conexão criptografada). Com ws:// a senha viajaria em claro.".into();
            return;
        }
        l.gw = Some(gateway::open(&l.server, ctx.clone()));
        let (username, password, invite) = (l.user.trim().to_string(), l.pass.clone(), l.invite.clone());
        l.pending = Some(if l.register {
            C2S::Register { username, password, invite, proto: p::PROTO_VERSION }
        } else {
            C2S::Login { username, password, proto: p::PROTO_VERSION }
        });
    }

    fn poll_login(&mut self, ctx: &egui::Context) {
        loop {
            let Some(ev) = self.login.gw.as_ref().and_then(|g| g.rx.try_recv().ok()) else { break };
            match ev {
                Event::Connected => {
                    if let (Some(gw), Some(m)) = (&self.login.gw, self.login.pending.take()) {
                        gw.send(m);
                    }
                }
                Event::Msg(S2C::Auth { token, me }) => {
                    let gw = self.login.gw.take().expect("gateway");
                    self.token_saved = token.clone();
                    self.login.pass.clear();
                    self.login.error.clear();
                    self.login.resuming = false;
                    self.session = Some(Session::new(gw, self.login.server.clone(), ctx.clone(), token, me));
                    return;
                }
                Event::Msg(S2C::Error { code, msg }) => {
                    if self.login.resuming {
                        // só descarta a sessão salva se o servidor disse que ela não vale mais
                        if matches!(code, p::ErrorCode::SessionExpired | p::ErrorCode::NotAuthenticated | p::ErrorCode::VersionMismatch) {
                            self.token_saved.clear();
                        } else {
                            self.login.error = msg;
                        }
                    } else {
                        self.login.error = msg;
                    }
                    self.login.resuming = false;
                    self.login.gw = None;
                }
                Event::Closed(why) => {
                    // mostra o motivo também ao retomar a sessão (certificado recusado, servidor fora...)
                    self.login.error = why;
                    self.login.resuming = false;
                    self.login.gw = None;
                }
                Event::Msg(_) => {}
            }
        }
    }

    fn logout(&mut self, reason: &str) {
        self.session = None;
        self.token_saved.clear();
        self.stop_publish();
        self.stop_watch();
        self.fullscreen = false;
        self.login.error = reason.to_string();
    }

    fn stop_publish(&mut self) {
        if let Some((_, h)) = self.publishing.take() {
            h.stop.store(true, Ordering::Relaxed);
        }
    }

    fn stop_watch(&mut self) {
        if let Some((_, h)) = self.watching.take() {
            h.stop.store(true, Ordering::Relaxed);
        }
        self.texture = None;
    }
}

impl eframe::App for App {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string("server", self.login.server.clone());
        storage.set_string("user", self.login.user.clone());
        storage.set_string("token", self.token_saved.clone());
        storage.set_string("fps", self.fps.to_string());
        storage.set_string("kbps", self.kbps.to_string());
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if single::take_raise() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        self.poll_login(&ctx);
        match self.session.take() {
            Some(mut s) => {
                if let Some(why) = s.pump() {
                    self.logout(&why);
                    self.login_ui(ui, &ctx);
                } else {
                    let keep = self.session_ui(ui, &ctx, &mut s);
                    if keep {
                        self.session = Some(s);
                    } else {
                        self.logout("");
                    }
                }
            }
            None => self.login_ui(ui, &ctx),
        }
    }
}

// ------------------------------------------------------------------ login

impl App {
    fn login_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        egui::CentralPanel::default().frame(egui::Frame::new().fill(BG)).show(ui, |ui| {
            let busy = self.login.gw.is_some();
            let top = ((ui.available_height() - 470.0) / 2.0).max(10.0);
            ui.vertical_centered(|ui| {
                ui.add_space(top);
                glass(14)
                    .inner_margin(egui::Margin::same(32))
                    .show(ui, |ui| {
                        ui.set_width(340.0);
                        ui.label(serif("Telacast", 34.0));
                        ui.label(
                            RichText::new(if self.login.resuming {
                                "Entrando…"
                            } else if self.login.register {
                                "Crie sua conta"
                            } else {
                                "Que bom te ver de novo!"
                            })
                            .color(MUTED),
                        );
                        ui.add_space(8.0);
                        ui.with_layout(Layout::top_down(Align::Min), |ui| {
                            ui.label(caps(&"SERVIDOR"));
                            ui.add(egui::TextEdit::singleline(&mut self.login.server).desired_width(f32::INFINITY));
                            ui.label(caps(&"NOME DE USUÁRIO"));
                            ui.add(egui::TextEdit::singleline(&mut self.login.user).desired_width(f32::INFINITY));
                            ui.label(caps(&"SENHA"));
                            let pw = ui.add(
                                egui::TextEdit::singleline(&mut self.login.pass)
                                    .password(true)
                                    .desired_width(f32::INFINITY),
                            );
                            let mut enter = pw.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter));
                            if self.login.register {
                                ui.label(caps(&"CÓDIGO DE CONVITE"));
                                let inv = ui.add(egui::TextEdit::singleline(&mut self.login.invite).desired_width(f32::INFINITY));
                                enter |= inv.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter));
                            }
                            if !self.login.error.is_empty() {
                                ui.label(RichText::new(&self.login.error).color(RED));
                            }
                            ui.add_space(6.0);
                            let label = if busy {
                                "Conectando…"
                            } else if self.login.register {
                                "Criar conta"
                            } else {
                                "Entrar"
                            };
                            if (big_button(ui, label, BLURPLE) || enter) && !busy {
                                self.start_login(ctx);
                            }
                            ui.add_space(4.0);
                            let toggle = if self.login.register {
                                "Já tem uma conta? Entrar"
                            } else {
                                "Precisa de uma conta? Registre-se"
                            };
                            if ui.link(toggle).clicked() {
                                self.login.register = !self.login.register;
                                self.login.error.clear();
                            }
                        });
                    });
            });
        });
    }
}

// ------------------------------------------------------------------ sessão

/// Sigla do servidor: primeira letra de cada palavra, sem forçar maiúscula (regra do Discord).
/// Consome um Enter sem Shift (Shift+Enter continua quebrando linha).
/// Separa a consulta em texto + filtros: `de:nome` (ou `de:eu`) e `em:#canal`. O resto é o texto.
/// Devolve (texto, autor, canal).
fn parse_search(query: &str, snap: &p::Snapshot, view: &View, me: i64) -> (String, Option<i64>, Option<i64>) {
    let members: Vec<&p::User> = match view {
        View::Chan { server, .. } => snap.servers.iter().find(|s| s.id == *server).map(|s| s.members.iter().collect()).unwrap_or_default(),
        View::Dm(c) => snap.dms.iter().filter(|d| d.channel_id == *c).map(|d| &d.user).collect(),
        View::Home => Vec::new(),
    };
    let channels: Vec<&p::Channel> = match view {
        View::Chan { server, .. } => snap.servers.iter().find(|s| s.id == *server).map(|s| s.channels.iter().filter(|c| c.kind == 0).collect()).unwrap_or_default(),
        _ => Vec::new(),
    };
    let (mut from, mut chan) = (None, None);
    let mut text: Vec<&str> = Vec::new();
    for tok in query.split_whitespace() {
        let low = tok.to_lowercase();
        if let Some(v) = low.strip_prefix("de:") {
            let who = if v == "eu" { Some(me) } else { members.iter().find(|u| u.name.to_lowercase() == v).map(|u| u.id) };
            if who.is_some() {
                from = who;
                continue;
            }
        }
        if let Some(v) = low.strip_prefix("em:") {
            let v = v.trim_start_matches('#');
            if let Some(c) = channels.iter().find(|c| c.name.to_lowercase() == v) {
                chan = Some(c.id);
                continue;
            }
        }
        text.push(tok);
    }
    (text.join(" "), from, chan)
}

/// Aviso de sistema numa linha só: "fulano entrou…" ou "fulano fixou uma mensagem…".
fn system_row(ui: &mut egui::Ui, m: &p::Message, acts: &mut Vec<Act>) {
    const JOIN: [&str; 6] =
        ["entrou no servidor.", "chegou.", "acabou de aparecer!", "pulou para o servidor.", "acabou de aterrissar.", "chegou por aqui. Diga oi!"];
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(egui::vec2(46.0, 20.0), egui::Sense::hover());
        let c = r.center() + egui::vec2(-3.0, 0.0);
        let p = ui.painter();
        if m.kind == 2 {
            // seta para dentro
            let st = egui::Stroke::new(1.6, OK);
            p.line_segment([c + egui::vec2(-6.0, 0.0), c + egui::vec2(5.0, 0.0)], st);
            p.line_segment([c + egui::vec2(1.0, -4.0), c + egui::vec2(5.0, 0.0)], st);
            p.line_segment([c + egui::vec2(1.0, 4.0), c + egui::vec2(5.0, 0.0)], st);
        } else {
            // alfinete
            let st = egui::Stroke::new(1.6, AMBER);
            p.circle_filled(c + egui::vec2(0.0, -3.0), 4.0, AMBER);
            p.line_segment([c + egui::vec2(0.0, 0.0), c + egui::vec2(0.0, 7.0)], st);
        }
        ui.spacing_mut().item_spacing.x = 5.0;
        ui.label(RichText::new(&m.author).strong().color(TEXT));
        if m.kind == 2 {
            ui.label(RichText::new(JOIN[(m.id as usize) % JOIN.len()]).color(TEXT2));
        } else {
            ui.label(RichText::new("fixou uma mensagem neste canal.").color(TEXT2));
            let r = ui.add(egui::Label::new(RichText::new("Ver todas as mensagens fixadas").color(AMBER).underline()).sense(egui::Sense::click()));
            if r.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if r.clicked() {
                acts.push(Act::OpenPins);
            }
        }
        ui.label(RichText::new(fmt_clock(m.ts)).size(11.5).color(MUTED));
    });
    ui.add_space(6.0);
}

/// Anexo de uma mensagem: imagem (até 400x300) ou cartão do arquivo; clicar abre no navegador.
fn attachment_view(ui: &mut egui::Ui, a: &p::Attachment, base: &str, token: &str, images: &mut files::Images, acts: &mut Vec<Act>) {
    let open = |ui: &egui::Ui| ui.ctx().open_url(egui::OpenUrl::new_tab(files::url(base, a, token)));
    ui.add_space(4.0);
    if a.mime.starts_with("image/") && a.width > 0 && a.height > 0 {
        let scale = (400.0 / a.width as f32).min(300.0 / a.height as f32).min(1.0);
        let size = egui::vec2(a.width as f32 * scale, a.height as f32 * scale).max(egui::vec2(40.0, 40.0));
        let ctx = ui.ctx().clone();
        match images.get(&ctx, base, token, a) {
            files::Img::Ready(tex) => {
                let r = ui.add(egui::Image::new(tex).fit_to_exact_size(size).corner_radius(egui::CornerRadius::same(8)).sense(egui::Sense::click()));
                if r.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if r.clicked() {
                    acts.push(Act::Modal(Modal::Preview(a.clone())));
                }
                return;
            }
            files::Img::Loading => {
                let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
                ui.painter().rect_filled(rect, 8.0, L3);
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, "Carregando…", egui::FontId::proportional(13.0), MUTED);
                ui.ctx().request_repaint_after(Duration::from_millis(300));
                return;
            }
            files::Img::Failed => {}
        }
    }
    // arquivo comum (ou imagem que não carregou)
    let card = egui::Frame::new()
        .fill(L3)
        .stroke(egui::Stroke::new(1.0, LINE2))
        .corner_radius(egui::CornerRadius::same(10))
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_max_width(360.0);
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(RichText::new(&a.name).color(AMBER).strong());
                    ui.label(RichText::new(files::human_size(a.size)).size(12.0).color(MUTED));
                });
                ui.add_space(14.0);
                if small_button(ui, "Baixar", FIELD) {
                    open(ui);
                }
            });
        });
    let _ = (card, acts);
}

/// Tile de anexo pendente (120x120): miniatura "cobrir" com raio 8, nome 12 px embaixo,
/// lixeira no hover, barra âmbar enquanto envia.
fn upload_tile(ui: &mut egui::Ui, u: &files::Upload, images: &mut files::Images, acts: &mut Vec<Act>) {
    let ext = std::path::Path::new(&u.name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let is_img = matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "gif" | "webp");
    let failed = matches!(u.state, files::UpState::Failed(_));
    let ctx = ui.ctx().clone();
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 4.0;
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(120.0, 120.0), egui::Sense::hover());
        let mut shown = false;
        if is_img {
            match images.get_local(&ctx, u.key, &u.path) {
                files::Img::Ready(tex) => {
                    let sz = tex.size_vec2();
                    // "cobrir": corta o excesso do lado maior, centralizado
                    let (kx, ky) = if sz.x > sz.y { (sz.y / sz.x, 1.0) } else { (1.0, sz.x / sz.y) };
                    let uv = egui::Rect::from_min_max(egui::pos2((1.0 - kx) / 2.0, (1.0 - ky) / 2.0), egui::pos2((1.0 + kx) / 2.0, (1.0 + ky) / 2.0));
                    egui::Image::new(tex).uv(uv).corner_radius(egui::CornerRadius::same(8)).paint_at(ui, rect);
                    shown = true;
                }
                files::Img::Loading => ctx.request_repaint_after(Duration::from_millis(200)),
                files::Img::Failed => {}
            }
        }
        let p = ui.painter();
        if !shown {
            p.rect_filled(rect, 8.0, L2);
            p.rect_stroke(rect, 8.0, egui::Stroke::new(1.0, LINE2), egui::StrokeKind::Inside);
            let label = if ext.is_empty() { "ARQ".to_string() } else { ext.to_uppercase().chars().take(5).collect() };
            p.text(rect.center(), Align2::CENTER_CENTER, label, egui::FontId::proportional(20.0), TEXT2);
        }
        match &u.state {
            files::UpState::Sending => {
                p.rect_filled(rect, 8.0, egui::Color32::from_black_alpha(90));
                // sem progresso real do envio: uma barra de 3 px corre pelo pé do tile
                let t = ctx.input(|i| i.time) as f32;
                let f = (t * 0.8).fract();
                let (w, x0) = (rect.width() * 0.4, rect.left() - rect.width() * 0.4 + f * rect.width() * 1.4);
                let bar = egui::Rect::from_min_max(egui::pos2(x0.max(rect.left()), rect.bottom() - 3.0), egui::pos2((x0 + w).min(rect.right()), rect.bottom()));
                if bar.width() > 0.0 {
                    p.rect_filled(bar, 0.0, AMBER);
                }
                ctx.request_repaint_after(Duration::from_millis(33));
            }
            files::UpState::Failed(_) => {
                p.rect_filled(rect, 8.0, egui::Color32::from_black_alpha(150));
                p.rect_stroke(rect, 8.0, egui::Stroke::new(1.0, RED), egui::StrokeKind::Inside);
                p.text(rect.center(), Align2::CENTER_CENTER, "Falhou", egui::FontId::proportional(13.0), RED);
            }
            files::UpState::Done(_) => {}
        }
        // lixeira no hover (canto superior direito)
        let pill = egui::Rect::from_min_size(egui::pos2(rect.right() - 34.0, rect.top() + 6.0), egui::vec2(28.0, 28.0));
        let over = resp.hovered() || ui.rect_contains_pointer(pill);
        if over {
            let r = ui.interact(pill, egui::Id::new(("rm-upload", u.key)), egui::Sense::click());
            let p = ui.painter();
            p.rect_filled(pill, 8.0, if r.hovered() { L4 } else { L3 });
            p.rect_stroke(pill, 8.0, egui::Stroke::new(1.0, LINE2), egui::StrokeKind::Inside);
            let (c, st) = (pill.center(), egui::Stroke::new(1.5, RED));
            p.line_segment([c + egui::vec2(-6.0, -4.0), c + egui::vec2(6.0, -4.0)], st);
            p.line_segment([c + egui::vec2(-2.0, -6.5), c + egui::vec2(2.0, -6.5)], st);
            p.rect_stroke(egui::Rect::from_min_max(c + egui::vec2(-4.5, -3.0), c + egui::vec2(4.5, 6.5)), 1.5, st, egui::StrokeKind::Inside);
            if r.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if r.clicked() {
                acts.push(Act::RemoveUpload(u.key));
            }
        }
        let cap: String = if u.name.chars().count() > 17 { format!("{}…", u.name.chars().take(16).collect::<String>()) } else { u.name.clone() };
        let tip = match &u.state {
            files::UpState::Failed(e) => format!("{}\n{e}", u.name),
            _ => u.name.clone(),
        };
        ui.add_sized([120.0, 14.0], egui::Label::new(RichText::new(cap).size(12.0).color(if failed { RED } else { MUTED })).truncate()).on_hover_text(tip);
    });
}

fn is_pending_or_own_echo(m: &p::Message, me: i64) -> bool {
    m.id < 0 || (m.author_id == me && m.nonce != 0)
}

/// A mensagem cita `me` (`<@id>`, `<@!id>`) ou todo mundo (`@everyone`, `@here`)?
fn mentions_me(content: &str, me: i64) -> bool {
    content.contains(&format!("<@{me}>")) || content.contains(&format!("<@!{me}>")) || content.contains("@everyone") || content.contains("@here")
}

/// Troca `@nome` (nome exato de alguém do canal, sem diferenciar maiúsculas) por `<@id>`.
fn mention_tokens(content: &str, users: &[&p::User]) -> String {
    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(at) = rest.find('@') {
        out.push_str(&rest[..at]);
        let before_ok = !rest[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric());
        let after = &rest[at + 1..];
        let run: String = after.chars().take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '.' | '-')).collect();
        // o nome mais longo que casa
        let hit = users
            .iter()
            .filter(|u| after.to_lowercase().starts_with(&u.name.to_lowercase()))
            .filter(|u| after[u.name.len().min(after.len())..].chars().next().map_or(true, |c| !c.is_alphanumeric()))
            .max_by_key(|u| u.name.len());
        match hit {
            Some(u) if before_ok && !run.is_empty() => {
                out.push_str(&format!("<@{}>", u.id));
                rest = &after[u.name.len()..];
            }
            _ => {
                out.push('@');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn take_plain_enter(ctx: &egui::Context) -> bool {
    ctx.input_mut(|i| {
        let pos = i.events.iter().position(|e| {
            matches!(e, egui::Event::Key { key: egui::Key::Enter, pressed: true, modifiers, .. } if !modifiers.shift && !modifiers.alt)
        });
        match pos {
            Some(p) => {
                i.events.remove(p);
                true
            }
            None => false,
        }
    })
}

fn initials(name: &str) -> String {
    let s: String = name
        .replace("'s ", " ")
        .split(|c: char| !c.is_alphanumeric())
        .filter_map(|w| w.chars().next())
        .collect();
    if s.is_empty() { "?".into() } else { s }
}

impl App {
    /// Desenha a sessão. Devolve false se o usuário saiu.
    fn session_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, s: &mut Session) -> bool {
        let mut acts: Vec<Act> = Vec::new();

        s.images.pump(ctx);
        // arrastar arquivos para a janela anexa ao canal aberto
        if !matches!(s.view, View::Home) {
            let dropped = ctx.input(|i| i.raw.dropped_files.clone());
            for f in dropped {
                acts.push(Act::AddFile(f.path().to_path_buf()));
            }
            if ctx.input(|i| !i.raw.hovered_files.is_empty()) {
                let screen = ctx.content_rect();
                let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("drop-hint")));
                p.rect_filled(screen, 0.0, Color32::from_black_alpha(170));
                p.text(screen.center(), Align2::CENTER_CENTER, "Solte para anexar", egui::FontId::new(26.0, serif_family()), TEXT);
            }
            // gancho de teste: anexa esse arquivo uma vez, ao abrir um canal
            if let Ok(path) = std::env::var("TELACAST_ATTACH") {
                if s.uploads.is_empty() && s.upload_key == 0 && s.current_channel().is_some() {
                    acts.push(Act::AddFile(path.into()));
                }
            }
        }

        // Ctrl+F abre e fecha o painel de busca (só dentro de um canal ou conversa)
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) && !matches!(s.view, View::Home) {
            s.pins_open = false;
            s.search = match s.search {
                Some(_) => None,
                None => Some(SearchState {
                    query: String::new(),
                    hits: Vec::new(),
                    more: false,
                    loading: false,
                    appending: false,
                    nonce: 0,
                    focus: true,
                    searched: false,
                    channel_scope: false,
                }),
            };
        }

        // Ctrl+K abre e fecha o seletor rápido (consumido antes de qualquer campo de texto)
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::K)) {
            self.qs = if self.qs.is_some() { None } else { Some(quick::Qs::new()) };
        }

        // ausente automático: sem nenhuma entrada por `idle_after`, avisa o servidor; ao voltar, avisa de novo
        if ctx.input(|i| !i.events.is_empty()) {
            self.last_input = std::time::SystemTime::now();
        }
        let parado = self.last_input.elapsed().unwrap_or_default();
        let idle_now = parado >= self.idle_after;
        if idle_now != s.idle_sent && matches!(s.link, Link::Up) {
            s.idle_sent = idle_now;
            s.gw.send(C2S::SetIdle { idle: idle_now });
        }
        if !idle_now {
            ctx.request_repaint_after(self.idle_after.saturating_sub(parado) + Duration::from_millis(200));
        }

        // faixa de conexão: aparece só enquanto a ligação com o servidor está fora
        if !self.fullscreen && !matches!(s.link, Link::Up) {
            egui::Panel::top("link")
                .frame(egui::Frame::new().fill(L3).inner_margin(egui::Margin::symmetric(16, 7)))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        dot(ui, AMBER);
                        match &s.link {
                            Link::Down { retry_at, .. } => {
                                let left = retry_at.saturating_duration_since(Instant::now()).as_secs() + 1;
                                ui.label(RichText::new(format!("Sem conexão com o servidor. Nova tentativa em {left} s.")).color(TEXT));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if small_button(ui, "Tentar agora", FIELD) {
                                        if let Link::Down { retry_at, .. } = &mut s.link {
                                            *retry_at = Instant::now();
                                        }
                                    }
                                });
                            }
                            _ => {
                                ui.label(RichText::new("Reconectando…").color(TEXT));
                            }
                        }
                    });
                });
        }

        if self.publishing.is_some() || self.watching.is_some() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        if self.fullscreen && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            acts.push(Act::Fullscreen);
        }

        if !self.fullscreen {
            self.rail(ui, s, &mut acts);
            self.channel_panel(ui, s, &mut acts);
            if s.search.is_some() {
                self.search_panel(ui, s, &mut acts);
            } else if s.pins_open && !matches!(s.view, View::Home) {
                self.pins_panel(ui, s, &mut acts);
            } else if let View::Chan { server, .. } = s.view {
                self.members_panel(ui, s, server, &mut acts);
            }
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(if self.fullscreen { Color32::BLACK } else { BG })
                    .inner_margin(egui::Margin::symmetric(if self.fullscreen { 0 } else { 28 }, 0)),
            )
            .show(ui, |ui| {
                if self.fullscreen {
                    self.stream_area(ui, ctx, s, &mut acts, true);
                } else {
                    self.center(ui, ctx, s, &mut acts);
                }
            });
        if let Some((msg, pos)) = s.picker {
            const EMOJIS: [&str; 48] = [
                "👍", "👎", "😀", "😂", "😅", "😍", "😘", "🤔", "😮", "😢", "😡", "😎", "🙌", "👏", "🙏", "💪", "👀", "🎉", "🔥", "💯",
                "✅", "❌", "⭐", "🚀", "💡", "🎮", "🍕", "☕", "🏆", "❤", "💔", "✨", "🎶", "🤝", "🤷", "👋", "😴", "🤯", "🥳", "😬",
                "🍻", "🎂", "📌", "⚡", "🌙", "☀", "🐱", "🐶",
            ];
            let m = egui::Modal::new(egui::Id::new("emoji-picker"))
                .backdrop_color(Color32::TRANSPARENT)
                .area(
                    egui::Area::new(egui::Id::new("emoji-picker"))
                        .kind(egui::UiKind::Modal)
                        .sense(egui::Sense::hover())
                        .order(egui::Order::Foreground)
                        .interactable(true)
                        .fixed_pos(pos),
                )
                .frame(glass(10).inner_margin(egui::Margin::same(8)))
                .show(ctx, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
                    for row in EMOJIS.chunks(8) {
                        ui.horizontal(|ui| {
                            for e in row {
                                let (r, resp) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::click());
                                if resp.hovered() {
                                    ui.painter().rect_filled(r, 8.0, L4);
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                ui.painter().text(r.center(), Align2::CENTER_CENTER, *e, egui::FontId::proportional(20.0), TEXT);
                                if resp.clicked() {
                                    acts.push(Act::React { message: msg, emoji: e.to_string(), add: true });
                                    s.picker = None;
                                }
                            }
                        });
                    }
                });
            if m.should_close() {
                s.picker = None;
            }
        }
        if let Some(q) = &mut self.qs {
            match quick::show(ctx, q, s) {
                quick::Out::Stay => {}
                quick::Out::Close => self.qs = None,
                quick::Out::Go(t) => {
                    self.qs = None;
                    acts.push(match t {
                        quick::Target::Server(id) => Act::OpenServer(id),
                        quick::Target::Channel { server, channel } => Act::Open(View::Chan { server, channel }),
                        quick::Target::User(user_id) => Act::Send(C2S::OpenDm { user_id }),
                    });
                }
            }
        }
        self.modals(ctx, s, &mut acts);
        self.toast(ctx, s);

        let mut keep = true;
        for a in acts {
            match a {
                Act::Open(v) => s.open(v),
                Act::OpenServer(id) => s.open_server(id),
                Act::Send(m) => s.gw.send(m),
                Act::Modal(m) => s.modal = m,
                Act::Reply(ch, r) => {
                    s.replying = Some((ch, r));
                    s.focus_composer = true;
                }
                Act::CancelReply => s.replying = None,
                Act::SendMsg { ch, content, reply, attachments } => {
                    // os arquivos já foram usados: saem do campo
                    let used: Vec<i64> = attachments.iter().map(|a| a.id).collect();
                    s.uploads.retain(|u| !matches!(&u.state, files::UpState::Done(a) if used.contains(&a.id)));
                    s.send_message(ch, content, reply, attachments);
                }
                Act::PickFile => {
                    let tx = s.picked_tx.clone();
                    let ctx2 = ctx.clone();
                    std::thread::spawn(move || {
                        if let Some(path) = files::pick_file() {
                            let _ = tx.send(path);
                            ctx2.request_repaint();
                        }
                    });
                }
                Act::AddFile(path) => s.start_upload(path),
                Act::RemoveUpload(key) => s.uploads.retain(|u| u.key != key),
                Act::Resend(nonce) => {
                    if let Some(p) = s.pending.iter_mut().find(|p| p.nonce == nonce) {
                        p.since = Instant::now();
                        let (ch, content, reply_to) = (p.ch, p.content.clone(), p.reply_to);
                        let ids = p.attachments.iter().map(|a| a.id).collect();
                        s.gw.send(C2S::Send { channel_id: ch, content, reply_to, nonce, attachments: ids });
                    }
                }
                Act::CloseSearch => s.search = None,
                Act::React { message, emoji, add } => s.gw.send(C2S::React { message_id: message, emoji, add }),
                Act::OpenPicker(id, pos) => s.picker = Some((id, pos)),
                Act::Pin { message, pin } => s.gw.send(C2S::Pin { message_id: message, pin }),
                Act::OpenPins => {
                    s.pins_open = true;
                    s.search = None;
                    if let Some(ch) = s.current_channel() {
                        s.pins.clear();
                        s.gw.send(C2S::Pins { channel_id: ch });
                    }
                }
                Act::ClosePins => s.pins_open = false,
                Act::Goto { channel, id } => {
                    let server = s.snap.servers.iter().find(|sv| sv.channels.iter().any(|c| c.id == channel)).map(|sv| sv.id);
                    let already = s.current_channel() == Some(channel);
                    match server {
                        Some(server) => s.open(View::Chan { server, channel }),
                        None => s.open(View::Dm(channel)),
                    }
                    if already {
                        s.jump_to(id);
                    } else {
                        s.pending_goto = Some((channel, id));
                    }
                }
                Act::Search(more) => {
                    let view = s.view.clone();
                    let (text, from, chan_override) = {
                        let q = s.search.as_ref().map(|x| x.query.clone()).unwrap_or_default();
                        parse_search(&q, &s.snap, &view, s.me.id)
                    };
                    if let Some(st) = &mut s.search {
                        if text.is_empty() && from.is_none() {
                            continue;
                        }
                        let (channel_id, server_id) = match &view {
                            View::Chan { server, channel } => match chan_override {
                                Some(c) => (Some(c), None),
                                None if st.channel_scope => (Some(*channel), None),
                                None => (None, Some(*server)),
                            },
                            View::Dm(c) => (Some(*c), None),
                            View::Home => continue,
                        };
                        let before = if more { st.hits.last().map(|h| h.message.id) } else { None };
                        st.nonce += 1;
                        st.loading = true;
                        st.appending = more;
                        if !more {
                            st.hits.clear();
                        }
                        let nonce = st.nonce;
                        s.gw.send(C2S::Search { nonce, channel_id, server_id, from, text, before });
                    }
                }
                Act::SetStatus(st) => {
                    s.me.status = st;
                    s.snap.me.status = st;
                    s.gw.send(C2S::SetStatus { status: st });
                }
                Act::ToggleSpoiler(msg, id) => {
                    if !s.revealed.remove(&(msg, id)) {
                        s.revealed.insert((msg, id));
                    }
                }
                Act::Discard(nonce) => {
                    s.pending.retain(|p| p.nonce != nonce);
                    for v in s.msgs.values_mut() {
                        v.retain(|m| m.id != -(nonce as i64));
                    }
                }
                Act::JumpTo(id) => s.jump_to(id),
                Act::StartEdit(id, text) => s.editing = Some((id, text)),
                Act::CancelEdit => s.editing = None,
                Act::SaveEdit(id, text) => {
                    s.editing = None;
                    if !text.trim().is_empty() {
                        s.gw.send(C2S::Edit { message_id: id, content: text });
                    }
                }
                Act::AskDelete(id, preview) => s.modal = Modal::DeleteMsg { id, preview },
                Act::Copy(text) => {
                    ctx.copy_text(text);
                    s.toast = Some(("Texto copiado".into(), Instant::now()));
                }
                Act::ToggleCategory(id) => {
                    if !s.collapsed.remove(&id) {
                        s.collapsed.insert(id);
                    }
                }
                Act::Logout => keep = false,
                Act::OpenAccount => {
                    s.modal = Modal::Account { old: String::new(), new: String::new(), again: String::new() };
                    s.gw.send(C2S::ListSessions);
                }
                Act::Watch(ch) => {
                    self.stop_watch();
                    let url = net::url(&self.login.server, "watch", ch, &s.token);
                    self.watching = Some((ch, viewer::start(url, ctx.clone())));
                }
                Act::StopWatch => {
                    self.stop_watch();
                    self.fullscreen = false;
                }
                Act::Publish(ch) => {
                    self.stop_publish();
                    let url = net::url(&self.login.server, "publish", ch, &s.token);
                    self.publishing = Some((ch, publisher::start(url, self.monitor, self.fps, self.kbps)));
                }
                Act::StopPublish => self.stop_publish(),
                Act::Fullscreen => {
                    self.fullscreen = !self.fullscreen;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
                }
            }
        }
        keep
    }

    // ---- barra de servidores
    fn rail(&mut self, ui: &mut egui::Ui, s: &Session, acts: &mut Vec<Act>) {
        let pr = egui::Panel::left("rail")
            .frame(egui::Frame::new().fill(RAIL).inner_margin(egui::Margin::symmetric(16, 12)))
            .resizable(false)
            .exact_size(72.0)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                let home = matches!(s.view, View::Home | View::Dm(_));
                let dm_unread = s.snap.dms.iter().any(|d| s.unread.contains(&d.channel_id));
                if rail_item(ui, "DM", home, dm_unread, false, s.snap.dms.iter().map(|d| s.mentions.get(&d.channel_id).copied().unwrap_or(0)).sum()).on_hover_text("Mensagens diretas").clicked() {
                    acts.push(Act::Open(View::Home));
                }
                let (sep, _) = ui.allocate_exact_size(egui::vec2(32.0, 1.0), egui::Sense::hover());
                ui.painter().rect_filled(sep.translate(egui::vec2(-4.0, 0.0)), 1.0, LINE2);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    for sv in &s.snap.servers {
                        let sel = matches!(&s.view, View::Chan { server, .. } if *server == sv.id);
                        let unread = sv.channels.iter().any(|c| s.unread.contains(&c.id));
                        if rail_item(ui, &initials(&sv.name), sel, unread, false, sv.channels.iter().map(|c| s.mentions.get(&c.id).copied().unwrap_or(0)).sum()).on_hover_text(&sv.name).clicked() {
                            acts.push(Act::OpenServer(sv.id));
                        }
                    }
                    if rail_item(ui, "+", false, false, true, 0).on_hover_text("Adicionar um servidor").clicked() {
                        acts.push(Act::Modal(Modal::AddServer { create: true, name: String::new(), code: String::new() }));
                    }
                });
            });
        edge(ui, pr.response.rect, true);
    }

    // ---- canais / DMs
    fn channel_panel(&mut self, ui: &mut egui::Ui, s: &Session, acts: &mut Vec<Act>) {
        let pr = egui::Panel::left("channels")
            .frame(egui::Frame::new().fill(SIDE))
            .resizable(false)
            .exact_size(240.0)
            .show(ui, |ui| {
                let w = ui.available_width();
                let list_h = (ui.available_height() - 60.0).max(50.0);
                ui.allocate_ui(egui::vec2(w, list_h), |ui| match &s.view {
                    View::Chan { server, channel } => {
                        let Some(sv) = s.snap.servers.iter().find(|x| x.id == *server) else { return };
                        self.server_header(ui, s, sv, acts);
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                            egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 8)).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                self.channel_tree(ui, s, sv, *channel, acts);
                            });
                        });
                    }
                    _ => {
                        egui::Frame::new().inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            let r = list_row(ui, matches!(s.view, View::Home), |ui| {
                                ui.label(RichText::new("Amigos").strong());
                                let pending = s.snap.friends.iter().filter(|f| f.status == p::FriendStatus::Incoming).count();
                                if pending > 0 {
                                    ui.label(RichText::new(format!(" {pending}")).strong().color(AMBER));
                                }
                            });
                            if r.clicked() {
                                acts.push(Act::Open(View::Home));
                            }
                            ui.add_space(6.0);
                            ui.label(caps("Mensagens diretas"));
                            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                                for d in &s.snap.dms {
                                    let sel = matches!(s.view, View::Dm(c) if c == d.channel_id);
                                    let unread = s.unread.contains(&d.channel_id);
                                    let r = list_row(ui, sel, |ui| {
                                        avatar(ui, &d.user.name, d.user.id, 24.0, Some(d.user.status), SIDE);
                                        let t = RichText::new(&d.user.name);
                                        ui.label(if unread { t.strong().color(TEXT) } else { t.color(TEXT2) });
                                    });
                                    if r.clicked() {
                                        acts.push(Act::Open(View::Dm(d.channel_id)));
                                    }
                                }
                            });
                        });
                    }
                });
                // cartão do usuário
                egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    egui::Frame::new()
                        .fill(L2)
                        .corner_radius(egui::CornerRadius::same(10))
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            let my = if s.me.status == p::Status::Offline { p::Status::Online } else { s.me.status };
                            let bg = ui.painter().add(egui::Shape::Noop);
                            let row = ui.horizontal(|ui| {
                                avatar(ui, &s.me.name, s.me.id, 32.0, Some(my), L2);
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 0.0;
                                    ui.label(RichText::new(&s.me.name).strong().color(TEXT));
                                    ui.label(RichText::new(status_label(my)).size(11.5).color(MUTED));
                                });
                            });
                            // clicar no cartão abre o menu de status
                            let rc = row.response.rect.expand2(egui::vec2(6.0, 4.0)).intersect(ui.max_rect().expand2(egui::vec2(0.0, 4.0)));
                            let card = ui.interact(rc, egui::Id::new("me-card"), egui::Sense::click());
                            if card.hovered() {
                                ui.painter().set(bg, egui::Shape::rect_filled(rc, 8.0, L3));
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            egui::Popup::menu(&card).width(220.0).show(|ui| {
                                ui.set_min_width(210.0);
                                for (st, label) in [
                                    (p::Status::Online, "Online"),
                                    (p::Status::Idle, "Ausente"),
                                    (p::Status::Dnd, "Não perturbe"),
                                    (p::Status::Invisible, "Invisível"),
                                ] {
                                    let (r, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 30.0), egui::Sense::click());
                                    if resp.hovered() {
                                        ui.painter().rect_filled(r, 6.0, L4);
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                    }
                                    presence_dot(ui.painter(), r.left_center() + egui::vec2(14.0, 0.0), 5.0, st, if resp.hovered() { L4 } else { L3 });
                                    ui.painter().text(
                                        r.left_center() + egui::vec2(32.0, 0.0),
                                        Align2::LEFT_CENTER,
                                        label,
                                        egui::FontId::proportional(14.0),
                                        if st == my { AMBER } else { TEXT },
                                    );
                                    if resp.clicked() {
                                        acts.push(Act::SetStatus(st));
                                        ui.close();
                                    }
                                }
                                ui.separator();
                                if ui.button("Minha conta").clicked() {
                                    acts.push(Act::OpenAccount);
                                    ui.close();
                                }
                                if ui.button(RichText::new("Sair").color(RED)).clicked() {
                                    acts.push(Act::Logout);
                                    ui.close();
                                }
                            });
                        });
                });
            });
        edge(ui, pr.response.rect, true);
    }

    /// Cabeçalho do servidor: nome em serifa, seta e menu suspenso.
    fn server_header(&mut self, ui: &mut egui::Ui, s: &Session, sv: &p::ServerInfo, acts: &mut Vec<Act>) {
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 52.0), egui::Sense::click());
        let open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&resp));
        let hot = ui.ctx().animate_bool_with_time(resp.id, resp.hovered() || open, 0.12);
        let p = ui.painter();
        p.rect_filled(rect, 0.0, lerp(L1, if open { L3 } else { L2 }, hot));
        p.text(
            rect.left_center() + egui::vec2(16.0, 0.0),
            Align2::LEFT_CENTER,
            &sv.name,
            egui::FontId::new(18.0, serif_family()),
            TEXT,
        );
        let ink = lerp(MUTED, TEXT, hot);
        let c = rect.right_center() - egui::vec2(20.0, 0.0);
        if open {
            let st = egui::Stroke::new(1.5, ink);
            p.line_segment([c + egui::vec2(-4.0, -4.0), c + egui::vec2(4.0, 4.0)], st);
            p.line_segment([c + egui::vec2(-4.0, 4.0), c + egui::vec2(4.0, -4.0)], st);
        } else {
            chevron(p, c, true, ink);
        }
        p.hline(rect.x_range(), rect.bottom() - 0.5, egui::Stroke::new(1.0, LINE));
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let owner = sv.owner == s.me.id;
        egui::Popup::menu(&resp).width(220.0).show(|ui| {
            ui.set_min_width(210.0);
            if ui.button("Convidar pessoas").clicked() {
                acts.push(Act::Modal(Modal::Invite(sv.invite.clone())));
            }
            if owner {
                ui.separator();
                if ui.button("Criar canal").clicked() {
                    acts.push(Act::Modal(Modal::NewChannel { server: sv.id, name: String::new(), kind: 0, parent: None }));
                }
                if ui.button("Criar categoria").clicked() {
                    acts.push(Act::Modal(Modal::NewChannel { server: sv.id, name: String::new(), kind: 4, parent: None }));
                }
            }
        });
    }

    /// Canais soltos primeiro, depois as categorias; canais de texto por posição.
    fn channel_tree(&mut self, ui: &mut egui::Ui, s: &Session, sv: &p::ServerInfo, current: i64, acts: &mut Vec<Act>) {
        let owner = sv.owner == s.me.id;
        let mut cats: Vec<&p::Channel> = sv.channels.iter().filter(|c| c.kind == 4).collect();
        cats.sort_by_key(|c| (c.position, c.id));
        let texts = |parent: Option<i64>| {
            let mut v: Vec<&p::Channel> = sv
                .channels
                .iter()
                .filter(|c| c.kind == 0 && match parent {
                    Some(p) => c.parent == Some(p),
                    None => c.parent.map_or(true, |p| !cats.iter().any(|k| k.id == p)),
                })
                .collect();
            v.sort_by_key(|c| (c.position, c.id));
            v
        };
        let row = |ui: &mut egui::Ui, c: &p::Channel, acts: &mut Vec<Act>| {
            let selected = c.id == current;
            let unread = s.unread.contains(&c.id);
            let live = s.snap.live.iter().any(|l| l.channel_id == c.id);
            let r = list_row(ui, selected, |ui| {
                let strong = selected || unread;
                ui.label(RichText::new("#").size(18.0).color(if strong { TEXT2 } else { MUTED }));
                let t = RichText::new(&c.name).color(if strong { TEXT } else { TEXT2 });
                ui.label(if unread && !selected { t.strong() } else { t });
                if live {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.label(RichText::new("AO VIVO").size(10.0).strong().color(AMBER));
                    });
                }
            });
            if let Some(n) = s.mentions.get(&c.id).copied().filter(|n| *n > 0 && !selected) {
                let rc = r.rect;
                badge_pill(ui.painter(), egui::pos2(rc.right() - 8.0, rc.center().y + 8.0), n, if selected { L3 } else { L1 });
            }
            if unread && !selected {
                let rc = r.rect;
                ui.painter().rect_filled(
                    egui::Rect::from_center_size(egui::pos2(rc.left() - 6.5, rc.center().y), egui::vec2(3.0, 8.0)),
                    2.0,
                    TEXT,
                );
            }
            if r.clicked() {
                acts.push(Act::Open(View::Chan { server: sv.id, channel: c.id }));
            }
        };
        for c in texts(None) {
            row(ui, c, acts);
        }
        for (i, cat) in cats.iter().enumerate() {
            let kids = texts(Some(cat.id));
            let collapsed = s.collapsed.contains(&cat.id);
            ui.add_space(if i == 0 && texts(None).is_empty() { 4.0 } else { 14.0 });
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), egui::Sense::click());
            let hot = ui.ctx().animate_bool_with_time(resp.id.with("h"), resp.hovered(), 0.12);
            let ink = lerp(MUTED, TEXT2, hot);
            let plus_rect = egui::Rect::from_center_size(rect.right_center() - egui::vec2(12.0, 0.0), egui::vec2(20.0, 20.0));
            let plus = owner.then(|| ui.interact(plus_rect, resp.id.with("plus"), egui::Sense::click()));
            let p = ui.painter();
            chevron(p, rect.left_center() + egui::vec2(8.0, 0.0), !collapsed, ink);
            p.text(
                rect.left_center() + egui::vec2(20.0, 0.0),
                Align2::LEFT_CENTER,
                cat.name.to_uppercase(),
                egui::FontId::proportional(11.5),
                ink,
            );
            let mut plus_hit = false;
            if let Some(pl) = &plus {
                if resp.hovered() || pl.hovered() {
                    p.text(plus_rect.center(), Align2::CENTER_CENTER, "+", egui::FontId::proportional(18.0), if pl.hovered() { TEXT } else { MUTED });
                }
                if pl.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                if pl.clicked() {
                    plus_hit = true;
                    acts.push(Act::Modal(Modal::NewChannel { server: sv.id, name: String::new(), kind: 0, parent: Some(cat.id) }));
                }
            }
            if resp.hovered() && !plus_hit {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() && !plus.as_ref().is_some_and(|p| p.hovered()) {
                acts.push(Act::ToggleCategory(cat.id));
            }
            for c in kids {
                if collapsed && c.id != current && !s.unread.contains(&c.id) {
                    continue;
                }
                row(ui, c, acts);
            }
        }
    }

    // ---- busca de mensagens (Ctrl+F)
    fn search_panel(&mut self, ui: &mut egui::Ui, s: &mut Session, acts: &mut Vec<Act>) {
        let in_server = matches!(s.view, View::Chan { .. });
        let place = match &s.view {
            View::Chan { server, .. } => s.snap.servers.iter().find(|x| x.id == *server).map(|x| x.name.clone()).unwrap_or_default(),
            _ => String::new(),
        };
        let pr = egui::Panel::right("search")
            .frame(egui::Frame::new().fill(SIDE))
            .resizable(false)
            .exact_size(400.0)
            .show(ui, |ui| {
                // cabeçalho
                ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 52.0), Layout::left_to_right(Align::Center), |ui| {
                    ui.add_space(16.0);
                    ui.label(serif("Resultados de busca", 19.0));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(10.0);
                        let (r, resp) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, 8.0, L4);
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        let c = r.center();
                        let st = egui::Stroke::new(1.5, if resp.hovered() { TEXT } else { MUTED });
                        ui.painter().line_segment([c + egui::vec2(-5.0, -5.0), c + egui::vec2(5.0, 5.0)], st);
                        ui.painter().line_segment([c + egui::vec2(-5.0, 5.0), c + egui::vec2(5.0, -5.0)], st);
                        if resp.clicked() {
                            acts.push(Act::CloseSearch);
                        }
                    });
                });
                rule_bleed(ui, 0.0);
                ui.add_space(10.0);
                let Some(st) = &mut s.search else { return };
                egui::Frame::new().inner_margin(egui::Margin::symmetric(14, 0)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    // campo de busca
                    let id = egui::Id::new("search-input");
                    let focused = ui.memory(|m| m.has_focus(id));
                    egui::Frame::new()
                        .fill(L3)
                        .stroke(egui::Stroke::new(1.0, if focused { AMBER } else { LINE2 }))
                        .corner_radius(egui::CornerRadius::same(10))
                        .inner_margin(egui::Margin::symmetric(12, 2))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            let hint = if in_server { format!("Buscar em {place}") } else { "Buscar nesta conversa".to_string() };
                            let r = ui.add(
                                egui::TextEdit::singleline(&mut st.query)
                                    .id(id)
                                    .frame(egui::Frame::NONE)
                                    .hint_text(hint)
                                    .desired_width(f32::INFINITY)
                                    .margin(egui::Margin::symmetric(0, 8)),
                            );
                            if st.focus {
                                st.focus = false;
                                r.request_focus();
                            }
                            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                acts.push(Act::Search(false));
                                r.request_focus();
                            }
                        });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if in_server {
                            ui.selectable_value(&mut st.channel_scope, false, "No servidor");
                            ui.selectable_value(&mut st.channel_scope, true, "Neste canal");
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if small_button(ui, "Buscar", BLURPLE) {
                                acts.push(Act::Search(false));
                            }
                        });
                    });
                    ui.label(RichText::new("Filtros: de:nome  em:#canal  (de:eu para as suas)").size(11.5).color(MUTED));
                });
                ui.add_space(6.0);
                rule_bleed(ui, 0.0);
                let Some(st) = &mut s.search else { return };
                if st.loading && st.hits.is_empty() {
                    ui.add_space(20.0);
                    ui.vertical_centered(|ui| ui.label(RichText::new("Buscando…").color(MUTED)));
                    ui.ctx().request_repaint_after(Duration::from_millis(300));
                } else if st.searched && st.hits.is_empty() {
                    ui.add_space(24.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Buscamos por toda parte.").color(TEXT2));
                        ui.label(RichText::new("Infelizmente, não encontramos nada.").color(MUTED));
                    });
                } else if !st.searched {
                    ui.add_space(24.0);
                    ui.vertical_centered(|ui| ui.label(RichText::new("Escreva e aperte Enter para buscar.").color(MUTED)));
                }
                let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
                let mut go: Option<(i64, i64)> = None;
                let mut want_more = false;
                if !st.hits.is_empty() {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add_space(16.0);
                        ui.label(RichText::new(format!("{}{} resultado{}", st.hits.len(), if st.more { "+" } else { "" }, if st.hits.len() == 1 { "" } else { "s" })).size(12.5).color(MUTED));
                    });
                }
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for h in &st.hits {
                        let m = &h.message;
                        let bg = ui.painter().add(egui::Shape::Noop);
                        let card = egui::Frame::new().inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new(format!("# {}", h.channel_name)).size(12.0).color(MUTED));
                            ui.horizontal_top(|ui| {
                                avatar(ui, &m.author, m.author_id, 32.0, None, SIDE);
                                ui.add_space(4.0);
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 2.0;
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(&m.author).strong().color(TEXT));
                                        ui.label(RichText::new(format!("{} {}", fmt_day(m.ts), fmt_clock(m.ts))).size(11.5).color(MUTED));
                                    });
                                    let blocks = telacast_md::parse(&m.content);
                                    let empty = HashSet::new();
                                    let env = md_view::Env { snap: &s.snap, revealed: &empty, msg: m.id, now: now_secs };
                                    let mut clicks = Vec::new();
                                    md_view::show(ui, &blocks, &env, &md_view::Base { size: 14.5, color: TEXT2 }, m.edited, &mut clicks);
                                });
                            });
                        });
                        let r = ui.interact(card.response.rect, egui::Id::new(("hit", m.id)), egui::Sense::click());
                        if r.hovered() {
                            ui.painter().set(bg, egui::Shape::rect_filled(card.response.rect, 0.0, L2));
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if r.clicked() {
                            go = Some((m.channel_id, m.id));
                        }
                        let (line, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
                        ui.painter().rect_filled(line, 0.0, LINE);
                    }
                    if st.more {
                        ui.add_space(10.0);
                        ui.vertical_centered(|ui| {
                            if small_button(ui, if st.loading { "Carregando…" } else { "Mostrar mais" }, FIELD) && !st.loading {
                                want_more = true;
                            }
                        });
                        ui.add_space(14.0);
                    }
                });
                if want_more {
                    acts.push(Act::Search(true));
                }
                if let Some((channel, id)) = go {
                    acts.push(Act::Goto { channel, id });
                }
            });
        edge(ui, pr.response.rect, false);
    }

    // ---- mensagens fixadas
    fn pins_panel(&mut self, ui: &mut egui::Ui, s: &mut Session, acts: &mut Vec<Act>) {
        let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        let pr = egui::Panel::right("pins")
            .frame(egui::Frame::new().fill(SIDE))
            .resizable(false)
            .exact_size(380.0)
            .show(ui, |ui| {
                ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 52.0), Layout::left_to_right(Align::Center), |ui| {
                    ui.add_space(16.0);
                    ui.label(serif("Mensagens fixadas", 19.0));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.add_space(10.0);
                        let (r, resp) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, 8.0, L4);
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        let c = r.center();
                        let st = egui::Stroke::new(1.5, if resp.hovered() { TEXT } else { MUTED });
                        ui.painter().line_segment([c + egui::vec2(-5.0, -5.0), c + egui::vec2(5.0, 5.0)], st);
                        ui.painter().line_segment([c + egui::vec2(-5.0, 5.0), c + egui::vec2(5.0, -5.0)], st);
                        if resp.clicked() {
                            acts.push(Act::ClosePins);
                        }
                    });
                });
                rule_bleed(ui, 0.0);
                if s.pins.is_empty() {
                    ui.add_space(24.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Este canal não tem mensagens fixadas...").color(TEXT2));
                        ui.label(RichText::new("por enquanto.").color(MUTED));
                    });
                }
                let mut go: Option<(i64, i64)> = None;
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for m in &s.pins {
                        let bg = ui.painter().add(egui::Shape::Noop);
                        let card = egui::Frame::new().inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal_top(|ui| {
                                avatar(ui, &m.author, m.author_id, 32.0, None, SIDE);
                                ui.add_space(4.0);
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 2.0;
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(&m.author).strong().color(TEXT));
                                        ui.label(RichText::new(format!("{} {}", fmt_day(m.ts), fmt_clock(m.ts))).size(11.5).color(MUTED));
                                    });
                                    let blocks = telacast_md::parse(&m.content);
                                    let empty = HashSet::new();
                                    let env = md_view::Env { snap: &s.snap, revealed: &empty, msg: m.id, now: now_secs };
                                    let mut clicks = Vec::new();
                                    md_view::show(ui, &blocks, &env, &md_view::Base { size: 14.5, color: TEXT2 }, m.edited, &mut clicks);
                                });
                            });
                        });
                        let r = ui.interact(card.response.rect, egui::Id::new(("pin", m.id)), egui::Sense::click());
                        if r.hovered() {
                            ui.painter().set(bg, egui::Shape::rect_filled(card.response.rect, 0.0, L2));
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if r.clicked() {
                            go = Some((m.channel_id, m.id));
                        }
                        let (line, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 1.0), egui::Sense::hover());
                        ui.painter().rect_filled(line, 0.0, LINE);
                    }
                });
                if let Some((channel, id)) = go {
                    acts.push(Act::Goto { channel, id });
                }
            });
        edge(ui, pr.response.rect, false);
    }

    // ---- membros
    fn members_panel(&mut self, ui: &mut egui::Ui, s: &Session, server: i64, acts: &mut Vec<Act>) {
        let Some(sv) = s.snap.servers.iter().find(|x| x.id == server) else { return };
        let pr = egui::Panel::right("members")
            .frame(egui::Frame::new().fill(SIDE).inner_margin(egui::Margin { left: 10, right: 10, top: 16, bottom: 10 }))
            .resizable(false)
            .exact_size(220.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    for (title, online) in [("ONLINE", true), ("OFFLINE", false)] {
                        let list: Vec<&p::User> = sv.members.iter().filter(|m| m.online == online).collect();
                        if list.is_empty() {
                            continue;
                        }
                        ui.label(caps(&format!("{title} — {}", list.len())));
                        for m in list {
                            let r = list_row(ui, false, |ui| {
                                avatar(ui, &m.name, m.id, 28.0, Some(m.status), SIDE);
                                let t = RichText::new(&m.name);
                                ui.label(if online { t } else { t.color(MUTED) });
                                if m.id == sv.owner {
                                    ui.label(RichText::new("dono").size(10.0).color(YELLOW));
                                }
                            });
                            if r.clicked() && m.id != s.me.id {
                                acts.push(Act::Send(C2S::OpenDm { user_id: m.id }));
                            }
                            if m.id != s.me.id {
                                let is_blocked = s.snap.blocked.iter().any(|b| b.id == m.id);
                                egui::Popup::context_menu(&r).width(190.0).show(|ui| {
                                    ui.set_min_width(180.0);
                                    if ui.button("Enviar mensagem").clicked() {
                                        acts.push(Act::Send(C2S::OpenDm { user_id: m.id }));
                                        ui.close();
                                    }
                                    ui.separator();
                                    if is_blocked {
                                        if ui.button("Desbloquear").clicked() {
                                            acts.push(Act::Send(C2S::Unblock { user_id: m.id }));
                                            ui.close();
                                        }
                                    } else if ui.button(RichText::new("Bloquear").color(RED)).clicked() {
                                        acts.push(Act::Modal(Modal::ConfirmBlock { user_id: m.id, name: m.name.clone() }));
                                        ui.close();
                                    }
                                });
                            }
                        }
                        ui.add_space(8.0);
                    }
                });
            });
        edge(ui, pr.response.rect, false);
    }

    // ---- centro
    fn center(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, s: &mut Session, acts: &mut Vec<Act>) {
        let age = s.view_since.elapsed().as_secs_f32() / 0.22;
        if age < 1.0 {
            ui.set_opacity(0.15 + 0.85 * ease_out(age));
            ctx.request_repaint();
        }
        match s.view.clone() {
            View::Home => {
                if self.watching.is_some() {
                    self.stream_area(ui, ctx, s, acts, false);
                }
                self.friends_view(ui, s, acts);
            }
            View::Dm(ch) => {
                let title = s
                    .snap
                    .dms
                    .iter()
                    .find(|d| d.channel_id == ch)
                    .map(|d| d.user.name.clone())
                    .unwrap_or_default();
                self.chat_view(ui, ctx, s, acts, ch, &title, false, "Enviar mensagem");
            }
            View::Chan { server, channel } => {
                let name = s
                    .snap
                    .servers
                    .iter()
                    .find(|x| x.id == server)
                    .and_then(|x| x.channels.iter().find(|c| c.id == channel))
                    .map(|c| c.name.clone())
                    .unwrap_or_default();
                self.chat_view(ui, ctx, s, acts, channel, &name, true, &format!("Conversar em #{name}"));
            }
        }
    }

    fn chat_view(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        s: &mut Session,
        acts: &mut Vec<Act>,
        ch: i64,
        title: &str,
        hash: bool,
        hint: &str,
    ) {
        // cabeçalho
        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 63.0), Layout::left_to_right(Align::Center), |ui| {
            if hash {
                ui.label(RichText::new("#").size(26.0).color(MUTED));
            }
            ui.label(serif(title, 24.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                match &self.publishing {
                    Some((pch, h)) => {
                        let st = h.status.lock().unwrap().clone();
                        let done = st.starts_with("erro") || st == "parado";
                        if small_button(ui, if done { "Fechar" } else { "Parar transmissão" }, RED) {
                            acts.push(Act::StopPublish);
                        }
                        dot(ui, status_color(&st));
                        ui.label(RichText::new(if *pch == ch { st } else { format!("{st} (outro canal)") }).size(12.0));
                    }
                    None => {
                        if small_button(ui, "Ao vivo", GREEN) {
                            acts.push(Act::Modal(Modal::GoLive));
                        }
                    }
                }
                if small_button(ui, "Fixadas", FIELD) {
                    acts.push(if s.pins_open { Act::ClosePins } else { Act::OpenPins });
                }
            });
        });
        rule_bleed(ui, 28.0);
        ui.add_space(10.0);

        // alguém ao vivo neste canal?
        if let Some(l) = s.snap.live.iter().find(|l| l.channel_id == ch) {
            let mine = l.user_id == s.me.id;
            let who = s.name_of(l.user_id);
            let watching_here = matches!(&self.watching, Some((c, _)) if *c == ch);
            egui::Frame::new()
                .fill(CARD)
                .stroke(egui::Stroke::new(1.0, LINE2))
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(egui::Margin::same(10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        dot(ui, AMBER);
                        if mine {
                            ui.label("Você está ao vivo neste canal");
                        } else {
                            ui.label(format!("{who} está ao vivo"));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if watching_here {
                                    if small_button(ui, "Fechar", FIELD) {
                                        acts.push(Act::StopWatch);
                                    }
                                } else if small_button(ui, "Assistir", BLURPLE) {
                                    acts.push(Act::Watch(ch));
                                }
                            });
                        }
                    });
                });
        }
        if self.watching.is_some() {
            self.stream_area(ui, ctx, s, acts, false);
        }

        // mensagens
        let me_id = s.me.id;
        let is_owner = s.snap.servers.iter().any(|sv| sv.owner == me_id && sv.channels.iter().any(|c| c.id == ch));
        let can_pin = is_owner || matches!(s.view, View::Dm(_));
        let (http_base, token) = (s.http_base.clone(), s.token.clone());
        let blocked_ids: HashSet<i64> = s.snap.blocked.iter().map(|u| u.id).collect();
        let replying_here = matches!(&s.replying, Some((c, _)) if *c == ch);
        let uploads_here = s.uploads.iter().any(|u| u.channel == ch);
        let input_h = 82.0 + 22.0 + if replying_here { 38.0 } else { 0.0 } + if uploads_here { 172.0 } else { 0.0 };
        let h = (ui.available_height() - input_h).max(60.0);
        let now_secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
        // Esc dispensa o divisor "novas mensagens" (o canal já foi marcado como lido ao abrir)
        if s.replying.is_none() && s.editing.is_none() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            s.marker = None;
        }
        let prev_anchor = s.anchor.take().filter(|a| a.0 == ch);
        let scroll = egui::ScrollArea::vertical()
            .id_salt(("chat", ch))
            .max_height(h)
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                let mut new_anchor: Option<(i64, i64, f32)> = None;
                let empty = Vec::new();
                let msgs = s.msgs.get(&ch).unwrap_or(&empty);
                if msgs.is_empty() {
                    ui.add_space(24.0);
                    ui.label(RichText::new("Ainda não há mensagens. Diga olá!").color(MUTED));
                }
                for (i, m) in msgs.iter().enumerate() {
                    let new_day = i == 0 || !same_day(msgs[i - 1].ts, m.ts);
                    if new_day {
                        day_divider(ui, &fmt_day(m.ts));
                    }
                    // primeira mensagem de outra pessoa depois de onde eu tinha parado de ler
                    let first_new = matches!(s.marker, Some((mch, mk, _)) if mch == ch && m.id > mk && m.author_id != me_id
                        && (i == 0 || msgs[i - 1].id <= mk));
                    if first_new {
                        let r = new_messages_divider(ui);
                        if let Some((_, _, scrolled)) = &mut s.marker {
                            if !*scrolled {
                                *scrolled = true;
                                ui.scroll_to_rect(r, Some(Align::TOP));
                            }
                        }
                    }
                    // mensagem de alguém que você bloqueou: some, com um aviso e a opção de mostrar
                    if blocked_ids.contains(&m.author_id) && m.kind == 0 && !s.revealed.contains(&(m.id, u32::MAX)) {
                        ui.horizontal(|ui| {
                            ui.add_space(54.0);
                            ui.label(RichText::new("Mensagem de usuário bloqueado.").size(13.0).color(MUTED));
                            let r = ui.add(egui::Label::new(RichText::new("Mostrar mensagem").size(13.0).color(AMBER).underline()).sense(egui::Sense::click()));
                            if r.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if r.clicked() {
                                acts.push(Act::ToggleSpoiler(m.id, u32::MAX));
                            }
                        });
                        continue;
                    }
                    // avisos de sistema (entrou, fixou): linha própria, sem barra de ações, quebra o agrupamento
                    if m.kind == 2 || m.kind == 3 {
                        system_row(ui, m, acts);
                        continue;
                    }
                    let grouped = !new_day && !first_new && (i == 0 || msgs[i - 1].kind == 0)
                        && msgs[i - 1].author_id == m.author_id
                        && m.ts - msgs[i - 1].ts < 5 * 60 * 1000
                        && m.reply.is_none();
                    let born = *s.seen.entry(m.id).or_insert_with(Instant::now);
                    // a mensagem própria que acabou de ser confirmada não "entra" de novo
                    let age = if is_pending_or_own_echo(m, me_id) { 1.0 } else { born.elapsed().as_secs_f32() / 0.22 };
                    let editing_this = matches!(&s.editing, Some((id, _)) if *id == m.id);
                    let is_pending = m.id < 0;
                    let pend = if is_pending {
                        s.pending.iter().find(|p| p.nonce == (-m.id) as u64).map(|p| (p.nonce, p.since.elapsed()))
                    } else {
                        None
                    };
                    let link_up = matches!(s.link, Link::Up);
                    let mine = m.author_id == me_id;
                    let can_delete = mine || is_owner;
                    let reply_ref = p::Reply { id: m.id, author: m.author.clone(), content: m.content.chars().take(120).collect() };
                    // menu do botão direito, aberto em cima do texto (que continua selecionável)
                    let show_menu = |resp: &egui::Response, acts: &mut Vec<Act>| {
                        egui::Popup::context_menu(resp).width(190.0).show(|ui| {
                            ui.set_min_width(180.0);
                            if ui.button("Responder").clicked() {
                                acts.push(Act::Reply(ch, reply_ref.clone()));
                                ui.close();
                            }
                            if can_pin && ui.button(if m.pinned { "Desafixar mensagem" } else { "Fixar mensagem" }).clicked() {
                                acts.push(Act::Pin { message: m.id, pin: !m.pinned });
                                ui.close();
                            }
                            if ui.button("Reagir").clicked() {
                                acts.push(Act::OpenPicker(m.id, ui.ctx().pointer_latest_pos().unwrap_or_default()));
                                ui.close();
                            }
                            if mine && ui.button("Editar mensagem").clicked() {
                                acts.push(Act::StartEdit(m.id, m.content.clone()));
                                ui.close();
                            }
                            if ui.button("Copiar mensagem inteira").clicked() {
                                acts.push(Act::Copy(m.content.clone()));
                                ui.close();
                            }
                            if can_delete {
                                ui.separator();
                                if ui.button(RichText::new("Apagar mensagem").color(RED)).clicked() {
                                    acts.push(Act::AskDelete(m.id, m.content.clone()));
                                    ui.close();
                                }
                            }
                        });
                    };
                    let bg = ui.painter().add(egui::Shape::Noop);
                    let row = ui.horizontal_top(|ui| {
                        if age < 1.0 {
                            ui.set_opacity(ease_out(age));
                            ui.ctx().request_repaint();
                        }
                        if is_pending {
                            ui.set_opacity(0.55);
                        }
                        if grouped {
                            ui.add_space(54.0);
                        } else {
                            avatar(ui, &m.author, m.author_id, 40.0, None, BG);
                            ui.add_space(6.0);
                        }
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            if let Some(r) = &m.reply {
                                let hr = ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 6.0;
                                    ui.label(RichText::new("↳").size(13.0).color(MUTED));
                                    ui.label(RichText::new(&r.author).size(13.0).strong().color(TEXT2));
                                    let snip: String = r.content.chars().take(90).map(|c| if c == '\n' { ' ' } else { c }).collect();
                                    ui.label(RichText::new(snip).size(13.0).color(MUTED));
                                })
                                .response;
                                // clicar na resposta leva até a mensagem original
                                let hit = ui.interact(hr.rect, egui::Id::new(("replyjump", m.id)), egui::Sense::click());
                                if hit.hovered() {
                                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                    ui.painter().hline(hr.rect.x_range(), hr.rect.bottom(), egui::Stroke::new(1.0, MUTED));
                                }
                                if hit.clicked() {
                                    acts.push(Act::JumpTo(r.id));
                                }
                            }
                            if !grouped {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&m.author).size(15.5).strong().color(TEXT));
                                    ui.label(RichText::new(fmt_clock(m.ts)).size(12.0).color(MUTED));
                                    if m.pinned {
                                        ui.label(RichText::new("Fixada").size(11.5).color(AMBER));
                                    }
                                });
                            }
                            if editing_this {
                                if let Some((_, buf)) = &mut s.editing {
                                    let id = egui::Id::new(("edit", m.id));
                                    let save = ui.memory(|mem| mem.has_focus(id)) && take_plain_enter(ui.ctx());
                                    let r = ui.add(
                                        egui::TextEdit::multiline(buf)
                                            .id(id)
                                            .desired_width(ui.available_width().min(640.0))
                                            .desired_rows(1),
                                    );
                                    let first: bool = ui.data_mut(|d| d.get_temp::<bool>(id.with("init")).is_none());
                                    if first {
                                        ui.data_mut(|d| d.insert_temp(id.with("init"), true));
                                        r.request_focus();
                                    }
                                    ui.label(RichText::new("Esc cancela · Enter salva").size(12.0).color(MUTED));
                                    if save {
                                        ui.data_mut(|d| d.remove_temp::<bool>(id.with("init")));
                                        acts.push(Act::SaveEdit(m.id, buf.clone()));
                                    } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                        ui.data_mut(|d| d.remove_temp::<bool>(id.with("init")));
                                        acts.push(Act::CancelEdit);
                                    }
                                }
                            } else {
                                let blocks = telacast_md::parse(&m.content);
                                let env = md_view::Env { snap: &s.snap, revealed: &s.revealed, msg: m.id, now: now_secs };
                                let mut clicks = Vec::new();
                                let resps = md_view::show(ui, &blocks, &env, &md_view::Base { size: 15.5, color: TEXT2 }, m.edited, &mut clicks);
                                if !is_pending {
                                    for r in &resps {
                                        show_menu(r, acts);
                                    }
                                }
                                for c in clicks {
                                    match c {
                                        md_view::Click::Spoiler(id) => acts.push(Act::ToggleSpoiler(m.id, id)),
                                    }
                                }
                            }
                            for a in &m.attachments {
                                attachment_view(ui, a, &http_base, &token, &mut s.images, acts);
                            }
                            if !m.reactions.is_empty() && !is_pending {
                                ui.add_space(2.0);
                                ui.horizontal_wrapped(|ui| {
                                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
                                    for r in &m.reactions {
                                        let resp = reaction_chip(ui, &r.emoji, Some(r.count), r.me);
                                        if resp.clicked() {
                                            acts.push(Act::React { message: m.id, emoji: r.emoji.clone(), add: !r.me });
                                        }
                                    }
                                    let plus = reaction_chip(ui, "+", None, false);
                                    if plus.clicked() {
                                        acts.push(Act::OpenPicker(m.id, plus.rect.left_bottom()));
                                    }
                                });
                            }
                            if let Some((nonce, waited)) = pend {
                                ui.horizontal(|ui| {
                                    if !link_up {
                                        ui.label(RichText::new("Sem conexão. Será enviada ao reconectar.").size(11.5).color(MUTED));
                                    } else if waited < Duration::from_secs(15) {
                                        ui.label(RichText::new("Enviando…").size(11.5).color(MUTED));
                                        ui.ctx().request_repaint_after(Duration::from_secs(1));
                                    } else {
                                        ui.label(RichText::new("Não foi enviada.").size(11.5).color(RED));
                                        if small_button(ui, "Tentar de novo", FIELD) {
                                            acts.push(Act::Resend(nonce));
                                        }
                                        if small_button(ui, "Descartar", FIELD) {
                                            acts.push(Act::Discard(nonce));
                                        }
                                    }
                                });
                            }
                        });
                    });
                    let rc = row.response.rect;
                    // mantém a mesma mensagem no lugar quando páginas antigas entram na frente
                    if i == 0 {
                        new_anchor = Some((ch, m.id, rc.top()));
                    }
                    if let Some((_, aid, atop)) = prev_anchor {
                        // durante um salto a rolagem é decidida pelo próprio salto
                        if i > 0 && m.id == aid && s.jump.is_none() && s.jump_wanted.is_none() {
                            let delta = rc.top() - atop;
                            if delta.abs() > 0.5 {
                                ui.scroll_with_delta_animation(egui::vec2(0.0, -delta), egui::style::ScrollAnimation::none());
                            }
                        }
                    }
                    let full = egui::Rect::from_min_max(
                        egui::pos2(ui.max_rect().left() - 28.0, rc.top()),
                        egui::pos2(ui.max_rect().right() + 28.0, rc.bottom() + 0.5),
                    );
                    if let Some(j) = &mut s.jump {
                        if j.id == m.id {
                            if !j.scrolled {
                                j.scrolled = true;
                                ui.scroll_to_rect(full, Some(Align::Center));
                            }
                            let a = (1.0 - j.since.elapsed().as_secs_f32() / 1.8).clamp(0.0, 1.0);
                            ui.painter().set(bg, egui::Shape::rect_filled(full, 0.0, alpha(AMBER, 0.16 * a)));
                            ui.ctx().request_repaint();
                        }
                    }
                    if !editing_this && !is_pending && ui.rect_contains_pointer(full) {
                        ui.painter().set(bg, egui::Shape::rect_filled(full, 0.0, Color32::from_white_alpha(6)));
                        if grouped {
                            ui.painter().text(
                                egui::pos2(rc.left() + 20.0, rc.top() + 11.0),
                                Align2::CENTER_CENTER,
                                fmt_clock(m.ts),
                                egui::FontId::proportional(11.0),
                                MUTED,
                            );
                        }
                        // barra de ações, desenhada sem ocupar espaço no layout
                        let mut items: Vec<(&str, u8)> = vec![("Responder", 0), ("Reagir", 4)];
                        if mine {
                            items.push(("Editar", 1));
                        }
                        if can_delete {
                            items.push(("Apagar", 2));
                        }
                        if can_pin {
                            items.push((if m.pinned { "Desafixar" } else { "Fixar" }, 5));
                        }
                        items.push(("Copiar", 3));
                        let widths: Vec<f32> = items
                            .iter()
                            .map(|(t, _)| ui.painter().layout_no_wrap(t.to_string(), egui::FontId::proportional(13.0), TEXT).size().x + 22.0)
                            .collect();
                        let total: f32 = widths.iter().sum();
                        let bar = egui::Rect::from_min_size(
                            egui::pos2(full.right() - 28.0 - total, full.top() - 1.0),
                            egui::vec2(total, 26.0),
                        );
                        ui.painter().rect_filled(bar, 8.0, L3);
                        ui.painter().rect_stroke(bar, 8.0, egui::Stroke::new(1.0, LINE2), egui::StrokeKind::Inside);
                        let mut x = bar.left();
                        for ((label, kind), w) in items.iter().zip(&widths) {
                            let r = egui::Rect::from_min_size(egui::pos2(x, bar.top()), egui::vec2(*w, 26.0));
                            x += w;
                            let b = ui.interact(r, egui::Id::new(("msg-act", m.id, *kind)), egui::Sense::click());
                            if b.hovered() {
                                ui.painter().rect_filled(r.shrink(1.0), 7.0, L4);
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            let ink = match (*kind, b.hovered()) {
                                (2, true) => RED,
                                (_, true) => TEXT,
                                _ => TEXT2,
                            };
                            ui.painter().text(r.center(), Align2::CENTER_CENTER, *label, egui::FontId::proportional(13.0), ink);
                            if b.clicked() {
                                acts.push(match kind {
                                    0 => Act::Reply(ch, reply_ref.clone()),
                                    1 => Act::StartEdit(m.id, m.content.clone()),
                                    2 => Act::AskDelete(m.id, m.content.clone()),
                                    4 => Act::OpenPicker(m.id, egui::pos2(r.left(), bar.bottom() + 4.0)),
                                    5 => Act::Pin { message: m.id, pin: !m.pinned },
                                    _ => Act::Copy(m.content.clone()),
                                });
                            }
                        }
                    }
                    ui.add_space(if grouped { 1.0 } else { 8.0 });
                }
                s.anchor = new_anchor;
            });
        // chegou perto do topo: busca as mensagens mais antigas
        if scroll.state.offset().y < 150.0 {
            s.load_older(ch);
        }

        if s.jump.as_ref().is_some_and(|j| j.since.elapsed() > Duration::from_millis(1800)) {
            s.jump = None;
        }

        // "fulano está digitando…" (a linha reserva a altura mesmo vazia)
        let now = Instant::now();
        let typers: Vec<String> = {
            let mut ids: Vec<i64> = s
                .typing
                .iter()
                .filter(|((c, u), t)| *c == ch && *u != me_id && now.duration_since(**t) < Duration::from_secs(8))
                .map(|((_, u), _)| *u)
                .collect();
            ids.sort();
            ids.into_iter().map(|u| s.name_of(u)).collect()
        };
        if !typers.is_empty() {
            ui.ctx().request_repaint_after(Duration::from_millis(800));
        }
        ui.allocate_ui(egui::vec2(ui.available_width(), 22.0), |ui| {
            ui.add_space(4.0);
            let text = match typers.len() {
                0 => String::new(),
                1 => format!("{} está digitando…", typers[0]),
                2 => format!("{} e {} estão digitando…", typers[0], typers[1]),
                3 => format!("{}, {} e {} estão digitando…", typers[0], typers[1], typers[2]),
                _ => "Várias pessoas estão digitando…".into(),
            };
            ui.label(RichText::new(text).size(12.5).color(TEXT2));
        });

        // "respondendo a…"
        if let Some((rch, r)) = s.replying.clone() {
            if rch == ch {
                egui::Frame::new()
                    .fill(L2)
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(egui::Margin::symmetric(14, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Respondendo a").size(13.0).color(MUTED));
                            ui.label(RichText::new(&r.author).size(13.0).strong().color(TEXT));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if small_button(ui, "Cancelar", FIELD) {
                                    acts.push(Act::CancelReply);
                                }
                            });
                        });
                    });
                ui.add_space(4.0);
            }
        }

        // entrada
        let input_id = egui::Id::new(("compose", ch));
        let focused = ctx.memory(|mem| mem.has_focus(input_id));
        egui::Frame::new()
            .fill(L3)
            .stroke(egui::Stroke::new(1.0, if focused { AMBER } else { LINE2 }))
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(egui::Margin { left: 10, right: 8, top: 8, bottom: 8 })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                // bandeja de anexos pendentes: no topo da caixa, miniaturas 120x120
                if s.uploads.iter().any(|u| u.channel == ch) {
                    ui.add_space(4.0);
                    egui::ScrollArea::horizontal().id_salt(("tray", ch)).auto_shrink([false, true]).show(ui, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
                            for u in s.uploads.iter().filter(|u| u.channel == ch) {
                                upload_tile(ui, u, &mut s.images, acts);
                            }
                        });
                    });
                    ui.add_space(8.0);
                    let y = ui.cursor().top();
                    let x = ui.max_rect().x_range();
                    ui.painter().hline(egui::Rangef::new(x.min - 10.0, x.max + 8.0), y, egui::Stroke::new(1.0, LINE2));
                    ui.add_space(9.0);
                }
                ui.horizontal(|ui| {
                    // "+" anexa um arquivo (também dá para arrastar e soltar na janela)
                    let (pr, presp) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::click());
                    let hot = presp.hovered();
                    ui.painter().circle_filled(pr.center(), 14.0, if hot { L4 } else { L2 });
                    ui.painter().text(pr.center(), Align2::CENTER_CENTER, "+", egui::FontId::proportional(20.0), if hot { TEXT } else { TEXT2 });
                    if hot {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if presp.on_hover_text("Anexar arquivo").clicked() {
                        acts.push(Act::PickFile);
                    }
                    let r = ui.add_sized(
                        [ui.available_width() - 84.0, 32.0],
                        egui::TextEdit::singleline(&mut s.input)
                            .id(input_id)
                            .frame(egui::Frame::NONE)
                            .hint_text(hint)
                            .vertical_align(Align::Center)
                            .margin(egui::Margin::symmetric(0, 6)),
                    );
                    if s.focus_composer {
                        s.focus_composer = false;
                        r.request_focus();
                    }
                    if r.changed() && !s.input.trim().is_empty() && s.last_typing.elapsed() > Duration::from_secs(4) {
                        s.last_typing = Instant::now();
                        acts.push(Act::Send(C2S::Typing { channel_id: ch }));
                    }
                    if r.has_focus() && replying_here && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        acts.push(Act::CancelReply);
                    }
                    let enter = r.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter));
                    let clicked = small_button(ui, "Enviar", BLURPLE);
                    let atts: Vec<p::Attachment> = s
                        .uploads
                        .iter()
                        .filter(|u| u.channel == ch)
                        .filter_map(|u| if let files::UpState::Done(a) = &u.state { Some(a.clone()) } else { None })
                        .collect();
                    let uploading = s.uploads.iter().any(|u| u.channel == ch && matches!(u.state, files::UpState::Sending));
                    if (enter || clicked) && (!s.input.trim().is_empty() || !atts.is_empty()) && !uploading {
                        let reply = s.replying.as_ref().filter(|(c, _)| *c == ch).map(|(_, r)| r.clone());
                        let had_reply = reply.is_some();
                        acts.push(Act::SendMsg { ch, content: std::mem::take(&mut s.input), reply, attachments: atts });
                        if had_reply {
                            acts.push(Act::CancelReply);
                        }
                        r.request_focus();
                    }
                });
            });
        ui.add_space(22.0);
    }

    // ---- amigos
    fn friends_view(&mut self, ui: &mut egui::Ui, s: &mut Session, acts: &mut Vec<Act>) {
        let pending = s.snap.friends.iter().filter(|f| f.status != p::FriendStatus::Friend).count();
        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 63.0), Layout::left_to_right(Align::Center), |ui| {
            ui.label(serif("Amigos", 24.0));
            ui.add_space(12.0);
            ui.selectable_value(&mut s.tab, HomeTab::Online, "Disponível");
            ui.selectable_value(&mut s.tab, HomeTab::All, "Todos");
            ui.selectable_value(&mut s.tab, HomeTab::Pending, format!("Pendente ({pending})"));
            ui.selectable_value(&mut s.tab, HomeTab::Blocked, "Bloqueados");
            ui.selectable_value(&mut s.tab, HomeTab::Add, "Adicionar amigo");
        });
        rule_bleed(ui, 28.0);

        if s.tab == HomeTab::Blocked {
            ui.add_space(8.0);
            if s.snap.blocked.is_empty() {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| ui.label(RichText::new("Você não bloqueou ninguém.").color(MUTED)));
                return;
            }
            ui.label(caps(&format!("Bloqueados — {}", s.snap.blocked.len())));
            ui.add_space(4.0);
            for u in &s.snap.blocked {
                ui.horizontal(|ui| {
                    avatar(ui, &u.name, u.id, 36.0, None, BG);
                    ui.label(RichText::new(&u.name).strong().color(TEXT));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if small_button(ui, "Desbloquear", FIELD) {
                            acts.push(Act::Send(C2S::Unblock { user_id: u.id }));
                        }
                    });
                });
                ui.separator();
            }
            return;
        }
        if s.tab == HomeTab::Add {
            ui.add_space(8.0);
            ui.label(RichText::new("ADICIONAR AMIGO").strong());
            ui.label(RichText::new("Você pode adicionar amigos com o nome de usuário deles.").color(MUTED));
            ui.horizontal(|ui| {
                let r = ui.add_sized(
                    [ui.available_width() - 140.0, 36.0],
                    egui::TextEdit::singleline(&mut s.add_friend)
                        .hint_text("Digite um nome de usuário")
                        .margin(egui::Margin::symmetric(10, 8)),
                );
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                let click = small_button(ui, "Enviar pedido", BLURPLE);
                if (enter || click) && !s.add_friend.trim().is_empty() {
                    acts.push(Act::Send(C2S::FriendRequest { username: std::mem::take(&mut s.add_friend) }));
                    s.toast = Some(("Pedido de amizade enviado".into(), Instant::now()));
                }
            });
            return;
        }

        let list: Vec<&p::Friend> = s
            .snap
            .friends
            .iter()
            .filter(|f| match s.tab {
                HomeTab::Online => f.status == p::FriendStatus::Friend && f.user.online,
                HomeTab::All => f.status == p::FriendStatus::Friend,
                HomeTab::Pending => f.status != p::FriendStatus::Friend,
                HomeTab::Add | HomeTab::Blocked => false,
            })
            .collect();
        ui.add_space(4.0);
        if list.is_empty() {
            ui.add_space(30.0);
            ui.vertical_centered(|ui| {
                ui.label(
                    RichText::new(match s.tab {
                        HomeTab::Online => "Ninguém disponível agora.",
                        HomeTab::All => "Você ainda não tem amigos. Use \"Adicionar amigo\".",
                        _ => "Nenhum pedido pendente.",
                    })
                    .color(MUTED),
                );
            });
            return;
        }
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for f in list {
                egui::Frame::new()
                    .inner_margin(egui::Margin::symmetric(6, 6))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let av = avatar(ui, &f.user.name, f.user.id, 36.0, Some(f.user.status), BG);
                            egui::Popup::context_menu(&av).width(180.0).show(|ui| {
                                ui.set_min_width(170.0);
                                if ui.button(RichText::new("Bloquear").color(RED)).clicked() {
                                    acts.push(Act::Modal(Modal::ConfirmBlock { user_id: f.user.id, name: f.user.name.clone() }));
                                    ui.close();
                                }
                            });
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                ui.label(RichText::new(&f.user.name).strong());
                                ui.label(
                                    RichText::new(match f.status {
                                        p::FriendStatus::Friend if f.user.online => status_label(f.user.status),
                                        p::FriendStatus::Friend => "Offline",
                                        p::FriendStatus::Incoming => "Pedido de amizade recebido",
                                        p::FriendStatus::Outgoing => "Pedido de amizade enviado",
                                    })
                                    .size(12.0)
                                    .color(MUTED),
                                );
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| match f.status {
                                p::FriendStatus::Friend => {
                                    if small_button(ui, "Remover", FIELD) {
                                        acts.push(Act::Send(C2S::FriendRemove { user_id: f.user.id }));
                                    }
                                    if small_button(ui, "Mensagem", BLURPLE) {
                                        acts.push(Act::Send(C2S::OpenDm { user_id: f.user.id }));
                                    }
                                }
                                p::FriendStatus::Incoming => {
                                    if small_button(ui, "Recusar", RED) {
                                        acts.push(Act::Send(C2S::FriendRemove { user_id: f.user.id }));
                                    }
                                    if small_button(ui, "Aceitar", GREEN) {
                                        acts.push(Act::Send(C2S::FriendAccept { user_id: f.user.id }));
                                    }
                                }
                                p::FriendStatus::Outgoing => {
                                    if small_button(ui, "Cancelar", FIELD) {
                                        acts.push(Act::Send(C2S::FriendRemove { user_id: f.user.id }));
                                    }
                                }
                            });
                        });
                    });
                ui.separator();
            }
        });
    }

    // ---- vídeo da transmissão
    fn stream_area(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, s: &Session, acts: &mut Vec<Act>, full: bool) {
        let Some((ch, h)) = &self.watching else { return };
        if let Some(img) = h.frame.lock().unwrap().take() {
            match &mut self.texture {
                Some(t) => t.set(img, egui::TextureOptions::LINEAR),
                None => self.texture = Some(ctx.load_texture("remote", img, egui::TextureOptions::LINEAR)),
            }
        }
        let st = h.status.lock().unwrap().clone();
        let size = if full {
            ui.available_size()
        } else {
            egui::vec2(ui.available_width(), (ui.available_height() * 0.55).clamp(180.0, 560.0))
        };
        let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
        let p = ui.painter();
        p.rect_filled(rect, egui::CornerRadius::same(if full { 0 } else { 8 }), Color32::BLACK);
        match &self.texture {
            Some(tex) => {
                let ts = tex.size_vec2();
                let scale = (rect.width() / ts.x).min(rect.height() / ts.y);
                let r = egui::Rect::from_center_size(rect.center(), ts * scale);
                p.image(
                    tex.id(),
                    r,
                    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                    Color32::WHITE,
                );
            }
            None => {
                p.text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    if st.starts_with("erro") { st.clone() } else { "Aguardando vídeo…".to_string() },
                    egui::FontId::proportional(15.0),
                    MUTED,
                );
            }
        }
        let who = s
            .snap
            .live
            .iter()
            .find(|l| l.channel_id == *ch)
            .map(|l| s.name_of(l.user_id))
            .unwrap_or_default();
        let label = format!("AO VIVO · {who}");
        let galley = p.layout_no_wrap(label, egui::FontId::proportional(12.0), Color32::WHITE);
        let pill = egui::Rect::from_min_size(rect.min + egui::vec2(10.0, 10.0), galley.size() + egui::vec2(22.0, 10.0));
        p.rect_filled(pill, egui::CornerRadius::same(5), Color32::from_black_alpha(170));
        p.circle_filled(pill.left_center() + egui::vec2(10.0, 0.0), 4.0, status_color(&st));
        p.galley(pill.min + egui::vec2(20.0, 5.0), galley, Color32::WHITE);

        if resp.double_clicked() {
            acts.push(Act::Fullscreen);
        }
        let btn = egui::Rect::from_min_size(rect.right_top() + egui::vec2(-78.0, 8.0), egui::vec2(68.0, 26.0));
        if ui.put(btn, egui::Button::new("Fechar").fill(Color32::from_black_alpha(170))).clicked() {
            acts.push(Act::StopWatch);
        }
        ui.add_space(6.0);
    }

    // ---- janelas modais
    fn modals(&mut self, ctx: &egui::Context, s: &mut Session, acts: &mut Vec<Act>) {
        let mut close = false;
        let mut dismissed = false; // Esc, clique fora ou X
        let cur = s.current_channel();
        match &mut s.modal {
            Modal::None => {}
            Modal::AddServer { create, name, code } => {
                modal(ctx, "Adicionar um servidor", 360.0, &mut dismissed, |ui| {
                        ui.set_width(320.0);
                        ui.horizontal(|ui| {
                            ui.selectable_value(create, true, "Criar o meu");
                            ui.selectable_value(create, false, "Entrar com convite");
                        });
                        ui.add_space(6.0);
                        if *create {
                            ui.label(caps(&"NOME DO SERVIDOR"));
                            ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
                            if big_button(ui, "Criar", BLURPLE) && !name.trim().is_empty() {
                                acts.push(Act::Send(C2S::CreateServer { name: name.clone() }));
                                close = true;
                            }
                        } else {
                            ui.label(caps(&"CÓDIGO DO CONVITE"));
                            ui.add(egui::TextEdit::singleline(code).desired_width(f32::INFINITY));
                            if big_button(ui, "Entrar no servidor", BLURPLE) && !code.trim().is_empty() {
                                acts.push(Act::Send(C2S::JoinServer { code: code.clone() }));
                                close = true;
                            }
                        }
                        if small_button(ui, "Cancelar", FIELD) {
                            close = true;
                        }
                    });
            }
            Modal::NewChannel { server, name, kind, parent } => {
                let cats: Vec<(i64, String)> = s
                    .snap
                    .servers
                    .iter()
                    .find(|x| x.id == *server)
                    .map(|sv| sv.channels.iter().filter(|c| c.kind == 4).map(|c| (c.id, c.name.clone())).collect())
                    .unwrap_or_default();
                modal(ctx, if *kind == 4 { "Criar categoria" } else { "Criar canal" }, 360.0, &mut dismissed, |ui| {
                        ui.set_width(320.0);
                        ui.horizontal(|ui| {
                            ui.selectable_value(kind, 0, "Canal de texto");
                            ui.selectable_value(kind, 4, "Categoria");
                        });
                        ui.add_space(6.0);
                        ui.label(caps(if *kind == 4 { "Nome da categoria" } else { "Nome do canal" }));
                        ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
                        if *kind == 0 && !cats.is_empty() {
                            ui.label(caps("Categoria"));
                            let shown = parent
                                .and_then(|p| cats.iter().find(|c| c.0 == p).map(|c| c.1.clone()))
                                .unwrap_or_else(|| "Nenhuma".into());
                            egui::ComboBox::from_id_salt("newchan-parent").width(ui.available_width()).selected_text(shown).show_ui(ui, |ui| {
                                ui.selectable_value(parent, None, "Nenhuma");
                                for (id, n) in &cats {
                                    ui.selectable_value(parent, Some(*id), n);
                                }
                            });
                        }
                        ui.add_space(6.0);
                        if big_button(ui, "Criar", BLURPLE) && !name.trim().is_empty() {
                            acts.push(Act::Send(C2S::CreateChannel {
                                server_id: *server,
                                name: name.clone(),
                                kind: *kind,
                                parent_id: if *kind == 0 { *parent } else { None },
                            }));
                            close = true;
                        }
                        if small_button(ui, "Cancelar", FIELD) {
                            close = true;
                        }
                    });
            }
            Modal::Preview(a) => {
                let (base, token) = (s.http_base.clone(), s.token.clone());
                let thumb = match s.images.get(ctx, &base, &token, a) {
                    files::Img::Ready(t) => Some(t.clone()),
                    _ => None,
                };
                let tex = match s.images.get_full(ctx, &base, &token, a) {
                    files::Img::Ready(t) => Some(t.clone()),
                    _ => thumb,
                };
                let screen = ctx.content_rect();
                let max = egui::vec2((screen.width() - 120.0).max(200.0), (screen.height() - 150.0).max(150.0));
                let m = egui::Modal::new(egui::Id::new("img-preview"))
                    .backdrop_color(egui::Color32::from_black_alpha(220))
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| {
                        ui.vertical_centered(|ui| {
                            match &tex {
                                Some(t) => {
                                    let sz = t.size_vec2();
                                    let k = (max.x / sz.x).min(max.y / sz.y);
                                    ui.add(egui::Image::new(t).fit_to_exact_size(sz * k).corner_radius(egui::CornerRadius::same(6)));
                                }
                                None => {
                                    ui.add_space(max.y / 2.0);
                                    ui.label(RichText::new("Carregando…").color(MUTED));
                                    ui.add_space(max.y / 2.0);
                                    ctx.request_repaint_after(Duration::from_millis(200));
                                }
                            }
                            ui.add_space(10.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("{} · {}", a.name, files::human_size(a.size))).size(13.0).color(TEXT2));
                                if small_button(ui, "Baixar", FIELD) {
                                    ctx.open_url(egui::OpenUrl::new_tab(files::url(&base, a, &token)));
                                }
                                if small_button(ui, "Fechar", FIELD) {
                                    ui.close();
                                }
                            });
                        });
                    });
                if m.should_close() {
                    dismissed = true;
                }
            }
            Modal::Account { old, new, again } => {
                let sessions = s.sessions.clone();
                modal(ctx, "Minha conta", 440.0, &mut dismissed, |ui| {
                    ui.set_width(400.0);
                    ui.label(caps("TROCAR SENHA"));
                    ui.add(egui::TextEdit::singleline(old).password(true).hint_text("Senha atual").desired_width(f32::INFINITY));
                    ui.add(egui::TextEdit::singleline(new).password(true).hint_text("Nova senha (8 a 128 caracteres)").desired_width(f32::INFINITY));
                    ui.add(egui::TextEdit::singleline(again).password(true).hint_text("Repita a nova senha").desired_width(f32::INFINITY));
                    let mismatch = !again.is_empty() && new != again;
                    if mismatch {
                        ui.label(RichText::new("As senhas não conferem.").size(12.5).color(RED));
                    }
                    let ready = !old.is_empty() && (8..=128).contains(&new.chars().count()) && new == again;
                    ui.add_enabled_ui(ready, |ui| {
                        if big_button(ui, "Trocar senha", BLURPLE) {
                            acts.push(Act::Send(C2S::ChangePassword { old: old.clone(), new: new.clone() }));
                        }
                    });
                    ui.add_space(10.0);
                    ui.label(caps("SESSÕES ABERTAS"));
                    let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
                    for ss in &sessions {
                        egui::Frame::new().fill(L2).corner_radius(egui::CornerRadius::same(8)).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    let title = if ss.current { format!("{} · esta sessão", ss.device) } else { ss.device.clone() };
                                    ui.label(RichText::new(if ss.device.is_empty() { "Origem desconhecida".to_string() } else { title }).color(TEXT));
                                    ui.label(RichText::new(format!("Último uso {} · aberta {}", ago(now_ms, ss.last_used), ago(now_ms, ss.created))).size(12.5).color(MUTED));
                                });
                                if !ss.current {
                                    ui.with_layout(egui::Layout::right_to_left(Align::Center), |ui| {
                                        if small_button(ui, "Encerrar", FIELD) {
                                            acts.push(Act::Send(C2S::RevokeSession { id: ss.id }));
                                        }
                                    });
                                }
                            });
                        });
                    }
                    if sessions.iter().any(|x| !x.current) && small_button(ui, "Encerrar todas as outras", FIELD) {
                        acts.push(Act::Send(C2S::RevokeOtherSessions));
                    }
                    if small_button(ui, "Fechar", FIELD) {
                        close = true;
                    }
                });
            }
            Modal::ConfirmBlock { user_id, name } => {
                modal(ctx, "Bloquear usuário", 360.0, &mut dismissed, |ui| {
                    ui.label(RichText::new(format!("Bloquear {name}?")).strong().color(TEXT));
                    ui.add_space(4.0);
                    ui.label(RichText::new("Vocês deixam de ser amigos, ninguém abre conversa privada com o outro e as mensagens dela(e) nos servidores ficam escondidas para você.").color(TEXT2));
                    ui.add_space(10.0);
                    if big_button(ui, "Bloquear", RED) {
                        acts.push(Act::Send(C2S::Block { user_id: *user_id }));
                        close = true;
                    }
                    if small_button(ui, "Cancelar", FIELD) {
                        close = true;
                    }
                });
            }
            Modal::DeleteMsg { id, preview } => {
                modal(ctx, "Apagar mensagem", 360.0, &mut dismissed, |ui| {
                        ui.set_width(340.0);
                        ui.label("Tem certeza de que quer apagar esta mensagem?");
                        ui.add_space(6.0);
                        egui::Frame::new()
                            .fill(L2)
                            .corner_radius(egui::CornerRadius::same(8))
                            .inner_margin(egui::Margin::same(10))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                let shown: String = preview.chars().take(200).collect();
                                ui.label(RichText::new(shown).color(TEXT2));
                            });
                        ui.add_space(8.0);
                        if big_button(ui, "Apagar", RED) {
                            acts.push(Act::Send(C2S::Delete { message_id: *id }));
                            close = true;
                        }
                        if small_button(ui, "Cancelar", FIELD) {
                            close = true;
                        }
                    });
            }
            Modal::Invite(code) => {
                modal(ctx, "Convidar amigos", 360.0, &mut dismissed, |ui| {
                        ui.set_width(300.0);
                        ui.label("Mande este código para quem você quer convidar:");
                        ui.label(RichText::new(code.as_str()).size(22.0).strong().monospace());
                        ui.horizontal(|ui| {
                            if small_button(ui, "Copiar", BLURPLE) {
                                ctx.copy_text(code.clone());
                            }
                            if small_button(ui, "Fechar", FIELD) {
                                close = true;
                            }
                        });
                    });
            }
            Modal::GoLive => {
                modal(ctx, "Transmitir tela", 360.0, &mut dismissed, |ui| {
                        ui.set_width(320.0);
                        ui.label(caps(&"TELA"));
                        egui::ComboBox::from_id_salt("mon")
                            .width(ui.available_width())
                            .selected_text(self.monitors.get(self.monitor).cloned().unwrap_or_default())
                            .show_ui(ui, |ui| {
                                for (i, n) in self.monitors.iter().enumerate() {
                                    ui.selectable_value(&mut self.monitor, i, n);
                                }
                            });
                        ui.label(caps(&"QUADROS POR SEGUNDO"));
                        ui.horizontal_wrapped(|ui| {
                            for f in [30, 60, 90, 120, 144] {
                                ui.selectable_value(&mut self.fps, f, f.to_string());
                            }
                        });
                        ui.label(caps(&"QUALIDADE (MBPS)"));
                        ui.horizontal_wrapped(|ui| {
                            for (k, l) in [(4000, "4"), (8000, "8"), (15000, "15"), (30000, "30")] {
                                ui.selectable_value(&mut self.kbps, k, l);
                            }
                        });
                        ui.add_space(4.0);
                        if let Some(ch) = cur {
                            if big_button(ui, "Ao vivo", GREEN) {
                                acts.push(Act::Publish(ch));
                                close = true;
                            }
                        } else {
                            ui.label(RichText::new("Abra um canal ou conversa para transmitir.").color(MUTED));
                        }
                        if small_button(ui, "Cancelar", FIELD) {
                            close = true;
                        }
                    });
            }
        }
        if close || dismissed {
            s.modal = Modal::None;
        }
    }

    fn toast(&mut self, ctx: &egui::Context, s: &mut Session) {
        if let Some((msg, t)) = &s.toast {
            if t.elapsed() > Duration::from_secs(4) {
                s.toast = None;
                return;
            }
            ctx.request_repaint_after(Duration::from_millis(500));
            egui::Area::new(egui::Id::new("toast"))
                .anchor(Align2::CENTER_BOTTOM, [0.0, -24.0])
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(L3)
                        .stroke(egui::Stroke::new(1.0, LINE2))
                        .corner_radius(egui::CornerRadius::same(10))
                        .inner_margin(egui::Margin::same(12))
                        .show(ui, |ui| ui.label(msg));
                });
        }
    }
}
