//! Visual "Grafite": cinzas em camadas, um único acento âmbar, títulos em serifa, só escuro.
//! Sem gradientes, sem brilhos, sem fundo animado. O que se move é curto e serve para dar retorno
//! ao toque (hover, seleção, entrada de mensagem).
use eframe::egui::{
    self, vec2, Align2, Color32, CursorIcon, FontFamily, FontId, Rect, Response, RichText, Sense,
    Stroke, StrokeKind, TextStyle, Ui,
};

// ---- camadas (só tom)
pub const L0: Color32 = Color32::from_rgb(0x0a, 0x0a, 0x0b); // barra de servidores
pub const L1: Color32 = Color32::from_rgb(0x10, 0x10, 0x12); // canais e membros
pub const L2: Color32 = Color32::from_rgb(0x15, 0x15, 0x17); // área principal
pub const L3: Color32 = Color32::from_rgb(0x1c, 0x1c, 0x1f); // campos, cartões
pub const L4: Color32 = Color32::from_rgb(0x25, 0x25, 0x28); // hover
pub const LINE: Color32 = Color32::from_rgb(0x24, 0x24, 0x27);
pub const LINE2: Color32 = Color32::from_rgb(0x30, 0x30, 0x34);

pub const TEXT: Color32 = Color32::from_rgb(0xec, 0xeb, 0xe7);
pub const TEXT2: Color32 = Color32::from_rgb(0xb7, 0xb5, 0xae);
pub const MUTED: Color32 = Color32::from_rgb(0x8a, 0x88, 0x82);

pub const AMBER: Color32 = Color32::from_rgb(0xe9, 0xb3, 0x5e); // o único acento
pub const AMBER_HOT: Color32 = Color32::from_rgb(0xf0, 0xc0, 0x70);
pub const AMBER_INK: Color32 = Color32::from_rgb(0x1a, 0x12, 0x04);
pub const OK: Color32 = Color32::from_rgb(0x74, 0xb9, 0x8f); // só presença online
pub const RED: Color32 = Color32::from_rgb(0xd9, 0x80, 0x6f); // só erro e ações destrutivas

// Nomes usados pelo resto do app.
pub const BG: Color32 = L2;
pub const SIDE: Color32 = L1;
pub const RAIL: Color32 = L0;
pub const CARD: Color32 = L3;
pub const FIELD: Color32 = L3; // botão secundário
pub const BLURPLE: Color32 = AMBER; // botão primário
pub const GREEN: Color32 = Color32::from_rgb(0x74, 0xb9, 0x8f); // "ação positiva": também vira primário
pub const YELLOW: Color32 = AMBER;

pub fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t.clamp(0.0, 1.0)).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

pub fn serif_family() -> FontFamily {
    FontFamily::Name("serif".into())
}

/// Título em serifa (Newsreader).
pub fn serif(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text).font(FontId::new(size, serif_family())).color(TEXT)
}

/// Rótulo de seção: pequeno, em caixa alta, apagado.
pub fn caps(text: &str) -> RichText {
    RichText::new(text.to_uppercase()).size(11.0).color(MUTED)
}

