//! Desenha o Markdown do chat (`telacast-md`) com o tema Grafite.
//!
//! Cada bloco de texto vira um `LayoutJob` mostrado por um `Label` selecionável; os trechos clicáveis
//! (spoiler, link) são achados pelo cursor do galley.
use crate::theme::*;
use eframe::egui::{self, epaint::text::VariationCoords, text::LayoutJob, text::TextFormat, Color32, FontFamily, FontId, Stroke};
use std::collections::HashSet;
use telacast_md::{Block, Inline};
use telacast_proto as p;

/// O que o chat precisa saber para desenhar uma mensagem.
pub struct Env<'a> {
    pub snap: &'a p::Snapshot,
    pub revealed: &'a HashSet<(i64, u32)>,
    pub msg: i64,
    /// Instante atual em segundos Unix (para o carimbo relativo `<t:..:R>`).
    pub now: i64,
}

/// Pedido que o desenho devolve ao app.
pub enum Click {
    Spoiler(u32),
    /// Clicou numa menção `@nome`: usuário e ponto do clique.
    User(i64, egui::Pos2),
}

pub struct Base {
    pub size: f32,
    pub color: Color32,
}

enum Hit {
    Spoiler(u32, bool),
    Link(String),
    User(i64),
}
struct Span {
    a: usize,
    b: usize,
    hit: Hit,
}

#[derive(Clone)]
struct St {
    w: f32,
    ital: bool,
    under: bool,
    strike: bool,
    mono: bool,
    color: Color32,
    bg: Color32,
    size: f32,
}

fn fmt(st: &St, lh: f32) -> TextFormat {
    TextFormat {
        font_id: FontId::new(if st.mono { st.size * 0.92 } else { st.size }, if st.mono { FontFamily::Monospace } else { FontFamily::Proportional }),
        color: st.color,
        background: st.bg,
        italics: st.ital,
        underline: if st.under { Stroke::new(1.0, st.color) } else { Stroke::NONE },
        strikethrough: if st.strike { Stroke::new(1.0, st.color) } else { Stroke::NONE },
        coords: VariationCoords::new([(b"wght", st.w)]),
        line_height: Some(lh),
        ..Default::default()
    }
}

pub fn user_name(snap: &p::Snapshot, id: i64) -> Option<String> {
    if snap.me.id == id {
        return Some(snap.me.name.clone());
    }
    snap.servers
        .iter()
        .flat_map(|s| s.members.iter())
        .chain(snap.friends.iter().map(|f| &f.user))
        .chain(snap.dms.iter().map(|d| &d.user))
        .find(|u| u.id == id)
        .map(|u| u.name.clone())
}

fn channel_name(snap: &p::Snapshot, id: i64) -> Option<String> {
    snap.servers.iter().flat_map(|s| s.channels.iter()).find(|c| c.id == id && c.kind == 0).map(|c| c.name.clone())
}

struct Cx<'a> {
    env: &'a Env<'a>,
    lh: f32,
    spans: Vec<Span>,
    chars: usize,
}

fn add(job: &mut LayoutJob, cx: &mut Cx, text: &str, st: &St) {
    job.append(text, 0.0, fmt(st, cx.lh));
    cx.chars += text.chars().count();
}

fn push(job: &mut LayoutJob, cx: &mut Cx, st: &St, nodes: &[Inline]) {
    for n in nodes {
        match n {
            Inline::Text(t) => add(job, cx, t, st),
            Inline::Strong(k) => push(job, cx, &St { w: 700.0, color: TEXT, ..st.clone() }, k),
            Inline::Em(k) => push(job, cx, &St { ital: true, ..st.clone() }, k),
            Inline::Under(k) => push(job, cx, &St { under: true, ..st.clone() }, k),
            Inline::Strike(k) => push(job, cx, &St { strike: true, ..st.clone() }, k),
            Inline::Code(t) => add(job, cx, t, &St { mono: true, color: TEXT, bg: L3, ..st.clone() }),
            Inline::Spoiler { id, kids } => {
                let open = cx.env.revealed.contains(&(cx.env.msg, *id));
                let a = cx.chars;
                let s = if open {
                    St { bg: alpha(MUTED, 0.16), ..st.clone() }
                } else {
                    St { color: LINE2, bg: LINE2, ..st.clone() }
                };
                push(job, cx, &s, kids);
                cx.spans.push(Span { a, b: cx.chars, hit: Hit::Spoiler(*id, open) });
            }
            Inline::Link { kids, url } => {
                let a = cx.chars;
                push(job, cx, &St { color: AMBER, under: true, ..st.clone() }, kids);
                cx.spans.push(Span { a, b: cx.chars, hit: Hit::Link(url.clone()) });
            }
            Inline::User(id) => {
                let name = user_name(cx.env.snap, *id).unwrap_or_else(|| "usuário-desconhecido".into());
                let a = cx.chars;
                add(job, cx, &format!("@{name}"), &St { w: 600.0, color: AMBER_HOT, bg: alpha(AMBER, 0.14), ..st.clone() });
                cx.spans.push(Span { a, b: cx.chars, hit: Hit::User(*id) });
            }
            Inline::Channel(id) => {
                let name = channel_name(cx.env.snap, *id).unwrap_or_else(|| "canal-desconhecido".into());
                add(job, cx, &format!("#{name}"), &St { w: 600.0, color: AMBER_HOT, bg: alpha(AMBER, 0.14), ..st.clone() });
            }
            Inline::Broadcast(b) => add(job, cx, b, &St { w: 600.0, color: AMBER_HOT, bg: alpha(AMBER, 0.14), ..st.clone() }),
            Inline::Time { unix, style } => {
                add(job, cx, &fmt_stamp(*unix, *style, cx.env.now), &St { color: TEXT, bg: L3, ..st.clone() });
            }
        }
    }
}

