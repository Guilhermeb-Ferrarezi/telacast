package app.telacast.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.telacast.proto.Message
import app.telacast.tema.Grafite
import app.telacast.tema.Tipos

/** Reações rápidas na folha e o seletor completo (emojis aceitos pelo servidor: sem letras, números nem espaço). */
val REACOES_RAPIDAS = listOf("👍", "❤️", "😂", "🎉", "😮", "😢")
val EMOJIS = listOf(
    "👍", "👎", "❤️", "😂", "🎉", "😮", "😢", "🔥", "👏", "🙏", "😍", "😎",
    "🤔", "👀", "✅", "❌", "💯", "🚀", "⭐", "☕", "😅", "😭", "🥳", "😡",
    "🙌", "💪", "🤝", "😴", "🤯", "🫡", "💀", "✨", "🍕", "🎮", "📌", "💡",
)

class AcoesDaFolha(
    val aoFechar: () -> Unit = {},
    val aoResponder: () -> Unit = {},
    val aoReagir: (String) -> Unit = {},
    val aoMaisEmojis: () -> Unit = {},
    val aoEditar: () -> Unit = {},
    val aoCopiar: () -> Unit = {},
    val aoFixar: () -> Unit = {},
    val aoApagar: () -> Unit = {},
)

/** Folha inferior de uma mensagem (toque longo), desenhada na composição: sem `Dialog`. */
@Composable
private fun FolhaBase(aoFechar: () -> Unit, conteudo: @Composable () -> Unit) {
    Box(Modifier.fillMaxSize()) {
        Box(
            Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.55f))
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = aoFechar),
        )
        val forma = RoundedCornerShape(topStart = 16.dp, topEnd = 16.dp)
        Column(
            Modifier.align(Alignment.BottomCenter).fillMaxWidth().background(Grafite.L2, forma).border(1.dp, Grafite.Linha2, forma)
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = {})
                .windowInsetsPadding(WindowInsets.navigationBars).padding(bottom = 8.dp),
        ) {
            Box(Modifier.align(Alignment.CenterHorizontally).padding(top = 8.dp, bottom = 8.dp).size(width = 36.dp, height = 4.dp).background(Grafite.L4, RoundedCornerShape(2.dp)))
            conteudo()
        }
    }
}

@Composable
fun FolhaDeAcoes(m: Message, minha: Boolean, podeApagar: Boolean, podeFixar: Boolean, acoes: AcoesDaFolha) {
    FolhaBase(acoes.aoFechar) {
        // prévia selecionável: aqui o texto pode ser selecionado e copiado em pedaços (a lista usa toque longo para abrir a folha)
        Column(Modifier.padding(horizontal = 20.dp, vertical = 4.dp)) {
            Text(m.author, style = Tipos.Corpo.copy(fontWeight = FontWeight.SemiBold, fontSize = 14.sp))
            if (m.content.isNotEmpty()) {
                SelectionContainer {
                    Text(m.content, style = Tipos.Corpo.copy(fontSize = 14.sp, color = Grafite.Texto2), maxLines = 4, overflow = TextOverflow.Ellipsis)
                }
            }
        }
        Row(Modifier.fillMaxWidth().padding(horizontal = 14.dp, vertical = 10.dp), horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
            for (e in REACOES_RAPIDAS) {
                val eu = m.reactions.firstOrNull { it.emoji == e }?.me == true
                Toque({ acoes.aoReagir(e) }, Modifier.size(44.dp).border(1.dp, if (eu) Grafite.Ambar else Grafite.Linha2, CircleShape), CircleShape,
                    fundo = if (eu) Grafite.Ambar.copy(alpha = 0.12f) else Grafite.L3, fundoPressionado = Grafite.L4) {
                    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Text(e, style = Tipos.Corpo.copy(fontSize = 20.sp, color = Grafite.Texto)) }
                }
            }
            Toque(acoes.aoMaisEmojis, Modifier.size(44.dp).border(1.dp, Grafite.Linha2, CircleShape), CircleShape, fundo = Grafite.L3, fundoPressionado = Grafite.L4) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { IconeMais(Grafite.Texto2, tamanho = 20.dp) }
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(Grafite.Linha))
        LinhaDaFolha("Responder", acoes.aoResponder)
        if (minha) LinhaDaFolha("Editar mensagem", acoes.aoEditar)
        LinhaDaFolha("Copiar texto", acoes.aoCopiar)
        if (podeFixar) LinhaDaFolha(if (m.pinned) "Desafixar mensagem" else "Fixar mensagem", acoes.aoFixar)
        if (podeApagar) LinhaDaFolha("Apagar mensagem", acoes.aoApagar, Grafite.Vermelho)
    }
}

@Composable
private fun LinhaDaFolha(rotulo: String, aoClicar: () -> Unit, cor: Color = Grafite.Texto) {
    Toque(aoClicar, Modifier.fillMaxWidth().height(52.dp), fundoPressionado = Grafite.L3) {
        Row(Modifier.fillMaxSize().padding(horizontal = 20.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(rotulo, style = Tipos.Corpo.copy(fontSize = 16.sp, color = cor))
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
fun FolhaDeEmojis(aoEscolher: (String) -> Unit, aoFechar: () -> Unit) {
    FolhaBase(aoFechar) {
        Text("Reagir", style = Tipos.Subtitulo.copy(fontSize = 20.sp), modifier = Modifier.padding(horizontal = 20.dp, vertical = 6.dp))
        FlowRow(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp), horizontalArrangement = Arrangement.SpaceBetween, verticalArrangement = Arrangement.spacedBy(4.dp), maxItemsInEachRow = 6) {
            for (e in EMOJIS) {
                Toque({ aoEscolher(e) }, Modifier.size(52.dp), RoundedCornerShape(12.dp), fundoPressionado = Grafite.L3) {
                    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { Text(e, style = Tipos.Corpo.copy(fontSize = 26.sp, color = Grafite.Texto)) }
                }
            }
        }
        Spacer(Modifier.height(4.dp))
    }
}