pub fn apply_theme(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let load = |fonts: &mut egui::FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.into(), std::sync::Arc::new(egui::FontData::from_static(bytes)));
    };
    load(&mut fonts, "Inter", include_bytes!("../assets/fonts/Inter.ttf"));
    load(&mut fonts, "Newsreader", include_bytes!("../assets/fonts/Newsreader.ttf"));
    load(&mut fonts, "JetBrainsMono", include_bytes!("../assets/fonts/JetBrainsMono.ttf"));
    if let Some(f) = fonts.families.get_mut(&FontFamily::Proportional) {
        f.insert(0, "Inter".into());
    }
    if let Some(f) = fonts.families.get_mut(&FontFamily::Monospace) {
        f.insert(0, "JetBrainsMono".into());
    }
    let mut serif = vec!["Newsreader".to_string()];
    serif.extend(fonts.families[&FontFamily::Proportional].clone());
    fonts.families.insert(serif_family(), serif);
    ctx.set_fonts(fonts);

    let mut v = egui::Visuals::dark();
    v.panel_fill = L2;
    v.window_fill = L3;
    v.window_stroke = Stroke::new(1.0, LINE2);
    v.window_corner_radius = egui::CornerRadius::same(12);
    v.window_shadow = egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(120) };
    v.popup_shadow = v.window_shadow;
    v.menu_corner_radius = egui::CornerRadius::same(10);
    v.extreme_bg_color = L3; // fundo dos campos de texto
    v.faint_bg_color = L3;
    v.hyperlink_color = AMBER;
    v.selection.bg_fill = alpha(AMBER, 0.28);
    v.selection.stroke = Stroke::new(1.0, AMBER);
    v.text_cursor.stroke = Stroke::new(2.0, AMBER);
    let r = egui::CornerRadius::same(9);
    v.widgets.noninteractive.bg_fill = L2;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT2);
    v.widgets.inactive.bg_fill = L3;
    v.widgets.inactive.weak_bg_fill = L3;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, LINE2);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT2);
    v.widgets.hovered.bg_fill = L4;
    v.widgets.hovered.weak_bg_fill = L4;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, LINE2);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.active.bg_fill = L4;
    v.widgets.active.weak_bg_fill = L4;
    v.widgets.active.bg_stroke = Stroke::new(1.0, AMBER);
    v.widgets.active.fg_stroke = Stroke::new(1.0, TEXT);
    v.widgets.open = v.widgets.hovered;
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = r;
    }
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = vec2(8.0, 6.0);
        s.spacing.button_padding = vec2(12.0, 6.0);
        s.interaction.tooltip_delay = 0.1;
        s.interaction.selectable_labels = false; // texto de interface não seleciona nem mostra cursor de texto
        s.text_styles.insert(TextStyle::Body, FontId::new(14.5, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Button, FontId::new(13.5, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Small, FontId::new(12.0, FontFamily::Proportional));
        s.text_styles.insert(TextStyle::Monospace, FontId::new(13.5, FontFamily::Monospace));
        s.text_styles.insert(TextStyle::Heading, FontId::new(24.0, serif_family()));
    });
}

// ---- molduras

/// Folha para janelas e cartões: camada 3, filete de 1 px, sem sombra.
pub fn glass(radius: u8) -> egui::Frame {
    egui::Frame::new()
        .fill(L3)
        .stroke(Stroke::new(1.0, LINE2))
        .corner_radius(egui::CornerRadius::same(radius))
}

/// Filete vertical no lado de dentro de um painel lateral.
pub fn edge(ui: &Ui, panel: Rect, right_side: bool) {
    let x = if right_side { panel.right() - 0.5 } else { panel.left() + 0.5 };
    ui.painter().vline(x, panel.y_range(), Stroke::new(1.0, LINE));
}

/// Filete horizontal que sangra `bleed` px para os dois lados (atravessa a margem do painel).
pub fn rule_bleed(ui: &mut Ui, bleed: f32) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r.expand2(vec2(bleed, 0.0)), 0.0, LINE);
}

// ---- widgets

fn hand(ui: &Ui, resp: &Response) {
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
}

fn anim(ui: &Ui, id: egui::Id, on: bool, secs: f32) -> f32 {
    ui.ctx().animate_bool_with_time(id, on, secs)
}

/// Avatar redondo, cor suave por usuário, inicial no centro. `surface` é a cor do fundo atrás dele
/// (serve de contorno para a bolinha de presença).
pub fn avatar(ui: &mut Ui, name: &str, id: i64, size: f32, status: Option<telacast_proto::Status>, surface: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let p = ui.painter();
    let c = rect.center();
    let (bg, fg) = user_tint(id);
    p.circle_filled(c, size / 2.0, bg);
    let ch = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
    p.text(c, Align2::CENTER_CENTER, ch, FontId::proportional(size * 0.4), fg);
    if let Some(st) = status {
        presence_dot(p, rect.right_bottom() - vec2(size * 0.12, size * 0.12), size * 0.11, st, surface);
    }
    resp
}

