//! Notificações do sistema: a parte que decide (pura, sem rede, sem janela, sem D-Bus).
//!
//! O que entra aqui é só dado: o app monta um [`Incoming`] (ou chama [`judge_friend`]) e recebe de volta
//! um [`Notice`] pronto para mostrar ou o motivo de não mostrar ([`Why`]). Quem fala com o sistema
//! operacional é `notif_os.rs`. Separar assim deixa todas as regras testáveis sem tela.
//!
//! Regras (docs/ui-spec não cobre; seguem o Discord):
//! - nada com a janela em foco, desligado nas preferências, em "Não perturbe", de si mesmo, de bloqueados,
//!   de mensagens de sistema (entrou/fixou), de histórico ou de mensagem já vista;
//! - com "só menções e DMs" (padrão): DM, `@você`, `@everyone`/`@here` e resposta a uma mensagem sua;
//! - pedido de amizade recebido sempre notifica (se as notificações estiverem ligadas);
//! - anti-spam ([`Limiter`]): no máximo 1 por canal a cada 3 s (a última substitui as intermediárias) e 10 por minuto.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use telacast_md::{Block, Inline};

/// Máximo de caracteres do corpo (contando a reticência final).
pub const BODY_MAX: usize = 180;
/// Máximo de caracteres do título (nomes de canal e de servidor podem ser longos).
pub const TITLE_MAX: usize = 90;
/// Intervalo mínimo entre duas notificações do mesmo canal.
pub const PER_KEY: Duration = Duration::from_secs(3);
/// Teto de notificações por minuto, somando tudo.
pub const PER_MINUTE: usize = 10;

/// Preferências do usuário (guardadas no `storage` do eframe).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Prefs {
    /// Mostrar notificações do sistema.
    pub on: bool,
    /// Só DMs, menções e pedidos de amizade (desligado = toda mensagem de canal também notifica).
    pub only_dm_mentions: bool,
}

impl Default for Prefs {
    fn default() -> Self {
        Self { on: true, only_dm_mentions: true }
    }
}

/// Estado da janela e do usuário no momento do evento.
#[derive(Clone, Copy, Debug, Default)]
pub struct Env {
    /// A janela tem o foco do teclado.
    pub focused: bool,
    /// A janela está minimizada (nesse caso conta como "sem foco" mesmo que o sistema diga o contrário).
    pub minimized: bool,
    /// Meu status é "Não perturbe".
    pub dnd: bool,
    pub me: i64,
}

impl Env {
    /// O usuário está olhando para o app?
    pub fn watching(&self) -> bool {
        self.focused && !self.minimized
    }
}

/// Para onde o clique na notificação leva.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    /// Conversa privada (id do canal).
    Dm(i64),
    /// Canal de um servidor.
    Channel { server: i64, channel: i64 },
    /// Início, aba "Pendente".
    Pending,
}

/// Agrupa notificações para o anti-spam e para a substituição ("a última vence").
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Channel(i64),
    Friends,
}

/// Uma notificação pronta.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub key: Key,
    pub title: String,
    pub body: String,
    pub target: Target,
}

/// Por que não notificou (usado nos testes e no log de depuração).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Why {
    Disabled,
    Watching,
    Dnd,
    Own,
    Blocked,
    /// Mensagem do histórico (carregada, não chegada ao vivo) ou já vista.
    Stale,
    /// Mensagem de sistema (alguém entrou, fixou...).
    System,
    /// Canal comum sem menção, com "só menções e DMs" ligado.
    NotRelevant,
    Empty,
}

/// Uma mensagem que acabou de chegar.
pub struct Incoming<'a> {
    pub live: bool,
    pub channel_id: i64,
    /// `Some` em canal de servidor; `None` em DM.
    pub server_id: Option<i64>,
    pub author_id: i64,
    pub author: &'a str,
    pub channel_name: &'a str,
    pub server_name: &'a str,
    pub content: &'a str,
    /// `Message::kind`: 0 é mensagem comum.
    pub kind: u8,
    /// Nomes dos anexos.
    pub attachments: &'a [&'a str],
    /// É resposta a uma mensagem minha?
    pub reply_to_me: bool,
    pub author_blocked: bool,
}

/// Quem resolve ids em nomes (`<@id>` e `<#id>`).
pub trait Names {
    fn user(&self, id: i64) -> Option<String>;
    fn channel(&self, id: i64) -> Option<String>;
}

