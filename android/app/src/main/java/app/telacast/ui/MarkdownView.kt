package app.telacast.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.TextUnit
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.telacast.data.Formatacao
import app.telacast.md.Bloco
import app.telacast.md.Inline
import app.telacast.md.Markdown
import app.telacast.tema.Grafite
import app.telacast.tema.Mono
import app.telacast.tema.Tipos
import java.time.ZoneId

/** O que o desenho do Markdown precisa saber do app: nomes atuais e quais spoilers já foram revelados. */
class AmbienteMd(
    val nomeDoUsuario: (Long) -> String? = { null },
    val nomeDoCanal: (Long) -> String? = { null },
    val agoraSeg: Long = 0,
    val spoilersAbertos: Set<Int> = emptySet(),
    val zona: ZoneId = ZoneId.systemDefault(),
)

class AcoesMd(
    val aoSpoiler: (Int) -> Unit = {},
    val aoLink: (String) -> Unit = {},
    val aoUsuario: (Long) -> Unit = {},
)

private val Destaque = SpanStyle(fontWeight = FontWeight.SemiBold, color = Grafite.AmbarQuente, background = Grafite.Ambar.copy(alpha = 0.14f))

/** Monta o texto com estilos e trechos tocáveis (spoiler, link, menção) a partir dos nós do parser. */
fun construirTexto(nos: List<Inline>, amb: AmbienteMd, acoes: AcoesMd, tamanho: TextUnit, sufixo: String? = null): AnnotatedString = buildAnnotatedString {
    empilhar(nos, amb, acoes, tamanho, escondido = false)
    if (sufixo != null) withStyle(SpanStyle(fontSize = 11.5.sp, color = Grafite.Apagado, fontWeight = FontWeight.Normal, fontStyle = FontStyle.Normal)) { append(sufixo) }
}

private fun AnnotatedString.Builder.empilhar(nos: List<Inline>, amb: AmbienteMd, acoes: AcoesMd, tam: TextUnit, escondido: Boolean) {
    for (n in nos) {
        when (n) {
            is Inline.Texto -> append(n.s)
            is Inline.Negrito -> withStyle(SpanStyle(fontWeight = FontWeight.Bold, color = Grafite.Texto)) { empilhar(n.k, amb, acoes, tam, escondido) }
            is Inline.Italico -> withStyle(SpanStyle(fontStyle = FontStyle.Italic)) { empilhar(n.k, amb, acoes, tam, escondido) }
            is Inline.Sublinhado -> withStyle(SpanStyle(textDecoration = TextDecoration.Underline)) { empilhar(n.k, amb, acoes, tam, escondido) }
            is Inline.Tachado -> withStyle(SpanStyle(textDecoration = TextDecoration.LineThrough)) { empilhar(n.k, amb, acoes, tam, escondido) }
            is Inline.Codigo -> withStyle(SpanStyle(fontFamily = Mono, fontSize = tam * 0.92f, color = Grafite.Texto, background = Grafite.L3)) { append(n.s) }
            is Inline.Spoiler -> {
                val aberto = n.id in amb.spoilersAbertos
                val estilo = if (aberto) SpanStyle(background = Grafite.Apagado.copy(alpha = 0.16f)) else SpanStyle(color = Grafite.Linha2, background = Grafite.Linha2)
                withLink(LinkAnnotation.Clickable("spoiler-${n.id}") { acoes.aoSpoiler(n.id) }) {
                    withStyle(estilo) { empilhar(n.k, amb, acoes, tam, escondido = escondido || !aberto) }
                }
            }
            is Inline.Link -> {
                val estilo = SpanStyle(color = Grafite.Ambar, textDecoration = TextDecoration.Underline)
                if (escondido) withStyle(estilo) { empilhar(n.k, amb, acoes, tam, true) }
                else withLink(LinkAnnotation.Clickable("link:${n.url}") { acoes.aoLink(n.url) }) { withStyle(estilo) { empilhar(n.k, amb, acoes, tam, false) } }
            }
            is Inline.Usuario -> {
                val nome = amb.nomeDoUsuario(n.id) ?: "usuário-desconhecido"
                if (escondido) withStyle(Destaque) { append("@$nome") }
                else withLink(LinkAnnotation.Clickable("usuario-${n.id}") { acoes.aoUsuario(n.id) }) { withStyle(Destaque) { append("@$nome") } }
            }
            is Inline.Canal -> withStyle(Destaque) { append("#${amb.nomeDoCanal(n.id) ?: "canal-desconhecido"}") }
            is Inline.Todos -> withStyle(Destaque) { append(n.nome) }
            is Inline.Tempo -> withStyle(SpanStyle(color = Grafite.Texto, background = Grafite.L3)) { append(Formatacao.carimbo(n.unix, n.estilo, amb.agoraSeg, amb.zona)) }
        }
    }
}

