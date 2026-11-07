package app.telacast.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.telacast.data.Destaque
import app.telacast.data.Formatacao
import app.telacast.data.ItemDaLista
import app.telacast.data.Pendente
import app.telacast.proto.Message
import app.telacast.proto.Reaction
import app.telacast.proto.User
import app.telacast.tema.Formas
import app.telacast.tema.Grafite
import app.telacast.tema.Newsreader
import app.telacast.tema.Tipos
import java.time.ZoneId

/** O que cada linha da lista precisa saber (tudo vem pronto, as linhas não têm lógica). */
class ContextoDaLista(
    val eu: User,
    val agoraMs: Long,
    val conectado: Boolean = true,
    val nomeDoUsuario: (Long) -> String? = { null },
    val nomeDoCanal: (Long) -> String? = { null },
    val spoilers: Set<Pair<Long, Int>> = emptySet(),
    val textosAbertos: Set<Long> = emptySet(),
    val pendentes: Map<Long, Pendente> = emptyMap(),
    val destaque: Destaque? = null,
    val zona: ZoneId = ZoneId.systemDefault(),
)

class AcoesDaLista(
    val aoPressionar: (Message) -> Unit = {},
    val aoSaltar: (Long) -> Unit = {},
    val aoReagir: (mensagem: Long, emoji: String, adicionar: Boolean) -> Unit = { _, _, _ -> },
    val aoMaisReacoes: (Long) -> Unit = {},
    val aoReenviar: (Long) -> Unit = {},
    val aoDescartar: (Long) -> Unit = {},
    val aoMostrarBloqueada: (Long) -> Unit = {},
    val aoSpoiler: (mensagem: Long, spoiler: Int) -> Unit = { _, _ -> },
    val aoLink: (String) -> Unit = {},
    val aoUsuario: (Long) -> Unit = {},
    val aoVerFixadas: () -> Unit = {},
    val anexos: AcoesDeAnexo = AcoesDeAnexo(),
    val aoCarregarAnteriores: () -> Unit = {},
    val aoDestaqueTerminou: () -> Unit = {},
)

private const val FADE_DESTAQUE_MS = 1500L

/**
 * Lista de mensagens: `LazyColumn(reverseLayout = true)` (a mais nova embaixo, e novas páginas antigas entram em cima sem
 * pular a posição). Ao chegar perto do topo pede mais histórico; mensagem nova rola para o fim se você já estava lá.
 */
@Composable
fun ListaDeMensagens(
    itens: List<ItemDaLista>,
    temMais: Boolean,
    carregandoAntigas: Boolean,
    ctx: ContextoDaLista,
    acoes: AcoesDaLista,
    modifier: Modifier = Modifier,
    estado: LazyListState = rememberLazyListState(),
) {
    val reverso = remember(itens) { itens.asReversed() }

    // perto do topo (fim da lista invertida): pede a página mais antiga
    val pertoDoTopo by remember(reverso.size) {
        derivedStateOf { (estado.layoutInfo.visibleItemsInfo.lastOrNull()?.index ?: 0) >= reverso.size - 6 }
    }
    LaunchedEffect(pertoDoTopo, temMais, carregandoAntigas, reverso.size) {
        if (pertoDoTopo && temMais && !carregandoAntigas) acoes.aoCarregarAnteriores()
    }

    // mensagem nova: rola para o fim se já estava no fim (ou se a nova é minha)
    val ultima = reverso.firstOrNull()
    LaunchedEffect(ultima?.chave) {
        if (ultima == null) return@LaunchedEffect
        val minha = (ultima as? ItemDaLista.Mensagem)?.msg?.authorId == ctx.eu.id
        if (estado.firstVisibleItemIndex <= 1 || minha) estado.animateScrollToItem(0)
    }

    // salto até uma mensagem: rola e, depois do tempo do realce, avisa para limpar
    val destaque = ctx.destaque
    LaunchedEffect(destaque) {
        if (destaque == null) return@LaunchedEffect
        val i = reverso.indexOfFirst { it.chave == "m${destaque.id}" }
        if (i >= 0) estado.animateScrollToItem(i, -(estado.layoutInfo.viewportSize.height / 3))
        kotlinx.coroutines.delay(FADE_DESTAQUE_MS)
        acoes.aoDestaqueTerminou()
    }

    LazyColumn(modifier, state = estado, reverseLayout = true, contentPadding = androidx.compose.foundation.layout.PaddingValues(bottom = 8.dp, top = 4.dp)) {
        items(reverso, key = { it.chave }) { item -> ItemDeLista(item, ctx, acoes) }
        item(key = "topo") {
            Box(Modifier.fillMaxWidth().padding(vertical = 14.dp), contentAlignment = Alignment.Center) {
                Text(
                    when {
                        carregandoAntigas -> "Carregando mensagens antigas…"
                        temMais -> ""
                        itens.isEmpty() -> "Ainda não há mensagens. Diga olá!"
                        else -> "Início da conversa"
                    },
                    style = Tipos.Pequeno.copy(color = Grafite.Apagado),
                )
            }
        }
    }
}

