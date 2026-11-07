package app.telacast.ui

import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import app.telacast.proto.Message
import app.telacast.tema.Grafite
import app.telacast.tema.Tipos

/** Mensagens fixadas do canal (a mais recente primeiro); tocar numa leva até ela na conversa. */
@Composable
fun TelaDeFixadas(
    mensagens: List<Message>,
    amb: AmbienteMd,
    agoraMs: Long,
    zona: java.time.ZoneId,
    aoFechar: () -> Unit,
    aoIr: (canal: Long, mensagem: Long) -> Unit,
) {
    TelaCheia("Mensagens fixadas", aoFechar) {
        if (mensagens.isEmpty()) {
            EstadoVazio("Este canal não tem mensagens fixadas… por enquanto.")
        } else {
            LazyColumn {
                items(mensagens, key = { it.id }) { m -> CartaoDeMensagem(m, null, amb, zona, agoraMs) { aoIr(m.channelId, m.id) } }
            }
        }
    }
}