/// Bolinha de presença: online cheia, ausente em meia-lua, não perturbe com barra, invisível/offline vazada.
/// `surface` é a cor do fundo, usada para o contorno e para os recortes.
pub fn presence_dot(p: &egui::Painter, c: egui::Pos2, r: f32, st: telacast_proto::Status, surface: Color32) {
    use telacast_proto::Status;
    p.circle_filled(c, r * 1.55, surface);
    match st {
        Status::Online => {
            p.circle_filled(c, r, OK);
        }
        Status::Idle => {
            p.circle_filled(c, r, AMBER);
            p.circle_filled(c + vec2(-r * 0.45, -r * 0.45), r * 0.62, surface);
        }
        Status::Dnd => {
            p.circle_filled(c, r, RED);
            p.rect_filled(Rect::from_center_size(c, vec2(r * 1.2, r * 0.5)), 0.0, surface);
        }
        Status::Invisible | Status::Offline => {
            p.circle_filled(c, r, MUTED);
            p.circle_filled(c, r * 0.55, surface);
        }
    }
}

pub fn status_label(st: telacast_proto::Status) -> &'static str {
    use telacast_proto::Status;
    match st {
        Status::Online => "Online",
        Status::Idle => "Ausente",
        Status::Dnd => "Não perturbe",
        Status::Invisible => "Invisível",
        Status::Offline => "Offline",
    }
}

fn user_tint(id: i64) -> (Color32, Color32) {
    const P: [(Color32, Color32); 5] = [
        (Color32::from_rgb(0x3b, 0x35, 0x48), Color32::from_rgb(0xcf, 0xc6, 0xe6)),
        (Color32::from_rgb(0x2f, 0x45, 0x40), Color32::from_rgb(0xbf, 0xe0, 0xd6)),
        (Color32::from_rgb(0x4a, 0x3a, 0x36), Color32::from_rgb(0xe8, 0xcb, 0xbf)),
        (Color32::from_rgb(0x33, 0x40, 0x4f), Color32::from_rgb(0xc3, 0xd4, 0xe8)),
        (Color32::from_rgb(0x4b, 0x45, 0x30), Color32::from_rgb(0xea, 0xd9, 0xa4)),
    ];
    P[id.unsigned_abs() as usize % P.len()]
}

pub fn card(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) {
    glass(12).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if !title.is_empty() {
            ui.label(caps(title));
        }
        body(ui);
    });
}

/// Bolinha de estado. Âmbar significa "ao vivo" e respira devagar (opacidade, sem tamanho).
pub fn dot(ui: &mut Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
    let mut c = color;
    if color == AMBER {
        let t = ui.input(|i| i.time) as f32;
        let k = 0.5 + 0.5 * (t * std::f32::consts::TAU / 2.4).cos();
        c = alpha(AMBER, 0.3 + 0.7 * k);
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
    }
    ui.painter().circle_filled(r.center(), 3.5, c);
}

pub fn status_color(s: &str) -> Color32 {
    if s.starts_with("erro") || s.contains("indisponível") {
        RED
    } else if s.starts_with("transmitindo") || s.starts_with("assistindo") {
        AMBER
    } else {
        MUTED
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Primary,
    Danger,
    Quiet,
}

fn kind_of(fill: Color32) -> Kind {
    if fill == BLURPLE || fill == GREEN {
        Kind::Primary
    } else if fill == RED {
        Kind::Danger
    } else {
        Kind::Quiet
    }
}

fn paint_button(ui: &Ui, rect: Rect, kind: Kind, hot: f32) -> Color32 {
    let p = ui.painter();
    let r = egui::CornerRadius::same(9);
    match kind {
        Kind::Primary => {
            p.rect_filled(rect, r, lerp(AMBER, AMBER_HOT, hot));
            AMBER_INK
        }
        Kind::Danger => {
            p.rect_filled(rect, r, alpha(RED, 0.12 * hot));
            p.rect_stroke(rect, r, Stroke::new(1.0, lerp(LINE2, RED, 0.35 + 0.65 * hot)), StrokeKind::Inside);
            RED
        }
        Kind::Quiet => {
            p.rect_filled(rect, r, lerp(L2, L3, hot).gamma_multiply(hot.max(0.0).min(1.0)));
            p.rect_stroke(rect, r, Stroke::new(1.0, LINE2), StrokeKind::Inside);
            lerp(TEXT2, TEXT, hot)
        }
    }
}

/// Botão de largura total.
pub fn big_button(ui: &mut Ui, label: &str, fill: Color32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
    let hot = anim(ui, resp.id, resp.hovered(), 0.12);
    let ink = paint_button(ui, rect, kind_of(fill), hot);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(14.0), ink);
    hand(ui, &resp);
    resp.clicked()
}

