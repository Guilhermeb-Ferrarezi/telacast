package app.telacast.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.telacast.data.AppState
import app.telacast.estado.AbaInicio
import app.telacast.proto.Channel
import app.telacast.proto.Dm
import app.telacast.proto.Friend
import app.telacast.proto.FriendStatus
import app.telacast.proto.ServerInfo
import app.telacast.proto.Status
import app.telacast.proto.User
import app.telacast.tema.Formas
import app.telacast.tema.Grafite
import app.telacast.tema.Tipos

/** Barra do topo do centro (56 dp, camada L2, filete embaixo): botão da gaveta, título e uma ação à direita. */
@Composable
fun BarraSuperior(
    aoMenu: () -> Unit,
    novidade: Boolean,
    modifier: Modifier = Modifier,
    acao: (@Composable () -> Unit)? = null,
    titulo: @Composable RowScope.() -> Unit,
) {
    Row(
        modifier.fillMaxWidth().height(56.dp).background(Grafite.L2)
            .drawBehind { drawLine(Grafite.Linha, Offset(0f, size.height - 0.5f), Offset(size.width, size.height - 0.5f), 1f) }
            .padding(start = 4.dp, end = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Toque(aoMenu, Modifier.size(48.dp), Formas.Cartao, fundoPressionado = Grafite.L3) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { IconeMenu(Grafite.Texto2) }
            if (novidade) Box(Modifier.align(Alignment.TopEnd).padding(top = 10.dp, end = 10.dp).background(Grafite.L2, CircleShape).padding(2.dp).size(8.dp).background(Grafite.Ambar, CircleShape))
        }
        Row(Modifier.weight(1f).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically, content = titulo)
        if (acao != null) Box(Modifier.height(48.dp), contentAlignment = Alignment.Center) { acao() }
    }
}

/** Canal de texto aberto: barra com o nome e o botão de membros, lista de mensagens e compositor. */
@Composable
fun TelaDeCanal(
    estado: AppState, canal: Channel, aoMenu: () -> Unit, aoMembros: () -> Unit,
    dados: DadosDaConversa, acoes: AcoesDaConversa, modifier: Modifier = Modifier,
) {
    TelaDeConversa(
        estado, canal.id, "Mensagem em #${canal.name}", aoMenu, dados, acoes, modifier,
        acaoDireita = {
            Row {
                Toque(acoes.aoAbrirBusca, Modifier.size(44.dp), Formas.Cartao, fundoPressionado = Grafite.L3) { Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { IconeLupa(Grafite.Texto2) } }
                Toque(acoes.aoAbrirFixadas, Modifier.size(44.dp), Formas.Cartao, fundoPressionado = Grafite.L3) { Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { IconeAlfinete(Grafite.Texto2) } }
                Toque(aoMembros, Modifier.size(44.dp), Formas.Cartao, fundoPressionado = Grafite.L3) { Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { IconeMembros(Grafite.Texto2) } }
            }
        },
    ) {
        Text("#", style = Tipos.Titulo.copy(fontSize = 22.sp, color = Grafite.Apagado))
        Spacer(Modifier.width(6.dp))
        Text(canal.name, style = Tipos.Titulo.copy(fontSize = 20.sp, lineHeight = 26.sp), maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

@Composable
fun TelaDeDm(estado: AppState, dm: Dm, aoMenu: () -> Unit, dados: DadosDaConversa, acoes: AcoesDaConversa, modifier: Modifier = Modifier) {
    TelaDeConversa(
        estado, dm.channelId, "Conversar com @${dm.user.name}", aoMenu, dados, acoes, modifier,
        acaoDireita = {
            Row {
                Toque(acoes.aoAbrirBusca, Modifier.size(44.dp), Formas.Cartao, fundoPressionado = Grafite.L3) { Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { IconeLupa(Grafite.Texto2) } }
                Toque(acoes.aoAbrirFixadas, Modifier.size(44.dp), Formas.Cartao, fundoPressionado = Grafite.L3) { Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { IconeAlfinete(Grafite.Texto2) } }
            }
        },
    ) {
        Avatar(dm.user.name, dm.user.id, 28.dp, dm.user.status.presenca(), Grafite.L2)
        Spacer(Modifier.width(10.dp))
        Text(dm.user.name, style = Tipos.Titulo.copy(fontSize = 20.sp, lineHeight = 26.sp), maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** Painel de membros (lado direito). Por ora a lista simples do servidor; cargos e perfil chegam no B8. */
@Composable
fun PainelDeMembros(sv: ServerInfo?, modifier: Modifier = Modifier, aoAbrirPerfil: (Long, String) -> Unit = { _, _ -> }) {
    Column(modifier.fillMaxSize().background(Grafite.L1)) {
        val membros = sv?.members.orEmpty().sortedWith(compareByDescending<User> { it.online }.thenBy { it.name.lowercase() })
        Box(Modifier.fillMaxWidth().height(56.dp).padding(horizontal = 16.dp).drawBehind { drawLine(Grafite.Linha, Offset(0f, size.height - 0.5f), Offset(size.width, size.height - 0.5f), 1f) }, contentAlignment = Alignment.CenterStart) {
            Rotulo("Membros · ${membros.size}")
        }
        LazyColumn(contentPadding = androidx.compose.foundation.layout.PaddingValues(8.dp)) {
            items(membros, key = { it.id }) { u ->
                Toque({ aoAbrirPerfil(u.id, u.name) }, Modifier.fillMaxWidth().height(48.dp), Formas.Linha, fundoPressionado = Grafite.L2) {
                  Row(Modifier.fillMaxSize().padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Avatar(u.name, u.id, 32.dp, u.status.presenca(), Grafite.L1)
                    Spacer(Modifier.width(12.dp))
                    Text(u.name, style = Tipos.Corpo.copy(color = if (u.online) Grafite.Texto else Grafite.Apagado), maxLines = 1, overflow = TextOverflow.Ellipsis)
                  }
                }
            }
        }
    }
}
