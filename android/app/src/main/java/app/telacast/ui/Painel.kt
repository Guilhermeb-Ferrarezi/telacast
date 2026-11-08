package app.telacast.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.telacast.data.AppState
import app.telacast.estado.AbaInicio
import app.telacast.estado.Destino
import app.telacast.proto.Channel
import app.telacast.proto.FriendStatus
import app.telacast.proto.ServerInfo
import app.telacast.proto.Status
import app.telacast.voz.EstadoDeVoz
import app.telacast.tema.Formas
import app.telacast.tema.Grafite
import app.telacast.tema.Newsreader
import app.telacast.tema.Tipos

const val LARGURA_PAINEL = 240
const val ALTURA_LINHA = 40

/** Ações do painel de canais (todas opcionais nos testes de imagem). */
class AcoesPainel(
    val aoAbrirCanal: (servidor: Long, canal: Long) -> Unit = { _, _ -> },
    val aoAlternarCategoria: (Long) -> Unit = {},
    val aoCliqueEmVoz: (servidor: Long, canal: Long) -> Unit = { _, _ -> },
    val aoAbrirMenuServidor: () -> Unit = {},
    val aoAbrirMenuStatus: () -> Unit = {},
    val aoCriarCanal: (servidor: Long, categoria: Boolean, pai: Long?) -> Unit = { _, _, _ -> },
    val aoAbrirAmigos: () -> Unit = {},
    val aoAbrirDm: (Long) -> Unit = {},
)

/**
 * Painel de canais (240 dp, camada L1): cabeçalho do servidor, árvore (canais soltos, depois categorias recolhíveis),
 * e o cartão do usuário embaixo. Com `servidor == null` mostra o Início: "Amigos" e as mensagens diretas.
 */
@Composable
fun PainelDeCanais(
    estado: AppState,
    servidor: ServerInfo?,
    destino: Destino,
    recolhidas: Set<Long>,
    menuServidorAberto: Boolean,
    acoes: AcoesPainel,
    modifier: Modifier = Modifier,
    largura: androidx.compose.ui.unit.Dp = LARGURA_PAINEL.dp,
    voz: DadosDeVoz = DadosDeVoz(),
    acoesVoz: AcoesDeVoz = AcoesDeVoz(),
    transmissao: app.telacast.tela.EstadoDaTransmissao = app.telacast.tela.EstadoDaTransmissao(),
    acoesTela: AcoesDeTela = AcoesDeTela(),
) {
    Column(modifier.width(largura).fillMaxHeight().background(Grafite.L1)) {
        Column(Modifier.weight(1f)) {
            if (servidor != null) {
                CabecalhoDoServidor(servidor.name, menuServidorAberto, acoes.aoAbrirMenuServidor)
                ArvoreDeCanais(estado, servidor, destino, recolhidas, acoes, Modifier.weight(1f), voz)
            } else {
                ListaDeMensagensDiretas(estado, destino, acoes, Modifier.weight(1f))
            }
        }
        if (transmissao.ativa && transmissao.fase != app.telacast.tela.FaseDaTransmissao.PedindoPermissao) CartaoDeTransmissao(transmissao, acoesTela)
        if (voz.estado.naChamada) CartaoDeVoz(voz.estado, localDaVoz(estado, voz.estado.canal), voz.agoraMs, acoesVoz)
        CartaoDoUsuario(estado, acoes.aoAbrirMenuStatus)
    }
}

@Composable
fun CabecalhoDoServidor(nome: String, aberto: Boolean, aoClicar: () -> Unit) {
    Toque(aoClicar, Modifier.fillMaxWidth().height(52.dp), fundo = if (aberto) Grafite.L3 else Grafite.L1, fundoPressionado = Grafite.L2) {
        Row(
            Modifier.fillMaxSize().padding(start = 16.dp, end = 16.dp)
                .drawBehind { drawLine(Grafite.Linha, Offset(0f, size.height - 0.5f), Offset(size.width, size.height - 0.5f), 1f) },
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                nome, style = Tipos.Titulo.copy(fontSize = 18.sp, lineHeight = 24.sp, letterSpacing = 0.sp),
                maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
            )
            Spacer(Modifier.width(8.dp))
            if (aberto) IconeFechar(Grafite.Texto, tamanho = 16.dp) else IconeSeta(true, Grafite.Texto2)
        }
    }
}

