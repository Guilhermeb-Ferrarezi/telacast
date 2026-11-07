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
        if (acao != null) Box(Modifier.size(48.dp), contentAlignment = Alignment.Center) { acao() }
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
        acaoDireita = { Toque(aoMembros, Modifier.size(48.dp), Formas.Cartao, fundoPressionado = Grafite.L3) { Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { IconeMembros(Grafite.Texto2) } } },
    ) {
        Text("#", style = Tipos.Titulo.copy(fontSize = 22.sp, color = Grafite.Apagado))
        Spacer(Modifier.width(6.dp))
        Text(canal.name, style = Tipos.Titulo.copy(fontSize = 20.sp, lineHeight = 26.sp), maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

@Composable
fun TelaDeDm(estado: AppState, dm: Dm, aoMenu: () -> Unit, dados: DadosDaConversa, acoes: AcoesDaConversa, modifier: Modifier = Modifier) {
    TelaDeConversa(estado, dm.channelId, "Conversar com @${dm.user.name}", aoMenu, dados, acoes, modifier) {
        Avatar(dm.user.name, dm.user.id, 28.dp, dm.user.status.presenca(), Grafite.L2)
        Spacer(Modifier.width(10.dp))
        Text(dm.user.name, style = Tipos.Titulo.copy(fontSize = 20.sp, lineHeight = 26.sp), maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** Painel de membros (lado direito). Por ora a lista simples do servidor; cargos e perfil chegam no B8. */
@Composable
fun PainelDeMembros(sv: ServerInfo?, modifier: Modifier = Modifier) {
    Column(modifier.fillMaxSize().background(Grafite.L1)) {
        val membros = sv?.members.orEmpty().sortedWith(compareByDescending<User> { it.online }.thenBy { it.name.lowercase() })
        Box(Modifier.fillMaxWidth().height(56.dp).padding(horizontal = 16.dp).drawBehind { drawLine(Grafite.Linha, Offset(0f, size.height - 0.5f), Offset(size.width, size.height - 0.5f), 1f) }, contentAlignment = Alignment.CenterStart) {
            Rotulo("Membros · ${membros.size}")
        }
        LazyColumn(contentPadding = androidx.compose.foundation.layout.PaddingValues(8.dp)) {
            items(membros, key = { it.id }) { u ->
                Row(Modifier.fillMaxWidth().height(48.dp).padding(horizontal = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Avatar(u.name, u.id, 32.dp, u.status.presenca(), Grafite.L1)
                    Spacer(Modifier.width(12.dp))
                    Text(u.name, style = Tipos.Corpo.copy(color = if (u.online) Grafite.Texto else Grafite.Apagado), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
    }
}

class AcoesAmigos(
    val aoEscolherAba: (AbaInicio) -> Unit = {},
    val aoPedirAmizade: (String) -> Unit = {},
    val aoAceitar: (Long) -> Unit = {},
    val aoRemover: (Long) -> Unit = {},
    val aoConversar: (Long) -> Unit = {},
)

/** Início: título, abas (Disponível, Todos, Pendente, Adicionar amigo) e a lista. */
@Composable
fun TelaDeAmigos(estado: AppState, aba: AbaInicio, aoMenu: () -> Unit, a: AcoesAmigos, modifier: Modifier = Modifier) {
    val amigos = estado.snapshot?.friends.orEmpty()
    val pendentes = amigos.count { it.status != FriendStatus.Friend }
    Column(modifier.fillMaxSize().background(Grafite.L2)) {
        BarraSuperior(aoMenu, temNovidade(estado)) {
            Text("Amigos", style = Tipos.Titulo.copy(fontSize = 20.sp, lineHeight = 26.sp))
        }
        Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp, vertical = 10.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            for (ab in AbaInicio.entries) {
                val rotulo = if (ab == AbaInicio.Pendente && pendentes > 0) "Pendente ($pendentes)" else ab.rotulo
                val sel = ab == aba
                Toque({ a.aoEscolherAba(ab) }, Modifier.height(34.dp), Formas.Selo, fundo = if (sel) Grafite.L4 else Grafite.L2, fundoPressionado = Grafite.L3) {
                    Box(Modifier.padding(horizontal = 14.dp).height(34.dp), contentAlignment = Alignment.Center) {
                        Text(rotulo, style = Tipos.Botao.copy(color = if (sel) Grafite.Texto else Grafite.Apagado, fontSize = 13.sp))
                    }
                }
            }
        }
        if (aba == AbaInicio.Adicionar) {
            AdicionarAmigo(a.aoPedirAmizade)
            return@Column
        }
        val lista = amigos.filter {
            when (aba) {
                AbaInicio.Disponivel -> it.status == FriendStatus.Friend && it.user.online
                AbaInicio.Todos -> it.status == FriendStatus.Friend
                AbaInicio.Pendente -> it.status != FriendStatus.Friend
                AbaInicio.Adicionar -> false
            }
        }.sortedBy { it.user.name.lowercase() }
        if (lista.isEmpty()) {
            Text(
                when (aba) {
                    AbaInicio.Disponivel -> "Ninguém disponível agora."
                    AbaInicio.Todos -> "Você ainda não tem amigos. Use \"Adicionar amigo\"."
                    else -> "Nenhum pedido pendente."
                },
                style = Tipos.CorpoApagado, modifier = Modifier.padding(20.dp),
            )
        } else {
            LazyColumn(contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 12.dp)) {
                items(lista, key = { it.user.id }) { f -> LinhaDeAmigo(f, a) }
            }
        }
    }
}

@Composable
private fun LinhaDeAmigo(f: Friend, a: AcoesAmigos) {
    val u = f.user
    Row(Modifier.fillMaxWidth().height(64.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        Avatar(u.name, u.id, 40.dp, u.status.presenca(), Grafite.L2)
        Spacer(Modifier.width(12.dp))
        Column(Modifier.weight(1f)) {
            Text(u.name, style = Tipos.Corpo.copy(fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                when (f.status) {
                    FriendStatus.Friend -> if (u.online) u.status.rotulo() else "Offline"
                    FriendStatus.Incoming -> "Pedido recebido"
                    FriendStatus.Outgoing -> "Pedido enviado"
                },
                style = Tipos.Corpo.copy(fontSize = 12.sp, lineHeight = 16.sp, color = Grafite.Apagado), maxLines = 1, overflow = TextOverflow.Ellipsis,
            )
        }
        when (f.status) {
            FriendStatus.Friend -> Botao("Mensagem", TipoBotao.Primario, aoClicar = { a.aoConversar(u.id) }, altura = 36.dp)
            FriendStatus.Incoming -> Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Botao("Recusar", TipoBotao.Perigo, aoClicar = { a.aoRemover(u.id) }, altura = 36.dp)
                Botao("Aceitar", TipoBotao.Primario, aoClicar = { a.aoAceitar(u.id) }, altura = 36.dp)
            }
            FriendStatus.Outgoing -> Botao("Cancelar", TipoBotao.Quieto, aoClicar = { a.aoRemover(u.id) }, altura = 36.dp)
        }
    }
}

@Composable
private fun AdicionarAmigo(aoPedir: (String) -> Unit) {
    var nome by remember { mutableStateOf("") }
    Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) {
        Rotulo("Adicionar amigo")
        Text("Você pode adicionar amigos com o nome de usuário deles.", style = Tipos.CorpoApagado, modifier = Modifier.padding(top = 6.dp, bottom = 14.dp))
        Campo(nome, { nome = it }, "Digite um nome de usuário", aoEnviar = { aoPedir(nome); nome = "" })
        Spacer(Modifier.height(12.dp))
        Botao("Enviar pedido", TipoBotao.Primario, Modifier.fillMaxWidth(), aoClicar = { aoPedir(nome); nome = "" })
    }
}
