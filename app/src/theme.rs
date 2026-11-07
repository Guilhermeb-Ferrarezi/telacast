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
pub fn avatar(ui: &mut Ui, name: &str, id: i64, size: f32, online: Option<bool>, surface: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let p = ui.painter();
    let c = rect.center();
    let (bg, fg) = user_tint(id);
    p.circle_filled(c, size / 2.0, bg);
    let ch = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
    p.text(c, Align2::CENTER_CENTER, ch, FontId::proportional(size * 0.4), fg);
    if let Some(on) = online {
        let d = rect.right_bottom() - vec2(size * 0.12, size * 0.12);
        p.circle_filled(d, size * 0.17, surface);
        p.circle_filled(d, size * 0.11, if on { OK } else { MUTED });
    }
    resp
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

/// Botão quadrado da barra de servidores com a sigla. Selecionado: claro, com cantos mais abertos.
/// Com `accent == GREEN` vira o botão "adicionar" (só contorno).
pub fn rail_button(ui: &mut Ui, label: &str, selected: bool, accent: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(42.0, 42.0), Sense::click());
    let hov = anim(ui, resp.id.with("h"), resp.hovered(), 0.12);
    let sel = anim(ui, resp.id.with("s"), selected, 0.18);
    let p = ui.painter();
    let radius = (12.0 + 2.0 * ease_out(sel)) as u8;
    let cr = egui::CornerRadius::same(radius);
    if accent == GREEN {
        p.rect_stroke(rect, cr, Stroke::new(1.0, lerp(LINE2, TEXT2, hov)), StrokeKind::Inside);
        p.text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(18.0), lerp(MUTED, TEXT, hov));
    } else {
        let fill = lerp(lerp(L3, L4, hov), TEXT, ease_out(sel));
        p.rect_filled(rect, cr, fill);
        let ink = lerp(lerp(TEXT2, TEXT, hov), L0, ease_out(sel));
        p.text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(13.0), ink);
    }
    hand(ui, &resp);
    resp
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
