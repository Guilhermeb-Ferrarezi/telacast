mod gateway;
mod gpu;
mod net;
mod publisher;
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
            Ok(Box::new(App::new(cc)))
        }),
    )
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
    Add,
}

enum Modal {
    None,
    AddServer { create: bool, name: String, code: String },
    NewChannel { server: i64, name: String },
    Invite(String),
    GoLive,
}

enum Act {
    Open(View),
    OpenServer(i64),
    Send(C2S),
    Modal(Modal),
    Logout,
    Watch(i64),
    StopWatch,
    Publish(i64),
    StopPublish,
    Fullscreen,
}

struct Session {
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
    toast: Option<(String, Instant)>,
    last_channel: HashMap<i64, i64>,
    view_since: Instant,
    seen: HashMap<i64, Instant>,
}

impl Session {
    fn new(gw: Gateway, token: String, me: p::User) -> Self {
        Self {
            gw,
            token,
            snap: p::Snapshot { me: me.clone(), servers: vec![], friends: vec![], dms: vec![], live: vec![] },
            me,
            view: View::Home,
            tab: HomeTab::Online,
            msgs: HashMap::new(),
            requested: HashSet::new(),
            unread: HashSet::new(),
            input: String::new(),
            add_friend: String::new(),
            modal: Modal::None,
            toast: None,
            last_channel: HashMap::new(),
            view_since: Instant::now(),
            seen: HashMap::new(),
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
            self.unread.remove(&ch);
            if self.requested.insert(ch) {
                self.gw.send(C2S::History { channel_id: ch, before: None });
            }
        }
        self.view = v;
        self.view_since = Instant::now();
    }

