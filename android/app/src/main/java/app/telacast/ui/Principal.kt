package app.telacast.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.telacast.data.AppState
import app.telacast.estado.Destino
import app.telacast.estado.Dialogo
import app.telacast.estado.MenuAberto
import app.telacast.estado.Navegacao
import app.telacast.net.EstadoConexao
import app.telacast.proto.Status
import app.telacast.tema.Grafite
import app.telacast.tema.Tipos
import kotlin.math.roundToInt

/** Tudo o que a tela principal pode pedir; os testes de imagem usam os padrões vazios. */
class AcoesPrincipais(
    val painel: AcoesPainel = AcoesPainel(),
    val dialogo: AcoesDialogo = AcoesDialogo(),
    val amigos: AcoesAmigos = AcoesAmigos(),
    val aoAbrirGaveta: () -> Unit = {},
    val aoFecharGaveta: () -> Unit = {},
    val aoAbrirMembros: () -> Unit = {},
    val aoFecharMembros: () -> Unit = {},
    val aoFecharMenu: () -> Unit = {},
    val aoAbrirInicio: () -> Unit = {},
    val aoSelecionarServidor: (Long) -> Unit = {},
    val aoAdicionarServidor: () -> Unit = {},
    val aoConvidar: (String) -> Unit = {},
    val aoEscolherStatus: (Status) -> Unit = {},
    val aoPedirSaida: () -> Unit = {},
    val conversa: AcoesDaConversa = AcoesDaConversa(),
    val folha: AcoesDaFolha = AcoesDaFolha(),
)

/**
 * Navegação estilo Discord mobile: o centro (canal, DM ou amigos) e, por cima, a gaveta da esquerda (trilho de
 * servidores + canais) e o painel de membros da direita. Abrem por botão ou arrastando; tocar no escurecido fecha.
 * Sem estado próprio de navegação aqui: tudo vem em [nav], então qualquer combinação pode ser desenhada nos testes.
 */
@Composable
fun TelaPrincipal(
    estado: AppState,
    nav: Navegacao,
    recolhidas: Set<Long>,
    aviso: String?,
    acoes: AcoesPrincipais,
    modifier: Modifier = Modifier,
    conversa: DadosDaConversa = DadosDaConversa(),
) {
    BoxWithConstraints(modifier.fillMaxSize().background(Grafite.L2)) {
        val larguraGaveta = minOf((LARGURA_TRILHO + LARGURA_PAINEL).dp, maxWidth - 56.dp)
        val larguraMembros = minOf(280.dp, maxWidth - 56.dp)
        val pxGaveta = with(LocalDensity.current) { larguraGaveta.toPx() }
        val limiarPx = with(LocalDensity.current) { 48.dp.toPx() }

        val alvoEsq = if (nav.gavetaAberta) 1f else 0f
        val alvoDir = if (nav.membrosAbertos) 1f else 0f
        val animEsq by animateFloatAsState(alvoEsq, tween(200), label = "gaveta")
        val animDir by animateFloatAsState(alvoDir, tween(200), label = "membros")
        var arrasto by remember { mutableFloatStateOf(0f) }
        var arrastando by remember { mutableStateOf(false) }
        val pEsq = if (arrastando) (alvoEsq + arrasto / pxGaveta).coerceIn(0f, 1f) else animEsq
        val pDir = if (arrastando) (alvoDir - arrasto / pxGaveta).coerceIn(0f, 1f) else animDir

        Box(
            Modifier.fillMaxSize().pointerInput(nav.gavetaAberta, nav.membrosAbertos) {
                detectHorizontalDragGestures(
                    onDragStart = { arrastando = true; arrasto = 0f },
                    onDragCancel = { arrastando = false },
                    onDragEnd = {
                        arrastando = false
                        when {
                            arrasto > limiarPx -> if (nav.membrosAbertos) acoes.aoFecharMembros() else acoes.aoAbrirGaveta()
                            arrasto < -limiarPx -> if (nav.gavetaAberta) acoes.aoFecharGaveta() else acoes.aoAbrirMembros()
                        }
                    },
                    onHorizontalDrag = { change, dx -> change.consume(); arrasto += dx },
                )
            },
        ) {
            Column(Modifier.fillMaxSize().windowInsetsPadding(WindowInsets.statusBars)) {
                (estado.conexao as? EstadoConexao.Reconectando)?.let { FaixaDeReconexao(it.tentativa) }
                Box(Modifier.weight(1f)) { Centro(estado, nav, acoes, conversa) }
            }

            CamadaLateral(pEsq, aEsquerda = true, largura = larguraGaveta, aoFechar = acoes.aoFecharGaveta) {
                Gaveta(estado, nav, recolhidas, acoes, larguraGaveta)
            }
            CamadaLateral(pDir, aEsquerda = false, largura = larguraMembros, aoFechar = acoes.aoFecharMembros) {
                val sv = (nav.destino as? Destino.Canal)?.let { d -> estado.snapshot?.servers?.firstOrNull { it.id == d.servidor } }
                Box(Modifier.windowInsetsPadding(WindowInsets.statusBars)) { PainelDeMembros(sv) }
            }

            nav.folha?.let { f -> FolhaDaMensagem(estado, f.mensagem, f.emojis, acoes.folha) }
            nav.dialogo?.let { DialogoAtual(it, acoes.dialogo) }
            if (aviso != null) {
                AvisoCurto(aviso, Modifier.align(Alignment.BottomCenter).windowInsetsPadding(WindowInsets.navigationBars).padding(bottom = 16.dp))
            }
        }
    }
}