/// A mensagem cita `me` (`<@id>`, `<@!id>`) ou todo mundo (`@everyone`, `@here`)? Mesma regra do servidor.
pub fn mentions_me(content: &str, me: i64) -> bool {
    content.contains(&format!("<@{me}>")) || content.contains(&format!("<@!{me}>")) || content.contains("@everyone") || content.contains("@here")
}

/// Passa pelos filtros gerais (ligado, foco, não perturbe).
fn gate(prefs: Prefs, env: Env) -> Result<(), Why> {
    if !prefs.on {
        return Err(Why::Disabled);
    }
    if env.watching() {
        return Err(Why::Watching);
    }
    if env.dnd {
        return Err(Why::Dnd);
    }
    Ok(())
}

/// Decide se uma mensagem nova vira notificação e monta o texto.
pub fn judge_message(prefs: Prefs, env: Env, m: &Incoming, names: &dyn Names) -> Result<Notice, Why> {
    gate(prefs, env)?;
    if !m.live {
        return Err(Why::Stale);
    }
    if m.author_id == env.me {
        return Err(Why::Own);
    }
    if m.author_blocked {
        return Err(Why::Blocked);
    }
    if m.kind != 0 {
        return Err(Why::System);
    }
    let dm = m.server_id.is_none();
    let mention = mentions_me(m.content, env.me) || m.reply_to_me;
    if prefs.only_dm_mentions && !dm && !mention {
        return Err(Why::NotRelevant);
    }
    let body = body_text(m.content, m.attachments, names);
    if body.is_empty() {
        return Err(Why::Empty);
    }
    let title = if dm {
        truncate(m.author, TITLE_MAX)
    } else {
        truncate(&format!("{} em #{} · {}", m.author, m.channel_name, m.server_name), TITLE_MAX)
    };
    let target = match m.server_id {
        Some(server) => Target::Channel { server, channel: m.channel_id },
        None => Target::Dm(m.channel_id),
    };
    Ok(Notice { key: Key::Channel(m.channel_id), title, body, target })
}

/// Pedido de amizade recebido.
pub fn judge_friend(prefs: Prefs, env: Env, name: &str, blocked: bool) -> Result<Notice, Why> {
    gate(prefs, env)?;
    if blocked {
        return Err(Why::Blocked);
    }
    Ok(Notice {
        key: Key::Friends,
        title: "Pedido de amizade".into(),
        body: truncate(&format!("{name} quer ser seu amigo"), BODY_MAX),
        target: Target::Pending,
    })
}

// ------------------------------------------------------------------ texto

/// Corpo da notificação: texto sem Markdown, anexos como "📎 nome", no máximo [`BODY_MAX`] caracteres.
pub fn body_text(content: &str, attachments: &[&str], names: &dyn Names) -> String {
    let mut out = plain_text(content, names);
    for a in attachments {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str("📎 ");
        out.push_str(&collapse(a));
    }
    truncate(&out, BODY_MAX)
}

/// Texto da mensagem sem marcação: negrito/itálico/etc. somem, spoiler vira "spoiler oculto", `<@id>` vira
/// `@nome`, links ficam com o texto, quebras de linha viram espaço.
pub fn plain_text(content: &str, names: &dyn Names) -> String {
    let mut parts: Vec<String> = Vec::new();
    for b in telacast_md::parse(content) {
        block_text(&b, names, &mut parts);
    }
    collapse(&parts.join(" "))
}

fn block_text(b: &Block, names: &dyn Names, out: &mut Vec<String>) {
    match b {
        Block::Paragraph(k) | Block::Heading(_, k) | Block::Subtext(k) => {
            let mut s = String::new();
            inline_text(k, names, &mut s);
            out.push(s);
        }
        Block::Quote(inner) => {
            for b in inner {
                block_text(b, names, out);
            }
        }
        Block::List { items, .. } => {
            for it in items {
                let mut s = String::new();
                inline_text(&it.kids, names, &mut s);
                out.push(s);
            }
        }
        Block::Code { text, .. } => out.push(text.clone()),
    }
}

