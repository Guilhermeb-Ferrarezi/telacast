//! Visual "Glass / Aurora": fundo escuro com brilhos que se movem, painéis de vidro,
//! acentos em gradiente violeta → ciano e transições animadas.
use eframe::egui::{
    self, epaint::Mesh, pos2, vec2, Align2, Color32, CursorIcon, FontId, Painter, Pos2, Rect, Response,
    RichText, Sense, Shape, Stroke, StrokeKind, Ui,
};
use std::f32::consts::TAU;

// ---- paleta
pub const VOID: Color32 = Color32::from_rgb(0x08, 0x08, 0x0d);
pub const VIOLET: Color32 = Color32::from_rgb(0x8b, 0x5c, 0xf6);
pub const CYAN: Color32 = Color32::from_rgb(0x22, 0xd3, 0xee);
pub const MAGENTA: Color32 = Color32::from_rgb(0xc0, 0x26, 0xd3);

pub const BLURPLE: Color32 = VIOLET; // cor de acento principal (vira gradiente nos botões)
pub const GREEN: Color32 = Color32::from_rgb(0x34, 0xd3, 0x99);
pub const RED: Color32 = Color32::from_rgb(0xfb, 0x71, 0x85);
pub const YELLOW: Color32 = Color32::from_rgb(0xfb, 0xbf, 0x24);
pub const MUTED: Color32 = Color32::from_rgb(0x8b, 0x8b, 0xa0);
pub const TEXT: Color32 = Color32::from_rgb(0xea, 0xea, 0xf2);

// Superfícies translúcidas (deixam a aurora aparecer por baixo).
pub const BG: Color32 = Color32::TRANSPARENT;
pub const SIDE: Color32 = Color32::from_rgba_premultiplied(9, 9, 12, 12);
pub const RAIL: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 110);
pub const CARD: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 85);
pub const FIELD: Color32 = Color32::from_rgba_premultiplied(18, 18, 22, 22);
pub const HOVER: Color32 = Color32::from_rgba_premultiplied(32, 32, 38, 38);
pub const GLASS: Color32 = Color32::from_rgba_premultiplied(10, 10, 13, 13);
const DARK_OPAQUE: Color32 = Color32::from_rgb(0x12, 0x12, 0x1a);

pub fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t.clamp(0.0, 1.0)).round() as u8;
    Color32::from_rgba_premultiplied(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()), l(a.a(), b.a()))
}

pub fn ease_out(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

pub fn apply_theme(ctx: &egui::Context) {
    // Fonte Inter embutida (as de reserva do egui continuam para símbolos que faltarem).
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "Inter".into(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!("../assets/fonts/Inter.ttf"))),
    );
    if let Some(f) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        f.insert(0, "Inter".into());
    }
    ctx.set_fonts(fonts);

    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::TRANSPARENT;
    v.window_fill = DARK_OPAQUE;
    v.window_stroke = Stroke::new(1.0, Color32::from_white_alpha(26));
    v.window_corner_radius = egui::CornerRadius::same(16);
    v.window_shadow = egui::Shadow { offset: [0, 16], blur: 40, spread: 0, color: Color32::from_black_alpha(140) };
    v.popup_shadow = v.window_shadow;
    v.extreme_bg_color = FIELD;
    v.override_text_color = Some(TEXT);
    v.selection.bg_fill = alpha(VIOLET, 0.55);
    v.selection.stroke = Stroke::new(1.0, CYAN);
    let r = egui::CornerRadius::same(8);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
    ] {
        w.corner_radius = r;
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, Color32::from_white_alpha(14));
    v.widgets.inactive.bg_fill = FIELD;
    v.widgets.inactive.weak_bg_fill = FIELD;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_white_alpha(18));
    v.widgets.hovered.bg_fill = HOVER;
    v.widgets.hovered.weak_bg_fill = HOVER;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, alpha(VIOLET, 0.6));
    v.widgets.active.bg_fill = alpha(VIOLET, 0.85);
    v.widgets.active.weak_bg_fill = alpha(VIOLET, 0.85);
    v.widgets.active.bg_stroke = Stroke::new(1.0, CYAN);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
    });
}

// ---- primitivas de desenho

/// Retângulo com gradiente por canto (sem cantos arredondados).
pub fn grad_rect(p: &Painter, r: Rect, tl: Color32, tr: Color32, br: Color32, bl: Color32) {
    let mut m = Mesh::default();
    m.colored_vertex(r.left_top(), tl);
    m.colored_vertex(r.right_top(), tr);
    m.colored_vertex(r.right_bottom(), br);
    m.colored_vertex(r.left_bottom(), bl);
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    p.add(Shape::mesh(m));
}

