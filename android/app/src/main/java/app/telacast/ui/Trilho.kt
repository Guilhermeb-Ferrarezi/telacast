package app.telacast.ui

import androidx.compose.animation.animateColorAsState
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.telacast.data.AppState
import app.telacast.proto.ServerInfo
import app.telacast.tema.Formas
import app.telacast.tema.Grafite
import app.telacast.tema.Tipos

const val LARGURA_TRILHO = 72

/**
 * Item do trilho: quadrado arredondado de 40 dp (raio 12) com a sigla, e uma pílula encostada na borda esquerda
 * (8 dp não lido, 40 dp selecionado). Selecionado fica âmbar (o único acento), igual a `rail_item` do desktop.
 * Em toque não existe "hover", então a altura de 20 dp só aparece no instante do toque (fundo âmbar).
 */
@Composable
fun ItemDoTrilho(
    rotulo: String,
    selecionado: Boolean,
    naoLido: Boolean,
    mencoes: Int,
    aoClicar: () -> Unit,
    modifier: Modifier = Modifier,
    adicionar: Boolean = false,
) {
    val alturaPilula by animateDpAsState(if (selecionado) 40.dp else if (naoLido) 8.dp else 0.dp, tween(200), label = "pilula")
    val fundo by animateColorAsState(if (selecionado) Grafite.Ambar else Grafite.L3, tween(150), label = "fundo")
    val tinta by animateColorAsState(if (selecionado) Grafite.AmbarTinta else Grafite.Texto2, tween(150), label = "tinta")
    Box(modifier.fillMaxWidth().height(40.dp)) {
        if (alturaPilula > 0.dp) {
            Box(
                Modifier.align(Alignment.CenterStart).width(4.dp).height(alturaPilula)
                    .background(Grafite.Texto, RoundedCornerShape(topEnd = 4.dp, bottomEnd = 4.dp)),
            )
        }
        Toque(
            aoClicar, Modifier.align(Alignment.Center).size(40.dp), Formas.Cartao,
            fundo = if (adicionar) androidx.compose.ui.graphics.Color.Transparent else fundo,
            fundoPressionado = Grafite.Ambar,
        ) {
            if (adicionar) {
                Box(Modifier.fillMaxWidth().height(40.dp).border(1.dp, Grafite.Linha2, Formas.Cartao), contentAlignment = Alignment.Center) {
                    Text("+", style = Tipos.Corpo.copy(fontSize = 20.sp, color = Grafite.Apagado, fontWeight = FontWeight.Light))
                }
            } else {
                val n = rotulo.length.coerceIn(1, 7)
                val tam = listOf(18, 18, 16, 16, 14, 12, 10)[n - 1]
                Box(Modifier.fillMaxWidth().height(40.dp), contentAlignment = Alignment.Center) {
                    Text(rotulo, style = Tipos.Corpo.copy(fontSize = tam.sp, lineHeight = (tam + 2).sp, color = tinta, fontWeight = FontWeight.SemiBold), maxLines = 1)
                }
            }
        }
        if (mencoes > 0) {
            // selo âmbar com anel da cor do trilho, no canto inferior direito do ícone
            Box(
                Modifier.align(Alignment.BottomEnd).padding(end = 10.dp).offset(y = 4.dp)
                    .background(Grafite.L0, CircleShape).padding(2.dp),
            ) { SeloNaoLido(mencoes) }
        }
    }
}

/** Trilho de servidores (72 dp, camada L0): Início no topo, filete, servidores e o botão "+". */
@Composable
fun TrilhoDeServidores(
    estado: AppState,
    servidorNoPainel: Long?,
    aoAbrirInicio: () -> Unit,
    aoSelecionar: (Long) -> Unit,
    aoAdicionar: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val sn = estado.snapshot
    val dmsNaoLidas = sn?.dms?.any { it.channelId in estado.naoLidos } == true
    val dmMencoes = sn?.dms?.sumOf { estado.mencoes[it.channelId] ?: 0 } ?: 0
    Column(
        modifier.width(LARGURA_TRILHO.dp).fillMaxHeight().background(Grafite.L0)
            .drawBehind { drawLine(Grafite.Linha, Offset(size.width - 0.5f, 0f), Offset(size.width - 0.5f, size.height), 1f) }
            .padding(vertical = 12.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        ItemDoTrilho("DM", servidorNoPainel == null, dmsNaoLidas, dmMencoes, aoAbrirInicio)
        Box(Modifier.align(Alignment.CenterHorizontally).width(32.dp).height(1.dp).background(Grafite.Linha2))
        LazyColumn(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            items(sn?.servers.orEmpty(), key = { it.id }) { sv ->
                ItemDoTrilho(
                    siglaDe(sv.name), servidorNoPainel == sv.id, sv.channels.any { it.id in estado.naoLidos },
                    sv.channels.sumOf { estado.mencoes[it.id] ?: 0 }, { aoSelecionar(sv.id) },
                )
            }
            item { ItemDoTrilho("+", false, false, 0, aoAdicionar, adicionar = true) }
        }
    }
}

/** Há algo não lido em algum servidor ou DM? (para o ponto do botão da gaveta) */
fun temNovidade(estado: AppState): Boolean = estado.naoLidos.isNotEmpty() || estado.mencoes.isNotEmpty()

internal fun ServerInfo.mencoesTotais(estado: AppState) = channels.sumOf { estado.mencoes[it.id] ?: 0 }