fn inline_text(kids: &[Inline], names: &dyn Names, out: &mut String) {
    for k in kids {
        match k {
            Inline::Text(t) | Inline::Code(t) => out.push_str(t),
            Inline::Strong(v) | Inline::Em(v) | Inline::Under(v) | Inline::Strike(v) => inline_text(v, names, out),
            Inline::Spoiler { .. } => out.push_str("spoiler oculto"),
            Inline::Link { kids, url } => {
                if kids.is_empty() {
                    out.push_str(url);
                } else {
                    inline_text(kids, names, out);
                }
            }
            Inline::User(id) => {
                out.push('@');
                out.push_str(&names.user(*id).unwrap_or_else(|| "usuário".into()));
            }
            Inline::Channel(id) => {
                out.push('#');
                out.push_str(&names.channel(*id).unwrap_or_else(|| "canal".into()));
            }
            Inline::Time { unix, .. } => {
                use chrono::TimeZone;
                if let chrono::LocalResult::Single(t) = chrono::Local.timestamp_opt(*unix, 0) {
                    out.push_str(&t.format("%d/%m %H:%M").to_string());
                }
            }
            Inline::Broadcast(s) => out.push_str(s),
        }
    }
}

/// Troca qualquer sequência de espaços e quebras por um espaço só e apara as pontas.
fn collapse(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Corta em `max` caracteres (não bytes: nunca parte um caractere UTF-8) e põe "…" no fim se cortou.
/// O resultado inteiro, com a reticência, tem no máximo `max` caracteres.
pub fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    while out.ends_with(' ') {
        out.pop();
    }
    out.push('…');
    out
}

/// Escapa `& < >`: servidores de notificação que entendem "body-markup" interpretam HTML no corpo, e
/// sem isso uma mensagem com `<b>` ou `<a href>` viraria formatação (ou link) na notificação.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))] // só o backend do Linux usa
pub fn escape_markup(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            c => o.push(c),
        }
    }
    o
}

// ------------------------------------------------------------------ contador no título

/// Título da janela: "Telacast", "(N) Telacast" (menções e DMs não lidas) ou "(•) Telacast" (só mensagens
/// comuns de servidor não lidas). Só mostra o contador com a janela fora de foco.
pub fn window_title(watching: bool, mentions: u32, unread_dms: u32, unread_other: bool) -> String {
    if watching {
        return "Telacast".into();
    }
    let n = mentions.saturating_add(unread_dms);
    if n > 0 {
        let shown = if n > 99 { "99+".to_string() } else { n.to_string() };
        format!("({shown}) Telacast")
    } else if unread_other {
        "(•) Telacast".into()
    } else {
        "Telacast".into()
    }
}

// ------------------------------------------------------------------ anti-spam

/// Limita a taxa: 1 por canal a cada [`PER_KEY`] (a última substitui as retidas) e [`PER_MINUTE`] por minuto.
#[derive(Default)]
pub struct Limiter {
    last: HashMap<Key, Instant>,
    held: HashMap<Key, Notice>,
    sent: VecDeque<Instant>,
    /// Maior id de mensagem já considerado por canal (reentrega após reconectar não notifica de novo).
    seen: HashMap<i64, i64>,
}

impl Limiter {
    /// Registra o id de uma mensagem do canal. `false` = já vista (ou mais velha que a última vista).
    pub fn is_new(&mut self, channel: i64, id: i64) -> bool {
        let e = self.seen.entry(channel).or_insert(0);
        if id <= *e {
            return false;
        }
        *e = id;
        true
    }

    fn minute_full(&mut self, now: Instant) -> bool {
        while self.sent.front().is_some_and(|t| now.duration_since(*t) >= Duration::from_secs(60)) {
            self.sent.pop_front();
        }
        self.sent.len() >= PER_MINUTE
    }

    fn mark(&mut self, now: Instant, key: Key) {
        self.last.insert(key, now);
        self.sent.push_back(now);
    }

    /// Oferece uma notificação. `Some` = mostre agora; `None` = ficou retida (volta em [`Limiter::due`]) ou
    /// foi descartada pelo teto por minuto.
    pub fn offer(&mut self, now: Instant, n: Notice) -> Option<Notice> {
        if let Some(t) = self.last.get(&n.key) {
            if now.duration_since(*t) < PER_KEY {
                self.held.insert(n.key, n); // a última vence
                return None;
            }
        }
        if self.minute_full(now) {
            return None;
        }
        self.mark(now, n.key);
        Some(n)
    }