/** Canais soltos primeiro; depois cada categoria (recolhível) com seus canais de texto e, abaixo, os de voz. */
@Composable
fun ArvoreDeCanais(
    estado: AppState,
    sv: ServerInfo,
    destino: Destino,
    recolhidas: Set<Long>,
    acoes: AcoesPainel,
    modifier: Modifier = Modifier,
    voz: DadosDeVoz = DadosDeVoz(),
) {
    val dono = sv.owner == estado.eu?.id
    val atual = ((destino as? Destino.Canal)?.takeIf { it.servidor == sv.id }?.canal) ?: ((destino as? Destino.Voz)?.takeIf { it.servidor == sv.id }?.canal)
    val categorias = sv.channels.filter { it.kind == Channel.CATEGORIA }.sortedWith(compareBy({ it.position }, { it.id }))
    fun filhos(pai: Long?) = sv.channels.filter { c ->
        (c.kind == Channel.TEXTO || c.kind == Channel.VOZ) &&
            if (pai != null) c.parent == pai else c.parent == null || categorias.none { it.id == c.parent }
    }.sortedWith(compareBy({ it.kind }, { it.position }, { it.id }))

    val soltos = filhos(null)
    LazyColumn(modifier, contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 8.dp, vertical = 8.dp)) {
        items(soltos, key = { it.id }) { c -> LinhaDeCanal2(estado, sv, c, atual, acoes, voz) }
        categorias.forEachIndexed { i, cat ->
            val recolhida = cat.id in recolhidas
            item(key = "cat-${cat.id}") {
                CabecalhoDeCategoria(cat.name, recolhida, dono, topo = if (i == 0 && soltos.isEmpty()) 4.dp else 14.dp,
                    aoAlternar = { acoes.aoAlternarCategoria(cat.id) }, aoMais = { acoes.aoCriarCanal(sv.id, false, cat.id) })
            }
            // recolhida: só aparece o que não pode sumir (canal aberto, não lido, voz com gente)
            val visiveis = filhos(cat.id).filter { c ->
                !recolhida || c.id == atual || c.id in estado.naoLidos || estado.voz.any { it.channelId == c.id }
            }
            items(visiveis, key = { it.id }) { c -> LinhaDeCanal2(estado, sv, c, atual, acoes, voz) }
        }
    }
}