@Composable
private fun ItemDeLista(item: ItemDaLista, ctx: ContextoDaLista, acoes: AcoesDaLista) {
    when (item) {
        is ItemDaLista.DivisorDeDia -> DivisorDeDiaView(item.rotulo)
        ItemDaLista.NovasMensagens -> DivisorNovasMensagens()
        is ItemDaLista.Sistema -> LinhaDeSistema(item.msg, ctx, acoes)
        is ItemDaLista.Bloqueada -> LinhaBloqueada(item.msg, acoes)
        is ItemDaLista.Mensagem -> MensagemView(item, ctx, acoes)
    }
}

/** Filete, rótulo em serifa itálica, filete (como `day_divider` do desktop). */
@Composable
fun DivisorDeDiaView(rotulo: String) {
    Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.weight(1f).height(1.dp).background(Grafite.Linha))
        Text(rotulo, style = Tipos.Divisor, modifier = Modifier.padding(horizontal = 12.dp))
        Box(Modifier.weight(1f).height(1.dp).background(Grafite.Linha))
    }
}

/** Filete âmbar com a etiqueta "NOVAS MENSAGENS" na ponta direita (`new_messages_divider`). */
@Composable
fun DivisorNovasMensagens() {
    Box(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp).height(18.dp), contentAlignment = Alignment.CenterEnd) {
        Box(Modifier.fillMaxWidth().height(1.dp).background(Grafite.Ambar.copy(alpha = 0.55f)))
        Box(Modifier.height(16.dp).background(Grafite.Ambar, RoundedCornerShape(8.dp)).padding(horizontal = 8.dp), contentAlignment = Alignment.Center) {
            Text("NOVAS MENSAGENS", style = Tipos.Corpo.copy(fontSize = 10.5.sp, lineHeight = 13.sp, color = Grafite.AmbarTinta, fontWeight = FontWeight.SemiBold))
        }
    }
}

private val ENTROU = listOf("entrou no servidor.", "chegou.", "acabou de aparecer!", "pulou para o servidor.", "acabou de aterrissar.", "chegou por aqui. Diga oi!")

@Composable
private fun LinhaDeSistema(m: Message, ctx: ContextoDaLista, acoes: AcoesDaLista) {
    Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.width(52.dp).height(20.dp).drawBehind {
            val c = Offset(20.dp.toPx(), size.height / 2)
            val u = density
            if (m.kind == 2) { // seta para dentro
                val cor = Grafite.Ok
                drawLine(cor, c + Offset(-6f, 0f) * u, c + Offset(5f, 0f) * u, 1.6f * u, StrokeCap.Round)
                drawLine(cor, c + Offset(1f, -4f) * u, c + Offset(5f, 0f) * u, 1.6f * u, StrokeCap.Round)
                drawLine(cor, c + Offset(1f, 4f) * u, c + Offset(5f, 0f) * u, 1.6f * u, StrokeCap.Round)
            } else { // alfinete
                drawCircle(Grafite.Ambar, 4f * u, c + Offset(0f, -3f) * u)
                drawLine(Grafite.Ambar, c, c + Offset(0f, 7f) * u, 1.6f * u, StrokeCap.Round)
            }
        })
        Column(Modifier.weight(1f)) {
            val texto = androidx.compose.ui.text.buildAnnotatedString {
                withStyle(androidx.compose.ui.text.SpanStyle(fontWeight = FontWeight.SemiBold, color = Grafite.Texto)) { append(m.author) }
                append(" ")
                append(if (m.kind == 2) ENTROU[(m.id % ENTROU.size).toInt().let { if (it < 0) it + ENTROU.size else it }] else "fixou uma mensagem neste canal.")
                withStyle(androidx.compose.ui.text.SpanStyle(fontSize = 11.5.sp, color = Grafite.Apagado)) { append("  ${Formatacao.hora(m.ts, ctx.zona)}") }
            }
            Text(texto, style = Tipos.Corpo.copy(fontSize = 14.sp, color = Grafite.Texto2))
            if (m.kind == 3) {
                Text(
                    "Ver todas as mensagens fixadas",
                    style = Tipos.Corpo.copy(fontSize = 13.sp, color = Grafite.Ambar, textDecoration = TextDecoration.Underline),
                    modifier = Modifier.clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { acoes.aoVerFixadas() }.padding(vertical = 4.dp),
                )
            }
        }
    }
}