/// Gradiente radial suave: cor `col` com opacidade `a` no centro, sumindo até `r`.
pub fn blob(p: &Painter, c: Pos2, r: f32, col: Color32, a: f32) {
    const RINGS: usize = 6;
    const N: usize = 40;
    let mut m = Mesh::default();
    m.colored_vertex(c, alpha(col, a));
    for k in 1..=RINGS {
        let t = k as f32 / RINGS as f32;
        let al = a * (1.0 - t).powi(2);
        for i in 0..N {
            let ang = i as f32 / N as f32 * TAU;
            m.colored_vertex(c + vec2(ang.cos(), ang.sin()) * (r * t), alpha(col, al));
        }
    }
    for i in 0..N {
        let (a1, a2) = (1 + i as u32, 1 + ((i + 1) % N) as u32);
        m.add_triangle(0, a1, a2);
    }
    for k in 1..RINGS {
        let base0 = 1 + ((k - 1) * N) as u32;
        let base1 = 1 + (k * N) as u32;
        for i in 0..N as u32 {
            let j = (i + 1) % N as u32;
            m.add_triangle(base0 + i, base0 + j, base1 + i);
            m.add_triangle(base0 + j, base1 + j, base1 + i);
        }
    }
    p.add(Shape::mesh(m));
}

/// Anel com gradiente violeta → ciano ao redor do ângulo, girando com `rot`.
pub fn ring(p: &Painter, c: Pos2, r: f32, thick: f32, rot: f32) {
    const N: usize = 48;
    let mut m = Mesh::default();
    for i in 0..N {
        let ang = i as f32 / N as f32 * TAU;
        let t = 0.5 + 0.5 * (ang + rot).sin();
        let col = lerp(VIOLET, CYAN, t);
        let dir = vec2(ang.cos(), ang.sin());
        m.colored_vertex(c + dir * r, col);
        m.colored_vertex(c + dir * (r + thick), col);
    }
    for i in 0..N as u32 {
        let j = (i + 1) % N as u32;
        let (a, b, cc, d) = (2 * i, 2 * i + 1, 2 * j, 2 * j + 1);
        m.add_triangle(a, b, cc);
        m.add_triangle(b, d, cc);
    }
    p.add(Shape::mesh(m));
}

/// Pílula (cantos totalmente redondos) com gradiente horizontal. Use cores opacas.
pub fn pill(p: &Painter, rect: Rect, c1: Color32, c2: Color32) {
    let r = rect.height() / 2.0;
    let l = pos2(rect.left() + r, rect.center().y);
    let rr = pos2(rect.right() - r, rect.center().y);
    p.circle_filled(l, r, c1);
    p.circle_filled(rr, r, c2);
    grad_rect(
        p,
        Rect::from_min_max(pos2(l.x, rect.top()), pos2(rr.x, rect.bottom())),
        c1,
        c2,
        c2,
        c1,
    );
}

/// Fundo animado: escuridão profunda com três brilhos que derivam devagar.
pub fn aurora(p: &Painter, rect: Rect, t: f32) {
    p.rect_filled(rect, 0.0, VOID);
    let (w, h) = (rect.width(), rect.height());
    let big = w.max(h);
    let drift = |ph: f32, sx: f32, sy: f32| vec2((t * 0.11 + ph).sin() * sx, (t * 0.09 + ph * 1.7).cos() * sy);
    blob(p, rect.min + vec2(w * 0.12, h * 0.05) + drift(0.0, w * 0.07, h * 0.08), big * 0.62, VIOLET, 0.30);
    blob(p, rect.min + vec2(w * 0.92, h * 0.95) + drift(2.1, w * 0.06, h * 0.07), big * 0.58, CYAN, 0.20);
    blob(p, rect.min + vec2(w * 0.60, h * 0.30) + drift(4.2, w * 0.08, h * 0.06), big * 0.38, MAGENTA, 0.12);
}

