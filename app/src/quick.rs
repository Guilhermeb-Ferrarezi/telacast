//! Seletor rápido (Ctrl+K): janela central com campo de busca e lista de destinos.
//! A lógica de pontuação fica em `telacast-qs`; aqui montamos as linhas e desenhamos.
use crate::{theme::*, Session};
use eframe::egui::{self, Align2, Key, Modifiers, Sense, Vec2};
use telacast_proto as p;
use telacast_qs as qs;

pub struct Qs {
    pub query: String,
    sel: usize,
    last_query: String,
    focus: bool,
}

impl Qs {
    pub fn new() -> Self {
        Self { query: String::new(), sel: usize::MAX, last_query: String::from("\u{0}"), focus: true }
    }
}

#[derive(Clone, Debug)]
pub enum Target {
    Server(i64),
    Channel { server: i64, channel: i64 },
    User(i64),
}

pub enum Out {
    Stay,
    Close,
    Go(Target),
}

enum Kind {
    Header,
    Server,
    Channel,
    User(p::Status),
}

struct Row {
    kind: Kind,
    target: Option<Target>,
    id: i64,
    label: String,
    hint: String,
    badge: u32,
    unread: bool,
}

fn header(text: &str) -> Row {
    Row { kind: Kind::Header, target: None, id: 0, label: text.into(), hint: String::new(), badge: 0, unread: false }
}

/// Quantas vezes o usuário foi a este destino (reforça a nota).
fn freq(s: &Session, kind: u8, id: i64) -> u32 {
    s.visits.get(&(kind, id)).copied().unwrap_or(0)
}

fn all_users(s: &Session) -> Vec<Row> {
    use std::collections::HashMap;
    let mut m: HashMap<i64, (String, p::Status, bool, bool)> = HashMap::new();
    for sv in &s.snap.servers {
        for u in &sv.members {
            if u.id != s.me.id {
                m.entry(u.id).or_insert((u.name.clone(), u.status, false, false));
            }
        }
    }
    for f in &s.snap.friends {
        let e = m.entry(f.user.id).or_insert((f.user.name.clone(), f.user.status, false, false));
        e.2 = f.status == p::FriendStatus::Friend;
    }
    let mut dm_channel: HashMap<i64, i64> = HashMap::new();
    for d in &s.snap.dms {
        let e = m.entry(d.user.id).or_insert((d.user.name.clone(), d.user.status, false, false));
        e.3 = true;
        dm_channel.insert(d.user.id, d.channel_id);
    }
    m.into_iter()
        .map(|(id, (name, st, friend, dm))| Row {
            kind: Kind::User(st),
            target: Some(Target::User(id)),
            id,
            hint: if friend { "Amigo".into() } else if dm { "Conversa".into() } else { String::new() },
            label: name,
            badge: dm_channel.get(&id).and_then(|c| s.mentions.get(c)).copied().unwrap_or(0),
            unread: dm_channel.get(&id).is_some_and(|c| s.unread.contains(c)),
        })
        .collect()
}

fn all_channels(s: &Session) -> Vec<Row> {
    s.snap
        .servers
        .iter()
        .flat_map(|sv| sv.channels.iter().filter(|c| c.kind == 0).map(move |c| (sv, c)))
        .map(|(sv, c)| Row {
            kind: Kind::Channel,
            target: Some(Target::Channel { server: sv.id, channel: c.id }),
            id: c.id,
            label: c.name.clone(),
            hint: sv.name.clone(),
            badge: s.mentions.get(&c.id).copied().unwrap_or(0),
            unread: s.unread.contains(&c.id),
        })
        .collect()
}

fn all_servers(s: &Session) -> Vec<Row> {
    s.snap
        .servers
        .iter()
        .map(|sv| Row {
            kind: Kind::Server,
            target: Some(Target::Server(sv.id)),
            id: sv.id,
            label: sv.name.clone(),
            hint: "Servidor".into(),
            badge: sv.channels.iter().map(|c| s.mentions.get(&c.id).copied().unwrap_or(0)).sum(),
            unread: sv.channels.iter().any(|c| s.unread.contains(&c.id)),
        })
        .collect()
}