pub fn small_button(ui: &mut Ui, label: &str, fill: Color32) -> bool {
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(13.0), TEXT);
    let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 24.0, 30.0), Sense::click());
    let hot = anim(ui, resp.id, resp.hovered(), 0.12);
    let ink = paint_button(ui, rect, kind_of(fill), hot);
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(13.0), ink);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, ink);
    hand(ui, &resp);
    resp.clicked()
}

/// Item da barra de servidores: quadrado arredondado de 40 px com a sigla, e uma pílula
/// encostada na borda esquerda do painel (8 não lido, 20 hover, 40 selecionado).
/// Hover e selecionado pintam o fundo de âmbar. `add` desenha o botão "adicionar" (só contorno).
pub fn rail_item(ui: &mut Ui, label: &str, selected: bool, unread: bool, add: bool, badge: u32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(40.0, 40.0), Sense::click());
    let hov = anim(ui, resp.id.with("h"), resp.hovered() || selected, 0.15);
    let target = if selected { 40.0 } else if resp.hovered() { 20.0 } else if unread { 8.0 } else { 0.0 };
    let h = ui.ctx().animate_value_with_time(resp.id.with("pill"), target, 0.2);
    let p = ui.painter();
    let cr = egui::CornerRadius::same(12);
    let press = if resp.is_pointer_button_down_on() { 1.0 } else { 0.0 };
    let rect = rect.translate(vec2(0.0, press));
    if h > 0.5 {
        let bar = Rect::from_center_size(
            egui::pos2(rect.left() - 14.0, rect.center().y - press),
            vec2(4.0, h),
        );
        p.rect_filled(bar, egui::CornerRadius { nw: 0, sw: 0, ne: 4, se: 4 }, TEXT);
    }
    if add {
        p.rect_stroke(rect, cr, Stroke::new(1.0, lerp(LINE2, TEXT2, hov)), StrokeKind::Inside);
        p.text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(20.0), lerp(MUTED, TEXT, hov));
    } else {
        p.rect_filled(rect, cr, lerp(L3, AMBER, ease_out(hov)));
        let n = label.chars().count();
        let size = [18.0, 18.0, 16.0, 16.0, 14.0, 12.0, 10.0][n.clamp(1, 7) - 1];
        p.text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(size), lerp(TEXT2, AMBER_INK, ease_out(hov)));
    }
    if badge > 0 {
        badge_pill(ui.painter(), rect.right_bottom() - vec2(2.0, 2.0), badge, L0);
    }
    hand(ui, &resp);
    resp
}

/// Selo âmbar com a contagem (mínimo 16 px), ancorado pelo canto inferior direito; `ring` é o contorno.
pub fn badge_pill(p: &egui::Painter, corner: egui::Pos2, n: u32, ring: Color32) {
    let text = if n > 99 { "99+".to_string() } else { n.to_string() };
    let w = if n < 10 { 16.0 } else if n < 100 { 22.0 } else { 28.0 };
    let r = Rect::from_min_max(corner - vec2(w, 16.0), corner);
    p.rect_filled(r.expand(2.0), 10.0, ring);
    p.rect_filled(r, 8.0, AMBER);
    p.text(r.center(), Align2::CENTER_CENTER, text, FontId::proportional(11.0), AMBER_INK);
}