/// Moldura de vidro: fundo translúcido, borda de luz de 1 px e sombra.
pub fn glass(radius: u8) -> egui::Frame {
    egui::Frame::new()
        .fill(GLASS)
        .stroke(Stroke::new(1.0, Color32::from_white_alpha(22)))
        .corner_radius(egui::CornerRadius::same(radius))
        .shadow(egui::Shadow { offset: [0, 10], blur: 30, spread: 0, color: Color32::from_black_alpha(110) })
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

/// Avatar redondo com a inicial. Online ganha um anel em gradiente que gira devagar.
pub fn avatar(ui: &mut Ui, name: &str, id: i64, size: f32, online: Option<bool>, _bg: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let p = ui.painter();
    let c = rect.center();
    let (c1, c2) = user_gradient(id);
    p.circle_filled(c, size / 2.0, c1);
    // brilho no canto superior para dar volume
    blob(p, c - vec2(size * 0.18, size * 0.2), size * 0.5, Color32::WHITE, 0.16);
    let ch = name.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
    p.text(c, Align2::CENTER_CENTER, ch, FontId::proportional(size * 0.46), Color32::WHITE);
    let _ = c2;
    if online == Some(true) && size >= 26.0 {
        let t = ui.input(|i| i.time) as f32;
        ring(p, c, size / 2.0 + 1.5, 1.6, t * 0.6);
    }
    if let Some(on) = online {
        let d = rect.right_bottom() - vec2(size * 0.14, size * 0.14);
        p.circle_filled(d, size * 0.2, DARK_OPAQUE);
        p.circle_filled(d, size * 0.14, if on { GREEN } else { MUTED });
    }
    resp
}

fn user_gradient(id: i64) -> (Color32, Color32) {
    const P: [(Color32, Color32); 6] = [
        (Color32::from_rgb(0x7c, 0x5c, 0xff), Color32::from_rgb(0x4f, 0x8c, 0xff)),
        (Color32::from_rgb(0x10, 0xb9, 0x81), Color32::from_rgb(0x06, 0xb6, 0xd4)),
        (Color32::from_rgb(0xec, 0x48, 0x99), Color32::from_rgb(0xa8, 0x55, 0xf7)),
        (Color32::from_rgb(0xf5, 0x9e, 0x0b), Color32::from_rgb(0xef, 0x44, 0x44)),
        (Color32::from_rgb(0x38, 0xbd, 0xf8), Color32::from_rgb(0x63, 0x66, 0xf1)),
        (Color32::from_rgb(0xf4, 0x3f, 0x5e), Color32::from_rgb(0xfb, 0x92, 0x3c)),
    ];
    P[id.unsigned_abs() as usize % P.len()]
}

pub fn user_color(id: i64) -> Color32 {
    // cor do nome no chat: o primeiro tom do gradiente, um pouco clareado para legibilidade
    lerp(user_gradient(id).0, Color32::WHITE, 0.25)
}

pub fn card(ui: &mut Ui, title: &str, body: impl FnOnce(&mut Ui)) {
    glass(12).inner_margin(egui::Margin::same(14)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        if !title.is_empty() {
            ui.label(RichText::new(title.to_uppercase()).size(11.0).strong().color(MUTED));
        }
        body(ui);
    });
}

/// Bolinha de status; vermelha ("ao vivo") pulsa.
pub fn dot(ui: &mut Ui, color: Color32) {
    let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
    let p = ui.painter();
    if color == RED {
        let t = ui.input(|i| i.time) as f32;
        let k = 0.5 + 0.5 * (t * 3.0).sin();
        p.circle_filled(r.center(), 4.5 + 2.5 * k, alpha(RED, 0.30 * (1.0 - k)));
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
    }
    p.circle_filled(r.center(), 4.0, color);
}

pub fn status_color(s: &str) -> Color32 {
    if s.starts_with("erro") || s.contains("indisponível") {
        RED
    } else if s.starts_with("transmitindo") || s.starts_with("assistindo") {
        GREEN
    } else {
        YELLOW
    }
}

fn paint_button(ui: &Ui, rect: Rect, fill: Color32, hot: f32) {
    let p = ui.painter();
    if hot > 0.0 {
        let glow = if fill == BLURPLE { CYAN } else { fill };
        blob(p, rect.center(), rect.width().max(80.0) * 0.55, glow, 0.20 * hot);
    }
    if fill == BLURPLE {
        pill(p, rect, lerp(VIOLET, Color32::WHITE, 0.10 * hot), lerp(CYAN, Color32::WHITE, 0.10 * hot));
    } else {
        let base = if fill == FIELD { Color32::from_rgb(0x27, 0x27, 0x32) } else { fill };
        pill(p, rect, lerp(base, Color32::WHITE, 0.12 * hot), lerp(base, Color32::WHITE, 0.12 * hot));
    }
}

