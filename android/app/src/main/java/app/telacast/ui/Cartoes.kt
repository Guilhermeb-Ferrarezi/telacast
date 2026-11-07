package app.telacast.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.telacast.data.Formatacao
import app.telacast.proto.Message
import app.telacast.tema.Formas
import app.telacast.tema.Grafite
import app.telacast.tema.Tipos

/** Tela cheia com barra (voltar, título em serifa) usada por Fixadas, Busca e Ajustes. */
@Composable
fun TelaCheia(titulo: String, aoFechar: () -> Unit, modifier: Modifier = Modifier, conteudo: @Composable () -> Unit) {
    Column(modifier.fillMaxSize().background(Grafite.L2).windowInsetsPadding(WindowInsets.statusBars)) {
        Row(
            Modifier.fillMaxWidth().height(56.dp).drawBehind { drawLine(Grafite.Linha, Offset(0f, size.height - 0.5f), Offset(size.width, size.height - 0.5f), 1f) }.padding(horizontal = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Toque(aoFechar, Modifier.size(48.dp), Formas.Cartao, fundoPressionado = Grafite.L3) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { IconeVoltar(Grafite.Texto2) }
            }
            Text(titulo, style = Tipos.Titulo.copy(fontSize = 20.sp, lineHeight = 26.sp), modifier = Modifier.padding(start = 4.dp), maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        conteudo()
    }
}

/**
 * Cartão de uma mensagem fora da conversa (fixadas, resultado de busca): local em cinza (`# canal`), avatar de 32,
 * autor, dia e hora, e o texto em Markdown. Tocar leva até a mensagem.
 */
@Composable
fun CartaoDeMensagem(m: Message, local: String?, amb: AmbienteMd, zona: java.time.ZoneId, agoraMs: Long, aoClicar: () -> Unit) {
    Toque(aoClicar, Modifier.fillMaxWidth(), fundoPressionado = Grafite.L1) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp)) {
            if (local != null) Text(local, style = Tipos.Pequeno.copy(fontSize = 12.sp, color = Grafite.Apagado), modifier = Modifier.padding(bottom = 6.dp), maxLines = 1, overflow = TextOverflow.Ellipsis)
            Row {
                Avatar(m.author, m.authorId, 32.dp, null, Grafite.L2)
                Spacer(Modifier.width(10.dp))
                Column(Modifier.weight(1f)) {
                    Row(verticalAlignment = Alignment.Bottom) {
                        Text(m.author, style = Tipos.Corpo.copy(fontWeight = FontWeight.SemiBold, fontSize = 14.5.sp), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
                        Spacer(Modifier.width(8.dp))
                        Text("${Formatacao.dia(m.ts, agoraMs, zona)} ${Formatacao.hora(m.ts, zona)}", style = Tipos.Pequeno.copy(fontSize = 11.5.sp, color = Grafite.Apagado))
                    }
                    if (m.content.isNotEmpty()) TextoMarkdown(m.content, amb, AcoesMd(), tamanho = 14.5.sp, editada = m.edited)
                    else Text("(anexo)", style = Tipos.CorpoApagado.copy(fontSize = 14.sp))
                }
            }
        }
    }
    Box(Modifier.fillMaxWidth().height(1.dp).background(Grafite.Linha))
}
