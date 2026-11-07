use super::*;

fn p(s: &str) -> Vec<Block> {
    parse(s)
}
fn para(s: &str) -> Vec<Inline> {
    match p(s).into_iter().next() {
        Some(Block::Paragraph(k)) => k,
        other => panic!("esperava parágrafo, veio {other:?}"),
    }
}
fn t(s: &str) -> Inline {
    Inline::Text(s.into())
}

#[test]
fn texto_simples_e_quebra_de_linha() {
    assert_eq!(para("olá mundo"), vec![t("olá mundo")]);
    assert_eq!(para("a\nb"), vec![t("a\nb")], "a quebra de linha é preservada dentro do parágrafo");
}

#[test]
fn negrito_italico_sublinhado_tachado() {
    assert_eq!(para("**oi**"), vec![Inline::Strong(vec![t("oi")])]);
    assert_eq!(para("*oi*"), vec![Inline::Em(vec![t("oi")])]);
    assert_eq!(para("_oi_"), vec![Inline::Em(vec![t("oi")])]);
    assert_eq!(para("__oi__"), vec![Inline::Under(vec![t("oi")])], "__ é sublinhado, não negrito");
    assert_eq!(para("~~oi~~"), vec![Inline::Strike(vec![t("oi")])]);
    assert_eq!(para("***oi***"), vec![Inline::Strong(vec![Inline::Em(vec![t("oi")])])]);
    assert_eq!(para("a **b *c* d** e"), vec![t("a "), Inline::Strong(vec![t("b "), Inline::Em(vec![t("c")]), t(" d")]), t(" e")]);
}

#[test]
fn delimitadores_soltos_viram_texto() {
    assert_eq!(para("2 * 3 * 4"), vec![t("2 * 3 * 4")], "asterisco com espaço não abre itálico");
    assert_eq!(para("snake_case_name"), vec![t("snake_case_name")], "_ dentro de palavra não é itálico");
    assert_eq!(para("**sem fechar"), vec![t("**sem fechar")]);
    assert_eq!(para("|| sem fechar"), vec![t("|| sem fechar")]);
}

#[test]
fn escape() {
    assert_eq!(para("\\*não\\*"), vec![t("*não*")]);
    assert_eq!(para("a\\nb"), vec![t("a\\nb")], "barra antes de letra não escapa");
}

#[test]
fn codigo_em_linha_nao_interpreta_dentro() {
    assert_eq!(para("`a **b** c`"), vec![Inline::Code("a **b** c".into())]);
    assert_eq!(para("``x ` y``"), vec![Inline::Code("x ` y".into())]);
    assert_eq!(para("`sem fechar"), vec![t("`sem fechar")]);
}

#[test]
fn spoiler_com_ids_diferentes() {
    let k = para("||um|| e ||**dois**||");
    assert_eq!(
        k,
        vec![
            Inline::Spoiler { id: 0, kids: vec![t("um")] },
            t(" e "),
            Inline::Spoiler { id: 1, kids: vec![Inline::Strong(vec![t("dois")])] },
        ]
    );
}

#[test]
fn links() {
    assert_eq!(
        para("veja https://exemplo.com/a?b=1."),
        vec![t("veja "), Inline::Link { kids: vec![t("https://exemplo.com/a?b=1")], url: "https://exemplo.com/a?b=1".into() }, t(".")]
    );
    assert_eq!(
        para("[site](https://exemplo.com)"),
        vec![Inline::Link { kids: vec![t("site")], url: "https://exemplo.com".into() }]
    );
    // esquema não permitido vira texto
    assert_eq!(para("[x](javascript:alert(1))"), vec![t("[x](javascript:alert(1))")]);
}