const MESES: [&str; 12] = ["janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro", "novembro", "dezembro"];
const DIAS: [&str; 7] = ["domingo", "segunda-feira", "terça-feira", "quarta-feira", "quinta-feira", "sexta-feira", "sábado"];

/// Carimbo `<t:unix:estilo>` em português do Brasil.
pub fn fmt_stamp(unix: i64, style: char, now: i64) -> String {
    use chrono::{Datelike, Local, TimeZone, Timelike};
    let Some(t) = Local.timestamp_opt(unix, 0).single() else { return format!("<t:{unix}>") };
    let longa = format!("{} de {} de {}", t.day(), MESES[t.month0() as usize], t.year());
    let hm = format!("{:02}:{:02}", t.hour(), t.minute());
    match style {
        't' => hm,
        'T' => format!("{hm}:{:02}", t.second()),
        'd' => format!("{:02}/{:02}/{}", t.day(), t.month(), t.year()),
        'D' => longa,
        'F' => format!("{}, {longa} {hm}", DIAS[t.weekday().num_days_from_sunday() as usize]),
        's' => format!("{:02}/{:02}/{} {hm}", t.day(), t.month(), t.year()),
        'S' => format!("{:02}/{:02}/{} {hm}:{:02}", t.day(), t.month(), t.year(), t.second()),
        'R' => relative(unix - now),
        _ => format!("{longa} {hm}"),
    }
}

fn relative(diff: i64) -> String {
    let future = diff > 0;
    let s = diff.unsigned_abs();
    let (n, un) = if s < 45 {
        return if future { "em alguns segundos".into() } else { "há alguns segundos".into() };
    } else if s < 90 {
        (1, "minuto")
    } else if s < 45 * 60 {
        (((s + 30) / 60) as i64, "minuto")
    } else if s < 90 * 60 {
        (1, "hora")
    } else if s < 22 * 3600 {
        (((s + 1800) / 3600) as i64, "hora")
    } else if s < 36 * 3600 {
        (1, "dia")
    } else if s < 26 * 86400 {
        (((s + 43200) / 86400) as i64, "dia")
    } else if s < 45 * 86400 {
        (1, "mês")
    } else if s < 320 * 86400 {
        (((s + 15 * 86400) / (30 * 86400)) as i64, "mês")
    } else {
        (((s + 182 * 86400) / (365 * 86400)).max(1) as i64, "ano")
    };
    let un = match (un, n) {
        ("mês", n) if n > 1 => "meses".to_string(),
        ("ano", n) if n > 1 => "anos".to_string(),
        (u, n) if n > 1 => format!("{u}s"),
        (u, _) => u.to_string(),
    };
    let n_txt = if n == 1 && matches!(un.as_str(), "hora") { "uma".to_string() } else if n == 1 && un == "minuto" { "um".to_string() } else { n.to_string() };
    if future { format!("em {n_txt} {un}") } else { format!("há {n_txt} {un}") }
}

fn base_state(base: &Base) -> St {
    St { w: 400.0, ital: false, under: false, strike: false, mono: false, color: base.color, bg: Color32::TRANSPARENT, size: base.size }
}

/// Mostra um bloco de texto e trata cliques em spoiler e link. Devolve a resposta do rótulo.
fn text_block(ui: &mut egui::Ui, mut job: LayoutJob, spans: Vec<Span>, clicks: &mut Vec<Click>) -> egui::Response {
    job.wrap.max_width = ui.available_width();
    let galley = ui.painter().layout_job(job.clone());
    let r = ui.add(egui::Label::new(job).selectable(true));
    let at = |pos: egui::Pos2| -> Option<&Span> {
        let idx = galley.cursor_from_pos(pos - r.rect.min).index.0;
        spans.iter().find(|s| idx >= s.a && idx < s.b)
    };
    if let Some(pos) = r.hover_pos() {
        if at(pos).is_some() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
    }
    if r.clicked() {
        if let Some(pos) = r.interact_pointer_pos() {
            match at(pos).map(|s| &s.hit) {
                Some(Hit::Spoiler(id, _)) => clicks.push(Click::Spoiler(*id)),
                Some(Hit::User(id)) => clicks.push(Click::User(*id, pos)),
                Some(Hit::Link(url)) => {
                    // link dentro de spoiler fechado não abre
                    let hidden = spans.iter().any(|s| matches!(s.hit, Hit::Spoiler(_, false)) && at(pos).is_some_and(|h| h.a >= s.a && h.b <= s.b));
                    if !hidden {
                        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                    }
                }
                None => {}
            }
        }
    }
    r
}