    fn open_server(&mut self, id: i64) {
        let Some(sv) = self.snap.servers.iter().find(|s| s.id == id) else { return };
        let wanted = self.last_channel.get(&id).copied();
        let ch = sv
            .channels
            .iter()
            .find(|c| Some(c.id) == wanted)
            .or(sv.channels.first())
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
        while let Ok(ev) = self.gw.rx.try_recv() {
            match ev {
                Event::Closed(why) => return Some(why),
                Event::Connected => {}
                Event::Msg(m) => match m {
                    S2C::Snapshot(sn) => {
                        self.me = sn.me.clone();
                        self.snap = sn;
                        self.fix_view();
                    }
                    S2C::Message(m) => {
                        let ch = m.channel_id;
                        let mine = m.author_id == self.me.id;
                        if self.requested.contains(&ch) {
                            self.msgs.entry(ch).or_default().push(m);
                        }
                        if !mine && self.current_channel() != Some(ch) {
                            self.unread.insert(ch);
                        }
                    }
                    S2C::History { channel_id, messages } => {
                        self.msgs.insert(channel_id, messages);
                    }
                    S2C::DmOpened { channel_id } => self.open(View::Dm(channel_id)),
                    S2C::ServerJoined { server_id } => self.open_server(server_id),
                    S2C::Error { msg } => self.toast = Some((msg, Instant::now())),
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
        if !token.is_empty() {
            login.gw = Some(gateway::open(&login.server, cc.egui_ctx.clone()));
            login.pending = Some(C2S::Resume { token: token.clone() });
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
        }
    }

    fn start_login(&mut self, ctx: &egui::Context) {
        let l = &mut self.login;
        l.error.clear();
        l.gw = Some(gateway::open(&l.server, ctx.clone()));
        let (username, password, invite) = (l.user.trim().to_string(), l.pass.clone(), l.invite.clone());
        l.pending = Some(if l.register {
            C2S::Register { username, password, invite }
        } else {
            C2S::Login { username, password }
        });
    }

    fn poll_login(&mut self) {
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
                    self.session = Some(Session::new(gw, token, me));
                    return;
                }
                Event::Msg(S2C::Error { msg }) => {
                    if self.login.resuming {
                        self.token_saved.clear();
                    } else {
                        self.login.error = msg;
                    }
                    self.login.resuming = false;
                    self.login.gw = None;
                }
                Event::Closed(why) => {
                    if !self.login.resuming {
                        self.login.error = why;
                    }
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
        self.poll_login();
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

fn initials(name: &str) -> String {
    let mut it = name.split_whitespace().filter_map(|w| w.chars().next());
    match (it.next(), it.next()) {
        (Some(a), Some(b)) => format!("{a}{b}").to_uppercase(),
        (Some(a), None) => name.chars().take(2).collect::<String>().to_uppercase().replace(a, &a.to_uppercase().to_string()),
        _ => "?".into(),
    }
}

impl App {
    /// Desenha a sessão. Devolve false se o usuário saiu.
    fn session_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, s: &mut Session) -> bool {
        let mut acts: Vec<Act> = Vec::new();

        if self.publishing.is_some() || self.watching.is_some() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
        if self.fullscreen && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            acts.push(Act::Fullscreen);
        }

        if !self.fullscreen {
            self.rail(ui, s, &mut acts);
            self.channel_panel(ui, s, &mut acts);
            if let View::Chan { server, .. } = s.view {
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
        self.modals(ctx, s, &mut acts);
        self.toast(ctx, s);

        let mut keep = true;
        for a in acts {
            match a {
                Act::Open(v) => s.open(v),
                Act::OpenServer(id) => s.open_server(id),
                Act::Send(m) => s.gw.send(m),
                Act::Modal(m) => s.modal = m,
                Act::Logout => keep = false,
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
            .frame(egui::Frame::new().fill(RAIL).inner_margin(egui::Margin::symmetric(13, 14)))
            .resizable(false)
            .exact_size(68.0)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                let home = matches!(s.view, View::Home | View::Dm(_));
                ui.vertical_centered(|ui| ui.label(serif("t", 26.0)));
                if rail_button(ui, "DM", home, BLURPLE).on_hover_text("Mensagens diretas").clicked() {
                    acts.push(Act::Open(View::Home));
                }
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 8.0;
                    for sv in &s.snap.servers {
                        let sel = matches!(&s.view, View::Chan { server, .. } if *server == sv.id);
                        if rail_button(ui, &initials(&sv.name), sel, BLURPLE).on_hover_text(&sv.name).clicked() {
                            acts.push(Act::OpenServer(sv.id));
                        }
                    }
                    if rail_button(ui, "+", false, GREEN).on_hover_text("Adicionar um servidor").clicked() {
                        acts.push(Act::Modal(Modal::AddServer { create: true, name: String::new(), code: String::new() }));
                    }
                });
            });
        edge(ui, pr.response.rect, true);
    }

    // ---- canais / DMs
    fn channel_panel(&mut self, ui: &mut egui::Ui, s: &Session, acts: &mut Vec<Act>) {
        let pr = egui::Panel::left("channels")
            .frame(egui::Frame::new().fill(SIDE).inner_margin(egui::Margin::same(10)))
            .resizable(false)
            .exact_size(240.0)
            .show(ui, |ui| {
                let list_h = (ui.available_height() - 52.0).max(50.0);
                ui.allocate_ui(egui::vec2(ui.available_width(), list_h), |ui| {
                    match &s.view {
                        View::Chan { server, channel } => {
                            let Some(sv) = s.snap.servers.iter().find(|x| x.id == *server) else { return };
                            ui.horizontal(|ui| {
                                ui.label(serif(&sv.name, 22.0));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if small_button(ui, "Convite", FIELD) {
                                        acts.push(Act::Modal(Modal::Invite(sv.invite.clone())));
                                    }
                                });
                            });
                            ui.separator();
                            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(caps(&"CANAIS DE TEXTO"));
                                    if sv.owner == s.me.id {
                                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                            if ui.small_button("+").on_hover_text("Criar canal").clicked() {
                                                acts.push(Act::Modal(Modal::NewChannel { server: sv.id, name: String::new() }));
                                            }
                                        });
                                    }
                                });
                                for c in &sv.channels {
                                    let live = s.snap.live.iter().any(|l| l.channel_id == c.id);
                                    let unread = s.unread.contains(&c.id);
                                    let r = list_row(ui, c.id == *channel, |ui| {
                                        let col = if unread || c.id == *channel { TEXT } else { TEXT2 };
                                        let t = RichText::new(format!("# {}", c.name)).color(col);
                                        ui.label(if unread { t.strong() } else { t });
                                        if live {
                                            ui.label(RichText::new("AO VIVO").size(10.0).strong().color(AMBER));
                                        }
                                    });
                                    if r.clicked() {
                                        acts.push(Act::Open(View::Chan { server: sv.id, channel: c.id }));
                                    }
                                }
                            });
                        }
                        _ => {
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
                            ui.label(caps(&"MENSAGENS DIRETAS"));
                            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                                for d in &s.snap.dms {
                                    let sel = matches!(s.view, View::Dm(c) if c == d.channel_id);
                                    let unread = s.unread.contains(&d.channel_id);
                                    let r = list_row(ui, sel, |ui| {
                                        avatar(ui, &d.user.name, d.user.id, 24.0, Some(d.user.online), SIDE);
                                        let t = RichText::new(&d.user.name);
                                        ui.label(if unread { t.strong() } else { t.color(MUTED) });
                                    });
                                    if r.clicked() {
                                        acts.push(Act::Open(View::Dm(d.channel_id)));
                                    }
                                }
                            });
                        }
                    }
                });
                // cartão do usuário
                egui::Frame::new()
                    .fill(L2)
                    .corner_radius(egui::CornerRadius::same(10))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            avatar(ui, &s.me.name, s.me.id, 32.0, Some(true), L2);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                ui.label(RichText::new(&s.me.name).strong());
                                ui.label(RichText::new("Online").size(11.0).color(MUTED));
                            });
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                if small_button(ui, "Sair", FIELD) {
                                    acts.push(Act::Logout);
                                }
                            });
                        });
                    });
            });
        edge(ui, pr.response.rect, true);
    }

    // ---- membros
    fn members_panel(&mut self, ui: &mut egui::Ui, s: &Session, server: i64, acts: &mut Vec<Act>) {
        let Some(sv) = s.snap.servers.iter().find(|x| x.id == server) else { return };
        let pr = egui::Panel::right("members")
            .frame(egui::Frame::new().fill(SIDE).inner_margin(egui::Margin::same(10)))
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
                                avatar(ui, &m.name, m.id, 28.0, Some(m.online), SIDE);
                                let t = RichText::new(&m.name);
                                ui.label(if online { t } else { t.color(MUTED) });
                                if m.id == sv.owner {
                                    ui.label(RichText::new("dono").size(10.0).color(YELLOW));
                                }
                            });
                            if r.clicked() && m.id != s.me.id {
                                acts.push(Act::Send(C2S::OpenDm { user_id: m.id }));
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
                self.chat_view(ui, ctx, s, acts, ch, &title, "Enviar mensagem");
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
                self.chat_view(ui, ctx, s, acts, channel, &name, &format!("Conversar em #{name}"));
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
        hint: &str,
    ) {
        // cabeçalho
        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 63.0), Layout::left_to_right(Align::Center), |ui| {
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
        let input_h = 82.0;
        let h = (ui.available_height() - input_h).max(60.0);
        egui::ScrollArea::vertical()
            .id_salt(("chat", ch))
            .max_height(h)
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
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
                    let grouped = !new_day
                        && msgs[i - 1].author_id == m.author_id
                        && m.ts - msgs[i - 1].ts < 5 * 60 * 1000;
                    let born = *s.seen.entry(m.id).or_insert_with(Instant::now);
                    let age = born.elapsed().as_secs_f32() / 0.22;
                    ui.horizontal_top(|ui| {
                        if age < 1.0 {
                            ui.set_opacity(ease_out(age));
                            ui.ctx().request_repaint();
                        }
                        if grouped {
                            ui.add_space(50.0);
                        } else {
                            avatar(ui, &m.author, m.author_id, 36.0, None, BG);
                            ui.add_space(6.0);
                        }
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            if !grouped {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&m.author).strong().color(TEXT));
                                    ui.label(RichText::new(fmt_clock(m.ts)).size(11.5).color(MUTED));
                                });
                            }
                            ui.add(egui::Label::new(RichText::new(&m.content).color(TEXT2)).wrap());
                        });
                    });
                    ui.add_space(if grouped { 1.0 } else { 8.0 });
                }
            });

        // entrada
        ui.add_space(10.0);
        let input_id = egui::Id::new(("compose", ch));
        let focused = ctx.memory(|mem| mem.has_focus(input_id));
        egui::Frame::new()
            .fill(L3)
            .stroke(egui::Stroke::new(1.0, if focused { AMBER } else { LINE2 }))
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(egui::Margin { left: 16, right: 8, top: 8, bottom: 8 })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let r = ui.add_sized(
                        [ui.available_width() - 84.0, 32.0],
                        egui::TextEdit::singleline(&mut s.input)
                            .id(input_id)
                            .frame(egui::Frame::NONE)
                            .hint_text(hint)
                            .margin(egui::Margin::symmetric(0, 6)),
                    );
                    let enter = r.lost_focus() && ctx.input(|i| i.key_pressed(egui::Key::Enter));
                    let clicked = small_button(ui, "Enviar", BLURPLE);
                    if (enter || clicked) && !s.input.trim().is_empty() {
                        acts.push(Act::Send(C2S::Send { channel_id: ch, content: std::mem::take(&mut s.input) }));
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
            ui.selectable_value(&mut s.tab, HomeTab::Add, "Adicionar amigo");
        });
        rule_bleed(ui, 28.0);

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
                HomeTab::Add => false,
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
                            avatar(ui, &f.user.name, f.user.id, 36.0, Some(f.user.online), BG);
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                ui.label(RichText::new(&f.user.name).strong());
                                ui.label(
                                    RichText::new(match f.status {
                                        p::FriendStatus::Friend if f.user.online => "Online",
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
        let cur = s.current_channel();
        match &mut s.modal {
            Modal::None => {}
            Modal::AddServer { create, name, code } => {
                egui::Window::new("Adicionar um servidor")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
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
                        if ui.button("Cancelar").clicked() {
                            close = true;
                        }
                    });
            }
            Modal::NewChannel { server, name } => {
                egui::Window::new("Criar canal")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.set_width(300.0);
                        ui.label(caps(&"NOME DO CANAL"));
                        ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
                        if big_button(ui, "Criar canal", BLURPLE) && !name.trim().is_empty() {
                            acts.push(Act::Send(C2S::CreateChannel { server_id: *server, name: name.clone() }));
                            close = true;
                        }
                        if ui.button("Cancelar").clicked() {
                            close = true;
                        }
                    });
            }
            Modal::Invite(code) => {
                egui::Window::new("Convidar amigos")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.set_width(300.0);
                        ui.label("Mande este código para quem você quer convidar:");
                        ui.label(RichText::new(code.as_str()).size(22.0).strong().monospace());
                        ui.horizontal(|ui| {
                            if ui.button("Copiar").clicked() {
                                ctx.copy_text(code.clone());
                            }
                            if ui.button("Fechar").clicked() {
                                close = true;
                            }
                        });
                    });
            }
            Modal::GoLive => {
                egui::Window::new("Transmitir tela")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
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
                        if ui.button("Cancelar").clicked() {
                            close = true;
                        }
                    });
            }
        }
        if close {
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