fn kind_code(r: &Row) -> u8 {
    match r.kind {
        Kind::Server => 0,
        Kind::Channel => 1,
        Kind::User(_) => 2,
        Kind::Header => 9,
    }
}

/// Pontua e ordena `rows` para a consulta; `limit` por tipo.
fn scored(s: &Session, rows: Vec<Row>, q: &str, limit: usize) -> Vec<Row> {
    let fmax = s.visits.values().copied().max().unwrap_or(0);
    let mut v: Vec<(Row, f32, String)> = rows
        .into_iter()
        .filter_map(|r| {
            let f = if q.is_empty() { 1 } else { qs::field_score(&r.label, q) };
            if f == 0 {
                return None;
            }
            let friend = r.hint == "Amigo";
            let has_dm = r.hint == "Conversa" || friend && s.snap.dms.iter().any(|d| d.user.id == r.id);
            let b = qs::boost(freq(s, kind_code(&r), r.id), fmax, friend, has_dm);
            let score = qs::final_score(f, b);
            let label = r.label.clone();
            Some((r, score, label))
        })
        .collect();
    qs::rank(&mut v);
    v.into_iter().take(limit).map(|x| x.0).collect()
}

fn build(s: &Session, query: &str) -> Vec<Row> {
    let (mode, q) = qs::parse_prefix(query);
    match (mode, q.is_empty()) {
        (qs::Mode::All, true) => {
            // sem consulta: menções, não lidos e canais anteriores
            let cur = s.current_channel();
            let mut out = Vec::new();
            let mut seen = std::collections::HashSet::new();
            let mut section = |title: &str, rows: Vec<Row>, out: &mut Vec<Row>| {
                let rows: Vec<Row> = rows.into_iter().filter(|r| seen.insert((kind_code(r), r.id))).collect();
                if !rows.is_empty() {
                    out.push(header(title));
                    out.extend(rows);
                }
            };
            let mut chans = all_channels(s);
            chans.extend(all_users(s).into_iter().filter(|u| u.badge > 0 || u.unread));
            let ment: Vec<Row> = chans.iter().filter(|r| r.badge > 0).map(clone_row).collect();
            section("Menções", ment, &mut out);
            let unread: Vec<Row> = chans.iter().filter(|r| r.unread && Some(r.id) != cur).map(clone_row).collect();
            section("Canais não lidos", unread, &mut out);
            let have = out.len() > 0;
            let max = if have { 3 } else { 7 };
            let users = all_users(s);
            let recent: Vec<Row> = s
                .recent
                .iter()
                .filter(|c| Some(**c) != cur)
                .filter_map(|c| {
                    chans.iter().find(|r| matches!(r.kind, Kind::Channel) && r.id == *c).map(clone_row).or_else(|| {
                        s.snap.dms.iter().find(|d| d.channel_id == *c).and_then(|d| users.iter().find(|u| u.id == d.user.id)).map(clone_row)
                    })
                })
                .take(max)
                .collect();
            section("Canais anteriores", recent, &mut out);
            out
        }
        (qs::Mode::User, _) => with_header("Procurando usuários", scored(s, all_users(s), q, 100)),
        (qs::Mode::Text, _) => with_header("Procurando canais de texto", scored(s, all_channels(s), q, 100)),
        (qs::Mode::Server, _) => with_header("Procurando servidores", scored(s, all_servers(s), q, 100)),
        (qs::Mode::All, false) => {
            // tudo misturado: até 5 de cada tipo, ordenados juntos
            let mut rows = scored(s, all_users(s), q, 5);
            rows.extend(scored(s, all_channels(s), q, 5));
            rows.extend(scored(s, all_servers(s), q, 5));
            let fmax = s.visits.values().copied().max().unwrap_or(0);
            let mut v: Vec<(Row, f32, String)> = rows
                .into_iter()
                .map(|r| {
                    let f = qs::field_score(&r.label, q);
                    let friend = r.hint == "Amigo";
                    let b = qs::boost(freq(s, kind_code(&r), r.id), fmax, friend, r.hint == "Conversa");
                    let label = r.label.clone();
                    (r, qs::final_score(f, b), label)
                })
                .collect();
            qs::rank(&mut v);
            v.into_iter().map(|x| x.0).collect()
        }
    }
}