@Composable
private fun LinhaBloqueada(m: Message, acoes: AcoesDaLista) {
    val texto = androidx.compose.ui.text.buildAnnotatedString {
        append("Mensagem de usuário bloqueado. ")
        withLink(androidx.compose.ui.text.LinkAnnotation.Clickable("mostrar") { acoes.aoMostrarBloqueada(m.id) }) {
            withStyle(androidx.compose.ui.text.SpanStyle(color = Grafite.Ambar, textDecoration = TextDecoration.Underline)) { append("Mostrar mensagem") }
        }
    }
    Text(texto, style = Tipos.Corpo.copy(fontSize = 13.sp, color = Grafite.Apagado), modifier = Modifier.fillMaxWidth().padding(start = 64.dp, end = 12.dp, top = 8.dp, bottom = 8.dp))
}

@OptIn(ExperimentalFoundationApi::class, ExperimentalLayoutApi::class)
@Composable
fun MensagemView(item: ItemDaLista.Mensagem, ctx: ContextoDaLista, acoes: AcoesDaLista) {
    val m = item.msg
    val destacada = ctx.destaque?.id == m.id
    val fundoDestaque by animateFloatAsState(if (destacada) 0.16f else 0f, tween(if (destacada) 150 else 400), label = "destaque")
    val amb = AmbienteMd(
        nomeDoUsuario = ctx.nomeDoUsuario, nomeDoCanal = ctx.nomeDoCanal, agoraSeg = ctx.agoraMs / 1000, zona = ctx.zona,
        spoilersAbertos = ctx.spoilers.filter { it.first == m.id }.mapTo(HashSet()) { it.second },
    )
    val acoesMd = AcoesMd(aoSpoiler = { acoes.aoSpoiler(m.id, it) }, aoLink = acoes.aoLink, aoUsuario = acoes.aoUsuario)
    val pendente = if (item.pendente) ctx.pendentes[-m.id] else null

    Row(
        Modifier.fillMaxWidth()
            .background(Grafite.Ambar.copy(alpha = fundoDestaque))
            .combinedClickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = {}, onLongClick = { if (!item.pendente) acoes.aoPressionar(m) })
            .padding(start = 12.dp, end = 12.dp, top = if (item.agrupada) 1.dp else 8.dp, bottom = 1.dp),
    ) {
        if (item.agrupada) Spacer(Modifier.width(52.dp)) else { Avatar(m.author, m.authorId, 40.dp, null, Grafite.L2); Spacer(Modifier.width(12.dp)) }
        Column(Modifier.weight(1f)) {
            Column(Modifier.alpha(if (item.pendente) 0.55f else 1f)) {
                m.reply?.let { r ->
                    Row(
                        Modifier.clickable(interactionSource = remember { MutableInteractionSource() }, indication = null) { acoes.aoSaltar(r.id) }.padding(vertical = 3.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text("↳", style = Tipos.Corpo.copy(fontSize = 13.sp, color = Grafite.Apagado))
                        Spacer(Modifier.width(6.dp))
                        Text(r.author, style = Tipos.Corpo.copy(fontSize = 13.sp, fontWeight = FontWeight.SemiBold, color = Grafite.Texto2), maxLines = 1)
                        Spacer(Modifier.width(6.dp))
                        Text(
                            r.content.take(90).replace('\n', ' '), style = Tipos.Corpo.copy(fontSize = 13.sp, color = Grafite.Apagado),
                            maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false),
                        )
                    }
                }
                if (!item.agrupada) {
                    Row(verticalAlignment = Alignment.Bottom) {
                        Text(m.author, style = Tipos.Corpo.copy(fontSize = 15.5.sp, fontWeight = FontWeight.SemiBold, color = Grafite.Texto), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
                        Spacer(Modifier.width(8.dp))
                        Text(Formatacao.hora(m.ts, ctx.zona), style = Tipos.Pequeno.copy(fontSize = 12.sp, color = Grafite.Apagado))
                        if (m.pinned) { Spacer(Modifier.width(8.dp)); Text("Fixada", style = Tipos.Pequeno.copy(fontSize = 11.5.sp, color = Grafite.Ambar)) }
                    }
                } else if (m.pinned) {
                    Text("Fixada", style = Tipos.Pequeno.copy(fontSize = 11.5.sp, color = Grafite.Ambar))
                }
                if (m.content.isNotEmpty()) TextoMarkdown(m.content, amb, acoesMd, editada = m.edited)
                for (a in m.attachments) AnexoView(a, a.id in ctx.textosAbertos, acoes.anexos, Modifier.padding(top = 4.dp))
                if (m.reactions.isNotEmpty() && !item.pendente) {
                    FlowRow(Modifier.padding(top = 4.dp), horizontalArrangement = Arrangement.spacedBy(4.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        for (r in m.reactions) ChipDeReacao(r.emoji, r.count, r.me) { acoes.aoReagir(m.id, r.emoji, !r.me) }
                        ChipDeReacao("+", null, false) { acoes.aoMaisReacoes(m.id) }
                    }
                }
            }
            if (pendente != null || item.pendente) EstadoDoEnvioView(pendente, -m.id, ctx, acoes)
        }
    }
}

/** Chip de reação: emoji e contagem; âmbar (borda e fundo suave) quando eu reagi. */
@Composable
fun ChipDeReacao(emoji: String, contagem: Int?, eu: Boolean, aoClicar: () -> Unit) {
    val forma = Formas.Reacao
    Toque(
        aoClicar, Modifier.height(28.dp).border(1.dp, if (eu) Grafite.Ambar else Grafite.Linha2, forma), forma,
        fundo = if (eu) Grafite.Ambar.copy(alpha = 0.12f) else Grafite.L3, fundoPressionado = Grafite.L4,
    ) {
        Row(Modifier.height(28.dp).padding(horizontal = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(emoji, style = Tipos.Corpo.copy(fontSize = 15.sp, color = Grafite.Texto))
            if (contagem != null) {
                Spacer(Modifier.width(6.dp))
                Text(contagem.toString(), style = Tipos.Corpo.copy(fontSize = 12.5.sp, color = if (eu) Grafite.Ambar else Grafite.Texto2, fontWeight = FontWeight.Medium))
            }
        }
    }
}

/** "Enviando…", "Sem conexão. Será enviada ao reconectar." ou "Não foi enviada." com Tentar de novo e Descartar. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun EstadoDoEnvioView(p: Pendente?, nonce: Long, ctx: ContextoDaLista, acoes: AcoesDaLista) {
    val espera = if (p != null) ctx.agoraMs - p.ts else 0L
    FlowRow(Modifier.padding(top = 4.dp), verticalArrangement = Arrangement.spacedBy(6.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        when {
            !ctx.conectado -> Text("Sem conexão. Será enviada ao reconectar.", style = Tipos.Pequeno.copy(fontSize = 11.5.sp, color = Grafite.Apagado))
            espera < 15_000 -> Text("Enviando…", style = Tipos.Pequeno.copy(fontSize = 11.5.sp, color = Grafite.Apagado))
            else -> {
                Text("Não foi enviada.", style = Tipos.Pequeno.copy(fontSize = 11.5.sp, color = Grafite.Vermelho))
                Botao("Tentar de novo", TipoBotao.Quieto, altura = 32.dp, aoClicar = { acoes.aoReenviar(nonce) })
                Botao("Descartar", TipoBotao.Quieto, altura = 32.dp, aoClicar = { acoes.aoDescartar(nonce) })
            }
        }
    }
}