    /// Retidas cujo intervalo já passou (mostre-as). Respeita o teto por minuto.
    pub fn due(&mut self, now: Instant) -> Vec<Notice> {
        let ready: Vec<Key> = self
            .held
            .keys()
            .filter(|k| self.last.get(k).is_none_or(|t| now.duration_since(*t) >= PER_KEY))
            .copied()
            .collect();
        let mut out = Vec::new();
        for k in ready {
            let Some(n) = self.held.remove(&k) else { continue };
            if self.minute_full(now) {
                continue;
            }
            self.mark(now, k);
            out.push(n);
        }
        out
    }

    /// Quando a próxima retida pode sair (para pedir o redesenho no momento certo).
    pub fn next_due(&self) -> Option<Instant> {
        self.held.keys().filter_map(|k| self.last.get(k).map(|t| *t + PER_KEY)).min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct N;
    impl Names for N {
        fn user(&self, id: i64) -> Option<String> {
            (id == 7).then(|| "beto".to_string())
        }
        fn channel(&self, id: i64) -> Option<String> {
            (id == 9).then(|| "geral".to_string())
        }
    }

    const ME: i64 = 1;
    fn away() -> Env {
        Env { focused: false, minimized: false, dnd: false, me: ME }
    }
    fn msg<'a>() -> Incoming<'a> {
        Incoming {
            live: true,
            channel_id: 100,
            server_id: Some(5),
            author_id: 2,
            author: "beto",
            channel_name: "geral",
            server_name: "Casa",
            content: "oi",
            kind: 0,
            attachments: &[],
            reply_to_me: false,
            author_blocked: false,
        }
    }
    fn dm<'a>() -> Incoming<'a> {
        Incoming { server_id: None, ..msg() }
    }
    fn ok(m: &Incoming) -> Notice {
        judge_message(Prefs::default(), away(), m, &N).expect("devia notificar")
    }
    fn no(m: &Incoming) -> Why {
        judge_message(Prefs::default(), away(), m, &N).expect_err("não devia notificar")
    }

    #[test]
    fn dm_notifica_com_o_nome_do_autor() {
        let n = ok(&dm());
        assert_eq!(n.title, "beto");
        assert_eq!(n.body, "oi");
        assert_eq!(n.target, Target::Dm(100));
        assert_eq!(n.key, Key::Channel(100));
    }

    #[test]
    fn mensagem_de_canal_comum_nao_notifica_por_padrao() {
        assert_eq!(no(&msg()), Why::NotRelevant);
    }

    #[test]
    fn mencao_notifica_com_autor_canal_e_servidor() {
        let m = Incoming { content: "oi <@1>, tudo bem?", ..msg() };
        let n = ok(&m);
        assert_eq!(n.title, "beto em #geral · Casa");
        assert_eq!(n.target, Target::Channel { server: 5, channel: 100 });
        let m = Incoming { content: "<@!1> vem", ..msg() };
        ok(&m);
        let m = Incoming { content: "<@12> vem", ..msg() };
        assert_eq!(no(&m), Why::NotRelevant, "<@12> não é <@1>");
    }

    #[test]
    fn everyone_here_e_resposta_contam_como_mencao() {
        ok(&Incoming { content: "@everyone reunião", ..msg() });
        ok(&Incoming { content: "@here quem está aí", ..msg() });
        ok(&Incoming { reply_to_me: true, ..msg() });
    }

    #[test]
    fn com_o_filtro_desligado_todo_canal_notifica() {
        let p = Prefs { on: true, only_dm_mentions: false };
        assert!(judge_message(p, away(), &msg(), &N).is_ok());
    }

    #[test]
    fn desligado_nas_preferencias() {
        let p = Prefs { on: false, only_dm_mentions: true };
        assert_eq!(judge_message(p, away(), &dm(), &N).unwrap_err(), Why::Disabled);
        assert_eq!(judge_friend(p, away(), "ana", false).unwrap_err(), Why::Disabled);
    }

    #[test]
    fn janela_em_foco_nao_notifica_e_minimizada_conta_como_fora() {
        let focused = Env { focused: true, ..away() };
        assert_eq!(judge_message(Prefs::default(), focused, &dm(), &N).unwrap_err(), Why::Watching);
        assert_eq!(judge_friend(Prefs::default(), focused, "ana", false).unwrap_err(), Why::Watching);
        let min = Env { focused: true, minimized: true, ..away() };
        assert!(judge_message(Prefs::default(), min, &dm(), &N).is_ok());
    }

    #[test]
    fn nao_perturbe_silencia_tudo() {
        let dnd = Env { dnd: true, ..away() };
        assert_eq!(judge_message(Prefs::default(), dnd, &dm(), &N).unwrap_err(), Why::Dnd);
        let m = Incoming { content: "@everyone", ..msg() };
        assert_eq!(judge_message(Prefs::default(), dnd, &m, &N).unwrap_err(), Why::Dnd);
        assert_eq!(judge_friend(Prefs::default(), dnd, "ana", false).unwrap_err(), Why::Dnd);
    }

    #[test]
    fn autor_proprio_e_bloqueado_nao_notificam() {
        assert_eq!(no(&Incoming { author_id: ME, ..dm() }), Why::Own);
        assert_eq!(no(&Incoming { author_blocked: true, ..dm() }), Why::Blocked);
        assert_eq!(no(&Incoming { author_blocked: true, content: "@everyone", ..msg() }), Why::Blocked);
        assert_eq!(judge_friend(Prefs::default(), away(), "ana", true).unwrap_err(), Why::Blocked);
    }

    #[test]
    fn historico_e_mensagem_de_sistema_nao_notificam() {
        assert_eq!(no(&Incoming { live: false, ..dm() }), Why::Stale);
        assert_eq!(no(&Incoming { kind: 2, ..msg() }), Why::System);
        assert_eq!(no(&Incoming { kind: 3, ..dm() }), Why::System);
    }

    #[test]
    fn mensagem_vazia_nao_notifica() {
        assert_eq!(no(&Incoming { content: "   ", ..dm() }), Why::Empty);
    }

    #[test]
    fn pedido_de_amizade() {
        let n = judge_friend(Prefs::default(), away(), "ana", false).unwrap();
        assert_eq!(n.title, "Pedido de amizade");
        assert_eq!(n.body, "ana quer ser seu amigo");
        assert_eq!(n.target, Target::Pending);
        assert_eq!(n.key, Key::Friends);
    }

    #[test]
    fn markdown_some_do_corpo() {
        let t = |s: &str| plain_text(s, &N);
        assert_eq!(t("**negrito** e *itálico* e __sublinhado__ e ~~tachado~~"), "negrito e itálico e sublinhado e tachado");
        assert_eq!(t("use `cargo run` agora"), "use cargo run agora");
        assert_eq!(t("# Título\ncorpo"), "Título corpo");
        assert_eq!(t("> citação\nresposta"), "citação resposta");
        assert_eq!(t("- um\n- dois"), "um dois");
        assert_eq!(t("```rust\nfn main() {}\n```"), "fn main() {}");
        assert_eq!(t("veja [o site](https://exemplo.com) agora"), "veja o site agora");
        assert_eq!(t("linha 1\n\n\nlinha   2"), "linha 1 linha 2");
    }

    #[test]
    fn spoiler_vira_texto_fixo_sem_vazar_o_conteudo() {
        let t = plain_text("o final é ||o mordomo||", &N);
        assert_eq!(t, "o final é spoiler oculto");
        assert!(!t.contains("mordomo"));
    }

    #[test]
    fn mencoes_viram_nomes() {
        assert_eq!(plain_text("oi <@7>!", &N), "oi @beto!");
        assert_eq!(plain_text("oi <@8>", &N), "oi @usuário");
        assert_eq!(plain_text("vá a <#9>", &N), "vá a #geral");
        assert_eq!(plain_text("@everyone atenção", &N), "@everyone atenção");
    }

    #[test]
    fn anexos_viram_clipe_com_nome() {
        assert_eq!(body_text("", &["foto.png"], &N), "📎 foto.png");
        assert_eq!(body_text("olha", &["a.png", "b.pdf"], &N), "olha 📎 a.png 📎 b.pdf");
        let n = judge_message(Prefs::default(), away(), &Incoming { content: "", attachments: &["x.zip"], ..dm() }, &N).unwrap();
        assert_eq!(n.body, "📎 x.zip");
    }

    #[test]
    fn truncamento_respeita_caracteres_e_utf8() {
        assert_eq!(truncate("abc", 5), "abc");
        assert_eq!(truncate("abcdef", 5), "abcd…");
        // cada "ã" tem 2 bytes e cada emoji 4: cortar por byte partiria o caractere
        let s = "ãõç😀".repeat(100);
        let t = truncate(&s, BODY_MAX);
        assert_eq!(t.chars().count(), BODY_MAX);
        assert!(t.ends_with('…'));
        let body = body_text(&"á".repeat(500), &[], &N);
        assert_eq!(body.chars().count(), BODY_MAX);
        // exatamente no limite não corta
        assert_eq!(truncate(&"x".repeat(BODY_MAX), BODY_MAX).chars().count(), BODY_MAX);
        assert!(!truncate(&"x".repeat(BODY_MAX), BODY_MAX).ends_with('…'));
        // o espaço antes do corte não fica pendurado antes da reticência
        assert_eq!(truncate("aaaa bbbb", 6), "aaaa…");
    }

    #[test]
    fn titulo_longo_tambem_e_cortado() {
        let m = Incoming { content: "@everyone", channel_name: &"c".repeat(200), ..msg() };
        let n = ok(&m);
        assert!(n.title.chars().count() <= TITLE_MAX);
    }

    #[test]
    fn escapa_marcacao() {
        assert_eq!(escape_markup("<b>oi</b> & <a href=\"x\">y</a>"), "&lt;b&gt;oi&lt;/b&gt; &amp; &lt;a href=\"x\"&gt;y&lt;/a&gt;");
        assert_eq!(escape_markup("normal"), "normal");
    }

    fn notice(ch: i64, body: &str) -> Notice {
        Notice { key: Key::Channel(ch), title: "t".into(), body: body.into(), target: Target::Dm(ch) }
    }

    #[test]
    fn anti_spam_um_por_canal_a_cada_3s_e_a_ultima_vence() {
        let mut l = Limiter::default();
        let t0 = Instant::now();
        assert_eq!(l.offer(t0, notice(1, "a")).unwrap().body, "a");
        // outro canal passa na hora
        assert!(l.offer(t0, notice(2, "x")).is_some());
        // mesmo canal dentro de 3 s fica retido; a mais nova substitui
        assert!(l.offer(t0 + Duration::from_secs(1), notice(1, "b")).is_none());
        assert!(l.offer(t0 + Duration::from_secs(2), notice(1, "c")).is_none());
        assert!(l.due(t0 + Duration::from_secs(2)).is_empty());
        assert_eq!(l.next_due(), Some(t0 + PER_KEY));
        let out = l.due(t0 + Duration::from_secs(3));
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].body, "c", "a última vence");
        assert!(l.due(t0 + Duration::from_secs(4)).is_empty(), "não repete");
        assert_eq!(l.next_due(), None);
        // passado o intervalo da retida, volta a passar direto
        assert!(l.offer(t0 + Duration::from_secs(7), notice(1, "d")).is_some());
    }

    #[test]
    fn anti_spam_dez_por_minuto() {
        let mut l = Limiter::default();
        let t0 = Instant::now();
        for i in 0..PER_MINUTE as i64 {
            assert!(l.offer(t0 + Duration::from_millis(i as u64 * 10), notice(i, "x")).is_some(), "{i}");
        }
        assert!(l.offer(t0 + Duration::from_secs(1), notice(99, "x")).is_none(), "a 11ª no mesmo minuto cai");
        // uma retida também respeita o teto
        assert!(l.offer(t0 + Duration::from_secs(2), notice(0, "r")).is_none());
        assert!(l.due(t0 + Duration::from_secs(4)).is_empty());
        // passado um minuto volta
        assert!(l.offer(t0 + Duration::from_secs(61), notice(99, "y")).is_some());
    }

    #[test]
    fn mensagem_ja_vista_nao_repete() {
        let mut l = Limiter::default();
        assert!(l.is_new(1, 10));
        assert!(!l.is_new(1, 10), "reentrega");
        assert!(!l.is_new(1, 9), "mais velha");
        assert!(l.is_new(1, 11));
        assert!(l.is_new(2, 5), "outro canal");
    }

    #[test]
    fn titulo_da_janela_com_contador() {
        assert_eq!(window_title(true, 5, 2, true), "Telacast", "em foco volta ao normal");
        assert_eq!(window_title(false, 0, 0, false), "Telacast");
        assert_eq!(window_title(false, 3, 0, false), "(3) Telacast");
        assert_eq!(window_title(false, 1, 2, false), "(3) Telacast", "menções + DMs");
        assert_eq!(window_title(false, 0, 0, true), "(•) Telacast");
        assert_eq!(window_title(false, 80, 40, false), "(99+) Telacast");
    }
}