@Composable
private fun FaixaDeReconexao(tentativa: Int) {
    Box(Modifier.fillMaxWidth().background(Grafite.L3).padding(vertical = 6.dp), contentAlignment = Alignment.Center) {
        Text(
            if (tentativa <= 1) "Sem conexão. Reconectando…" else "Sem conexão. Reconectando… (tentativa $tentativa)",
            style = Tipos.Pequeno.copy(color = Grafite.Ambar, fontSize = 12.5.sp),
        )
    }
}

@Composable
private fun Centro(estado: AppState, nav: Navegacao, acoes: AcoesPrincipais, conversa: DadosDaConversa) {
    val sn = estado.snapshot
    when (val d = nav.destino) {
        is Destino.Canal -> {
            val canal = sn?.servers?.firstOrNull { it.id == d.servidor }?.channels?.firstOrNull { it.id == d.canal }
            if (canal != null) TelaDeCanal(estado, canal, acoes.aoAbrirGaveta, acoes.aoAbrirMembros, conversa, acoes.conversa)
            else TelaDeAmigos(estado, nav.abaInicio, acoes.aoAbrirGaveta, acoes.amigos)
        }
        is Destino.Dm -> {
            val dm = sn?.dms?.firstOrNull { it.channelId == d.canal }
            if (dm != null) TelaDeDm(estado, dm, acoes.aoAbrirGaveta, conversa, acoes.conversa)
            else TelaDeAmigos(estado, nav.abaInicio, acoes.aoAbrirGaveta, acoes.amigos)
        }
        Destino.Amigos -> TelaDeAmigos(estado, nav.abaInicio, acoes.aoAbrirGaveta, acoes.amigos)
    }
}