/** Mensagem inteira: blocos de texto, título, subtexto, citação, lista e código, no tema Grafite. */
@Composable
fun TextoMarkdown(
    texto: String,
    amb: AmbienteMd,
    acoes: AcoesMd,
    modifier: Modifier = Modifier,
    tamanho: TextUnit = 15.5.sp,
    cor: androidx.compose.ui.graphics.Color = Grafite.Texto2,
    editada: Boolean = false,
) {
    val blocos = remember(texto) { Markdown.parse(texto) }
    Column(modifier) {
        val ultimo = blocos.lastIndex
        blocos.forEachIndexed { i, b ->
            val sufixo = if (editada && i == ultimo) " (editado)" else null
            Bloco(b, amb, acoes, tamanho, cor, sufixo)
        }
        if (editada && blocos.lastOrNull().let { it !is app.telacast.md.Bloco.Paragrafo && it !is app.telacast.md.Bloco.Titulo && it !is app.telacast.md.Bloco.Subtexto && it !is app.telacast.md.Bloco.Lista && it !is app.telacast.md.Bloco.Citacao }) {
            Text("(editado)", style = Tipos.Pequeno.copy(color = Grafite.Apagado, fontSize = 11.5.sp))
        }
    }
}

private fun estiloBase(tam: TextUnit, cor: androidx.compose.ui.graphics.Color) = Tipos.Corpo.copy(fontSize = tam, lineHeight = tam * 1.42f, color = cor)

@Composable
private fun Bloco(b: app.telacast.md.Bloco, amb: AmbienteMd, acoes: AcoesMd, tam: TextUnit, cor: androidx.compose.ui.graphics.Color, sufixo: String?) {
    when (b) {
        is app.telacast.md.Bloco.Paragrafo -> Text(construirTexto(b.k, amb, acoes, tam, sufixo), style = estiloBase(tam, cor))
        is app.telacast.md.Bloco.Titulo -> {
            val t = when (b.nivel) { 1 -> 24.sp; 2 -> 20.sp; else -> 17.5.sp }
            Text(
                construirTexto(b.k, amb, acoes, t, sufixo),
                style = estiloBase(t, Grafite.Texto).copy(fontWeight = FontWeight.Bold), modifier = Modifier.padding(top = 4.dp),
            )
        }
        is app.telacast.md.Bloco.Subtexto -> Text(construirTexto(b.k, amb, acoes, 12.5.sp, sufixo), style = estiloBase(12.5.sp, Grafite.Apagado))
        is app.telacast.md.Bloco.Lista -> Column {
            b.itens.forEachIndexed { n, it ->
                Row(Modifier.padding(start = 6.dp + (it.profundidade * 18).dp)) {
                    val marca = b.inicio?.let { s -> "${s + n}." } ?: if (it.profundidade % 2 == 0) "•" else "◦"
                    Text(marca, style = estiloBase(tam, Grafite.Apagado), textAlign = TextAlign.End, modifier = Modifier.width(22.dp))
                    Text(
                        construirTexto(it.k, amb, acoes, tam, if (n == b.itens.lastIndex) sufixo else null),
                        style = estiloBase(tam, cor), modifier = Modifier.padding(start = 6.dp),
                    )
                }
            }
        }
        is app.telacast.md.Bloco.Citacao -> Row(Modifier.height(IntrinsicSize.Min)) {
            Box(Modifier.width(4.dp).fillMaxHeight().background(Grafite.Linha2, RoundedCornerShape(2.dp)))
            Column(Modifier.padding(start = 10.dp)) {
                b.blocos.forEachIndexed { j, ib -> Bloco(ib, amb, acoes, tam, cor, if (j == b.blocos.lastIndex) sufixo else null) }
            }
        }
        is app.telacast.md.Bloco.Codigo -> Box(
            Modifier.padding(vertical = 2.dp).fillMaxWidth().background(Grafite.L1, RoundedCornerShape(6.dp)).border(1.dp, Grafite.Linha, RoundedCornerShape(6.dp))
                .horizontalScroll(rememberScrollState()).padding(horizontal = 10.dp, vertical = 8.dp),
        ) {
            Text(b.texto, style = Tipos.Corpo.copy(fontFamily = Mono, fontSize = 13.sp, lineHeight = 19.sp, color = Grafite.Texto), softWrap = false)
        }
    }
}