fn with_header(text: &str, mut rows: Vec<Row>) -> Vec<Row> {
    rows.insert(0, header(text));
    rows
}

fn clone_row(r: &Row) -> Row {
    Row {
        kind: match r.kind {
            Kind::Header => Kind::Header,
            Kind::Server => Kind::Server,
            Kind::Channel => Kind::Channel,
            Kind::User(st) => Kind::User(st),
        },
        target: r.target.clone(),
        id: r.id,
        label: r.label.clone(),
        hint: r.hint.clone(),
        badge: r.badge,
        unread: r.unread,
    }
}

pub fn show(ctx: &egui::Context, q: &mut Qs, s: &Session) -> Out {
    let mut out = Out::Stay;
    let rows = build(s, &q.query);
    let selectable: Vec<bool> = rows.iter().map(|r| r.target.is_some()).collect();
    // consulta mudou: seleção volta ao primeiro resultado
    if q.query != q.last_query {
        q.last_query = q.query.clone();
        q.sel = selectable.iter().position(|&x| x).unwrap_or(0);
    }
    if q.sel >= rows.len() || !selectable.get(q.sel).copied().unwrap_or(false) {
        q.sel = selectable.iter().position(|&x| x).unwrap_or(0);
    }

    // teclas consumidas ANTES do campo de texto enxergar o evento
    let (down, up, enter, esc, toggle) = ctx.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::ArrowDown) || i.consume_key(Modifiers::CTRL, Key::N),
            i.consume_key(Modifiers::NONE, Key::ArrowUp) || i.consume_key(Modifiers::CTRL, Key::P),
            i.consume_key(Modifiers::NONE, Key::Enter),
            i.consume_key(Modifiers::NONE, Key::Escape),
            false,
        )
    });
    let _ = toggle;
    let mut moved = false;
    if down {
        q.sel = qs::step(&selectable, q.sel, 1);
        moved = true;
    }
    if up {
        q.sel = qs::step(&selectable, q.sel, -1);
        moved = true;
    }
    if esc {
        if q.query.is_empty() {
            return Out::Close;
        }
        q.query.clear();
    }
    if enter {
        if let Some(t) = rows.get(q.sel).and_then(|r| r.target.clone()) {
            return Out::Go(t);
        }
    }

    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::new("qs-scrim")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let r = ui.allocate_rect(screen, Sense::click());
        ui.painter().rect_filled(screen, 0.0, egui::Color32::from_black_alpha(150));
        if r.clicked() {
            out = Out::Close;
        }
    });

    let pointer_moved = ctx.input(|i| i.pointer.delta() != Vec2::ZERO);
    let mut clicked: Option<Target> = None;
    egui::Area::new(egui::Id::new("qs-modal"))
        .order(egui::Order::Foreground)
        .anchor(Align2::CENTER_TOP, egui::vec2(0.0, (screen.height() * 0.12).max(24.0)))
        .show(ctx, |ui| {
            glass(12).inner_margin(egui::Margin::ZERO).show(ui, |ui| {
                ui.set_width(540.0);
                egui::Frame::new().inner_margin(egui::Margin::symmetric(18, 8)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let id = egui::Id::new("qs-input");
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut q.query)
                            .id(id)
                            .frame(egui::Frame::NONE)
                            .hint_text("Aonde você gostaria de ir?")
                            .font(egui::FontId::proportional(18.0))
                            .desired_width(f32::INFINITY)
                            .margin(egui::Margin::symmetric(0, 10)),
                    );
                    if q.focus {
                        q.focus = false;
                        r.request_focus();
                    }
                });
                rule_bleed(ui, 0.0);
                egui::ScrollArea::vertical().max_height(7.0 * 34.0 + 10.0).auto_shrink([false, true]).show(ui, |ui| {
                    ui.add_space(6.0);
                    if rows.is_empty() {
                        ui.add_space(14.0);
                        ui.vertical_centered(|ui| {
                            ui.label(egui::RichText::new("Não consegue encontrar o que você está procurando?").color(MUTED));
                        });
                        ui.add_space(14.0);
                    }
                    for (i, row) in rows.iter().enumerate() {
                        let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 34.0), if row.target.is_some() { Sense::click() } else { Sense::hover() });
                        if matches!(row.kind, Kind::Header) {
                            ui.painter().text(rect.left_center() + egui::vec2(18.0, 0.0), Align2::LEFT_CENTER, row.label.to_uppercase(), egui::FontId::proportional(11.5), MUTED);
                            continue;
                        }
                        if resp.hovered() && pointer_moved {
                            q.sel = i;
                        }
                        let selected = q.sel == i;
                        if selected {
                            ui.painter().rect_filled(rect.shrink2(egui::vec2(8.0, 0.0)), 8.0, L4);
                            if moved {
                                resp.scroll_to_me(None);
                            }
                        }
                        let surface = if selected { L4 } else { L3 };
                        let icon = egui::Rect::from_min_size(rect.left_center() + egui::vec2(18.0, -10.0), egui::vec2(20.0, 20.0));
                        match row.kind {
                            Kind::User(st) => {
                                ui.scope_builder(egui::UiBuilder::new().max_rect(icon), |ui| {
                                    avatar(ui, &row.label, row.id, 20.0, Some(st), surface);
                                });
                            }
                            Kind::Channel => {
                                ui.painter().text(icon.center(), Align2::CENTER_CENTER, "#", egui::FontId::proportional(18.0), MUTED);
                            }
                            Kind::Server => {
                                ui.painter().rect_filled(icon, 6.0, if selected { L3 } else { L4 });
                                let ini: String = row.label.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
                                ui.painter().text(icon.center(), Align2::CENTER_CENTER, ini, egui::FontId::proportional(11.0), TEXT2);
                            }
                            Kind::Header => {}
                        }
                        let strong = row.unread || row.badge > 0;
                        let g = ui.painter().layout_no_wrap(row.label.clone(), egui::FontId::proportional(14.5), if strong { TEXT } else { TEXT2 });
                        let tx = icon.right() + 10.0;
                        ui.painter().galley(egui::pos2(tx, rect.center().y - g.size().y / 2.0), g.clone(), TEXT);
                        if !row.hint.is_empty() {
                            ui.painter().text(egui::pos2(tx + g.size().x + 10.0, rect.center().y), Align2::LEFT_CENTER, &row.hint, egui::FontId::proportional(12.5), MUTED);
                        }
                        if row.badge > 0 {
                            badge_pill(ui.painter(), egui::pos2(rect.right() - 18.0, rect.center().y + 8.0), row.badge, surface);
                        }
                        if resp.clicked() {
                            clicked = row.target.clone();
                        }
                    }
                    ui.add_space(4.0);
                });
                rule_bleed(ui, 0.0);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(18.0);
                    ui.label(egui::RichText::new("Dica: comece com").size(12.0).color(MUTED));
                    for (k, t) in [("@", "pessoas"), ("#", "canais"), ("*", "servidores")] {
                        let (r, _) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), Sense::hover());
                        ui.painter().rect_filled(r, 5.0, L4);
                        ui.painter().text(r.center(), Align2::CENTER_CENTER, k, egui::FontId::proportional(12.0), TEXT);
                        ui.label(egui::RichText::new(t).size(12.0).color(MUTED));
                    }
                });
                ui.add_space(8.0);
            });
        });
    if let Some(t) = clicked {
        return Out::Go(t);
    }
    out
}