#[test]
fn mencoes_e_carimbos() {
    assert_eq!(para("oi <@123>!"), vec![t("oi "), Inline::User(123), t("!")]);
    assert_eq!(para("<@!7>"), vec![Inline::User(7)]);
    assert_eq!(para("em <#99>"), vec![t("em "), Inline::Channel(99)]);
    assert_eq!(para("<t:1700000000>"), vec![Inline::Time { unix: 1_700_000_000, style: 'f' }]);
    assert_eq!(para("<t:1700000000:R>"), vec![Inline::Time { unix: 1_700_000_000, style: 'R' }]);
    assert_eq!(para("<t:abc>"), vec![t("<t:abc>")], "carimbo inválido fica como texto");
    assert_eq!(para("@everyone venham"), vec![Inline::Broadcast("@everyone".into()), t(" venham")]);
    assert_eq!(para("mail@everyone.com"), vec![t("mail@everyone.com")]);
}

#[test]
fn titulos_subtexto() {
    assert_eq!(p("# Grande"), vec![Block::Heading(1, vec![t("Grande")])]);
    assert_eq!(p("### Médio **x**"), vec![Block::Heading(3, vec![t("Médio "), Inline::Strong(vec![t("x")])])]);
    assert_eq!(p("#### quatro"), vec![Block::Paragraph(vec![t("#### quatro")])], "só níveis 1 a 3");
    assert_eq!(p("#semespaço"), vec![Block::Paragraph(vec![t("#semespaço")])]);
    assert_eq!(p("-# pequeno"), vec![Block::Subtext(vec![t("pequeno")])]);
}

#[test]
fn citacoes() {
    assert_eq!(p("> um\n> dois"), vec![Block::Quote(vec![Block::Paragraph(vec![t("um\ndois")])])]);
    assert_eq!(p(">sem espaço"), vec![Block::Paragraph(vec![t(">sem espaço")])], "> exige espaço depois");
    let q = p("antes\n>>> tudo\nque vem\ndepois");
    assert_eq!(q.len(), 2);
    assert!(matches!(&q[1], Block::Quote(inner) if matches!(&inner[0], Block::Paragraph(k) if k == &vec![t("tudo\nque vem\ndepois")])));
}

#[test]
fn listas() {
    assert_eq!(
        p("- a\n- b"),
        vec![Block::List { start: None, items: vec![ListItem { depth: 0, kids: vec![t("a")] }, ListItem { depth: 0, kids: vec![t("b")] }] }]
    );
    assert_eq!(
        p("3. três\n4. quatro"),
        vec![Block::List { start: Some(3), items: vec![ListItem { depth: 0, kids: vec![t("três")] }, ListItem { depth: 0, kids: vec![t("quatro")] }] }]
    );
    let l = p("- a\n  - b");
    assert!(matches!(&l[0], Block::List { items, .. } if items[1].depth == 1));
    assert_eq!(p("-sem espaço"), vec![Block::Paragraph(vec![t("-sem espaço")])]);
}

#[test]
fn bloco_de_codigo() {
    assert_eq!(p("```rust\nfn main() {}\n```"), vec![Block::Code { lang: "rust".into(), text: "fn main() {}".into() }]);
    assert_eq!(p("```\nsem lang\n```"), vec![Block::Code { lang: "".into(), text: "sem lang".into() }]);
    // a primeira linha só é linguagem se vier seguida de quebra
    assert_eq!(p("```olá mundo```"), vec![Block::Code { lang: "".into(), text: "olá mundo".into() }]);
    // não interpreta markdown dentro
    assert_eq!(p("```\n**não**\n```"), vec![Block::Code { lang: "".into(), text: "**não**".into() }]);
    // sem fechar: vira texto
    assert_eq!(p("```aberto"), vec![Block::Paragraph(vec![t("```aberto")])]);
    // texto antes e depois
    let b = p("antes\n```\ncódigo\n```\ndepois");
    assert_eq!(b.len(), 3);
}

#[test]
fn profundidade_limitada_nao_trava() {
    let s = format!("{}x{}", "**".repeat(60), "**".repeat(60));
    let _ = parse(&s); // não pode estourar a pilha nem demorar
    let s2 = "[".repeat(5000);
    let _ = parse(&s2);
}

#[test]
fn utf8_nao_quebra() {
    assert_eq!(para("**ação** é 🚀 ótimo"), vec![Inline::Strong(vec![t("ação")]), t(" é 🚀 ótimo")]);
    assert_eq!(para("`código ç`"), vec![Inline::Code("código ç".into())]);
}
