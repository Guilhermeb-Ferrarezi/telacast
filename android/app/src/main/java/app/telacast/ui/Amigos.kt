package app.telacast.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
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
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.telacast.data.AppState
import app.telacast.data.ItemDeAmigos
import app.telacast.data.ItensDeAmigos
import app.telacast.data.textoVazioDeAmigos
import app.telacast.estado.AbaInicio
import app.telacast.proto.Friend
import app.telacast.proto.FriendStatus
import app.telacast.proto.User
import app.telacast.tema.Formas
import app.telacast.tema.Grafite
import app.telacast.tema.Tipos

class AcoesAmigos(
    val aoEscolherAba: (AbaInicio) -> Unit = {},
    val aoPedirAmizade: (String) -> Unit = {},
    val aoAceitar: (Long) -> Unit = {},
    /** Recusar um pedido recebido ou cancelar um enviado (não pede confirmação). */
    val aoRecusar: (Long) -> Unit = {},
    val aoConversar: (Long) -> Unit = {},
    val aoAbrirPerfil: (usuario: Long, nome: String) -> Unit = { _, _ -> },
    val aoDesbloquear: (Long) -> Unit = {},
)

/** Início (amigos): título, abas (Disponível, Todos, Pendente, Bloqueados, Adicionar amigo), busca e listas por seção. */
@Composable
fun TelaDeAmigos(estado: AppState, aba: AbaInicio, aoMenu: () -> Unit, a: AcoesAmigos, modifier: Modifier = Modifier, filtroInicial: String = "") {
    val amigos = estado.snapshot?.friends.orEmpty()
    val bloqueados = estado.snapshot?.blocked.orEmpty()
    val pendentes = amigos.count { it.status == FriendStatus.Incoming }
    var filtro by remember { mutableStateOf(filtroInicial) }
    Column(modifier.fillMaxSize().background(Grafite.L2)) {
        BarraSuperior(aoMenu, temNovidade(estado)) { Text("Amigos", style = Tipos.Titulo.copy(fontSize = 20.sp, lineHeight = 26.sp)) }
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
        if (aba == AbaInicio.Adicionar) { AdicionarAmigo(a.aoPedirAmizade); return@Column }

        val itens = ItensDeAmigos.montar(amigos, bloqueados, aba, filtro)
        if (amigos.isNotEmpty() || bloqueados.isNotEmpty()) {
            Box(Modifier.padding(horizontal = 16.dp).padding(bottom = 6.dp)) { Campo(filtro, { filtro = it }, "Buscar") }
        }
        if (itens.isEmpty()) {
            EstadoVazio(textoVazioDeAmigos(aba, filtro.isNotBlank()), if (aba == AbaInicio.Todos && filtro.isBlank()) "Adicionar amigo" else null) { a.aoEscolherAba(AbaInicio.Adicionar) }
        } else {
            LazyColumn(contentPadding = androidx.compose.foundation.layout.PaddingValues(bottom = 16.dp)) {
                items(itens, key = { it.chave }) { item ->
                    when (item) {
                        is ItemDeAmigos.Cabecalho -> Rotulo(item.rotulo, Modifier.padding(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 6.dp))
                        is ItemDeAmigos.Pessoa -> LinhaDeAmigo(item.amigo, a)
                        is ItemDeAmigos.Bloqueado -> LinhaDeBloqueado(item.usuario, a)
                    }
                }
            }
        }
    }
}