fn inline_job(env: &Env, base: &Base, st: &St, kids: &[Inline], suffix: Option<&str>) -> (LayoutJob, Vec<Span>) {
    let lh = (base.size * 1.42).round();
    let mut cx = Cx { env, lh, spans: Vec::new(), chars: 0 };
    let mut job = LayoutJob::default();
    push(&mut job, &mut cx, st, kids);
    if let Some(s) = suffix {
        add(&mut job, &mut cx, s, &St { size: 11.5, color: MUTED, w: 400.0, ital: false, under: false, strike: false, bg: Color32::TRANSPARENT, ..st.clone() });
    }
    (job, cx.spans)
}

/// Desenha a mensagem e devolve as respostas dos blocos de texto (para o menu de contexto) e os cliques.
pub fn show(ui: &mut egui::Ui, blocks: &[Block], env: &Env, base: &Base, edited: bool, clicks: &mut Vec<Click>) -> Vec<egui::Response> {
    let mut resp = Vec::new();
    let last = blocks.len().saturating_sub(1);
    for (i, b) in blocks.iter().enumerate() {
        let suffix = (edited && i == last).then_some(" (editado)");
        block(ui, b, env, base, suffix, clicks, &mut resp);
    }
    if edited && !matches!(blocks.last(), Some(Block::Paragraph(_) | Block::Heading(..) | Block::Subtext(_))) {
        ui.label(egui::RichText::new("(editado)").size(11.5).color(MUTED));
    }
    resp
}

fn block(ui: &mut egui::Ui, b: &Block, env: &Env, base: &Base, suffix: Option<&str>, clicks: &mut Vec<Click>, out: &mut Vec<egui::Response>) {
    let st0 = base_state(base);
    match b {
        Block::Paragraph(k) => {
            let (job, spans) = inline_job(env, base, &st0, k, suffix);
            out.push(text_block(ui, job, spans, clicks));
        }
        Block::Heading(level, k) => {
            let size = match level {
                1 => 24.0,
                2 => 20.0,
                _ => 17.5,
            };
            let hb = Base { size, color: TEXT };
            ui.add_space(4.0);
            let (job, spans) = inline_job(env, &hb, &St { w: 700.0, size, color: TEXT, ..st0 }, k, suffix);
            out.push(text_block(ui, job, spans, clicks));
        }
        Block::Subtext(k) => {
            let sb = Base { size: 12.5, color: MUTED };
            let (job, spans) = inline_job(env, &sb, &St { size: 12.5, color: MUTED, ..st0 }, k, suffix);
            out.push(text_block(ui, job, spans, clicks));
        }
        Block::List { start, items } => {
            for (n, it) in items.iter().enumerate() {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.add_space(6.0 + it.depth as f32 * 18.0);
                    let mark = match start {
                        Some(s) => format!("{}.", s + n as u64),
                        None => if it.depth % 2 == 0 { "•".into() } else { "◦".into() },
                    };
                    // marcador numa coluna fixa, alinhado à direita (1. e 10. ficam alinhados)
                    ui.allocate_ui_with_layout(
                        egui::vec2(22.0, (base.size * 1.42).round()),
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            ui.label(egui::RichText::new(mark).size(base.size).color(MUTED));
                        },
                    );
                    let sfx = if n + 1 == items.len() { suffix } else { None };
                    let (job, spans) = inline_job(env, base, &st0, &it.kids, sfx);
                    out.push(text_block(ui, job, spans, clicks));
                });
            }
        }
        Block::Quote(inner) => {
            let bar = ui.painter().add(egui::Shape::Noop);
            let r = ui.horizontal_top(|ui| {
                ui.add_space(14.0);
                ui.vertical(|ui| {
                    for (j, ib) in inner.iter().enumerate() {
                        let sfx = if j + 1 == inner.len() { suffix } else { None };
                        block(ui, ib, env, base, sfx, clicks, out);
                    }
                });
            });
            let rc = r.response.rect;
            ui.painter().set(bar, egui::Shape::rect_filled(egui::Rect::from_min_size(rc.min, egui::vec2(4.0, rc.height())), 2.0, LINE2));
        }
        Block::Code { lang: _, text } => {
            egui::Frame::new()
                .fill(L1)
                .stroke(Stroke::new(1.0, LINE))
                .corner_radius(egui::CornerRadius::same(6))
                .inner_margin(egui::Margin::symmetric(10, 8))
                .show(ui, |ui| {
                    ui.set_max_width(ui.available_width().min(720.0));
                    egui::ScrollArea::horizontal().id_salt(("code", env.msg, text.len())).show(ui, |ui| {
                        let r = ui.add(egui::Label::new(egui::RichText::new(text).monospace().size(13.5).color(TEXT)).extend().selectable(true));
                        out.push(r);
                    });
                });
        }
    }
}