@Composable
private fun LinhaDeCanal2(estado: AppState, sv: ServerInfo, c: Channel, atual: Long?, acoes: AcoesPainel, voz: DadosDeVoz) {
    if (c.kind == Channel.VOZ) {
        LinhaDeVoz(estado, sv, c, acoes, voz.estado, selecionado = c.id == atual, euId = estado.eu?.id ?: 0L)
        return
    }
    val selecionado = c.id == atual
    val naoLido = c.id in estado.naoLidos && !selecionado
    val mencoes = if (selecionado) 0 else estado.mencoes[c.id] ?: 0
    val aoVivo = estado.snapshot?.live?.any { it.channelId == c.id } == true
    Box(Modifier.fillMaxWidth().height(ALTURA_LINHA.dp)) {
        Toque(
            { acoes.aoAbrirCanal(sv.id, c.id) }, Modifier.fillMaxSize(), Formas.Linha,
            fundo = if (selecionado) Grafite.L3 else Grafite.L1, fundoPressionado = Grafite.L2,
        ) {
            Row(Modifier.fillMaxSize().padding(horizontal = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                val forte = selecionado || naoLido
                Text("#", style = Tipos.Corpo.copy(fontSize = 18.sp, color = if (forte) Grafite.Texto2 else Grafite.Apagado), modifier = Modifier.width(20.dp))
                Spacer(Modifier.width(8.dp))
                Text(
                    c.name,
                    style = Tipos.Corpo.copy(color = if (forte) Grafite.Texto else Grafite.Texto2, fontWeight = if (naoLido) FontWeight.SemiBold else FontWeight.Normal),
                    maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
                )
                if (aoVivo) Text("AO VIVO", style = Tipos.Corpo.copy(fontSize = 10.sp, color = Grafite.Ambar, fontWeight = FontWeight.Bold, letterSpacing = 1.sp))
                if (mencoes > 0) {
                    Spacer(Modifier.width(6.dp))
                    SeloNaoLido(mencoes)
                }
            }
        }
        if (naoLido) {
            // barra de não lido (3 x 8) na margem esquerda do painel
            Box(Modifier.align(Alignment.CenterStart).offset(x = (-6.5).dp).size(width = 3.dp, height = 8.dp).background(Grafite.Texto, RoundedCornerShape(2.dp)))
        }
    }
}

@Composable
private fun LinhaDeVoz(estado: AppState, sv: ServerInfo, c: Channel, acoes: AcoesPainel, voz: EstadoDeVoz, selecionado: Boolean, euId: Long) {
    val membros = estado.voz.filter { it.channelId == c.id }
    val nesteCanal = voz.naChamada && voz.canal == c.id
    val aoVivo = estado.snapshot?.live.orEmpty().filter { it.channelId == c.id }.map { it.userId }.toSet()
    Toque({ acoes.aoCliqueEmVoz(sv.id, c.id) }, Modifier.fillMaxWidth().height(ALTURA_LINHA.dp), Formas.Linha, fundo = if (selecionado) Grafite.L3 else Grafite.L1, fundoPressionado = Grafite.L2) {
        Row(Modifier.fillMaxSize().padding(horizontal = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.width(20.dp), contentAlignment = Alignment.CenterStart) { IconeAltoFalante(if (nesteCanal) Grafite.Ok else Grafite.Apagado) }
            Spacer(Modifier.width(8.dp))
            Text(c.name, style = Tipos.Corpo.copy(color = if (selecionado || nesteCanal) Grafite.Texto else Grafite.Texto2), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
        }
    }
    for (v in membros) {
        val nome = sv.members.firstOrNull { it.id == v.userId }?.name ?: "?"
        val eu = v.userId == euId
        val falando = if (eu) voz.euFalando && !voz.mudo && !voz.surdo else v.userId in voz.falando
        val mudo = if (eu && nesteCanal) voz.mudo || voz.surdo else v.muted || v.deafened
        Row(Modifier.fillMaxWidth().height(32.dp).padding(start = 28.dp, end = 6.dp), verticalAlignment = Alignment.CenterVertically) {
            Avatar(nome, v.userId, 22.dp, fundo = Grafite.L1, falando = falando)
            Spacer(Modifier.width(8.dp))
            Text(nome, style = Tipos.Corpo.copy(fontSize = 13.5.sp, color = if (mudo) Grafite.Apagado else Grafite.Texto2), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
            if (v.userId in aoVivo) { SeloAoVivo(); Spacer(Modifier.width(6.dp)) }
            if (if (eu && nesteCanal) voz.surdo else v.deafened) IconeFones(Grafite.Apagado, cortado = true, tamanho = 14.dp)
            else if (mudo) IconeMicrofone(Grafite.Apagado, cortado = true, tamanho = 14.dp)
        }
    }
}

@Composable
fun CabecalhoDeCategoria(nome: String, recolhida: Boolean, dono: Boolean, topo: androidx.compose.ui.unit.Dp, aoAlternar: () -> Unit, aoMais: () -> Unit) {
    Spacer(Modifier.height(topo))
    Row(Modifier.fillMaxWidth().height(32.dp), verticalAlignment = Alignment.CenterVertically) {
        Toque(aoAlternar, Modifier.weight(1f).fillMaxHeight(), fundo = Grafite.L1, fundoPressionado = Grafite.L1) {
            Row(Modifier.fillMaxSize().padding(start = 2.dp), verticalAlignment = Alignment.CenterVertically) {
                IconeSeta(!recolhida, Grafite.Apagado, Modifier.padding(horizontal = 2.dp))
                Spacer(Modifier.width(4.dp))
                Text(nome.uppercase(), style = Tipos.Rotulo.copy(fontSize = 11.5.sp, letterSpacing = 0.sp), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        if (dono) Toque(aoMais, Modifier.size(32.dp), fundo = Grafite.L1, fundoPressionado = Grafite.L3) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { IconeMais(Grafite.Apagado) }
        }
    }
}

/** Cartão do usuário (camada L2, raio 10): avatar de 32 com presença, nome e status; o toque abre o menu de status. */
@Composable
fun CartaoDoUsuario(estado: AppState, aoClicar: () -> Unit) {
    val eu = estado.eu ?: return
    val status = if (eu.status == Status.Offline) Status.Online else eu.status
    Box(Modifier.padding(start = 8.dp, end = 8.dp, bottom = 8.dp, top = 4.dp)) {
        Toque(aoClicar, Modifier.fillMaxWidth(), Formas.Menu, fundo = Grafite.L2, fundoPressionado = Grafite.L3) {
            Row(Modifier.padding(8.dp), verticalAlignment = Alignment.CenterVertically) {
                Avatar(eu.name, eu.id, 32.dp, status.presenca(), Grafite.L2)
                Spacer(Modifier.width(10.dp))
                Column(Modifier.weight(1f)) {
                    Text(eu.name, style = Tipos.Corpo.copy(fontWeight = FontWeight.SemiBold, lineHeight = 18.sp), maxLines = 1, overflow = TextOverflow.Ellipsis)
                    Text(status.rotulo(), style = Tipos.Corpo.copy(fontSize = 11.5.sp, lineHeight = 15.sp, color = Grafite.Apagado))
                }
                IconeSetaParaCima(Grafite.Apagado)
            }
        }
    }
}

/** Painel do Início: linha "Amigos" (com pedidos pendentes) e as mensagens diretas. */
@Composable
fun ListaDeMensagensDiretas(estado: AppState, destino: Destino, acoes: AcoesPainel, modifier: Modifier = Modifier) {
    val sn = estado.snapshot
    val pendentes = sn?.friends?.count { it.status == FriendStatus.Incoming } ?: 0
    Column(modifier.padding(top = 10.dp)) {
        Toque(acoes.aoAbrirAmigos, Modifier.padding(horizontal = 10.dp).fillMaxWidth().height(ALTURA_LINHA.dp), Formas.Linha, fundo = if (destino == Destino.Amigos) Grafite.L3 else Grafite.L1, fundoPressionado = Grafite.L2) {
            Row(Modifier.fillMaxSize().padding(horizontal = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                Text("Amigos", style = Tipos.Corpo.copy(fontWeight = FontWeight.SemiBold, color = if (destino == Destino.Amigos) Grafite.Texto else Grafite.Texto2))
                if (pendentes > 0) Text(" $pendentes", style = Tipos.Corpo.copy(fontWeight = FontWeight.SemiBold, color = Grafite.Ambar))
            }
        }
        Spacer(Modifier.height(8.dp))
        Rotulo("Mensagens diretas", Modifier.padding(start = 20.dp, bottom = 6.dp))
        LazyColumn(contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 10.dp, vertical = 0.dp)) {
            items(sn?.dms.orEmpty(), key = { it.channelId }) { d ->
                val selecionado = (destino as? Destino.Dm)?.canal == d.channelId
                val naoLido = d.channelId in estado.naoLidos && !selecionado
                val mencoes = if (selecionado) 0 else estado.mencoes[d.channelId] ?: 0
                Box(Modifier.fillMaxWidth().height(ALTURA_LINHA.dp + 4.dp)) {
                    Toque({ acoes.aoAbrirDm(d.channelId) }, Modifier.fillMaxSize(), Formas.Linha, fundo = if (selecionado) Grafite.L3 else Grafite.L1, fundoPressionado = Grafite.L2) {
                        Row(Modifier.fillMaxSize().padding(horizontal = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                            Avatar(d.user.name, d.user.id, 28.dp, d.user.status.presenca(), Grafite.L1)
                            Spacer(Modifier.width(10.dp))
                            Text(
                                d.user.name,
                                style = Tipos.Corpo.copy(color = if (selecionado || naoLido) Grafite.Texto else Grafite.Texto2, fontWeight = if (naoLido) FontWeight.SemiBold else FontWeight.Normal),
                                maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f),
                            )
                            if (mencoes > 0) SeloNaoLido(mencoes)
                        }
                    }
                    if (naoLido) Box(Modifier.align(Alignment.CenterStart).offset(x = (-6.5).dp).size(width = 3.dp, height = 8.dp).background(Grafite.Texto, RoundedCornerShape(2.dp)))
                }
            }
        }
    }
}