/// Linha "NOVAS MENSAGENS": filete âmbar com uma etiqueta na ponta direita.
pub fn new_messages_divider(ui: &mut Ui) -> Rect {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
    let p = ui.painter();
    p.hline(r.x_range(), r.center().y, Stroke::new(1.0, alpha(AMBER, 0.55)));
    let g = p.layout_no_wrap("NOVAS MENSAGENS".to_string(), FontId::proportional(10.5), AMBER_INK);
    let tag = Rect::from_min_size(egui::pos2(r.right() - g.size().x - 14.0, r.center().y - 8.0), vec2(g.size().x + 14.0, 16.0));
    p.rect_filled(tag, 8.0, AMBER);
    p.galley(tag.center() - g.size() / 2.0, g, AMBER_INK);
    r
}

/// Seta de categoria: aponta para baixo (aberta) ou para a direita (recolhida).
pub fn chevron(p: &egui::Painter, c: egui::Pos2, open: bool, color: Color32) {
    let (a, b, d) = if open {
        (c + vec2(-3.5, -1.8), c + vec2(0.0, 1.8), c + vec2(3.5, -1.8))
    } else {
        (c + vec2(-1.8, -3.5), c + vec2(1.8, 0.0), c + vec2(-1.8, 3.5))
    };
    let s = Stroke::new(1.5, color);
    p.line_segment([a, b], s);
    p.line_segment([b, d], s);
}

/// Linha clicável de lista (canal, DM, membro). Selecionada: camada 3. Hover: camada 2.
pub fn list_row(ui: &mut Ui, selected: bool, add: impl FnOnce(&mut Ui)) -> Response {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 34.0), Sense::click());
    let hov = anim(ui, resp.id.with("h"), resp.hovered() && !selected, 0.12);
    let p = ui.painter();
    let r = egui::CornerRadius::same(8);
    if selected {
        p.rect_filled(rect, r, L3);
    } else if hov > 0.0 {
        p.rect_filled(rect, r, lerp(L1, L2, hov));
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(rect.min + vec2(10.0, 0.0), rect.max - vec2(8.0, 0.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.spacing_mut().item_spacing.x = 10.0;
    add(&mut child);
    hand(ui, &resp);
    resp
}

// ---- datas

fn local(ts: i64) -> Option<chrono::DateTime<chrono::Local>> {
    use chrono::TimeZone;
    chrono::Local.timestamp_millis_opt(ts).single()
}

/// Hora do dia, "14:05".
pub fn fmt_clock(ts: i64) -> String {
    local(ts).map(|t| t.format("%H:%M").to_string()).unwrap_or_default()
}

/// Rótulo do dia para o divisor do chat: "hoje", "ontem" ou "07/11/2026".
pub fn fmt_day(ts: i64) -> String {
    let Some(t) = local(ts) else { return String::new() };
    let today = chrono::Local::now().date_naive();
    let d = t.date_naive();
    if d == today {
        "hoje".into()
    } else if today.pred_opt() == Some(d) {
        "ontem".into()
    } else {
        t.format("%d/%m/%Y").to_string()
    }
}

pub fn same_day(a: i64, b: i64) -> bool {
    match (local(a), local(b)) {
        (Some(x), Some(y)) => x.date_naive() == y.date_naive(),
        _ => true,
    }
}

/// Linha de divisor de dia: filete, rótulo em serifa itálica, filete.
pub fn day_divider(ui: &mut Ui, label: &str) {
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let w = ui.available_width();
        let g = ui.painter().layout_no_wrap(label.to_string(), FontId::new(14.0, serif_family()), MUTED);
        let side = ((w - g.size().x - 28.0) / 2.0).max(0.0);
        let (l, _) = ui.allocate_exact_size(vec2(side, 1.0), Sense::hover());
        ui.painter().rect_filled(Rect::from_center_size(l.center(), vec2(side, 1.0)), 0.0, LINE);
        ui.label(RichText::new(label).font(FontId::new(14.0, serif_family())).italics().color(MUTED));
        let (r, _) = ui.allocate_exact_size(vec2(side, 1.0), Sense::hover());
        ui.painter().rect_filled(Rect::from_center_size(r.center(), vec2(side, 1.0)), 0.0, LINE);
    });
    ui.add_space(4.0);
}