/** Estado vazio sem ilustração: um anel de traço fino e o texto (regra Grafite). */
@Composable
fun EstadoVazio(texto: String, acao: String? = null, aoAcao: () -> Unit = {}) {
    Column(Modifier.fillMaxWidth().padding(horizontal = 32.dp, vertical = 40.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Canvas(Modifier.size(56.dp)) {
            val u = density
            drawCircle(Grafite.Linha2, 26f * u, center, style = Stroke(1.5f * u))
            drawCircle(Grafite.Linha2, 3f * u, center)
        }
        Spacer(Modifier.height(16.dp))
        Text(texto, style = Tipos.Corpo.copy(fontSize = 14.sp, lineHeight = 20.sp, color = Grafite.Texto2), textAlign = TextAlign.Center, modifier = Modifier.widthLimite())
        if (acao != null) { Spacer(Modifier.height(16.dp)); Botao(acao, TipoBotao.Primario, aoClicar = aoAcao) }
    }
}

private fun Modifier.widthLimite() = this.then(Modifier.fillMaxWidth())

@Composable
private fun BotaoRedondo(aoClicar: () -> Unit, modifier: Modifier = Modifier, conteudo: @Composable () -> Unit) {
    Toque(aoClicar, modifier.size(40.dp).border(1.dp, Grafite.Linha2, CircleShape), CircleShape, fundo = Grafite.L3, fundoPressionado = Grafite.L4) {
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { conteudo() }
    }
}

/** Linha de pessoa (64 dp): avatar 40 com presença, nome, subtexto e botões redondos; tocar abre a conversa. */
@Composable
private fun LinhaDeAmigo(f: Friend, a: AcoesAmigos) {
    val u = f.user
    Toque({ if (f.status == FriendStatus.Friend) a.aoConversar(u.id) else a.aoAbrirPerfil(u.id, u.name) }, Modifier.fillMaxWidth().height(64.dp), fundoPressionado = Grafite.L1) {
        Row(Modifier.fillMaxSize().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
            Toque({ a.aoAbrirPerfil(u.id, u.name) }, Modifier.size(40.dp), CircleShape, fundoPressionado = Color.Transparent) { Avatar(u.name, u.id, 40.dp, u.status.presenca(), Grafite.L2) }
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Text(u.name, style = Tipos.Corpo.copy(fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(
                    when (f.status) {
                        FriendStatus.Friend -> if (u.online) u.status.rotulo() else "Offline"
                        FriendStatus.Incoming -> "Pedido de amizade recebido"
                        FriendStatus.Outgoing -> "Pedido de amizade enviado"
                    },
                    style = Tipos.Corpo.copy(fontSize = 12.sp, lineHeight = 16.sp, color = Grafite.Apagado), maxLines = 1, overflow = TextOverflow.Ellipsis,
                )
            }
            Spacer(Modifier.width(8.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                when (f.status) {
                    FriendStatus.Friend -> {
                        BotaoRedondo({ a.aoConversar(u.id) }) { IconeBalao(Grafite.Texto2) }
                        BotaoRedondo({ a.aoAbrirPerfil(u.id, u.name) }) { IconeMais3(Grafite.Texto2) }
                    }
                    FriendStatus.Incoming -> {
                        BotaoRedondo({ a.aoAceitar(u.id) }) { IconeConfirmar(Grafite.Ok) }
                        BotaoRedondo({ a.aoRecusar(u.id) }) { IconeFechar(Grafite.Vermelho) }
                    }
                    FriendStatus.Outgoing -> BotaoRedondo({ a.aoRecusar(u.id) }) { IconeFechar(Grafite.Vermelho) }
                }
            }
        }
    }
}

@Composable
private fun LinhaDeBloqueado(u: User, a: AcoesAmigos) {
    Row(Modifier.fillMaxWidth().height(64.dp).padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
        Avatar(u.name, u.id, 40.dp, null, Grafite.L2)
        Spacer(Modifier.width(12.dp))
        Text(u.name, style = Tipos.Corpo.copy(fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
        Botao("Desbloquear", TipoBotao.Quieto, altura = 36.dp, aoClicar = { a.aoDesbloquear(u.id) })
    }
}

@Composable
private fun AdicionarAmigo(aoPedir: (String) -> Unit) {
    var nome by remember { mutableStateOf("") }
    val pode = nome.trim().trimStart('@').isNotEmpty()
    fun enviar() { if (pode) { aoPedir(nome.trim().trimStart('@')); nome = "" } }
    Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) {
        Text("Adicionar amigo", style = Tipos.Subtitulo.copy(fontSize = 20.sp))
        Text("Você pode adicionar amigos com o nome de usuário deles.", style = Tipos.CorpoApagado, modifier = Modifier.padding(top = 6.dp, bottom = 14.dp))
        Campo(nome, { nome = it }, "Insira um nome de usuário", aoEnviar = ::enviar)
        Spacer(Modifier.height(12.dp))
        Botao("Enviar pedido de amizade", TipoBotao.Primario, Modifier.fillMaxWidth(), habilitado = pode, aoClicar = ::enviar)
    }
}