/// Botão de largura total, em pílula (gradiente quando a cor é o acento).
pub fn big_button(ui: &mut Ui, label: &str, fill: Color32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::click());
    let hot = anim(ui, resp.id, resp.hovered(), 0.15);
    paint_button(ui, rect, fill, hot);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(14.5), Color32::WHITE);
    hand(ui, &resp);
    resp.clicked()
}

pub fn small_button(ui: &mut Ui, label: &str, fill: Color32) -> bool {
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(13.0), Color32::WHITE);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + vec2(24.0, 12.0), Sense::click());
    let hot = anim(ui, resp.id, resp.hovered(), 0.15);
    paint_button(ui, rect, fill, hot);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, Color32::WHITE);
    hand(ui, &resp);
    resp.clicked()
}

/// Botão redondo da barra de servidores: vira "squircle" e acende ao selecionar/passar o mouse.
pub fn rail_button(ui: &mut Ui, label: &str, selected: bool, accent: Color32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(48.0, 48.0), Sense::click());
    let t = anim(ui, resp.id.with("a"), selected || resp.hovered(), 0.18);
    let sel = anim(ui, resp.id.with("s"), selected, 0.22);
    let p = ui.painter();
    let radius = (24.0 - 9.0 * ease_out(t)) as u8;
    if t > 0.0 {
        blob(p, rect.center(), 46.0, if accent == BLURPLE { VIOLET } else { accent }, 0.35 * t);
    }
    let base = Color32::from_rgb(0x1d, 0x1d, 0x27);
    let on = if accent == BLURPLE { lerp(VIOLET, CYAN, 0.35) } else { accent };
    p.rect_filled(rect, egui::CornerRadius::same(radius), lerp(base, on, ease_out(t)));
    p.rect_stroke(rect, egui::CornerRadius::same(radius), Stroke::new(1.0, Color32::from_white_alpha(20)), StrokeKind::Inside);
    p.text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(17.0), Color32::WHITE);
    // barra indicadora à esquerda
    let hbar = 8.0 + 28.0 * ease_out(sel.max(t * 0.0));
    if sel > 0.0 || t > 0.0 {
        let h = 8.0 + (hbar - 8.0).max(0.0) * 1.0 + 4.0 * t * (1.0 - sel);
        let bar = Rect::from_center_size(pos2(rect.left() - 11.0, rect.center().y), vec2(4.0, h));
        grad_rect(p, bar, VIOLET, VIOLET, CYAN, CYAN);
    }
    hand(ui, &resp);
    resp
}

/// Linha clicável de lista (canal, DM...), com brilho em gradiente ao selecionar/passar o mouse.
pub fn list_row(ui: &mut Ui, selected: bool, add: impl FnOnce(&mut Ui)) -> Response {
    let w = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(vec2(w, 36.0), Sense::click());
    let sel = anim(ui, resp.id.with("s"), selected, 0.2);
    let hov = anim(ui, resp.id.with("h"), resp.hovered() && !selected, 0.14);
    let p = ui.painter();
    let r = egui::CornerRadius::same(9);
    if hov > 0.0 {
        p.rect_filled(rect, r, Color32::from_white_alpha((14.0 * hov) as u8));
    }
    if sel > 0.0 {
        p.rect_filled(rect, r, alpha(VIOLET, 0.16 * sel));
        p.rect_stroke(rect, r, Stroke::new(1.0, alpha(VIOLET, 0.35 * sel)), StrokeKind::Inside);
        let bar = Rect::from_min_max(
            pos2(rect.left() + 1.0, rect.top() + 8.0),
            pos2(rect.left() + 4.0, rect.bottom() - 8.0),
        );
        grad_rect(p, bar, alpha(VIOLET, sel), alpha(VIOLET, sel), alpha(CYAN, sel), alpha(CYAN, sel));
    }
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(Rect::from_min_max(rect.min + vec2(12.0, 0.0), rect.max))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    add(&mut child);
    hand(ui, &resp);
    resp
}

pub fn fmt_time(ts: i64) -> String {
    use chrono::{Local, TimeZone};
    let Some(t) = Local.timestamp_millis_opt(ts).single() else {
        return String::new();
    };
    if t.date_naive() == Local::now().date_naive() {
        format!("hoje às {}", t.format("%H:%M"))
    } else {
        t.format("%d/%m/%Y %H:%M").to_string()
    }
}
