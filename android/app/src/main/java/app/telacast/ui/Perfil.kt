package app.telacast.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.telacast.data.Formatacao
import app.telacast.data.contaCriadaEm
import app.telacast.proto.FriendStatus
import app.telacast.proto.User
import app.telacast.tema.Formas
import app.telacast.tema.Grafite
import app.telacast.tema.Tipos

class AcoesDoPerfil(
    val aoFechar: () -> Unit = {},
    val aoConversar: (Long) -> Unit = {},
    val aoAdicionar: (nome: String) -> Unit = {},
    val aoAceitar: (Long) -> Unit = {},
    /** Desfazer amizade (pede confirmação). */
    val aoDesfazerAmizade: (Long, String) -> Unit = { _, _ -> },
    /** Cancelar pedido enviado ou recusar recebido (sem confirmação). */
    val aoCancelarPedido: (Long) -> Unit = {},
    val aoBloquear: (Long, String) -> Unit = { _, _ -> },
    val aoDesbloquear: (Long) -> Unit = {},
    val aoCopiarId: (Long) -> Unit = {},
    val aoMinhaConta: () -> Unit = {},
)

/**
 * Perfil em folha inferior (desktop: `profile_popout`): faixa lisa na cor da pessoa, avatar de 80 com presença, nome,
 * `@usuário`, status, "no Telacast desde" e as ações (Mensagem, amizade, bloquear). Sem imagem de capa nem brilho.
 */
@Composable
fun FolhaDePerfil(u: User, relacao: FriendStatus?, bloqueado: Boolean, euMesmo: Boolean, dono: Boolean, a: AcoesDoPerfil) {
    FolhaBase(a.aoFechar, comAlca = false) {
        val (bg, fg) = Grafite.tomDoUsuario(u.id)
        Box(Modifier.fillMaxWidth()) {
            Box(Modifier.fillMaxWidth().height(96.dp).background(lerp(bg, fg, 0.18f)))
            Box(Modifier.padding(start = 20.dp).offset(y = 56.dp).background(Grafite.L2, CircleShape).padding(6.dp)) {
                Avatar(u.name, u.id, 80.dp, u.status.presenca(), Grafite.L2)
            }
        }
        Column(Modifier.padding(start = 20.dp, end = 20.dp, top = 52.dp, bottom = 12.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(u.name, style = Tipos.Subtitulo.copy(fontSize = 24.sp, lineHeight = 28.sp))
                if (dono) { Spacer(Modifier.size(8.dp)); Text("dono", style = Tipos.Pequeno.copy(fontSize = 11.sp, color = Grafite.Ambar)) }
            }
            Text("@${u.name}", style = Tipos.Corpo.copy(fontSize = 13.5.sp, color = Grafite.Texto2))
            Text(u.status.rotulo(), style = Tipos.Corpo.copy(fontSize = 13.sp, color = Grafite.Apagado), modifier = Modifier.padding(top = 2.dp))
            Column(Modifier.padding(top = 14.dp).fillMaxWidth().background(Grafite.L3, Formas.Linha).border(1.dp, Grafite.Linha, Formas.Linha).padding(12.dp)) {
                Rotulo("No Telacast desde")
                Text(Formatacao.data(contaCriadaEm(u.id)), style = Tipos.Corpo.copy(fontSize = 14.sp), modifier = Modifier.padding(top = 4.dp))
            }
            Spacer(Modifier.height(14.dp))
            if (euMesmo) {
                Botao("Ajustes e conta", TipoBotao.Primario, Modifier.fillMaxWidth(), aoClicar = a.aoMinhaConta)
            } else {
                Botao(if (bloqueado) "Mensagem indisponível (bloqueado)" else "Mensagem", TipoBotao.Primario, Modifier.fillMaxWidth(), habilitado = !bloqueado, aoClicar = { a.aoConversar(u.id) })
                Spacer(Modifier.height(8.dp))
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    when (relacao) {
                        null -> Botao("Adicionar amigo", TipoBotao.Quieto, Modifier.weight(1f), habilitado = !bloqueado, aoClicar = { a.aoAdicionar(u.name) })
                        FriendStatus.Friend -> Botao("Desfazer amizade", TipoBotao.Quieto, Modifier.weight(1f), aoClicar = { a.aoDesfazerAmizade(u.id, u.name) })
                        FriendStatus.Incoming -> Botao("Aceitar pedido", TipoBotao.Quieto, Modifier.weight(1f), aoClicar = { a.aoAceitar(u.id) })
                        FriendStatus.Outgoing -> Botao("Cancelar pedido", TipoBotao.Quieto, Modifier.weight(1f), aoClicar = { a.aoCancelarPedido(u.id) })
                    }
                    if (bloqueado) Botao("Desbloquear", TipoBotao.Quieto, Modifier.weight(1f), aoClicar = { a.aoDesbloquear(u.id) })
                    else Botao("Bloquear", TipoBotao.Perigo, Modifier.weight(1f), aoClicar = { a.aoBloquear(u.id, u.name) })
                }
                Spacer(Modifier.height(4.dp))
                Toque({ a.aoCopiarId(u.id) }, Modifier.fillMaxWidth().height(44.dp), Formas.Linha, fundoPressionado = Grafite.L3) {
                    Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { Text("Copiar ID", style = Tipos.Botao.copy(color = Grafite.Texto2, fontWeight = FontWeight.Medium, fontSize = 13.sp)) }
                }
            }
        }
    }
}
