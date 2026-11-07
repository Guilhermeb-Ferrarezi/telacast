package app.telacast.md

import app.telacast.md.Bloco.Citacao
import app.telacast.md.Bloco.Codigo
import app.telacast.md.Bloco.Lista
import app.telacast.md.Bloco.Paragrafo
import app.telacast.md.Bloco.Subtexto
import app.telacast.md.Bloco.Titulo
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Mesmos casos de `md/src/tests.rs` (o parser do desktop): se o Rust mudar, o espelho precisa acompanhar. */
class MarkdownTest {
    private fun p(s: String) = Markdown.parse(s)
    private fun para(s: String): List<Inline> = (p(s).firstOrNull() as? Paragrafo)?.k ?: error("esperava parágrafo, veio ${p(s)}")
    private fun t(s: String) = Inline.Texto(s)

    @Test fun textoSimplesEQuebraDeLinha() {
        assertEquals(listOf(t("olá mundo")), para("olá mundo"))
        assertEquals("a quebra de linha é preservada dentro do parágrafo", listOf(t("a\nb")), para("a\nb"))
    }

    @Test fun negritoItalicoSublinhadoTachado() {
        assertEquals(listOf(Inline.Negrito(listOf(t("oi")))), para("**oi**"))
        assertEquals(listOf(Inline.Italico(listOf(t("oi")))), para("*oi*"))
        assertEquals(listOf(Inline.Italico(listOf(t("oi")))), para("_oi_"))
        assertEquals("__ é sublinhado, não negrito", listOf(Inline.Sublinhado(listOf(t("oi")))), para("__oi__"))
        assertEquals(listOf(Inline.Tachado(listOf(t("oi")))), para("~~oi~~"))
        assertEquals(listOf(Inline.Negrito(listOf(Inline.Italico(listOf(t("oi")))))), para("***oi***"))
        assertEquals(
            listOf(t("a "), Inline.Negrito(listOf(t("b "), Inline.Italico(listOf(t("c"))), t(" d"))), t(" e")),
            para("a **b *c* d** e"),
        )
    }

    @Test fun delimitadoresSoltosViramTexto() {
        assertEquals("asterisco com espaço não abre itálico", listOf(t("2 * 3 * 4")), para("2 * 3 * 4"))
        assertEquals("_ dentro de palavra não é itálico", listOf(t("snake_case_name")), para("snake_case_name"))
        assertEquals(listOf(t("**sem fechar")), para("**sem fechar"))
        assertEquals(listOf(t("|| sem fechar")), para("|| sem fechar"))
    }

    @Test fun escape() {
        assertEquals(listOf(t("*não*")), para("\\*não\\*"))
        assertEquals("barra antes de letra não escapa", listOf(t("a\\nb")), para("a\\nb"))
    }

    @Test fun codigoEmLinhaNaoInterpretaDentro() {
        assertEquals(listOf(Inline.Codigo("a **b** c")), para("`a **b** c`"))
        assertEquals(listOf(Inline.Codigo("x ` y")), para("``x ` y``"))
        assertEquals(listOf(t("`sem fechar")), para("`sem fechar"))
    }

    @Test fun spoilerComIdsDiferentes() {
        assertEquals(
            listOf(
                Inline.Spoiler(0, listOf(t("um"))), t(" e "),
                Inline.Spoiler(1, listOf(Inline.Negrito(listOf(t("dois"))))),
            ),
            para("||um|| e ||**dois**||"),
        )
    }

    @Test fun links() {
        assertEquals(
            listOf(t("veja "), Inline.Link(listOf(t("https://exemplo.com/a?b=1")), "https://exemplo.com/a?b=1"), t(".")),
            para("veja https://exemplo.com/a?b=1."),
        )
        assertEquals(listOf(Inline.Link(listOf(t("site")), "https://exemplo.com")), para("[site](https://exemplo.com)"))
        assertEquals("esquema não permitido vira texto", listOf(t("[x](javascript:alert(1))")), para("[x](javascript:alert(1))"))
    }

