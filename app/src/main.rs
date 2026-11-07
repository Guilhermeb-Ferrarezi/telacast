mod capture;
mod gateway;
mod files;
mod gpu;
mod md_view;
mod net;
mod publisher;
mod quick;
mod update;
mod voice;
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
    // sobras de imagens coladas por execuções que já terminaram (o nome leva o PID)
    if let Ok(rd) = std::fs::read_dir(std::env::temp_dir()) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if let Some(rest) = name.strip_prefix("telacast-paste-") {
                let alive = rest.split('-').next().is_some_and(|pid| std::path::Path::new("/proc").join(pid).exists());
                if !alive {
                    let _ = std::fs::remove_dir_all(e.path());
                }
            }
        }
    }
    eframe::run_native(
        "Telacast",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_decorations(false) // a barra de título é nossa (como a do Discord)
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

/// Cartão de perfil aberto (popout) ao clicar no nome, avatar, menção ou membro.
struct Profile {
    user: i64,
    name: String,
    /// Onde o clique aconteceu; o cartão abre ao lado.
    anchor: egui::Rect,
    /// Abre à esquerda do ponto (painel de membros, na borda direita).
    left: bool,
    /// Passada do egui em que abriu (o clique que abre não pode fechar).
    opened: u64,
    /// Lista "mais ações" aberta.
    more: bool,
}

/// Um item da grade do diálogo "Transmitir": a fonte e a miniatura mais recente.
struct GoItem {
    info: capture::SourceInfo,
    tex: Option<egui::TextureHandle>,
}

type GoRaw = Vec<(capture::SourceInfo, Option<capture::Thumb>)>;

/// Estado do diálogo "Transmitir" (fonte + qualidade).
struct GoLive {
    /// 0 = telas, 1 = janelas.
    tab: u8,
    source: Option<capture::Source>,
    /// Altura final; `None` = a da fonte.
    res: Option<u32>,
    /// 0 personalizado, 1 fluidez, 2 nitidez.
    preset: u8,
    advanced: bool,
    /// Bitrate manual (kbps); `None` = automático pela resolução e taxa.
    kbps: Option<u32>,
    items: Vec<GoItem>,
    rx: Option<std::sync::mpsc::Receiver<GoRaw>>,
    last: Option<Instant>,
    busy: bool,
}

impl GoLive {
    fn new() -> Self {
        Self { tab: 0, source: None, res: Some(1080), preset: 1, advanced: false, kbps: None, items: Vec::new(), rx: None, last: None, busy: false }
    }

    /// Atualiza as miniaturas em segundo plano (a cada ~2 s enquanto o diálogo está aberto).
    fn refresh(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.rx {
            if let Ok(raw) = rx.try_recv() {
                self.items = raw
                    .into_iter()
                    .enumerate()
                    .map(|(i, (info, th))| GoItem {
                        info,
                        tex: th.map(|t| {
                            ctx.load_texture(
                                format!("golive-{i}"),
                                egui::ColorImage::from_rgba_unmultiplied([t.width as usize, t.height as usize], &t.rgba),
                                egui::TextureOptions::LINEAR,
                            )
                        }),
                    })
                    .collect();
                self.busy = false;
                if self.source.is_none() || !self.items.iter().any(|i| Some(&i.info.source) == self.source.as_ref()) {
                    // sem escolha (ou a janela escolhida sumiu): a primeira tela
                    if self.source.is_none() {
                        self.source = self.items.iter().find(|i| matches!(i.info.source, capture::Source::Monitor(_))).map(|i| i.info.source.clone());
                    }
                }
            }
        }
        if !self.busy && self.last.map_or(true, |t| t.elapsed() > Duration::from_secs(2)) {
            let (tx, ctx2) = {
                let (tx, rx) = std::sync::mpsc::channel();
                self.rx = Some(rx);
                (tx, ctx.clone())
            };
            self.busy = true;
            self.last = Some(Instant::now());
            std::thread::spawn(move || {
                let mut out: GoRaw = Vec::new();
                for info in capture::list_monitors().into_iter().chain(capture::list_windows()) {
                    let th = capture::thumbnail(&info.source, 312);
                    out.push((info, th));
                }
                let _ = tx.send(out);
                ctx2.request_repaint();
            });
        }
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    fn selected(&self) -> Option<&capture::SourceInfo> {
        self.items.iter().map(|i| &i.info).find(|i| Some(&i.source) == self.source.as_ref())
    }

    /// Qualidade escolhida para a fonte `info`.
    fn quality(&self, info: &capture::SourceInfo, fps: u32) -> publisher::Quality {
        let out_h = publisher::output_size(info.width, info.height, self.res).1;
        publisher::Quality { height: self.res, fps, kbps: self.kbps.unwrap_or_else(|| publisher::suggested_kbps(out_h, fps)) }
    }
}

/// Anexos por mensagem e envios simultâneos.
const MAX_ATTACH: usize = 10;
const MAX_PARALLEL: usize = 3;

/// Mensagem já "enviada" pelo usuário que sai quando os anexos dela terminarem de subir.
struct Armed {
    ch: i64,
    content: String,
    reply: Option<p::Reply>,
    keys: Vec<u64>,
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
    /// Prévia de um anexo ainda não enviado (arquivo local).
    PreviewLocal { key: u64, name: String, path: std::path::PathBuf },
    /// Arquivo de texto aberto por inteiro (até 50 KB lidos).
    TextView(p::Attachment),
    /// Configurações de voz: microfone e saída de áudio.
    VoiceSettings(voice::Devices),
}

enum Act {
    Open(View),
    OpenServer(i64),
    Send(C2S),
    Modal(Modal),
    ToggleCategory(i64),
    Copy(String),
    Toast(String),
    ToggleText(i64),
    JoinVoice(i64),
    LeaveVoice,
    ToggleMute,
    ToggleDeaf,
    OpenVoiceView,
    /// Abre o diálogo Transmitir (com o canal de voz como destino, ou o aberto se `None`).
    OpenGoLive(Option<i64>),
    OpenVoiceSettings,
    SetVoiceOptions { ns: bool, monitor: bool },
    SetVoiceDevices { input: Option<String>, output: Option<String> },
    OpenProfile { user: i64, name: String, anchor: egui::Rect, left: bool },
    Reply(i64, p::Reply),
    CancelReply,
    JumpTo(i64),
    SendMsg { ch: i64, content: String, reply: Option<p::Reply>, attachments: Vec<p::Attachment> },
    /// Envia a mensagem assim que os anexos (chaves) terminarem de subir.
    QueueSend { ch: i64, content: String, reply: Option<p::Reply>, keys: Vec<u64> },
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
    texts: files::Texts,
    /// Prévias de texto expandidas (até 100 linhas em vez de 6).
    text_open: HashSet<i64>,
    /// Arquivos sendo enviados para o campo de mensagem (por canal) e o canal por onde chegam os resultados.
    uploads: Vec<files::Upload>,
    upload_key: u64,
    armed: Vec<Armed>,
    profile: Option<Profile>,
    /// Autocompletar de `@`: linha escolhida e posição do `@` dispensado com Esc.
    ac_sel: usize,
    ac_off: Option<usize>,
    /// Voz: quem está em cada canal, minhas escolhas de mudo/surdo e se a visão de voz ocupa o centro.
    voice: Vec<p::VoiceState>,
    voice_muted: bool,
    voice_deaf: bool,
    voice_view: bool,
    /// Histórico de navegação (setas ← → da barra de título).
    nav: Vec<View>,
    nav_i: usize,
    nav_moving: bool,
    voice_since: Option<Instant>,
    /// Volume de cada pessoa que ouço (0.0 a 2.0; ausente = 1.0).
    voice_volumes: HashMap<i64, f32>,
    /// Quem assiste a cada transmissão (o servidor manda; vale para o transmissor e para os espectadores).
    viewers: HashMap<i64, Vec<i64>>,
    /// Participante em foco na visão de voz (duplo clique no tile); `None` = grade.
    voice_focus: Option<i64>,
    /// Atualização do app: estado compartilhado com a thread de consulta/download.
    update: std::sync::Arc<std::sync::Mutex<update::State>>,
    update_dismissed: bool,
    /// Motor de áudio da voz (existe só enquanto estou num canal de voz) e dispositivos escolhidos.
    engine: Option<std::sync::Arc<voice::Engine>>,
    voice_in: Option<String>,
    voice_out: Option<String>,
    voice_err_shown: bool,
    /// Supressão de ruído (padrão ligada) e "ouvir meu microfone" (só com as configurações abertas).
    voice_ns: bool,
    voice_monitor: bool,
    /// Motor só para testar o microfone nas configurações, quando não estou numa chamada.
    test_engine: Option<std::sync::Arc<voice::Engine>>,
    /// Rolagem horizontal da bandeja de anexos e onde ela ficou na tela (para a roda do mouse).
    tray_x: f32,
    tray_rect: egui::Rect,
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
            snap: p::Snapshot { me: me.clone(), servers: vec![], friends: vec![], dms: vec![], live: vec![], blocked: vec![], voice: vec![] },
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
            texts: files::Texts::new(),
            text_open: HashSet::new(),
            uploads: Vec::new(),
            upload_key: 0,
            armed: Vec::new(),
            profile: None,
            ac_sel: 0,
            ac_off: None,
            voice: Vec::new(),
            voice_muted: false,
            voice_deaf: false,
            voice_view: false,
            nav: vec![View::Home],
            nav_i: 0,
            nav_moving: false,
            voice_since: None,
            voice_volumes: HashMap::new(),
            viewers: HashMap::new(),
            voice_focus: None,
            update: std::sync::Arc::new(std::sync::Mutex::new(update::State::Idle)),
            update_dismissed: false,
            engine: None,
            voice_in: None,
            voice_out: None,
            voice_err_shown: false,
            voice_ns: true,
            voice_monitor: false,
            test_engine: None,
            tray_x: 0.0,
            tray_rect: egui::Rect::NOTHING,
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
        if !self.nav_moving {
            // anda para a frente no histórico (descarta o "avançar" antigo)
            self.nav.truncate(self.nav_i + 1);
            self.nav.push(v.clone());
            self.nav_i = self.nav.len() - 1;
            if self.nav.len() > 60 {
                self.nav.remove(0);
                self.nav_i -= 1;
            }
        }
        self.view = v;
        self.view_since = Instant::now();
    }

    fn nav_go(&mut self, i: usize) {
        if i < self.nav.len() {
            self.nav_i = i;
            self.nav_moving = true;
            self.voice_view = false;
            let v = self.nav[i].clone();
            self.open(v);
            self.nav_moving = false;
        }
    }

    /// Título mostrado no centro da barra de título.
    fn window_title(&self) -> String {
        if self.voice_view {
            if let Some(mv) = self.my_voice() {
                return self.voice_place(mv.channel_id).map(|(c, _)| c).unwrap_or_default();
            }
        }
        match &self.view {
            View::Home => "Amigos".into(),
            View::Dm(c) => self.snap.dms.iter().find(|d| d.channel_id == *c).map(|d| d.user.name.clone()).unwrap_or_default(),
            View::Chan { channel, .. } => {
                let name = self.snap.servers.iter().flat_map(|sv| sv.channels.iter()).find(|x| x.id == *channel).map(|x| x.name.clone()).unwrap_or_default();
                format!("# {name}")
            }
        }
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

    /// Consulta o servidor por uma versão mais nova (em segundo plano). Uma vez por execução.
    fn check_update(&self) {
        let (state, base, ctx) = (self.update.clone(), self.http_base.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let res = update::check(&base);
            if let Ok(mut st) = state.lock() {
                *st = match res {
                    Ok(Some(m)) => update::State::Available(m),
                    // erro de consulta não incomoda: só aparece se o usuário pedir a verificação
                    _ => update::State::Idle,
                };
            }
            ctx.request_repaint();
        });
    }

    /// Baixa, confere (tamanho, SHA-256 e assinatura) e instala a versão `m`.
    fn start_update(&self, m: telacast_proto::UpdateManifest) {
        let (state, base, ctx) = (self.update.clone(), self.http_base.clone(), self.ctx.clone());
        if let Ok(mut st) = state.lock() {
            *st = update::State::Downloading { got: 0, total: m.size };
        }
        std::thread::spawn(move || {
            let progress = |got: u64, total: u64| {
                if let Ok(mut st) = state.lock() {
                    *st = update::State::Downloading { got, total };
                }
                ctx.request_repaint();
            };
            let res = update::download(&base, &m, &progress).and_then(|path| update::install(&path));
            if let Ok(mut st) = state.lock() {
                *st = match res {
                    Ok(()) => update::State::Ready,
                    Err(e) => update::State::Failed(e),
                };
            }
            ctx.request_repaint();
        });
    }

    /// Liga o motor de áudio: o que ele codifica sai pelo gateway e os quadros que chegam vão para ele.
    fn start_voice_engine(&mut self) {
        if self.engine.is_some() {
            return;
        }
        let bin_tx = self.gw.bin_tx.clone();
        let send: Box<dyn FnMut(u32, &[u8]) + Send> = Box::new(move |seq, pkt| {
            let mut f = Vec::with_capacity(5 + pkt.len());
            f.push(p::VOICE_FRAME);
            f.extend_from_slice(&seq.to_be_bytes());
            f.extend_from_slice(pkt);
            let _ = bin_tx.send(f);
        });
        match voice::Engine::start(voice::Config { input: self.voice_in.clone(), output: self.voice_out.clone() }, send) {
            Ok(e) => {
                let e = std::sync::Arc::new(e);
                e.set_muted(self.voice_muted || self.voice_deaf);
                e.set_deafened(self.voice_deaf);
                e.set_noise_suppression(self.voice_ns);
                e.set_monitor(self.voice_monitor);
                for (u, v) in &self.voice_volumes {
                    e.set_user_volume(*u, *v);
                }
                let e2 = e.clone();
                self.gw.set_sink(Some(Box::new(move |b: &[u8]| {
                    // [VOICE_FRAME][remetente i64][seq u32][opus]
                    if b.len() > 13 && b[0] == p::VOICE_FRAME {
                        let uid = i64::from_be_bytes(b[1..9].try_into().unwrap_or([0; 8]));
                        let seq = u32::from_be_bytes(b[9..13].try_into().unwrap_or([0; 4]));
                        e2.push_packet(uid, seq, &b[13..]);
                    }
                })));
                self.voice_err_shown = false;
                self.engine = Some(e);
            }
            Err(e) => {
                self.toast = Some((format!("Não consegui abrir o áudio: {e}"), Instant::now()));
                self.gw.send(C2S::VoiceLeave);
            }
        }
    }

    /// O motor que está com o microfone aberto agora (o da chamada ou o de teste das configurações).
    fn mic_engine(&self) -> Option<&std::sync::Arc<voice::Engine>> {
        self.engine.as_ref().or(self.test_engine.as_ref())
    }

    /// Abre só o microfone (sem enviar nada) para o medidor e o "ouvir meu microfone" das configurações.
    fn start_test_engine(&mut self) {
        if self.engine.is_some() || self.test_engine.is_some() {
            return;
        }
        match voice::Engine::start(voice::Config { input: self.voice_in.clone(), output: self.voice_out.clone() }, Box::new(|_, _| {})) {
            Ok(e) => {
                let e = std::sync::Arc::new(e);
                e.set_noise_suppression(self.voice_ns);
                e.set_monitor(self.voice_monitor);
                self.test_engine = Some(e);
            }
            Err(e) => self.toast = Some((format!("Não consegui abrir o microfone: {e}"), Instant::now())),
        }
    }

    fn apply_voice_options(&self) {
        for e in self.engine.iter().chain(self.test_engine.iter()) {
            e.set_noise_suppression(self.voice_ns);
            e.set_monitor(self.voice_monitor);
        }
    }

    fn stop_voice_engine(&mut self) {
        self.gw.set_sink(None);
        self.engine = None;
    }

    /// Passa ao motor meu mudo/surdo atuais.
    fn apply_voice_flags(&self) {
        if let Some(e) = &self.engine {
            e.set_muted(self.voice_muted || self.voice_deaf);
            e.set_deafened(self.voice_deaf);
        }
    }

    /// Meu estado de voz (canal onde estou), se estiver conectado.
    fn my_voice(&self) -> Option<&p::VoiceState> {
        self.voice.iter().find(|v| v.user_id == self.me.id)
    }

    /// Quem está no canal de voz `ch`, na ordem de entrada.
    fn voice_members(&self, ch: i64) -> Vec<&p::VoiceState> {
        self.voice.iter().filter(|v| v.channel_id == ch).collect()
    }

    /// (nome do canal, nome do servidor) do canal de voz `ch`.
    fn voice_place(&self, ch: i64) -> Option<(String, String)> {
        self.snap.servers.iter().find_map(|sv| sv.channels.iter().find(|c| c.id == ch).map(|c| (c.name.clone(), sv.name.clone())))
    }

    /// Está falando agora? (o motor de áudio responde)
    fn voice_speaking(&self, user: i64) -> bool {
        match &self.engine {
            Some(e) if user == self.me.id => e.self_speaking(),
            Some(e) => e.speaking(user),
            None => false,
        }
    }

    /// Nome de um usuário conhecido (ou "?").
    fn user_name_of(&self, id: i64) -> String {
        self.user_info(id).map(|u| u.name).unwrap_or_else(|| "?".into())
    }

    /// Manda ao servidor meu mudo/surdo (surdo implica mudo).
    fn send_voice_set(&self) {
        if self.my_voice().is_some() {
            self.gw.send(C2S::VoiceSet { muted: self.voice_muted || self.voice_deaf, deafened: self.voice_deaf });
        }
    }

    /// Dados de um usuário conhecido (você, membros dos servidores, amigos, DMs, bloqueados).
    fn user_info(&self, id: i64) -> Option<p::User> {
        if self.me.id == id {
            return Some(self.me.clone());
        }
        self.snap
            .servers
            .iter()
            .flat_map(|sv| sv.members.iter())
            .chain(self.snap.friends.iter().map(|f| &f.user))
            .chain(self.snap.dms.iter().map(|d| &d.user))
            .chain(self.snap.blocked.iter())
            .find(|u| u.id == id)
            .cloned()
    }

    /// Ctrl+V com uma imagem na área de transferência: grava um PNG temporário e envia como anexo.
    fn paste_image(&mut self, img: &egui::ColorImage) {
        if self.current_channel().is_none() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("telacast-paste-{}-{}", std::process::id(), self.upload_key + 1));
        let path = dir.join("imagem.png");
        let [w, h] = img.size;
        let saved = std::fs::create_dir_all(&dir).is_ok()
            && image::save_buffer(&path, img.as_raw(), w as u32, h as u32, image::ColorType::Rgba8).is_ok();
        if saved {
            self.start_upload(path);
        } else {
            self.toast = Some(("Não consegui ler a imagem colada.".into(), Instant::now()));
        }
    }

    /// Põe um arquivo na fila de envio do canal aberto (no máximo 10 por mensagem).
    fn start_upload(&mut self, path: std::path::PathBuf) {
        let Some(channel) = self.current_channel() else { return };
        if self.uploads.iter().filter(|u| u.channel == channel && !u.armed).count() >= MAX_ATTACH {
            self.toast = Some((format!("Você só pode anexar {MAX_ATTACH} arquivos por mensagem."), Instant::now()));
            return;
        }
        self.upload_key += 1;
        let key = self.upload_key;
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "arquivo".into());
        self.uploads.push(files::Upload { key, channel, name, path, state: files::UpState::Queued, armed: false, not_before: None });
        self.pump_uploads();
    }

    /// Dispara os envios da fila, até `MAX_PARALLEL` ao mesmo tempo (cada um numa thread).
    fn pump_uploads(&mut self) {
        let running = self.uploads.iter().filter(|u| matches!(u.state, files::UpState::Sending)).count();
        let mut slots = MAX_PARALLEL.saturating_sub(running);
        let (base, token, tx, ctx) = (self.http_base.clone(), self.token.clone(), self.upload_tx.clone(), self.ctx.clone());
        let now = Instant::now();
        for u in self.uploads.iter_mut().filter(|u| matches!(u.state, files::UpState::Queued)) {
            if slots == 0 {
                break;
            }
            if let Some(t) = u.not_before {
                if t > now {
                    self.ctx.request_repaint_after(t - now);
                    continue;
                }
            }
            slots -= 1;
            u.state = files::UpState::Sending;
            let (base, token, tx, ctx, key, channel, path) = (base.clone(), token.clone(), tx.clone(), ctx.clone(), u.key, u.channel, u.path.clone());
            std::thread::spawn(move || {
                let _ = tx.send((key, files::upload(&base, &token, channel, &path)));
                ctx.request_repaint();
            });
        }
    }

    /// Manda as mensagens que esperavam os anexos, na ordem em que foram pedidas (por canal).
    fn flush_armed(&mut self) {
        let mut blocked: HashSet<i64> = HashSet::new();
        let mut i = 0;
        while i < self.armed.len() {
            let ch = self.armed[i].ch;
            let busy = self.armed[i]
                .keys
                .iter()
                .any(|k| self.uploads.iter().any(|u| u.key == *k && matches!(u.state, files::UpState::Queued | files::UpState::Sending)));
            if busy || blocked.contains(&ch) {
                blocked.insert(ch);
                i += 1;
                continue;
            }
            let a = self.armed.remove(i);
            let atts: Vec<p::Attachment> = a
                .keys
                .iter()
                .filter_map(|k| self.uploads.iter().find(|u| u.key == *k))
                .filter_map(|u| if let files::UpState::Done(at) = &u.state { Some(at.clone()) } else { None })
                .collect();
            self.uploads.retain(|u| !(a.keys.contains(&u.key) && matches!(u.state, files::UpState::Done(_))));
            for u in self.uploads.iter_mut().filter(|u| a.keys.contains(&u.key)) {
                u.armed = false; // o que falhou volta para o campo
            }
            if a.content.trim().is_empty() && atts.is_empty() {
                self.toast = Some(("Os anexos não subiram; a mensagem não foi enviada.".into(), Instant::now()));
                continue;
            }
            self.send_message(a.ch, a.content, a.reply, atts);
        }
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
                    // o servidor pediu calma (limite por minuto): volta para a fila e tenta já já
                    Err(e) if e.contains("muitos envios seguidos") => {
                        u.not_before = Some(Instant::now() + Duration::from_secs(5));
                        files::UpState::Queued
                    }
                    Err(e) => files::UpState::Failed(e),
                };
            }
        }
        while let Ok(path) = self.picked_rx.try_recv() {
            self.start_upload(path);
        }
        self.pump_uploads();
        self.flush_armed();
        while let Ok(ev) = self.gw.rx.try_recv() {
            match ev {
                Event::Closed(why) => {
                    // a conexão caiu: o servidor já tirou a pessoa da voz
                    self.viewers.clear();
                    self.voice_focus = None;
                    self.stop_voice_engine();
                    self.voice.clear();
                    self.voice_view = false;
                    self.voice_since = None;
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
                        self.voice = sn.voice.clone();
                        if self.my_voice().is_none() && self.engine.is_some() {
                            self.stop_voice_engine();
                            self.voice_view = false;
                        }
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
                    S2C::Viewers { channel_id, users } => {
                        if users.is_empty() {
                            self.viewers.remove(&channel_id);
                        } else {
                            self.viewers.insert(channel_id, users);
                        }
                    }
                    S2C::VoiceUpdate(st) => {
                        self.voice.retain(|v| v.user_id != st.user_id);
                        let mine = st.user_id == self.me.id;
                        if mine && self.voice_since.is_none() {
                            self.voice_since = Some(Instant::now());
                        }
                        self.voice.push(st);
                        if mine {
                            self.start_voice_engine();
                        }
                    }
                    S2C::VoiceLeft { user_id, channel_id } => {
                        self.voice.retain(|v| !(v.user_id == user_id && v.channel_id == channel_id));
                        if user_id == self.me.id {
                            self.voice_view = false;
                            self.voice_since = None;
                            self.stop_voice_engine();
                        } else if let Some(e) = &self.engine {
                            e.remove_user(user_id);
                        }
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
    voice_in: Option<String>,
    voice_out: Option<String>,
    voice_ns: bool,
    monitors: Vec<String>,
    monitor: usize,
    golive: GoLive,
    /// Canal onde a transmissão vai ser feita (o de voz, quando o diálogo abre pela visão de voz).
    golive_target: Option<i64>,
    /// Controles sobre o vídeo: última atividade do mouse e até quando a lista de espectadores segura a barra.
    video_active: Instant,
    viewers_hover_until: Instant,
    /// Prévia da minha própria transmissão (miniatura da fonte, renovada a cada ~0,7 s).
    publishing_source: Option<capture::Source>,
    preview_tex: Option<egui::TextureHandle>,
    preview_rx: Option<std::sync::mpsc::Receiver<Option<capture::Thumb>>>,
    preview_last: Option<Instant>,
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
            voice_ns: get("voice_ns") != "0",
            voice_in: Some(get("voice_in")).filter(|v| !v.is_empty()),
            voice_out: Some(get("voice_out")).filter(|v| !v.is_empty()),
            fps: get("fps").parse().unwrap_or(60),
            kbps: get("kbps").parse().unwrap_or(8000),
            monitors: publisher::monitor_names(),
            golive: GoLive::new(),
            golive_target: None,
            video_active: Instant::now(),
            viewers_hover_until: Instant::now(),
            publishing_source: None,
            preview_tex: None,
            preview_rx: None,
            preview_last: None,
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
                    let mut sess = Session::new(gw, self.login.server.clone(), ctx.clone(), token, me);
                    sess.check_update();
                    sess.voice_in = self.voice_in.clone();
                    sess.voice_out = self.voice_out.clone();
                    sess.voice_ns = self.voice_ns;
                    self.session = Some(sess);
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
        self.publishing_source = None;
        self.preview_tex = None;
        self.preview_rx = None;
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
        storage.set_string("voice_ns", if self.voice_ns { "1" } else { "0" }.into());
        storage.set_string("voice_in", self.voice_in.clone().unwrap_or_default());
        storage.set_string("voice_out", self.voice_out.clone().unwrap_or_default());
        storage.set_string("server", self.login.server.clone());
        storage.set_string("user", self.login.user.clone());
        storage.set_string("token", self.token_saved.clone());
        storage.set_string("fps", self.fps.to_string());
        storage.set_string("kbps", self.kbps.to_string());
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if !self.fullscreen {
            self.titlebar(ui, &ctx);
            resize_edges(&ctx);
        }
        if single::take_raise() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
        }
        self.poll_login(&ctx);
        let pasted: Vec<std::sync::Arc<egui::ColorImage>> = ctx.input(|i| {
            i.events.iter().filter_map(|e| if let egui::Event::PasteImage(img) = e { Some(img.clone()) } else { None }).collect()
        });
        match self.session.take() {
            Some(mut s) => {
                for img in &pasted {
                    s.paste_image(img);
                }
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

// ------------------------------------------------------------------ barra de título

/// Bordas da janela sem moldura: o cursor muda e o arrasto redimensiona (o X não faz isso sem decoração).
fn resize_edges(ctx: &egui::Context) {
    use egui::{CursorIcon as C, ResizeDirection as D};
    if ctx.input(|i| i.viewport().maximized.unwrap_or(false)) {
        return;
    }
    let Some(p) = ctx.input(|i| i.pointer.hover_pos()) else { return };
    let r = ctx.content_rect();
    let m = 5.0;
    let (l, rt, t, b) = (p.x - r.left() < m, r.right() - p.x < m, p.y - r.top() < m, r.bottom() - p.y < m);
    let dir = match (l, rt, t, b) {
        (true, _, true, _) => Some((D::NorthWest, C::ResizeNwSe)),
        (_, true, true, _) => Some((D::NorthEast, C::ResizeNeSw)),
        (true, _, _, true) => Some((D::SouthWest, C::ResizeNeSw)),
        (_, true, _, true) => Some((D::SouthEast, C::ResizeNwSe)),
        (true, ..) => Some((D::West, C::ResizeHorizontal)),
        (_, true, ..) => Some((D::East, C::ResizeHorizontal)),
        (_, _, true, _) => Some((D::North, C::ResizeVertical)),
        (_, _, _, true) => Some((D::South, C::ResizeVertical)),
        _ => None,
    };
    if let Some((d, icon)) = dir {
        ctx.set_cursor_icon(icon);
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(d));
        }
    }
}

impl App {
    /// Barra de título própria, como a do Discord: ← → à esquerda, título no centro, minimizar/maximizar/fechar à direita.
    fn titlebar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let (title, can_back, can_fwd) = match &self.session {
            Some(s) => (s.window_title(), s.nav_i > 0, s.nav_i + 1 < s.nav.len()),
            None => ("Telacast".to_string(), false, false),
        };
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        let (mut go_back, mut go_fwd) = (false, false);
        egui::Panel::top("titlebar").frame(egui::Frame::NONE.fill(L0)).exact_size(32.0).show(ui, |ui| {
            let (rect, drag) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.0), egui::Sense::click_and_drag());
            if drag.drag_started_by(egui::PointerButton::Primary) {
                ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            if drag.double_clicked() {
                ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
            }
            // título centrado (com "#" ou o nome)
            let pt = ui.painter();
            pt.text(rect.center(), Align2::CENTER_CENTER, &title, egui::FontId::proportional(13.5), TEXT2);
            // ← →
            for (i, (can, forward)) in [(can_back, false), (can_fwd, true)].into_iter().enumerate() {
                let r = egui::Rect::from_min_size(egui::pos2(rect.left() + 10.0 + i as f32 * 28.0, rect.top() + 4.0), egui::vec2(26.0, 24.0));
                let resp = ui.interact(r, egui::Id::new(("nav-arrow", forward)), egui::Sense::click());
                let hot = resp.hovered() && can;
                let pt = ui.painter();
                if hot {
                    pt.rect_filled(r, 6.0, L3);
                    ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                let ink = if !can { alpha(TEXT2, 0.3) } else if hot { TEXT } else { TEXT2 };
                let c = r.center();
                let d = if forward { 1.0 } else { -1.0 };
                let st = egui::Stroke::new(1.6, ink);
                pt.line_segment([c + egui::vec2(-5.0 * d, 0.0), c + egui::vec2(5.0 * d, 0.0)], st);
                pt.line_segment([c + egui::vec2(5.0 * d, 0.0), c + egui::vec2(1.0 * d, -4.0)], st);
                pt.line_segment([c + egui::vec2(5.0 * d, 0.0), c + egui::vec2(1.0 * d, 4.0)], st);
                if resp.on_hover_text(if forward { "Avançar" } else { "Voltar" }).clicked() && can {
                    if forward {
                        go_fwd = true;
                    } else {
                        go_back = true;
                    }
                }
            }
            // minimizar, maximizar e fechar
            for (i, kind) in [0, 1, 2].into_iter().enumerate() {
                let x = rect.right() - 46.0 * (3 - i) as f32;
                let r = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(46.0, 32.0));
                let resp = ui.interact(r, egui::Id::new(("win-btn", kind)), egui::Sense::click());
                let hot = resp.hovered();
                let pt = ui.painter();
                if hot {
                    pt.rect_filled(r, 0.0, if kind == 2 { RED } else { L3 });
                    ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                let ink = if hot && kind == 2 { L0 } else if hot { TEXT } else { TEXT2 };
                let c = r.center();
                let st = egui::Stroke::new(1.3, ink);
                match kind {
                    0 => {
                        pt.line_segment([c + egui::vec2(-5.0, 0.0), c + egui::vec2(5.0, 0.0)], st);
                    }
                    1 => {
                        if maximized {
                            pt.rect_stroke(egui::Rect::from_center_size(c + egui::vec2(-1.0, 1.0), egui::vec2(8.0, 8.0)), 0.0, st, egui::StrokeKind::Middle);
                            pt.line_segment([c + egui::vec2(-1.0, -4.0), c + egui::vec2(5.0, -4.0)], st);
                            pt.line_segment([c + egui::vec2(5.0, -4.0), c + egui::vec2(5.0, 2.0)], st);
                        } else {
                            pt.rect_stroke(egui::Rect::from_center_size(c, egui::vec2(9.0, 9.0)), 0.0, st, egui::StrokeKind::Middle);
                        }
                    }
                    _ => {
                        pt.line_segment([c + egui::vec2(-5.0, -5.0), c + egui::vec2(5.0, 5.0)], st);
                        pt.line_segment([c + egui::vec2(-5.0, 5.0), c + egui::vec2(5.0, -5.0)], st);
                    }
                }
                if resp.clicked() {
                    match kind {
                        0 => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
                        1 => ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized)),
                        _ => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
                    }
                }
            }
        });
        if let Some(s) = self.session.as_mut() {
            if go_back && s.nav_i > 0 {
                s.nav_go(s.nav_i - 1);
            }
            if go_fwd && s.nav_i + 1 < s.nav.len() {
                s.nav_go(s.nav_i + 1);
            }
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
fn attachment_view(ui: &mut egui::Ui, a: &p::Attachment, base: &str, token: &str, images: &mut files::Images, texts: &mut files::Texts, expanded: bool, acts: &mut Vec<Act>) {
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
    // texto e código: prévia das primeiras linhas, como no Discord
    if files::is_text_like(a) {
        let ctx = ui.ctx().clone();
        match texts.get(&ctx, base, token, a) {
            files::Text::Ready { text, lines, bytes_left } => {
                text_card(ui, a, text, *lines, *bytes_left, expanded, files::url(base, a, token), acts);
                return;
            }
            files::Text::Loading => {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width().min(520.0), 96.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 8.0, L2);
                ui.painter().rect_stroke(rect, 8.0, egui::Stroke::new(1.0, LINE2), egui::StrokeKind::Inside);
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, "Carregando prévia…", egui::FontId::proportional(13.0), MUTED);
                ctx.request_repaint_after(Duration::from_millis(300));
                return;
            }
            files::Text::Failed => {}
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

/// Prévia ampliada de uma imagem sobre o app. Devolve `true` quando deve fechar (Esc, clique fora ou "Fechar").
/// Com `download`, mostra também o botão "Baixar".
fn lightbox(ctx: &egui::Context, tex: Option<egui::TextureHandle>, caption: &str, download: Option<String>) -> bool {
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
                    ui.label(RichText::new(caption).size(13.0).color(TEXT2));
                    if let Some(url) = &download {
                        if small_button(ui, "Baixar", FIELD) {
                            ctx.open_url(egui::OpenUrl::new_tab(url.clone()));
                        }
                    }
                    if small_button(ui, "Fechar", FIELD) {
                        ui.close();
                    }
                });
            });
        });
    m.should_close()
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
        let (rect, resp) = ui.allocate_exact_size(egui::vec2(120.0, 120.0), if is_img { egui::Sense::click() } else { egui::Sense::hover() });
        let mut shown = false;
        if is_img {
            match images.get_local(&ctx, u.key, &u.path, false) {
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
            files::UpState::Queued => {
                p.rect_filled(rect, 8.0, egui::Color32::from_black_alpha(130));
                p.text(rect.center(), Align2::CENTER_CENTER, "Na fila", egui::FontId::proportional(13.0), TEXT2);
            }
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
        if is_img && resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if is_img && resp.clicked() {
            acts.push(Act::Modal(Modal::PreviewLocal { key: u.key, name: u.name.clone(), path: u.path.clone() }));
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

/// O `@consulta` que termina no cursor: (índice do `@` em caracteres, consulta). O `@` precisa estar
/// no começo ou depois de espaço; a consulta só tem letras, números e `_ . -`.
fn mention_token(text: &str, cursor: usize) -> Option<(usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    let cur = cursor.min(chars.len());
    let mut i = cur;
    while i > 0 {
        let c = chars[i - 1];
        if c == '@' {
            if i - 1 > 0 && !chars[i - 2].is_whitespace() {
                return None;
            }
            return Some((i - 1, chars[i..cur].iter().collect()));
        }
        if !(c.is_alphanumeric() || matches!(c, '_' | '.' | '-')) {
            return None;
        }
        i -= 1;
    }
    None
}

/// Uma sugestão do autocompletar de `@`.
struct MentionCand {
    /// Texto inserido depois do `@`.
    name: String,
    /// Texto à direita da linha.
    note: String,
    /// `None` = menção especial (`@everyone`, `@here`).
    user: Option<(i64, p::Status)>,
}

/// Membros do canal (ou a pessoa da conversa) que combinam com a consulta: quem começa com ela primeiro,
/// depois quem contém; online antes de offline. No máximo 8. `@everyone` e `@here` só em servidores.
fn mention_candidates(s: &Session, ch: i64, q: &str) -> Vec<MentionCand> {
    let ql = q.to_lowercase();
    let (users, owner, in_server): (Vec<&p::User>, i64, bool) = match s.snap.servers.iter().find(|sv| sv.channels.iter().any(|c| c.id == ch)) {
        Some(sv) => (sv.members.iter().collect(), sv.owner, true),
        None => (s.snap.dms.iter().filter(|d| d.channel_id == ch).map(|d| &d.user).chain(std::iter::once(&s.me)).collect(), 0, false),
    };
    let mut hits: Vec<(u8, &p::User)> = users
        .into_iter()
        .filter_map(|u| {
            let n = u.name.to_lowercase();
            if n.starts_with(&ql) {
                Some((0, u))
            } else if n.contains(&ql) {
                Some((1, u))
            } else {
                None
            }
        })
        .collect();
    hits.sort_by_key(|(rank, u)| (*rank, !u.online, u.name.to_lowercase()));
    let mut out: Vec<MentionCand> = Vec::new();
    if in_server {
        for (name, note) in [("everyone", "Notifique todos neste canal"), ("here", "Notifique quem está online")] {
            if name.starts_with(&ql) {
                out.push(MentionCand { name: name.into(), note: note.into(), user: None });
            }
        }
    }
    for (_, u) in hits {
        out.push(MentionCand { name: u.name.clone(), note: if u.id == owner { "dono".into() } else { String::new() }, user: Some((u.id, u.status)) });
    }
    out.truncate(8);
    out
}

/// Troca o `@consulta` (de `start` até `cursor`) por `@nome ` e devolve onde o cursor deve ficar.
fn apply_mention(text: &mut String, start: usize, cursor: usize, name: &str) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let cur = cursor.min(chars.len());
    let before: String = chars[..start].iter().collect();
    let after: String = chars[cur..].iter().collect();
    let gap = if after.starts_with(' ') { "" } else { " " };
    *text = format!("{before}@{name}{gap}{after}");
    start + 1 + name.chars().count() + 1
}

/// Nomes (minúsculos) que viram menção no campo de mensagem: membros do canal, `everyone` e `here`.
fn known_mention_names(s: &Session, ch: i64) -> HashSet<String> {
    let mut out: HashSet<String> = HashSet::new();
    match s.snap.servers.iter().find(|sv| sv.channels.iter().any(|c| c.id == ch)) {
        Some(sv) => {
            out.extend(sv.members.iter().map(|m| m.name.to_lowercase()));
            out.insert("everyone".into());
            out.insert("here".into());
        }
        None => {
            out.extend(s.snap.dms.iter().filter(|d| d.channel_id == ch).map(|d| d.user.name.to_lowercase()));
            out.insert(s.me.name.to_lowercase());
        }
    }
    out
}

/// Texto do campo de mensagem com as menções completas (`@nome` de quem existe) em âmbar, como no chat.
fn mention_job(ui: &egui::Ui, text: &str, known: &HashSet<String>, wrap: f32) -> egui::text::LayoutJob {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let plain = egui::TextFormat { font_id: font.clone(), color: ui.visuals().text_color(), ..Default::default() };
    let hot = egui::TextFormat {
        font_id: font,
        color: AMBER_HOT,
        background: alpha(AMBER, 0.14),
        coords: egui::epaint::text::VariationCoords::new([(b"wght", 600.0)]),
        ..Default::default()
    };
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut last = 0; // byte
    let mut i = 0;
    while i < chars.len() {
        let (b, c) = chars[i];
        let starts = c == '@' && (i == 0 || chars[i - 1].1.is_whitespace());
        if starts {
            let mut j = i + 1;
            while j < chars.len() && (chars[j].1.is_alphanumeric() || matches!(chars[j].1, '_' | '.' | '-')) {
                j += 1;
            }
            let end = chars.get(j).map(|x| x.0).unwrap_or(text.len());
            let name = text[b + 1..end].to_lowercase();
            if !name.is_empty() && known.contains(&name) {
                job.append(&text[last..b], 0.0, plain.clone());
                job.append(&text[b..end], 0.0, hot.clone());
                last = end;
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }
    job.append(&text[last..], 0.0, plain);
    job
}

/// Mensagem do cartão de estado do vídeo que estou assistindo: (título, detalhe, mostrar "Fechar transmissão").
/// `None` = está tudo bem (vídeo rodando).
fn stream_state(status: &str, link_up: bool, has_frame: bool) -> Option<(String, String, bool)> {
    if !link_up {
        return Some(("Você foi desconectado(a)…".into(), "Segura firme, vamos te jogar de volta!".into(), false));
    }
    if status == "desconectado" || status.contains("ninguém está transmitindo") {
        return Some(("Esta transmissão terminou.".into(), String::new(), true));
    }
    if status.contains("máximo de espectadores") {
        return Some(("Esta transmissão está lotada.".into(), "Desculpe, ela está com o máximo de espectadores.".into(), true));
    }
    if let Some(e) = status.strip_prefix("erro: ") {
        // já tinha vídeo e a conexão caiu: o transmissor parou
        if has_frame {
            return Some(("Esta transmissão terminou.".into(), String::new(), true));
        }
        let e: String = e.chars().take(90).collect();
        return Some(("A transmissão não iniciou :(".into(), format!("Erro: {e}"), true));
    }
    if !has_frame {
        return Some(("Aguardando vídeo…".into(), String::new(), false));
    }
    None
}

/// Cartão de prévia de texto (no estilo do Discord): primeiras linhas em fonte monoespaçada e, embaixo,
/// uma barra com expandir/recolher, nome e tamanho, "Abrir" (tudo, num modal) e "Baixar".
/// Recolhido mostra 6 linhas; expandido, 100. O Discord lê só os primeiros ~50 KB do arquivo.
fn text_card(ui: &mut egui::Ui, a: &p::Attachment, text: &str, lines: usize, bytes_left: u64, expanded: bool, url: String, acts: &mut Vec<Act>) {
    let max_lines = if expanded { 100 } else { 6 };
    let whole = bytes_left == 0;
    let shown: String = text.split('\n').take(max_lines).collect::<Vec<_>>().join("\n");
    let mut notice = String::new();
    if whole && expanded && lines > max_lines {
        notice = format!("… ({} linhas a mais)", lines - max_lines);
    } else if !whole {
        notice = format!("… ({} restantes)", files::human_size(bytes_left));
    }
    let can_toggle = expanded || lines > 6 || !whole;
    let w = ui.available_width().min(520.0);
    egui::Frame::new().fill(L2).stroke(egui::Stroke::new(1.0, LINE2)).corner_radius(egui::CornerRadius::same(8)).show(ui, |ui| {
        ui.set_width(w - 2.0);
        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
        egui::Frame::new().inner_margin(egui::Margin { left: 12, right: 12, top: 10, bottom: 10 }).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let mut job = egui::text::LayoutJob::default();
            let mono = egui::FontId::monospace(12.5);
            job.append(&shown, 0.0, egui::TextFormat { font_id: mono.clone(), color: TEXT, line_height: Some(18.0), ..Default::default() });
            if !notice.is_empty() && expanded {
                job.append(&format!("\n{notice}"), 0.0, egui::TextFormat { font_id: mono, color: MUTED, line_height: Some(18.0), ..Default::default() });
            }
            job.wrap.max_width = ui.available_width();
            // recolhido corta em 6 linhas na tela (com as quebras), como o Discord; expandido, em 100
            job.wrap.max_rows = if expanded { 102 } else { 6 };
            ui.add(egui::Label::new(job).selectable(true));
        });
        let y = ui.cursor().top();
        ui.painter().hline(ui.max_rect().x_range(), y, egui::Stroke::new(1.0, LINE2));
        egui::Frame::new().inner_margin(egui::Margin::symmetric(10, 7)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if can_toggle && small_button(ui, if expanded { "Recolher" } else { "Expandir" }, FIELD) {
                    acts.push(Act::ToggleText(a.id));
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    let name: String = if a.name.chars().count() > 26 { format!("{}…", a.name.chars().take(25).collect::<String>()) } else { a.name.clone() };
                    ui.label(RichText::new(name).size(13.0).color(TEXT)).on_hover_text(&a.name);
                    let count = if whole { format!("{} · {} linhas", files::human_size(a.size), lines) } else { files::human_size(a.size) };
                    ui.label(RichText::new(count).size(11.5).color(MUTED));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    if small_button(ui, "Baixar", FIELD) {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(url.clone()));
                    }
                    if small_button(ui, "Abrir", FIELD) {
                        acts.push(Act::Modal(Modal::TextView(a.clone())));
                    }
                });
            });
        });
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
        s.texts.pump(ctx);
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
                    // vários arquivos separados por vírgula
                    for one in path.split(',').filter(|x| !x.is_empty()) {
                        acts.push(Act::AddFile(one.into()));
                    }
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

        // voz: redesenha para os anéis de "falando" e avisa se o dispositivo de áudio falhar
        if let Some(e) = &s.engine {
            ctx.request_repaint_after(Duration::from_millis(120));
            if let Some(err) = e.error() {
                if !s.voice_err_shown {
                    s.voice_err_shown = true;
                    s.toast = Some((format!("Problema no áudio: {err}"), Instant::now()));
                }
            }
        }

        // faixa de atualização (versão nova disponível, baixando, pronta ou com erro)
        let upd = s.update.lock().map(|g| g.clone()).unwrap_or_default();
        if !self.fullscreen && !s.update_dismissed && !matches!(upd, update::State::Idle) {
            egui::Panel::top("update")
                .frame(egui::Frame::new().fill(L3).inner_margin(egui::Margin::symmetric(16, 7)))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        dot(ui, AMBER);
                        match &upd {
                            update::State::Available(m) => {
                                ui.label(RichText::new(format!("Nova versão {} disponível (você está na {}).", m.version, update::CURRENT)).color(TEXT));
                                if !m.notes.is_empty() {
                                    ui.label(RichText::new(&m.notes).size(12.5).color(MUTED));
                                }
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if small_button(ui, "Depois", FIELD) {
                                        s.update_dismissed = true;
                                    }
                                    if small_button(ui, "Atualizar agora", BLURPLE) {
                                        s.start_update(m.clone());
                                    }
                                });
                            }
                            update::State::Downloading { got, total } => {
                                let pct = if *total > 0 { *got as f32 / *total as f32 } else { 0.0 };
                                ui.label(RichText::new(format!("Baixando e conferindo a atualização… {:.0}%", pct * 100.0)).color(TEXT));
                                ctx.request_repaint_after(Duration::from_millis(200));
                            }
                            update::State::Ready => {
                                ui.label(RichText::new("Atualização instalada, hash conferido. Reinicie para usar a nova versão.").color(TEXT));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if small_button(ui, "Reiniciar agora", BLURPLE) {
                                        update::restart();
                                    }
                                });
                            }
                            update::State::Failed(e) => {
                                ui.label(RichText::new(format!("Atualização recusada: {e}")).color(RED));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    if small_button(ui, "Fechar", FIELD) {
                                        s.update_dismissed = true;
                                    }
                                });
                            }
                            update::State::Idle => {}
                        }
                    });
                });
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
                    if self.watching.is_some() {
                        self.stream_area(ui, ctx, s, &mut acts, true);
                    } else if self.publishing.is_some() {
                        self.preview_fullscreen(ui, ctx, s, &mut acts);
                    } else {
                        // nada para mostrar (a transmissão acabou): sai da tela cheia
                        acts.push(Act::Fullscreen);
                    }
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
        self.profile_popout(ctx, s, &mut acts);
        self.toast(ctx, s);

        let mut keep = true;
        for a in acts {
            match a {
                Act::Open(v) => {
                    s.voice_view = false;
                    s.open(v);
                }
                Act::JoinVoice(ch) => {
                    if s.my_voice().map(|v| v.channel_id) != Some(ch) {
                        s.gw.send(C2S::VoiceJoin { channel_id: ch });
                        s.voice_since = Some(Instant::now());
                        if s.voice_muted || s.voice_deaf {
                            s.send_voice_set();
                        }
                    }
                    s.voice_view = true;
                }
                Act::LeaveVoice => {
                    s.gw.send(C2S::VoiceLeave);
                    s.voice_view = false;
                }
                Act::ToggleMute => {
                    if s.voice_deaf {
                        // ligar o microfone com o áudio desligado liga o áudio também (como no Discord)
                        s.voice_deaf = false;
                        s.voice_muted = false;
                    } else {
                        s.voice_muted = !s.voice_muted;
                    }
                    s.send_voice_set();
                    s.apply_voice_flags();
                }
                Act::ToggleDeaf => {
                    s.voice_deaf = !s.voice_deaf;
                    s.send_voice_set();
                    s.apply_voice_flags();
                }
                Act::OpenVoiceView => s.voice_view = true,
                Act::OpenGoLive(target) => {
                    self.golive_target = target;
                    s.modal = Modal::GoLive;
                }
                Act::OpenVoiceSettings => {
                    // listar os dispositivos pode levar um instante (e o ALSA reclama no terminal; é normal)
                    s.modal = Modal::VoiceSettings(voice::list_devices());
                    s.start_test_engine();
                }
                Act::SetVoiceOptions { ns, monitor } => {
                    s.voice_ns = ns;
                    self.voice_ns = ns;
                    s.voice_monitor = monitor;
                    if monitor {
                        s.start_test_engine();
                    }
                    s.apply_voice_options();
                }
                Act::SetVoiceDevices { input, output } => {
                    let changed = s.voice_in != input || s.voice_out != output;
                    self.voice_in = input.clone();
                    self.voice_out = output.clone();
                    s.voice_in = input;
                    s.voice_out = output;
                    if changed && s.engine.is_some() {
                        // reabre os dispositivos já com a escolha nova
                        s.stop_voice_engine();
                        s.start_voice_engine();
                    }
                    if changed && s.test_engine.is_some() {
                        s.test_engine = None;
                        s.start_test_engine();
                    }
                }
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
                Act::QueueSend { ch, content, reply, keys } => {
                    for u in s.uploads.iter_mut().filter(|u| keys.contains(&u.key)) {
                        u.armed = true;
                    }
                    s.armed.push(Armed { ch, content, reply, keys });
                    s.flush_armed();
                }
                Act::PickFile => {
                    let tx = s.picked_tx.clone();
                    let ctx2 = ctx.clone();
                    std::thread::spawn(move || {
                        for path in files::pick_files() {
                            let _ = tx.send(path);
                        }
                        ctx2.request_repaint();
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
                Act::Toast(msg) => s.toast = Some((msg, Instant::now())),
                Act::ToggleText(id) => {
                    if !s.text_open.remove(&id) {
                        s.text_open.insert(id);
                    }
                }
                Act::OpenProfile { user, name, anchor, left } => {
                    s.profile = Some(Profile { user, name, anchor, left, opened: ctx.cumulative_pass_nr(), more: false });
                }
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
                    let (source, q) = match self.golive.selected() {
                        Some(info) => (info.source.clone(), self.golive.quality(info, self.fps)),
                        None => (capture::Source::Monitor(self.monitor), publisher::Quality { height: self.golive.res, fps: self.fps, kbps: self.kbps }),
                    };
                    self.publishing_source = Some(source.clone());
                    self.publishing = Some((ch, publisher::start_source(url, source, q)));
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
                let in_voice = s.my_voice().is_some();
                let list_h = (ui.available_height() - 60.0 - if in_voice { 106.0 } else { 0.0 }).max(50.0);
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
                // "Voz conectada": só enquanto estou num canal de voz
                if let Some(mv) = s.my_voice() {
                    egui::Frame::new().inner_margin(egui::Margin { left: 8, right: 8, top: 0, bottom: 6 }).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        egui::Frame::new()
                            .fill(L2)
                            .stroke(egui::Stroke::new(1.0, LINE))
                            .corner_radius(egui::CornerRadius::same(10))
                            .inner_margin(egui::Margin::same(8))
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                let (place_ch, place_sv) = s.voice_place(mv.channel_id).unwrap_or_default();
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    let (sr, _) = ui.allocate_exact_size(egui::vec2(16.0, 36.0), egui::Sense::hover());
                                    signal_icon(ui.painter(), sr.center(), OK);
                                    let tr = ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing.y = 0.0;
                                        ui.label(RichText::new("Voz conectada").size(13.0).strong().color(OK));
                                        let sub = format!("{place_ch} / {place_sv}");
                                        let sub: String = if sub.chars().count() > 22 { format!("{}…", sub.chars().take(21).collect::<String>()) } else { sub };
                                        ui.label(RichText::new(sub).size(11.0).color(MUTED));
                                    });
                                    let tap = ui.interact(tr.response.rect, egui::Id::new("voice-panel-text"), egui::Sense::click());
                                    if tap.hovered() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                    }
                                    if tap.clicked() {
                                        acts.push(Act::OpenVoiceView);
                                    }
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        let (r, resp) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::click());
                                        let hot = resp.hovered();
                                        ui.painter().rect_filled(r, 8.0, if hot { L4 } else { L3 });
                                        hangup_icon(ui.painter(), r.center(), 18.0, if hot { RED } else { TEXT2 });
                                        if hot {
                                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                        }
                                        if resp.on_hover_text("Desconectar").clicked() {
                                            acts.push(Act::LeaveVoice);
                                        }
                                    });
                                });
                                // linha 2: Vídeo (câmera, em breve) e Tela (compartilhar), como no Discord
                                ui.add_space(6.0);
                                let sharing = self.publishing.as_ref().is_some_and(|(c, _)| *c == mv.channel_id);
                                let bw = (ui.available_width() - 6.0) / 2.0;
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 6.0;
                                    for (is_screen, label) in [(false, "Vídeo"), (true, "Tela")] {
                                        let (r, resp) = ui.allocate_exact_size(egui::vec2(bw, 32.0), egui::Sense::click());
                                        let enabled = is_screen;
                                        let active = is_screen && sharing;
                                        let hot = resp.hovered() && enabled;
                                        let pt = ui.painter();
                                        pt.rect_filled(r, 8.0, if active { alpha(AMBER, 0.16) } else if hot { L4 } else { L3 });
                                        if active {
                                            pt.rect_stroke(r, 8.0, egui::Stroke::new(1.0, AMBER), egui::StrokeKind::Inside);
                                        }
                                        let ink = if !enabled { alpha(TEXT2, 0.4) } else if active { AMBER } else { TEXT };
                                        let ic = egui::pos2(r.left() + 24.0, r.center().y);
                                        if is_screen {
                                            screen_icon(pt, ic, 18.0, ink);
                                        } else {
                                            camera_icon(pt, ic, 18.0, ink);
                                        }
                                        pt.text(egui::pos2(r.left() + 40.0, r.center().y), Align2::LEFT_CENTER, label, egui::FontId::proportional(13.0), ink);
                                        if hot {
                                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                        }
                                        let tip = if is_screen { if sharing { "Gerenciar a transmissão" } else { "Compartilhar a tela" } } else { "Câmera: em breve" };
                                        if resp.on_hover_text(tip).clicked() && enabled {
                                            acts.push(Act::OpenGoLive(Some(mv.channel_id)));
                                        }
                                    }
                                });
                            });
                    });
                }
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
                                let left = ui
                                    .horizontal(|ui| {
                                        avatar(ui, &s.me.name, s.me.id, 32.0, Some(my), L2);
                                        ui.vertical(|ui| {
                                            ui.spacing_mut().item_spacing.y = 0.0;
                                            ui.label(RichText::new(&s.me.name).strong().color(TEXT));
                                            ui.label(RichText::new(status_label(my)).size(11.5).color(MUTED));
                                        });
                                    })
                                    .response
                                    .rect;
                                // microfone e fone (mudo/surdo); riscado e avermelhado quando desligado
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.spacing_mut().item_spacing.x = 2.0;
                                    let deaf = s.voice_deaf;
                                    let muted = s.voice_muted || s.voice_deaf;
                                    for (is_deaf, off) in [(true, deaf), (false, muted)] {
                                        let (r, resp) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::click());
                                        let hot = resp.hovered();
                                        let pt = ui.painter();
                                        if off {
                                            pt.rect_filled(r, 8.0, alpha(RED, if hot { 0.22 } else { 0.14 }));
                                        } else if hot {
                                            pt.rect_filled(r, 8.0, L4);
                                        }
                                        let ink = if off { RED } else if hot { TEXT } else { TEXT2 };
                                        if is_deaf {
                                            headphone_icon(pt, r.center(), 18.0, ink, off);
                                        } else {
                                            mic_icon(pt, r.center(), 18.0, ink, off);
                                        }
                                        if hot {
                                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                        }
                                        let tip = match (is_deaf, off) {
                                            (true, true) => "Ativar áudio",
                                            (true, false) => "Desativar áudio",
                                            (false, true) => "Ativar microfone",
                                            (false, false) => "Desativar microfone",
                                        };
                                        if resp.on_hover_text(tip).clicked() {
                                            acts.push(if is_deaf { Act::ToggleDeaf } else { Act::ToggleMute });
                                        }
                                    }
                                });
                                left
                            });
                            // clicar no cartão abre o menu de status (só a parte do nome, os botões ficam livres)
                            let rc = row.inner.expand2(egui::vec2(6.0, 4.0)).intersect(ui.max_rect().expand2(egui::vec2(0.0, 4.0)));
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
                                if ui.button("Configurações de voz").clicked() {
                                    acts.push(Act::OpenVoiceSettings);
                                    ui.close();
                                }
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
                .filter(|c| (c.kind == 0 || c.kind == 1) && match parent {
                    Some(p) => c.parent == Some(p),
                    None => c.parent.map_or(true, |p| !cats.iter().any(|k| k.id == p)),
                })
                .collect();
            v.sort_by_key(|c| (c.kind, c.position, c.id));
            v
        };
        let row = |ui: &mut egui::Ui, c: &p::Channel, acts: &mut Vec<Act>| {
            if c.kind == 1 {
                self.voice_channel_row(ui, s, c, acts);
                return;
            }
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
                if collapsed && c.id != current && !s.unread.contains(&c.id) && s.voice_members(c.id).is_empty() {
                    continue;
                }
                row(ui, c, acts);
            }
        }
    }

    /// Canal de voz na lista: alto-falante + nome (clique entra) e, embaixo, quem está nele.
    fn voice_channel_row(&self, ui: &mut egui::Ui, s: &Session, c: &p::Channel, acts: &mut Vec<Act>) {
        let mine = s.my_voice().map(|v| v.channel_id) == Some(c.id);
        let selected = mine && s.voice_view;
        let members = s.voice_members(c.id);
        let r = list_row(ui, selected, |ui| {
            let (ir, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
            speaker_icon(ui.painter(), ir.center(), 18.0, if mine { TEXT2 } else { MUTED });
            ui.label(RichText::new(&c.name).color(if mine { TEXT } else { TEXT2 }));
        });
        if r.clicked() {
            acts.push(Act::JoinVoice(c.id));
        }
        for v in members {
            let name = s.user_name_of(v.user_id);
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 28.0), egui::Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(rect, 6.0, lerp(L1, L2, 1.0));
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let pt = ui.painter();
            let ac = egui::pos2(rect.left() + 28.0 + 11.0, rect.center().y);
            let (bg, fg) = user_tint(v.user_id);
            pt.circle_filled(ac, 11.0, bg);
            pt.text(ac, Align2::CENTER_CENTER, name.chars().next().map(|x| x.to_uppercase().to_string()).unwrap_or_default(), egui::FontId::proportional(11.0), fg);
            if s.voice_speaking(v.user_id) {
                speaking_ring(pt, ac, 11.0, 2.0, SIDE);
            }
            pt.text(egui::pos2(rect.left() + 28.0 + 28.0, rect.center().y), Align2::LEFT_CENTER, &name, egui::FontId::proportional(13.5), if v.deafened || v.muted { MUTED } else { TEXT2 });
            let ic = egui::pos2(rect.right() - 14.0, rect.center().y);
            if v.deafened {
                headphone_icon(pt, ic, 15.0, MUTED, true);
            } else if v.muted {
                mic_icon(pt, ic, 15.0, MUTED, true);
            }
            if resp.clicked() {
                acts.push(Act::OpenProfile { user: v.user_id, name: name.clone(), anchor: rect, left: false });
            }
        }
    }

    /// Pega o quadro de prévia mais recente da minha transmissão (o publisher entrega ~10 por segundo).
    fn refresh_preview(&mut self, ctx: &egui::Context) {
        let Some((_, h)) = &self.publishing else { return };
        let frame = h.preview.lock().ok().and_then(|mut g| g.take());
        if let Some((w, hgt, rgba)) = frame {
            let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, hgt as usize], &rgba);
            match &mut self.preview_tex {
                Some(tex) => tex.set(img, egui::TextureOptions::LINEAR),
                None => self.preview_tex = Some(ctx.load_texture("my-stream-preview", img, egui::TextureOptions::LINEAR)),
            }
        }
        ctx.request_repaint_after(Duration::from_millis(100));
    }

    /// Visão do canal de voz: grade de participantes (16:9, linha incompleta centralizada) e barra de controles.
    fn voice_view_ui(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, s: &mut Session, acts: &mut Vec<Act>) {
        let Some(mv) = s.my_voice().cloned() else { return };
        let (ch_name, sv_name) = s.voice_place(mv.channel_id).unwrap_or_default();
        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 63.0), Layout::left_to_right(Align::Center), |ui| {
            ui.add_space(20.0);
            let (ir, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
            speaker_icon(ui.painter(), ir.center(), 24.0, MUTED);
            ui.add_space(4.0);
            ui.label(serif(&ch_name, 24.0));
            ui.add_space(8.0);
            ui.label(RichText::new(sv_name).size(13.0).color(MUTED));
            if s.voice_focus.is_some() {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(20.0);
                    if small_button(ui, "Grade", FIELD) {
                        s.voice_focus = None;
                    }
                });
            }
        });
        if s.voice_focus.is_some() && ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            s.voice_focus = None;
        }
        let y = ui.cursor().top();
        ui.painter().hline(ui.max_rect().x_range(), y, egui::Stroke::new(1.0, LINE));
        let total = ui.available_rect_before_wrap();
        ui.allocate_rect(total, egui::Sense::hover());
        let controls_h = 72.0;
        let stage = egui::Rect::from_min_max(total.min + egui::vec2(16.0, 16.0), egui::pos2(total.max.x - 16.0, total.max.y - controls_h - 8.0));
        let members: Vec<p::VoiceState> = s.voice_members(mv.channel_id).into_iter().cloned().collect();
        // assistindo a uma transmissão do canal: o vídeo ocupa o palco e a grade vira uma faixa embaixo
        let watching_here = matches!(&self.watching, Some((c, _)) if *c == mv.channel_id);
        let live_here = s.snap.live.iter().find(|l| l.channel_id == mv.channel_id).cloned();
        let sharing = self.publishing.as_ref().is_some_and(|(c, _)| *c == mv.channel_id);
        let preview_here = sharing && !watching_here;
        if preview_here {
            self.refresh_preview(ctx);
        }
        // foco: um participante grande no palco e os outros na faixa (duplo clique num tile; "Grade" ou Esc voltam)
        let focus = s.voice_focus.filter(|u| members.iter().any(|m| m.user_id == *u));
        let focus_here = focus.is_some() && !watching_here && !preview_here;
        let (stream_rect, stage) = if watching_here || preview_here || focus_here {
            let strip = 120.0;
            (
                Some(egui::Rect::from_min_max(stage.min, egui::pos2(stage.max.x, stage.max.y - strip - 8.0))),
                egui::Rect::from_min_max(egui::pos2(stage.min.x, stage.max.y - strip), stage.max),
            )
        } else {
            (None, stage)
        };
        let list: Vec<p::VoiceState> = if focus_here { members.iter().filter(|m| Some(m.user_id) != focus).cloned().collect() } else { members.clone() };
        let n = list.len().max(1);
        // grade: o número de colunas que dá o maior tile 16:9
        let (gap, aspect) = (8.0_f32, 16.0 / 9.0);
        let mut best = (1usize, 0.0_f32);
        for cols in 1..=n {
            let rows = n.div_ceil(cols);
            let w = ((stage.width() - (cols - 1) as f32 * gap) / cols as f32).min(((stage.height() - (rows - 1) as f32 * gap) / rows as f32) * aspect);
            if w > best.1 {
                best = (cols, w);
            }
        }
        let (cols, tw) = (best.0, best.1.clamp(120.0, 1280.0));
        let th = tw / aspect;
        let rows = n.div_ceil(cols);
        let gh = rows as f32 * th + (rows - 1) as f32 * gap;
        let top = stage.center().y - gh / 2.0;
        // um tile de participante (grade, faixa ou foco)
        let draw_tile = |ui: &mut egui::Ui, s: &mut Session, acts: &mut Vec<Act>, rect: egui::Rect, v: &p::VoiceState| {
            let th = rect.height();
            let name = s.user_name_of(v.user_id);
            let (bg, fg) = user_tint(v.user_id);
            let resp = ui.interact(rect, egui::Id::new(("voice-tile", v.user_id)), egui::Sense::click());
            let speaking = s.voice_speaking(v.user_id);
            let pt = ui.painter();
            pt.rect_filled(rect, 8.0, bg);
            let d = (th * 0.44).clamp(40.0, 128.0);
            let c = rect.center();
            pt.circle_filled(c, d / 2.0, lerp(bg, fg, 0.22));
            pt.text(c, Align2::CENTER_CENTER, name.chars().next().map(|x| x.to_uppercase().to_string()).unwrap_or_default(), egui::FontId::proportional(d * 0.42), fg);
            // pílula do nome com o estado (mudo/surdo) antes do nome
            let galley = pt.layout_no_wrap(name.clone(), egui::FontId::proportional(12.5), TEXT);
            let icon_w = if v.deafened || v.muted { 20.0 } else { 0.0 };
            let pill = egui::Rect::from_min_size(
                egui::pos2(rect.left() + 8.0, rect.bottom() - 8.0 - 22.0),
                egui::vec2(galley.size().x + 16.0 + icon_w, 22.0),
            );
            pt.rect_filled(pill, 6.0, Color32::from_black_alpha(158));
            if v.deafened {
                headphone_icon(pt, egui::pos2(pill.left() + 16.0, pill.center().y), 14.0, TEXT2, true);
            } else if v.muted {
                mic_icon(pt, egui::pos2(pill.left() + 16.0, pill.center().y), 14.0, TEXT2, true);
            }
            pt.galley(egui::pos2(pill.left() + 8.0 + icon_w, pill.center().y - galley.size().y / 2.0), galley, TEXT);
            if speaking {
                pt.rect_stroke(rect, 8.0, egui::Stroke::new(3.0, OK), egui::StrokeKind::Inside);
            }
            // quem está transmitindo: etiqueta "AO VIVO" e, para os outros, o botão Assistir
            if live_here.as_ref().is_some_and(|l| l.user_id == v.user_id) {
                let g = pt.layout_no_wrap(if v.user_id == s.me.id { "VOCÊ ESTÁ AO VIVO".to_string() } else { "AO VIVO".to_string() }, egui::FontId::proportional(10.5), AMBER);
                let pill = egui::Rect::from_min_size(rect.min + egui::vec2(8.0, 8.0), g.size() + egui::vec2(14.0, 8.0));
                pt.rect_filled(pill, 5.0, Color32::from_black_alpha(170));
                pt.galley(pill.min + egui::vec2(7.0, 4.0), g, AMBER);
                if v.user_id != s.me.id && !watching_here && rect.height() >= 100.0 {
                    let btn = egui::Rect::from_center_size(egui::pos2(rect.center().x, rect.bottom() - 40.0), egui::vec2(110.0, 30.0));
                    if ui.put(btn, egui::Button::new(RichText::new("Assistir").color(L0)).fill(AMBER)).clicked() {
                        acts.push(Act::Watch(mv.channel_id));
                    }
                }
            }
            if resp.clicked() {
                acts.push(Act::OpenProfile { user: v.user_id, name: name.clone(), anchor: rect, left: false });
            }
            if resp.double_clicked() {
                s.voice_focus = if s.voice_focus == Some(v.user_id) { None } else { Some(v.user_id) };
                s.profile = None; // o 1º clique do duplo clique abriu o perfil
            }
            if v.user_id != s.me.id {
                let uid = v.user_id;
                egui::Popup::context_menu(&resp).width(230.0).show(|ui| {
                    ui.set_min_width(220.0);
                    ui.label(RichText::new(&name).strong());
                    ui.add_space(4.0);
                    ui.label(caps("Volume do usuário"));
                    let mut vol = s.voice_volumes.get(&uid).copied().unwrap_or(1.0);
                    if ui.add(egui::Slider::new(&mut vol, 0.0..=2.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)).show_value(true)).changed() {
                        s.voice_volumes.insert(uid, vol);
                        if let Some(e) = &s.engine {
                            e.set_user_volume(uid, vol);
                        }
                    }
                });
            }
        };
        for (i, v) in list.iter().enumerate() {
            let (row, col) = (i / cols, i % cols);
            let in_row = cols.min(list.len() - row * cols);
            let rw = in_row as f32 * tw + (in_row - 1) as f32 * gap;
            let x0 = stage.center().x - rw / 2.0;
            let rect = egui::Rect::from_min_size(egui::pos2(x0 + col as f32 * (tw + gap), top + row as f32 * (th + gap)), egui::vec2(tw, th));
            draw_tile(ui, s, acts, rect, v);
        }
        if let (Some(sr), true, Some(f)) = (stream_rect, focus_here, focus) {
            if let Some(v) = members.iter().find(|m| m.user_id == f) {
                draw_tile(ui, s, acts, sr, v);
            }
        } else if let Some(sr) = stream_rect {
            if watching_here {
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(sr));
                self.stream_area(&mut child, ctx, &*s, acts, true);
            } else {
                // a minha própria tela: prévia ao vivo no palco, como no Discord
                let pt = ui.painter();
                pt.rect_filled(sr, 8.0, Color32::BLACK);
                match &self.preview_tex {
                    Some(t) => {
                        let ts = t.size_vec2();
                        let k = (sr.width() / ts.x).min(sr.height() / ts.y);
                        egui::Image::new(t).fit_to_exact_size(ts * k).paint_at(ui, egui::Rect::from_center_size(sr.center(), ts * k));
                    }
                    None => {
                        pt.text(sr.center(), Align2::CENTER_CENTER, "Preparando a prévia…", egui::FontId::proportional(15.0), MUTED);
                    }
                }
                if ui.interact(sr, egui::Id::new("my-preview"), egui::Sense::click()).double_clicked() {
                    acts.push(Act::Fullscreen);
                }
                self.video_overlay(ui, ctx, &*s, sr, mv.channel_id, true, acts);
            }
        }
        // barra de controles
        let bar = egui::Rect::from_min_max(egui::pos2(total.min.x, total.max.y - controls_h), total.max);
        let (bw, bh, bgap) = (44.0_f32, 44.0_f32, 8.0_f32);
        let total_w = bw * 4.0 + 56.0 + bgap * 4.0;
        let mut x = bar.center().x - total_w / 2.0;
        let yb = bar.center().y - bh / 2.0;
        let deaf = s.voice_deaf;
        let muted = s.voice_muted || s.voice_deaf;
        for (is_deaf, off) in [(false, muted), (true, deaf)] {
            let r = egui::Rect::from_min_size(egui::pos2(x, yb), egui::vec2(bw, bh));
            x += bw + bgap;
            let resp = ui.interact(r, egui::Id::new(("voice-ctl", is_deaf)), egui::Sense::click());
            let hot = resp.hovered();
            let pt = ui.painter();
            pt.rect_filled(r, 12.0, if off { alpha(RED, if hot { 0.24 } else { 0.16 }) } else if hot { L4 } else { L3 });
            pt.rect_stroke(r, 12.0, egui::Stroke::new(1.0, LINE2), egui::StrokeKind::Inside);
            let ink = if off { RED } else { TEXT };
            if is_deaf {
                headphone_icon(pt, r.center(), 22.0, ink, off);
            } else {
                mic_icon(pt, r.center(), 22.0, ink, off);
            }
            if hot {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let tip = match (is_deaf, off) {
                (true, true) => "Ativar áudio",
                (true, false) => "Desativar áudio",
                (false, true) => "Ativar microfone",
                (false, false) => "Desativar microfone",
            };
            if resp.on_hover_text(tip).clicked() {
                acts.push(if is_deaf { Act::ToggleDeaf } else { Act::ToggleMute });
            }
        }
        // compartilhar tela (abre o diálogo Transmitir para este canal de voz)
        {
            let r = egui::Rect::from_min_size(egui::pos2(x, yb), egui::vec2(bw, bh));
            x += bw + bgap;
            let resp = ui.interact(r, egui::Id::new("voice-share"), egui::Sense::click());
            let hot = resp.hovered();
            let pt = ui.painter();
            pt.rect_filled(r, 12.0, if sharing { alpha(AMBER, 0.18) } else if hot { L4 } else { L3 });
            pt.rect_stroke(r, 12.0, egui::Stroke::new(1.0, if sharing { AMBER } else { LINE2 }), egui::StrokeKind::Inside);
            screen_icon(pt, r.center(), 24.0, if sharing { AMBER } else { TEXT });
            if hot {
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.on_hover_text(if sharing { "Gerenciar a transmissão" } else { "Compartilhar a tela" }).clicked() {
                acts.push(Act::OpenGoLive(Some(mv.channel_id)));
            }
        }
        let r = egui::Rect::from_min_size(egui::pos2(x, yb), egui::vec2(56.0, bh));
        let resp = ui.interact(r, egui::Id::new("voice-hangup"), egui::Sense::click());
        ui.painter().rect_filled(r, 12.0, if resp.hovered() { lerp(RED, TEXT, 0.12) } else { RED });
        hangup_icon(ui.painter(), r.center(), 24.0, L0);
        if resp.hovered() {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.on_hover_text("Desconectar").clicked() {
            acts.push(Act::LeaveVoice);
        }
        let g = egui::Rect::from_min_size(egui::pos2(r.right() + bgap, yb), egui::vec2(bw, bh));
        let gresp = ui.interact(g, egui::Id::new("voice-settings"), egui::Sense::click());
        let ghot = gresp.hovered();
        ui.painter().rect_filled(g, 12.0, if ghot { L4 } else { L3 });
        ui.painter().rect_stroke(g, 12.0, egui::Stroke::new(1.0, LINE2), egui::StrokeKind::Inside);
        gear_icon(ui.painter(), g.center(), 22.0, if ghot { TEXT } else { TEXT2 });
        if ghot {
            ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if gresp.on_hover_text("Configurações de voz").clicked() {
            acts.push(Act::OpenVoiceSettings);
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
                            if r.clicked() {
                                acts.push(Act::OpenProfile { user: m.id, name: m.name.clone(), anchor: r.rect, left: true });
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
        if s.voice_view && s.my_voice().is_some() {
            self.voice_view_ui(ui, ctx, s, acts);
            return;
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
                        if !done && small_button(ui, "Qualidade", FIELD) {
                            acts.push(Act::OpenGoLive(None));
                        }
                        dot(ui, status_color(&st));
                        ui.label(RichText::new(if *pch == ch { st } else { format!("{st} (outro canal)") }).size(12.0));
                    }
                    None => {
                        if small_button(ui, "Ao vivo", GREEN) {
                            acts.push(Act::OpenGoLive(None));
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
        let input_h = 82.0 + 22.0 + if replying_here { 38.0 } else { 0.0 } + if uploads_here { 180.0 + if s.armed.iter().any(|a| a.ch == ch) { 20.0 } else { 0.0 } } else { 0.0 };
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
                        // já "nasceu" faz tempo: ao mostrar, a mensagem não repete a animação de entrada
                        s.seen.entry(m.id).or_insert_with(|| Instant::now().checked_sub(Duration::from_secs(5)).unwrap_or_else(Instant::now));
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
                            let ar = avatar(ui, &m.author, m.author_id, 40.0, None, BG);
                            if ar.hovered() {
                                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                            }
                            if ar.clicked() && m.kind == 0 {
                                acts.push(Act::OpenProfile { user: m.author_id, name: m.author.clone(), anchor: ar.rect, left: false });
                            }
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
                                    let nr = ui.add(egui::Label::new(RichText::new(&m.author).size(15.5).strong().color(TEXT)).sense(egui::Sense::click()));
                                    if nr.hovered() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                        ui.painter().hline(nr.rect.x_range(), nr.rect.bottom() - 1.0, egui::Stroke::new(1.0, TEXT));
                                    }
                                    if nr.clicked() && m.kind == 0 {
                                        acts.push(Act::OpenProfile { user: m.author_id, name: m.author.clone(), anchor: nr.rect, left: false });
                                    }
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
                                        md_view::Click::User(uid, pos) => {
                                            let name = md_view::user_name(&s.snap, uid).unwrap_or_default();
                                            acts.push(Act::OpenProfile { user: uid, name, anchor: egui::Rect::from_min_size(pos, egui::vec2(0.0, 18.0)), left: false });
                                        }
                                    }
                                }
                            }
                            for a in &m.attachments {
                                attachment_view(ui, a, &http_base, &token, &mut s.images, &mut s.texts, s.text_open.contains(&a.id), acts);
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
        // autocompletar de "@": sugestões enquanto você digita um nome depois do @
        let cursor = egui::TextEdit::load_state(ctx, input_id)
            .and_then(|st| st.cursor.char_range())
            .map(|r| usize::from(r.primary.index))
            .unwrap_or_else(|| s.input.chars().count());
        let token = if focused { mention_token(&s.input, cursor) } else { None };
        if token.is_none() {
            s.ac_off = None;
        }
        let cands = match &token {
            Some((start, q)) if s.ac_off != Some(*start) => mention_candidates(s, ch, q),
            _ => Vec::new(),
        };
        let ac_open = !cands.is_empty();
        let mut ac_pick: Option<usize> = None;
        if ac_open {
            s.ac_sel = s.ac_sel.min(cands.len() - 1);
            // as setas, Tab, Enter e Esc são do autocompletar enquanto ele está aberto
            let n = cands.len();
            ctx.input_mut(|i| {
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                    s.ac_sel = (s.ac_sel + 1) % n;
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                    s.ac_sel = (s.ac_sel + n - 1) % n;
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::Tab) || i.consume_key(egui::Modifiers::NONE, egui::Key::Enter) {
                    ac_pick = Some(s.ac_sel);
                }
                if i.consume_key(egui::Modifiers::NONE, egui::Key::Escape) {
                    s.ac_off = token.as_ref().map(|t| t.0);
                }
            });
        }
        let mut ac_applied = |s: &mut Session, idx: usize| {
            if let Some((start, _)) = &token {
                let new_cursor = apply_mention(&mut s.input, *start, cursor, &cands[idx].name);
                if let Some(mut st) = egui::TextEdit::load_state(ctx, input_id) {
                    st.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(new_cursor))));
                    st.store(ctx, input_id);
                }
                s.ac_sel = 0;
                s.focus_composer = true;
            }
        };
        if let Some(i) = ac_pick {
            ac_applied(s, i);
        }
        let comp = egui::Frame::new()
            .fill(L3)
            .stroke(egui::Stroke::new(1.0, if focused { AMBER } else { LINE2 }))
            .corner_radius(egui::CornerRadius::same(12))
            .inner_margin(egui::Margin { left: 10, right: 8, top: 8, bottom: 8 })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                // bandeja de anexos pendentes: no topo da caixa, miniaturas 120x120
                if s.uploads.iter().any(|u| u.channel == ch) {
                    ui.add_space(4.0);
                    // barra fina que ocupa o próprio espaço (não cobre o nome dos arquivos)
                    let mut st = egui::style::ScrollStyle::solid();
                    st.bar_width = 6.0;
                    st.bar_inner_margin = 4.0;
                    st.foreground_color = true;
                    st.dormant_handle_opacity = 0.6;
                    st.active_handle_opacity = 0.8;
                    ui.spacing_mut().scroll = st;
                    ui.visuals_mut().widgets.inactive.fg_stroke.color = Color32::from_rgb(0x4a, 0x4a, 0x50);
                    ui.visuals_mut().widgets.hovered.fg_stroke.color = Color32::from_rgb(0x7a, 0x7a, 0x82);
                    ui.visuals_mut().widgets.active.fg_stroke.color = AMBER;
                    // a roda do mouse sobre a bandeja rola na horizontal
                    if ctx.pointer_hover_pos().is_some_and(|p| s.tray_rect.contains(p)) {
                        let dy = ctx.input(|i| i.smooth_scroll_delta.y);
                        if dy != 0.0 {
                            s.tray_x = (s.tray_x - dy).max(0.0);
                            ctx.input_mut(|i| i.smooth_scroll_delta.y = 0.0);
                        }
                    }
                    let out = egui::ScrollArea::horizontal()
                        .id_salt(("tray", ch))
                        .auto_shrink([false, true])
                        .horizontal_scroll_offset(s.tray_x)
                        .show(ui, |ui| {
                            ui.horizontal_top(|ui| {
                                ui.spacing_mut().item_spacing = egui::vec2(8.0, 0.0);
                                for u in s.uploads.iter().filter(|u| u.channel == ch) {
                                    upload_tile(ui, u, &mut s.images, acts);
                                }
                            });
                        });
                    s.tray_x = out.state.offset().x;
                    s.tray_rect = out.inner_rect;
                    if s.armed.iter().any(|a| a.ch == ch) {
                        ui.add_space(4.0);
                        ui.label(RichText::new("A mensagem sai assim que os anexos terminarem de subir.").size(12.0).color(AMBER));
                    }
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
                    let known = known_mention_names(s, ch);
                    let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, wrap: f32| {
                        let job = mention_job(ui, text.as_str(), &known, wrap);
                        ui.painter().layout_job(job)
                    };
                    let r = ui.add_sized(
                        [ui.available_width() - 84.0, 32.0],
                        egui::TextEdit::singleline(&mut s.input)
                            .layouter(&mut layouter)
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
                    // anexos desta mensagem: os que ainda não pertencem a outra mensagem na fila
                    let mine: Vec<&files::Upload> = s.uploads.iter().filter(|u| u.channel == ch && !u.armed).collect();
                    let atts: Vec<p::Attachment> =
                        mine.iter().filter_map(|u| if let files::UpState::Done(a) = &u.state { Some(a.clone()) } else { None }).collect();
                    let keys: Vec<u64> = mine.iter().filter(|u| !matches!(u.state, files::UpState::Failed(_))).map(|u| u.key).collect();
                    let uploading = mine.iter().any(|u| matches!(u.state, files::UpState::Queued | files::UpState::Sending));
                    if (enter || clicked) && (!s.input.trim().is_empty() || !keys.is_empty()) {
                        let reply = s.replying.as_ref().filter(|(c, _)| *c == ch).map(|(_, r)| r.clone());
                        let had_reply = reply.is_some();
                        let content = std::mem::take(&mut s.input);
                        if uploading {
                            // sem bloquear: a mensagem sai sozinha quando o último anexo terminar de subir
                            acts.push(Act::QueueSend { ch, content, reply, keys });
                        } else {
                            acts.push(Act::SendMsg { ch, content, reply, attachments: atts });
                        }
                        if had_reply {
                            acts.push(Act::CancelReply);
                        }
                        r.request_focus();
                    }
                });
            });
        if ac_open {
            // Tab, Esc e as setas verticais ficam com o campo (senão o egui move o foco e o Tab não completa)
            ctx.memory_mut(|m| {
                m.set_focus_lock_filter(input_id, egui::EventFilter { tab: true, horizontal_arrows: false, vertical_arrows: true, escape: true })
            });
            let rect = comp.response.rect;
            let q = token.as_ref().map(|t| t.1.clone()).unwrap_or_default();
            let mut clicked_row: Option<usize> = None;
            egui::Area::new(egui::Id::new("mention-ac"))
                .order(egui::Order::Foreground)
                .pivot(Align2::LEFT_BOTTOM)
                .fixed_pos(egui::pos2(rect.left(), rect.top() - 8.0))
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(L3)
                        .stroke(egui::Stroke::new(1.0, LINE2))
                        .corner_radius(egui::CornerRadius::same(10))
                        .shadow(egui::Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(100) })
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.set_width(rect.width() - 18.0);
                            ui.spacing_mut().item_spacing = egui::vec2(0.0, 2.0);
                            let head = if q.is_empty() { "Membros".to_string() } else { format!("Resultados de membros para @{q}") };
                            ui.label(RichText::new(head).size(12.5).color(TEXT2));
                            ui.add_space(4.0);
                            for (i, c) in cands.iter().enumerate() {
                                let (row, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 36.0), egui::Sense::click());
                                if resp.hovered() && ctx.input(|inp| inp.pointer.delta() != egui::Vec2::ZERO) {
                                    s.ac_sel = i;
                                }
                                let sel = s.ac_sel == i;
                                let pt = ui.painter();
                                if sel {
                                    pt.rect_filled(row, 6.0, L4);
                                }
                                let ac = egui::pos2(row.left() + 20.0, row.center().y);
                                match c.user {
                                    Some((uid, st)) => {
                                        let (bg, fg) = user_tint(uid);
                                        pt.circle_filled(ac, 12.0, bg);
                                        let ch0 = c.name.chars().next().map(|x| x.to_uppercase().to_string()).unwrap_or_default();
                                        pt.text(ac, Align2::CENTER_CENTER, ch0, egui::FontId::proportional(12.0), fg);
                                        presence_dot(pt, ac + egui::vec2(9.0, 9.0), 4.5, st, if sel { L4 } else { L3 });
                                    }
                                    None => {
                                        pt.circle_filled(ac, 12.0, alpha(AMBER, 0.2));
                                        pt.text(ac, Align2::CENTER_CENTER, "@", egui::FontId::proportional(14.0), AMBER);
                                    }
                                }
                                pt.text(egui::pos2(row.left() + 42.0, row.center().y), Align2::LEFT_CENTER, &c.name, egui::FontId::proportional(14.5), TEXT);
                                if !c.note.is_empty() {
                                    pt.text(egui::pos2(row.right() - 10.0, row.center().y), Align2::RIGHT_CENTER, &c.note, egui::FontId::proportional(12.0), MUTED);
                                }
                                if resp.hovered() {
                                    ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
                                }
                                if resp.clicked() {
                                    clicked_row = Some(i);
                                }
                            }
                        });
                });
            if let Some(i) = clicked_row {
                ac_applied(s, i);
                ctx.request_repaint();
            }
        }
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
    /// Tela cheia enquanto só eu transmito. Mostrar a própria tela ao vivo aqui vira um espelho infinito (a janela
    /// que está sendo capturada exibe a prévia dela mesma e tudo converge para preto), então, como o Discord,
    /// mostramos o cartão "Sua transmissão ainda está ligada!" com as métricas do encoder.
    fn preview_fullscreen(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, s: &Session, acts: &mut Vec<Act>) {
        let Some((ch, h)) = &self.publishing else { return };
        let ch = *ch;
        let status = h.status.lock().map(|g| g.clone()).unwrap_or_default();
        let (rect, resp) = ui.allocate_exact_size(ui.available_size(), egui::Sense::click());
        let pt = ui.painter();
        pt.rect_filled(rect, 0.0, Color32::BLACK);
        pt.text(rect.center() - egui::vec2(0.0, 22.0), Align2::CENTER_CENTER, "Sua transmissão ainda está ligada!", egui::FontId::proportional(22.0), TEXT);
        pt.text(
            rect.center() + egui::vec2(0.0, 8.0),
            Align2::CENTER_CENTER,
            "Pausamos esta prévia para preservar seus recursos (mostrar a tela inteira dentro dela mesma só gera um espelho).",
            egui::FontId::proportional(13.0),
            MUTED,
        );
        pt.text(rect.center() + egui::vec2(0.0, 36.0), Align2::CENTER_CENTER, status, egui::FontId::monospace(12.5), alpha(AMBER, 0.9));
        let btn = egui::Rect::from_center_size(rect.center() + egui::vec2(0.0, 80.0), egui::vec2(170.0, 34.0));
        if ui.put(btn, egui::Button::new("Sair da tela cheia")).clicked() {
            acts.push(Act::Fullscreen);
        }
        if resp.double_clicked() {
            acts.push(Act::Fullscreen);
        }
        ctx.request_repaint_after(Duration::from_millis(500));
        self.video_overlay(ui, ctx, s, rect, ch, true, acts);
    }

    /// Controles sobre o vídeo (assistindo ou a minha prévia): barras que somem após 1,8 s sem mexer o mouse,
    /// com título, quem transmite, espectadores, tela cheia, engrenagem (só a minha) e parar.
    #[allow(clippy::too_many_arguments)]
    fn video_overlay(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, s: &Session, rect: egui::Rect, ch: i64, mine: bool, acts: &mut Vec<Act>) {
        let pointer = ctx.input(|i| i.pointer.hover_pos());
        let inside = pointer.is_some_and(|p| rect.contains(p));
        if inside && ctx.input(|i| i.pointer.delta() != egui::Vec2::ZERO || i.pointer.any_pressed() || i.pointer.any_click()) {
            self.video_active = Instant::now();
        }
        let hold = self.viewers_hover_until > Instant::now();
        let visible = hold || (inside && self.video_active.elapsed() < Duration::from_millis(1800));
        let a = ctx.animate_bool_with_time(egui::Id::new(("video-controls", mine)), visible, 0.15);
        if visible {
            ctx.request_repaint_after(Duration::from_millis(250));
        }
        if a <= 0.01 {
            return;
        }
        let fade = |c: Color32| c.gamma_multiply(a);
        let top = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 44.0));
        let bottom = egui::Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 52.0), rect.max);
        let pt = ui.painter();
        let cr = |top_side: bool| if top_side { egui::CornerRadius { nw: 8, ne: 8, sw: 0, se: 0 } } else { egui::CornerRadius { nw: 0, ne: 0, sw: 8, se: 8 } };
        pt.rect_filled(top, cr(true), fade(Color32::from_black_alpha(150)));
        pt.rect_filled(bottom, cr(false), fade(Color32::from_black_alpha(150)));
        // título (canal / servidor) e quem transmite
        let place = s.voice_place(ch).map(|(c, sv)| format!("{c} / {sv}")).unwrap_or_default();
        pt.text(top.left_center() + egui::vec2(14.0, 0.0), Align2::LEFT_CENTER, place, egui::FontId::proportional(13.0), fade(TEXT));
        let who = if mine {
            "Você está transmitindo".to_string()
        } else {
            s.snap.live.iter().find(|l| l.channel_id == ch).map(|l| s.name_of(l.user_id)).unwrap_or_default()
        };
        pt.text(bottom.left_center() + egui::vec2(14.0, 0.0), Align2::LEFT_CENTER, who, egui::FontId::proportional(13.0), fade(TEXT));

        // botões, da direita para a esquerda: parar, tela cheia, engrenagem (minha)
        let mut x = bottom.right() - 12.0 - 32.0;
        let y = bottom.center().y - 16.0;
        let mut button = |ui: &mut egui::Ui, id: &str, tip: &str, danger: bool, draw: &dyn Fn(&egui::Painter, egui::Pos2, Color32)| -> bool {
            let r = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(32.0, 32.0));
            x -= 36.0;
            let resp = ui.interact(r, egui::Id::new(("video-btn", id, mine)), egui::Sense::click());
            let hot = resp.hovered();
            let pt = ui.painter();
            if hot {
                pt.rect_filled(r, 8.0, if danger { fade(alpha(RED, 0.35)) } else { fade(Color32::from_white_alpha(26)) });
                ctx.set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            draw(pt, r.center(), fade(if hot && danger { RED } else { TEXT }));
            resp.on_hover_text(tip).clicked()
        };
        if mine {
            if button(ui, "stop", "Parar de transmitir", true, &|p, c, col| stop_icon(p, c, 22.0, col)) {
                acts.push(Act::StopPublish);
            }
        } else if button(ui, "stop", "Parar de assistir", true, &|p, c, col| close_icon(p, c, 22.0, col)) {
            acts.push(Act::StopWatch);
        }
        let full = self.fullscreen;
        if button(ui, "full", if full { "Sair da tela cheia" } else { "Tela cheia" }, false, &|p, c, col| fullscreen_icon(p, c, 22.0, col, full)) {
            acts.push(Act::Fullscreen);
        }
        if mine && button(ui, "gear", "Qualidade da transmissão", false, &|p, c, col| gear_icon(p, c, 22.0, col)) {
            acts.push(Act::OpenGoLive(Some(ch)));
        }

        // espectadores: até 3 avatares sobrepostos e "+N"; passar o mouse abre a lista
        let viewers: Vec<i64> = s.viewers.get(&ch).cloned().unwrap_or_default().into_iter().filter(|u| !mine || *u != s.me.id).collect();
        if !viewers.is_empty() {
            let shown = viewers.len().min(3);
            let extra = viewers.len() - shown;
            let w = 24.0 + (shown as f32 - 1.0) * 18.0 + if extra > 0 { 30.0 } else { 0.0 };
            let chip = egui::Rect::from_min_size(egui::pos2(x - w + 36.0 - 8.0, bottom.center().y - 12.0), egui::vec2(w, 24.0));
            let resp = ui.interact(chip, egui::Id::new(("viewers-chip", mine)), egui::Sense::hover());
            let pt = ui.painter();
            for (i, u) in viewers.iter().take(shown).enumerate() {
                let c = egui::pos2(chip.left() + 12.0 + i as f32 * 18.0, chip.center().y);
                let (bg, fg) = user_tint(*u);
                pt.circle_filled(c, 12.5, fade(Color32::from_rgb(0x20, 0x20, 0x24)));
                pt.circle_filled(c, 11.0, fade(bg));
                pt.text(c, Align2::CENTER_CENTER, s.user_name_of(*u).chars().next().map(|x| x.to_uppercase().to_string()).unwrap_or_default(), egui::FontId::proportional(10.0), fade(fg));
            }
            if extra > 0 {
                pt.text(egui::pos2(chip.right() - 14.0, chip.center().y), Align2::CENTER_CENTER, format!("+{extra}"), egui::FontId::proportional(12.0), fade(TEXT));
            }
            let mut over_list = false;
            if resp.hovered() || self.viewers_hover_until > Instant::now() {
                let area = egui::Area::new(egui::Id::new(("viewers-list", mine)))
                    .order(egui::Order::Foreground)
                    .pivot(Align2::RIGHT_BOTTOM)
                    .fixed_pos(egui::pos2(chip.right(), chip.top() - 8.0))
                    .show(ctx, |ui| {
                        egui::Frame::new()
                            .fill(L3)
                            .stroke(egui::Stroke::new(1.0, LINE2))
                            .corner_radius(egui::CornerRadius::same(10))
                            .inner_margin(egui::Margin::same(10))
                            .show(ui, |ui| {
                                ui.set_min_width(190.0);
                                ui.label(RichText::new(format!("Espectadores - {}", viewers.len())).size(12.0).strong().color(TEXT2));
                                ui.add_space(4.0);
                                for u in viewers.iter().take(12) {
                                    ui.horizontal(|ui| {
                                        let (r, _) = ui.allocate_exact_size(egui::vec2(24.0, 24.0), egui::Sense::hover());
                                        let (bg, fg) = user_tint(*u);
                                        ui.painter().circle_filled(r.center(), 12.0, bg);
                                        ui.painter().text(r.center(), Align2::CENTER_CENTER, s.user_name_of(*u).chars().next().map(|x| x.to_uppercase().to_string()).unwrap_or_default(), egui::FontId::proportional(11.0), fg);
                                        ui.label(RichText::new(s.user_name_of(*u)).size(12.5).color(TEXT));
                                    });
                                }
                                if viewers.len() > 12 {
                                    ui.label(RichText::new(format!("e mais {}", viewers.len() - 12)).size(12.0).color(MUTED));
                                }
                            });
                    });
                over_list = pointer.is_some_and(|p| area.response.rect.contains(p));
            }
            if resp.hovered() || over_list {
                // fecha 150 ms depois de sair do chip e da lista
                self.viewers_hover_until = Instant::now() + Duration::from_millis(150);
                ctx.request_repaint_after(Duration::from_millis(160));
            }
        }
    }

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
            None => {}
        }
        let ch = *ch;
        // cartões de estado (aguardando, terminou, falhou, você caiu)
        let has_frame = self.texture.is_some();
        if let Some((title, sub, close)) = stream_state(&st, matches!(s.link, Link::Up), has_frame) {
            let pt = ui.painter();
            if has_frame {
                pt.rect_filled(rect, 0.0, Color32::from_black_alpha(150));
            }
            pt.text(rect.center() - egui::vec2(0.0, 14.0), Align2::CENTER_CENTER, title, egui::FontId::proportional(18.0), TEXT);
            if !sub.is_empty() {
                pt.text(rect.center() + egui::vec2(0.0, 12.0), Align2::CENTER_CENTER, sub, egui::FontId::proportional(13.0), MUTED);
            }
            if close {
                let btn = egui::Rect::from_center_size(rect.center() + egui::vec2(0.0, 52.0), egui::vec2(160.0, 32.0));
                if ui.put(btn, egui::Button::new("Fechar transmissão")).clicked() {
                    acts.push(Act::StopWatch);
                }
            }
        }
        if resp.double_clicked() {
            acts.push(Act::Fullscreen);
        }
        self.video_overlay(ui, ctx, s, rect, ch, false, acts);
        ui.add_space(6.0);
    }

    // ---- janelas modais
    fn modals(&mut self, ctx: &egui::Context, s: &mut Session, acts: &mut Vec<Act>) {
        let mut close = false;
        let mut dismissed = false; // Esc, clique fora ou X
        // saiu das configurações de voz: solta o microfone de teste e desliga o "ouvir meu microfone"
        if !matches!(s.modal, Modal::VoiceSettings(_)) && (s.test_engine.is_some() || s.voice_monitor) {
            s.test_engine = None;
            s.voice_monitor = false;
            s.apply_voice_options();
        }
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
                            ui.selectable_value(kind, 0, "Texto");
                            ui.selectable_value(kind, 1, "Voz");
                            ui.selectable_value(kind, 4, "Categoria");
                        });
                        ui.add_space(6.0);
                        ui.label(caps(if *kind == 4 { "Nome da categoria" } else { "Nome do canal" }));
                        ui.add(egui::TextEdit::singleline(name).desired_width(f32::INFINITY));
                        if *kind != 4 && !cats.is_empty() {
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
                                parent_id: if *kind != 4 { *parent } else { None },
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
                let caption = format!("{} · {}", a.name, files::human_size(a.size));
                if lightbox(ctx, tex, &caption, Some(files::url(&base, a, &token))) {
                    dismissed = true;
                }
            }
            Modal::VoiceSettings(devs) => {
                let (mut input, mut output) = (s.voice_in.clone(), s.voice_out.clone());
                let level = s.engine.as_ref().or(s.test_engine.as_ref()).map(|e| e.mic_level());
                let connected = s.engine.is_some();
                let (mut ns, mut monitor) = (s.voice_ns, s.voice_monitor);
                modal(ctx, "Configurações de voz", 440.0, &mut dismissed, |ui| {
                    ui.set_width(400.0);
                    let shown = |v: &Option<String>| v.clone().unwrap_or_else(|| "Padrão do sistema".into());
                    ui.label(caps("MICROFONE"));
                    egui::ComboBox::from_id_salt("voice-in").width(ui.available_width()).selected_text(shown(&input)).show_ui(ui, |ui| {
                        ui.selectable_value(&mut input, None, "Padrão do sistema");
                        for d in &devs.inputs {
                            ui.selectable_value(&mut input, Some(d.clone()), d);
                        }
                    });
                    ui.add_space(8.0);
                    ui.label(caps("SAÍDA DE ÁUDIO"));
                    egui::ComboBox::from_id_salt("voice-out").width(ui.available_width()).selected_text(shown(&output)).show_ui(ui, |ui| {
                        ui.selectable_value(&mut output, None, "Padrão do sistema");
                        for d in &devs.outputs {
                            ui.selectable_value(&mut output, Some(d.clone()), d);
                        }
                    });
                    ui.add_space(8.0);
                    ui.label(caps("NÍVEL DO MICROFONE"));
                    let (bar, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 10.0), egui::Sense::hover());
                    ui.painter().rect_filled(bar, 5.0, L2);
                    match level {
                        Some(l) => {
                            let w = bar.width() * l.clamp(0.0, 1.0);
                            ui.painter().rect_filled(egui::Rect::from_min_size(bar.min, egui::vec2(w, bar.height())), 5.0, OK);
                            ctx.request_repaint_after(Duration::from_millis(80));
                        }
                        None => {
                            ui.label(RichText::new("Entre num canal de voz para ver o nível do microfone.").size(12.0).color(MUTED));
                        }
                    }
                    if connected {
                        ui.label(RichText::new("Mudar o dispositivo reabre o áudio na hora.").size(12.0).color(MUTED));
                    }
                    ui.add_space(8.0);
                    ui.checkbox(&mut ns, "Supressão de ruído");
                    ui.label(RichText::new("Tira teclado, ventilador e barulho de fundo antes de enviar (RNNoise).").size(12.0).color(MUTED));
                    ui.add_space(4.0);
                    ui.checkbox(&mut monitor, "Ouvir meu microfone");
                    ui.label(RichText::new("Toca o que os outros ouvem. Use fone de ouvido: com caixa de som dá microfonia.").size(12.0).color(if monitor { AMBER } else { MUTED }));
                    ui.add_space(10.0);
                    if small_button(ui, "Fechar", FIELD) {
                        close = true;
                    }
                });
                if input != s.voice_in || output != s.voice_out {
                    acts.push(Act::SetVoiceDevices { input, output });
                }
                if ns != s.voice_ns || monitor != s.voice_monitor {
                    acts.push(Act::SetVoiceOptions { ns, monitor });
                }
            }
            Modal::TextView(a) => {
                let (base, token) = (s.http_base.clone(), s.token.clone());
                let url = files::url(&base, a, &token);
                let loaded = match s.texts.get(ctx, &base, &token, a) {
                    files::Text::Ready { text, bytes_left, .. } => Some((text.clone(), *bytes_left)),
                    _ => None,
                };
                let max_h = (ctx.content_rect().height() - 260.0).max(160.0);
                let name = a.name.clone();
                modal(ctx, &name, 720.0, &mut dismissed, |ui| {
                    ui.set_width(680.0);
                    match &loaded {
                        Some((text, left)) => {
                            egui::Frame::new().fill(L2).corner_radius(egui::CornerRadius::same(8)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                egui::ScrollArea::vertical().max_height(max_h).auto_shrink([false, true]).show(ui, |ui| {
                                    let mut job = egui::text::LayoutJob::default();
                                    let mono = egui::FontId::monospace(12.5);
                                    job.append(text, 0.0, egui::TextFormat { font_id: mono.clone(), color: TEXT, line_height: Some(18.0), ..Default::default() });
                                    if *left > 0 {
                                        job.append(&format!("\n… ({} restantes)", files::human_size(*left)), 0.0, egui::TextFormat { font_id: mono, color: MUTED, line_height: Some(18.0), ..Default::default() });
                                    }
                                    job.wrap.max_width = ui.available_width();
                                    ui.add(egui::Label::new(job).selectable(true));
                                });
                            });
                        }
                        None => {
                            ui.label(RichText::new("Carregando…").color(MUTED));
                            ui.ctx().request_repaint_after(Duration::from_millis(200));
                        }
                    }
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(files::human_size(a.size)).size(13.0).color(MUTED));
                        if small_button(ui, "Baixar", BLURPLE) {
                            ctx.open_url(egui::OpenUrl::new_tab(url.clone()));
                        }
                        if small_button(ui, "Fechar", FIELD) {
                            close = true;
                        }
                    });
                });
            }
            Modal::PreviewLocal { key, name, path } => {
                let thumb = match s.images.get_local(ctx, *key, path, false) {
                    files::Img::Ready(t) => Some(t.clone()),
                    _ => None,
                };
                let tex = match s.images.get_local(ctx, *key, path, true) {
                    files::Img::Ready(t) => Some(t.clone()),
                    _ => thumb,
                };
                if lightbox(ctx, tex, name, None) {
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
                            let id = egui::Id::new("invite-copied");
                            let copied = ctx.data(|d| d.get_temp::<Instant>(id)).is_some_and(|t| t.elapsed() < Duration::from_millis(2200));
                            if copied {
                                ctx.request_repaint_after(Duration::from_millis(2300));
                            }
                            if small_button(ui, if copied { "Copiado" } else { "Copiar" }, if copied { L4 } else { BLURPLE }) {
                                ctx.copy_text(code.clone());
                                ctx.data_mut(|d| d.insert_temp(id, Instant::now()));
                                acts.push(Act::Toast("Código de convite copiado".into()));
                            }
                            if small_button(ui, "Fechar", FIELD) {
                                close = true;
                            }
                        });
                    });
            }
            Modal::GoLive => {
                let cur = self.golive_target.or(cur);
                self.golive.refresh(ctx);
                let live = self.publishing.is_some();
                let place = cur
                    .and_then(|c| s.snap.servers.iter().find_map(|sv| sv.channels.iter().find(|x| x.id == c).map(|x| format!("{} / {}", x.name, sv.name))))
                    .or_else(|| cur.and_then(|c| s.snap.dms.iter().find(|d| d.channel_id == c).map(|d| d.user.name.clone())))
                    .unwrap_or_default();
                let mut go_publish = false;
                let mut go_stop = false;
                let gl = &mut self.golive;
                let fps = &mut self.fps;
                modal(ctx, "Transmitir", 540.0, &mut dismissed, |ui| {
                    ui.set_width(500.0);
                    ui.label(RichText::new("Compartilhar uma tela ou janela").size(12.0).color(MUTED));
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut gl.tab, 0, "Telas");
                        ui.selectable_value(&mut gl.tab, 1, "Janelas");
                    });
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical().max_height(220.0).auto_shrink([false, true]).show(ui, |ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(12.0, 8.0);
                        let want_window = gl.tab == 1;
                        let shown: Vec<usize> = (0..gl.items.len()).filter(|i| matches!(gl.items[*i].info.source, capture::Source::Window(_)) == want_window).collect();
                        if shown.is_empty() {
                            ui.label(RichText::new(if gl.busy || gl.items.is_empty() { "Procurando…" } else { "Nenhuma janela aberta para compartilhar." }).color(MUTED));
                        }
                        ui.horizontal_wrapped(|ui| {
                            for i in shown {
                                let (info, tex) = (&gl.items[i].info, &gl.items[i].tex);
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 4.0;
                                    let (rect, resp) = ui.allocate_exact_size(egui::vec2(156.0, 88.0), egui::Sense::click());
                                    match tex {
                                        Some(t) => {
                                            egui::Image::new(t).corner_radius(egui::CornerRadius::same(6)).paint_at(ui, rect);
                                        }
                                        None => {
                                            ui.painter().rect_filled(rect, 6.0, L2);
                                        }
                                    }
                                    let sel = gl.source.as_ref() == Some(&info.source);
                                    ui.painter().rect_stroke(
                                        rect,
                                        6.0,
                                        egui::Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { AMBER } else if resp.hovered() { LINE2 } else { LINE }),
                                        egui::StrokeKind::Inside,
                                    );
                                    if resp.hovered() {
                                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                                    }
                                    if resp.clicked() {
                                        gl.source = Some(info.source.clone());
                                    }
                                    let cap = match &info.source {
                                        capture::Source::Monitor(n) if info.title.is_empty() => format!("Tela {}", n + 1),
                                        _ => info.title.clone(),
                                    };
                                    ui.add_sized([156.0, 14.0], egui::Label::new(RichText::new(cap).size(12.0).color(if sel { TEXT } else { MUTED })).truncate());
                                });
                            }
                        });
                    });
                    ui.add_space(8.0);
                    if !place.is_empty() {
                        ui.label(caps("CANAL DE TRANSMISSÃO"));
                        ui.label(RichText::new(&place).size(13.0).color(TEXT2));
                        ui.add_space(6.0);
                    }
                    ui.label(caps("QUALIDADE DA TRANSMISSÃO"));
                    let src_h = gl.selected().map(|i| i.height).unwrap_or(0);
                    // a escolha anterior era maior que esta tela: usa a resolução da própria fonte
                    if matches!(gl.res, Some(v) if src_h > 0 && v > src_h) {
                        gl.res = None;
                    }
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        if ui.selectable_label(gl.preset == 1, "Fluidez").clicked() {
                            gl.preset = 1;
                            gl.res = Some(if src_h > 0 { src_h.min(1080) } else { 1080 });
                            *fps = 60;
                            gl.kbps = None;
                        }
                        if ui.selectable_label(gl.preset == 2, "Nitidez").clicked() {
                            gl.preset = 2;
                            gl.res = None;
                            *fps = 15;
                            gl.kbps = None;
                        }
                        if gl.preset == 0 {
                            ui.label(RichText::new("Personalizado").size(12.5).color(AMBER));
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(RichText::new("Resolução").size(12.5).color(TEXT2));
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        for (h, label) in [(Some(480u32), "480p"), (Some(720), "720p"), (Some(1080), "1080p"), (Some(1440), "1440p"), (None, "Fonte")] {
                            // não faz sentido ampliar além da tela
                            let too_big = matches!(h, Some(v) if src_h > 0 && v > src_h);
                            let r = ui.add_enabled(!too_big, egui::Button::selectable(gl.res == h, label));
                            if r.clicked() {
                                gl.res = h;
                                gl.preset = 0;
                                gl.kbps = None;
                            }
                        }
                    });
                    ui.add_space(4.0);
                    ui.label(RichText::new("Taxa de quadros").size(12.5).color(TEXT2));
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 6.0;
                        let rates: &[u32] = if gl.advanced { &[15, 30, 60, 90, 120, 144] } else { &[15, 30, 60] };
                        for f in rates {
                            if ui.add(egui::Button::selectable(*fps == *f, format!("{f} FPS"))).clicked() {
                                *fps = *f;
                                gl.preset = 0;
                                gl.kbps = None;
                            }
                        }
                    });
                    ui.add_space(6.0);
                    match gl.selected() {
                        Some(info) => {
                            let q = gl.quality(info, *fps);
                            ui.label(RichText::new(publisher::describe(info.width, info.height, &q)).monospace().size(11.5).color(MUTED));
                        }
                        None => {
                            ui.label(RichText::new("Escolha uma tela ou janela.").size(11.5).color(MUTED));
                        }
                    }
                    ui.add_space(6.0);
                    ui.add_enabled(false, egui::Checkbox::new(&mut false, "Compartilhar áudio")).on_disabled_hover_text("Em breve");
                    ui.add_space(4.0);
                    egui::CollapsingHeader::new("Avançado").default_open(gl.advanced).show(ui, |ui| {
                        gl.advanced = true;
                        let auto = gl.selected().map(|i| publisher::suggested_kbps(publisher::output_size(i.width, i.height, gl.res).1, *fps)).unwrap_or(8000);
                        let mut mbps = gl.kbps.unwrap_or(auto) as f32 / 1000.0;
                        ui.horizontal(|ui| {
                            if ui.add(egui::Slider::new(&mut mbps, 1.0..=40.0).suffix(" Mbps")).changed() {
                                gl.kbps = Some((mbps * 1000.0) as u32);
                                gl.preset = 0;
                            }
                            if gl.kbps.is_some() && small_button(ui, "Automático", FIELD) {
                                gl.kbps = None;
                            }
                        });
                    });
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        if small_button(ui, "Cancelar", FIELD) {
                            close = true;
                        }
                        if live && small_button(ui, "Parar de transmitir", RED) {
                            go_stop = true;
                            close = true;
                        }
                        let ready = cur.is_some() && gl.source.is_some();
                        ui.add_enabled_ui(ready, |ui| {
                            if small_button(ui, if live { "Aplicar" } else { "Transmitir" }, BLURPLE) {
                                go_publish = true;
                                close = true;
                            }
                        });
                    });
                });
                if go_stop {
                    acts.push(Act::StopPublish);
                }
                if go_publish {
                    if let Some(ch) = cur {
                        acts.push(Act::Publish(ch));
                    }
                }
            }
        }
        if close || dismissed {
            s.modal = Modal::None;
        }
    }

    /// Cartão de perfil: banner liso, avatar sobreposto, nome, presença, "membro desde" e ações.
    fn profile_popout(&mut self, ctx: &egui::Context, s: &mut Session, acts: &mut Vec<Act>) {
        let Some(pf) = s.profile.as_ref() else { return };
        let (uid, anchor, left, opened, more) = (pf.user, pf.anchor, pf.left, pf.opened, pf.more);
        let user = s.user_info(uid).unwrap_or(p::User { id: uid, name: pf.name.clone(), online: false, status: p::Status::Offline });
        let me = s.me.id;
        let is_me = uid == me;
        let is_owner = s.snap.servers.iter().any(|sv| sv.owner == uid && matches!(&s.view, View::Chan { server, .. } if *server == sv.id));
        let friend = s.snap.friends.iter().find(|f| f.user.id == uid).map(|f| f.status);
        let blocked = s.snap.blocked.iter().any(|b| b.id == uid);
        let since = (uid >> 22) + 1_767_225_600_000; // o id é um snowflake: carrega a data de criação da conta
        const W: f32 = 300.0;
        let screen = ctx.content_rect();
        let x = if left { anchor.left() - 16.0 - W } else { anchor.right() + 12.0 };
        let x = x.clamp(8.0, (screen.right() - W - 8.0).max(8.0));
        let y = anchor.top().clamp(8.0, (screen.bottom() - 340.0).max(8.0));
        let mut close = false;
        let mut toggle_more = false;
        let area = egui::Area::new(egui::Id::new("profile-popout"))
            .order(egui::Order::Foreground)
            .fixed_pos(egui::pos2(x, y))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(L3)
                    .stroke(egui::Stroke::new(1.0, LINE2))
                    .corner_radius(egui::CornerRadius::same(12))
                    .shadow(egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(120) })
                    .show(ui, |ui| {
                        ui.set_width(W);
                        ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
                        // banner liso (sem imagem) e avatar com anel de recorte
                        let (banner, _) = ui.allocate_exact_size(egui::vec2(W, 105.0), egui::Sense::hover());
                        ui.painter().rect_filled(banner, egui::CornerRadius { nw: 12, ne: 12, sw: 0, se: 0 }, banner_color(uid));
                        let c = egui::pos2(banner.left() + 10.0 + 40.0 + 6.0, banner.bottom() - 4.0);
                        let (bg, fg) = user_tint(uid);
                        let pt = ui.painter();
                        pt.circle_filled(c, 46.0, L3);
                        pt.circle_filled(c, 40.0, bg);
                        let ch = user.name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
                        pt.text(c, Align2::CENTER_CENTER, ch, egui::FontId::proportional(32.0), fg);
                        presence_dot(pt, c + egui::vec2(29.0, 29.0), 8.0, user.status, L3);
                        ui.add_space(44.0);
                        egui::Frame::new().inner_margin(egui::Margin { left: 16, right: 16, top: 0, bottom: 16 }).show(ui, |ui| {
                            ui.set_width(W - 32.0);
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&user.name).size(20.0).strong().color(TEXT));
                                if is_owner {
                                    ui.add_space(6.0);
                                    ui.label(RichText::new("dono").size(11.0).color(YELLOW));
                                }
                            });
                            ui.add_space(2.0);
                            ui.label(RichText::new(format!("@{}", user.name)).size(13.5).color(TEXT2));
                            ui.add_space(2.0);
                            ui.label(RichText::new(status_label(user.status)).size(13.0).color(MUTED));
                            ui.add_space(12.0);
                            egui::Frame::new().fill(L2).corner_radius(egui::CornerRadius::same(8)).inner_margin(egui::Margin::same(12)).show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(caps("NO TELACAST DESDE"));
                                ui.add_space(4.0);
                                ui.label(RichText::new(fmt_date(since)).size(14.0).color(TEXT));
                            });
                            ui.add_space(12.0);
                            if is_me {
                                if big_button(ui, "Minha conta", BLURPLE) {
                                    acts.push(Act::OpenAccount);
                                    close = true;
                                }
                            } else {
                                if big_button(ui, "Mensagem", BLURPLE) {
                                    acts.push(Act::Send(C2S::OpenDm { user_id: uid }));
                                    close = true;
                                }
                                ui.add_space(8.0);
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    match friend {
                                        None => {
                                            if small_button(ui, "Adicionar amigo", FIELD) {
                                                acts.push(Act::Send(C2S::FriendRequest { username: user.name.clone() }));
                                                acts.push(Act::Toast("Pedido de amizade enviado".into()));
                                            }
                                        }
                                        Some(p::FriendStatus::Friend) => {
                                            if small_button(ui, "Desfazer amizade", FIELD) {
                                                acts.push(Act::Send(C2S::FriendRemove { user_id: uid }));
                                            }
                                        }
                                        Some(p::FriendStatus::Incoming) => {
                                            if small_button(ui, "Aceitar pedido", FIELD) {
                                                acts.push(Act::Send(C2S::FriendAccept { user_id: uid }));
                                            }
                                        }
                                        Some(p::FriendStatus::Outgoing) => {
                                            if small_button(ui, "Cancelar pedido", FIELD) {
                                                acts.push(Act::Send(C2S::FriendRemove { user_id: uid }));
                                            }
                                        }
                                    }
                                    if small_button(ui, if more { "Menos" } else { "Mais" }, FIELD) {
                                        toggle_more = true;
                                    }
                                });
                                if more {
                                    ui.add_space(8.0);
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 8.0;
                                        if small_button(ui, "Copiar ID", FIELD) {
                                            ctx.copy_text(uid.to_string());
                                            acts.push(Act::Toast("ID copiado".into()));
                                        }
                                        if blocked {
                                            if small_button(ui, "Desbloquear", FIELD) {
                                                acts.push(Act::Send(C2S::Unblock { user_id: uid }));
                                                close = true;
                                            }
                                        } else if small_button(ui, "Bloquear", RED) {
                                            acts.push(Act::Modal(Modal::ConfirmBlock { user_id: uid, name: user.name.clone() }));
                                            close = true;
                                        }
                                    });
                                }
                            }
                        });
                    });
            });
        if toggle_more {
            if let Some(pf) = s.profile.as_mut() {
                pf.more = !pf.more;
            }
        }
        // fecha com Esc ou clique fora (o clique que abriu não conta)
        let outside = ctx.input(|i| i.pointer.any_pressed())
            && ctx.pointer_interact_pos().is_some_and(|p| !area.response.rect.contains(p))
            && ctx.cumulative_pass_nr() != opened;
        if close || outside || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            s.profile = None;
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
                .order(egui::Order::Tooltip)
                .anchor(Align2::CENTER_BOTTOM, [0.0, -24.0])
                .show(ctx, |ui| {
                    egui::Frame::new()
                        .fill(L3)
                        .stroke(egui::Stroke::new(1.0, LINE2))
                        .corner_radius(egui::CornerRadius::same(10))
                        .inner_margin(egui::Margin::same(12))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(egui::vec2(8.0, 14.0), egui::Sense::hover());
                                ui.painter().circle_filled(r.center(), 3.5, AMBER);
                                ui.label(msg);
                            });
                        });
                });
        }
    }
}