/** Gaveta: trilho de servidores à esquerda, painel de canais (ou do Início) ao lado, e os menus por cima do painel. */
@Composable
private fun Gaveta(estado: AppState, nav: Navegacao, recolhidas: Set<Long>, acoes: AcoesPrincipais, largura: Dp) {
    val sv = nav.servidorNoPainel?.let { id -> estado.snapshot?.servers?.firstOrNull { it.id == id } }
    Row(Modifier.fillMaxSize().background(Grafite.L1).windowInsetsPadding(WindowInsets.statusBars).windowInsetsPadding(WindowInsets.navigationBars)) {
        TrilhoDeServidores(estado, nav.servidorNoPainel, acoes.aoAbrirInicio, acoes.aoSelecionarServidor, acoes.aoAdicionarServidor)
        Box(Modifier.weight(1f).fillMaxHeight()) {
            PainelDeCanais(
                estado, sv, nav.destino, recolhidas, nav.menu == MenuAberto.Servidor,
                AcoesPainel(
                    aoAbrirCanal = acoes.painel.aoAbrirCanal, aoAlternarCategoria = acoes.painel.aoAlternarCategoria,
                    aoCliqueEmVoz = acoes.painel.aoCliqueEmVoz, aoAbrirMenuServidor = acoes.painel.aoAbrirMenuServidor,
                    aoAbrirMenuStatus = acoes.painel.aoAbrirMenuStatus, aoCriarCanal = acoes.painel.aoCriarCanal,
                    aoAbrirAmigos = acoes.painel.aoAbrirAmigos, aoAbrirDm = acoes.painel.aoAbrirDm,
                ),
                largura = largura - LARGURA_TRILHO.dp,
            )
            if (nav.menu != null) {
                // toque fora do menu fecha
                Box(Modifier.fillMaxSize().clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = acoes.aoFecharMenu))
            }
            if (nav.menu == MenuAberto.Servidor && sv != null) {
                MenuDoServidor(
                    dono = sv.owner == estado.eu?.id,
                    aoConvidar = { acoes.aoConvidar(sv.invite) },
                    aoCriarCanal = { acoes.painel.aoCriarCanal(sv.id, false, null) },
                    aoCriarCategoria = { acoes.painel.aoCriarCanal(sv.id, true, null) },
                    modifier = Modifier.align(Alignment.TopStart).padding(start = 8.dp, top = 50.dp),
                )
            }
            if (nav.menu == MenuAberto.Status) {
                MenuDeStatus(
                    atual = estado.eu?.status?.takeIf { it != Status.Offline } ?: Status.Online,
                    aoEscolher = acoes.aoEscolherStatus, aoSair = acoes.aoPedirSaida,
                    modifier = Modifier.align(Alignment.BottomStart).padding(start = 8.dp, bottom = 68.dp),
                )
            }
        }
    }
}

@Composable
private fun CamadaLateral(progresso: Float, aEsquerda: Boolean, largura: Dp, aoFechar: () -> Unit, conteudo: @Composable () -> Unit) {
    if (progresso <= 0.001f) return
    Box(Modifier.fillMaxSize()) {
        Box(
            Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.55f * progresso))
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = aoFechar),
        )
        val px = with(LocalDensity.current) { largura.toPx() }
        Box(
            Modifier.align(if (aEsquerda) Alignment.CenterStart else Alignment.CenterEnd).width(largura).fillMaxHeight()
                .offset { IntOffset((((if (aEsquerda) progresso - 1f else 1f - progresso)) * px).roundToInt(), 0) },
        ) { conteudo() }
    }
}

/** Folha inferior da mensagem aberta pelo toque longo (ou o seletor de emojis). */
@Composable
private fun FolhaDaMensagem(estado: AppState, id: Long, emojis: Boolean, a: AcoesDaFolha) {
    val canal = estado.canalAberto ?: return
    val m = estado.lista(canal).firstOrNull { it.id == id } ?: return
    val eu = estado.eu ?: return
    val dono = estado.snapshot?.servers?.any { it.owner == eu.id && it.channels.any { c -> c.id == canal } } == true
    val dm = estado.snapshot?.dms?.any { it.channelId == canal } == true
    if (emojis) FolhaDeEmojis(a.aoReagir, a.aoFechar)
    else FolhaDeAcoes(m, minha = m.authorId == eu.id, podeApagar = m.authorId == eu.id || dono, podeFixar = dono || dm, acoes = a)
}