    @Test fun mencoesECarimbos() {
        assertEquals(listOf(t("oi "), Inline.Usuario(123), t("!")), para("oi <@123>!"))
        assertEquals(listOf(Inline.Usuario(7)), para("<@!7>"))
        assertEquals(listOf(t("em "), Inline.Canal(99)), para("em <#99>"))
        assertEquals(listOf(Inline.Tempo(1_700_000_000, 'f')), para("<t:1700000000>"))
        assertEquals(listOf(Inline.Tempo(1_700_000_000, 'R')), para("<t:1700000000:R>"))
        assertEquals("carimbo inválido fica como texto", listOf(t("<t:abc>")), para("<t:abc>"))
        assertEquals(listOf(Inline.Todos("@everyone"), t(" venham")), para("@everyone venham"))
        assertEquals(listOf(t("mail@everyone.com")), para("mail@everyone.com"))
    }

    @Test fun titulosESubtexto() {
        assertEquals(listOf(Titulo(1, listOf(t("Grande")))), p("# Grande"))
        assertEquals(listOf(Titulo(3, listOf(t("Médio "), Inline.Negrito(listOf(t("x")))))), p("### Médio **x**"))
        assertEquals("só níveis 1 a 3", listOf(Paragrafo(listOf(t("#### quatro")))), p("#### quatro"))
        assertEquals(listOf(Paragrafo(listOf(t("#semespaço")))), p("#semespaço"))
        assertEquals(listOf(Subtexto(listOf(t("pequeno")))), p("-# pequeno"))
    }

    @Test fun citacoes() {
        assertEquals(listOf(Citacao(listOf(Paragrafo(listOf(t("um\ndois")))))), p("> um\n> dois"))
        assertEquals("> exige espaço depois", listOf(Paragrafo(listOf(t(">sem espaço")))), p(">sem espaço"))
        val q = p("antes\n>>> tudo\nque vem\ndepois")
        assertEquals(2, q.size)
        val interno = (q[1] as Citacao).blocos
        assertEquals(listOf(Paragrafo(listOf(t("tudo\nque vem\ndepois")))), interno)
    }

    @Test fun listas() {
        assertEquals(
            listOf(Lista(null, listOf(ItemDeLista(0, listOf(t("a"))), ItemDeLista(0, listOf(t("b")))))),
            p("- a\n- b"),
        )
        assertEquals(
            listOf(Lista(3, listOf(ItemDeLista(0, listOf(t("três"))), ItemDeLista(0, listOf(t("quatro")))))),
            p("3. três\n4. quatro"),
        )
        val l = p("- a\n  - b")
        assertEquals(1, (l[0] as Lista).itens[1].profundidade)
        assertEquals(listOf(Paragrafo(listOf(t("-sem espaço")))), p("-sem espaço"))
    }

    @Test fun blocoDeCodigo() {
        assertEquals(listOf(Codigo("rust", "fn main() {}")), p("```rust\nfn main() {}\n```"))
        assertEquals(listOf(Codigo("", "sem lang")), p("```\nsem lang\n```"))
        assertEquals("a primeira linha só é linguagem se vier seguida de quebra", listOf(Codigo("", "olá mundo")), p("```olá mundo```"))
        assertEquals("não interpreta markdown dentro", listOf(Codigo("", "**não**")), p("```\n**não**\n```"))
        assertEquals("sem fechar: vira texto", listOf(Paragrafo(listOf(t("```aberto")))), p("```aberto"))
        assertEquals(3, p("antes\n```\ncódigo\n```\ndepois").size)
    }

    @Test fun profundidadeLimitadaNaoTrava() {
        Markdown.parse("**".repeat(60) + "x" + "**".repeat(60))
        Markdown.parse("[".repeat(5000))
    }

    @Test fun utf8NaoQuebra() {
        assertEquals(listOf(Inline.Negrito(listOf(t("ação"))), t(" é 🚀 ótimo")), para("**ação** é 🚀 ótimo"))
        assertEquals(listOf(Inline.Codigo("código ç")), para("`código ç`"))
    }

    @Test fun mensagemDeVerdadeComTudoJunto() {
        val b = p("# Título\nolá **mundo** com ||segredo|| e <@5>\n> citado\n- a\n- b\n```kt\nval x = 1\n```")
        assertEquals(listOf("Titulo", "Paragrafo", "Citacao", "Lista", "Codigo"), b.map { it::class.simpleName })
        assertTrue((b[1] as Paragrafo).k.contains(Inline.Usuario(5)))
    }
}
