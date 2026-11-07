package app.telacast.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.telacast.tema.Formas
import app.telacast.tema.Grafite
import app.telacast.tema.Tipos

enum class Presenca { Online, Ausente, NaoPerturbe, Offline }

/** Rótulo de seção: pequeno, em caixa alta, apagado. */
@Composable
fun Rotulo(texto: String, modifier: Modifier = Modifier) {
    Text(texto.uppercase(), style = Tipos.Rotulo, modifier = modifier)
}

/** Filete horizontal de 1 px. */
@Composable
fun Filete(modifier: Modifier = Modifier, cor: Color = Grafite.Linha) {
    Box(modifier.fillMaxWidth().height(1.dp).background(cor))
}

/** Avatar redondo, cor suave por usuário, inicial no centro; `fundo` é a camada atrás (contorno da presença). */
@Composable
fun Avatar(nome: String, id: Long, tamanho: Dp = 40.dp, presenca: Presenca? = null, fundo: Color = Grafite.L2, modifier: Modifier = Modifier) {
    val (bg, fg) = Grafite.tomDoUsuario(id)
    val inicial = nome.trim().firstOrNull()?.uppercase() ?: "?"
    Box(
        modifier
            .size(tamanho)
            .drawBehind {
                drawCircle(bg)
                if (presenca != null) desenharPresenca(presenca, fundo, tamanho.toPx())
            },
        contentAlignment = Alignment.Center,
    ) {
        Text(inicial, style = Tipos.Corpo.copy(fontWeight = FontWeight.SemiBold, fontSize = (tamanho.value * 0.4f).sp, color = fg, lineHeight = (tamanho.value * 0.5f).sp))
    }
}

/** Bolinha de presença: online cheia, ausente em meia-lua, não perturbe com barra, offline vazada. */
private fun androidx.compose.ui.graphics.drawscope.DrawScope.desenharPresenca(p: Presenca, fundo: Color, lado: Float) {
    val r = lado * 0.11f
    val c = Offset(lado - lado * 0.12f - r, lado - lado * 0.12f - r)
    drawCircle(fundo, r * 1.55f, c)
    when (p) {
        Presenca.Online -> drawCircle(Grafite.Ok, r, c)
        Presenca.Ausente -> {
            drawCircle(Grafite.Ambar, r, c)
            drawCircle(fundo, r * 0.62f, c + Offset(-r * 0.45f, -r * 0.45f))
        }
        Presenca.NaoPerturbe -> {
            drawCircle(Grafite.Vermelho, r, c)
            drawRect(fundo, c - Offset(r * 0.6f, r * 0.25f), androidx.compose.ui.geometry.Size(r * 1.2f, r * 0.5f))
        }
        Presenca.Offline -> {
            drawCircle(Grafite.Apagado, r, c)
            drawCircle(fundo, r * 0.55f, c)
        }
    }
}

enum class TipoBotao { Primario, Perigo, Quieto }

/** Botão de 40 dp de altura, raio 9. Primário âmbar (esquenta ao toque), perigo vermelho, quieto com filete. */
@Composable
fun Botao(rotulo: String, tipo: TipoBotao = TipoBotao.Primario, modifier: Modifier = Modifier, aoClicar: () -> Unit = {}) {
    val fonte = remember { MutableInteractionSource() }
    val tocado by fonte.collectIsPressedAsState()
    val (fundo, tinta, borda) = when (tipo) {
        TipoBotao.Primario -> Triple(if (tocado) Grafite.AmbarQuente else Grafite.Ambar, Grafite.AmbarTinta, null)
        TipoBotao.Perigo -> Triple(if (tocado) Grafite.Vermelho.copy(alpha = 0.12f) else Color.Transparent, Grafite.Vermelho, BorderStroke(1.dp, if (tocado) Grafite.Vermelho else Color(0xFF6B4D49)))
        TipoBotao.Quieto -> Triple(if (tocado) Grafite.L3 else Color.Transparent, if (tocado) Grafite.Texto else Grafite.Texto2, BorderStroke(1.dp, Grafite.Linha2))
    }
    Box(
        modifier
            .height(40.dp)
            .clip(Formas.Botao)
            .background(fundo)
            .let { if (borda != null) it.border(borda, Formas.Botao) else it }
            .clickable(interactionSource = fonte, indication = null, onClick = aoClicar)
            .padding(horizontal = 16.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(rotulo, style = Tipos.Botao.copy(color = tinta, fontSize = 14.sp), textAlign = TextAlign.Center)
    }
}

/** Campo de texto: camada 3, filete de 1 px (âmbar com foco), cursor âmbar. */
@Composable
fun Campo(valor: String, aoMudar: (String) -> Unit, dica: String, modifier: Modifier = Modifier, focado: Boolean = false) {
    BasicTextField(
        value = valor,
        onValueChange = aoMudar,
        singleLine = true,
        textStyle = Tipos.Corpo,
        cursorBrush = SolidColor(Grafite.Ambar),
        modifier = modifier
            .fillMaxWidth()
            .height(44.dp)
            .clip(Formas.Botao)
            .background(Grafite.L3)
            .border(1.dp, if (focado) Grafite.Ambar else Grafite.Linha2, Formas.Botao),
        decorationBox = { interno ->
            Box(Modifier.padding(horizontal = 14.dp), contentAlignment = Alignment.CenterStart) {
                if (valor.isEmpty()) Text(dica, style = Tipos.CorpoApagado)
                interno()
            }
        },
    )
}

/** Selo âmbar de não lido: mínimo 16 dp, 22 dp com dois dígitos, 28 dp com "99+". */
@Composable
fun SeloNaoLido(n: Int, modifier: Modifier = Modifier) {
    val texto = if (n > 99) "99+" else n.toString()
    val largura = if (n < 10) 16.dp else if (n < 100) 22.dp else 28.dp
    Box(modifier.width(largura).height(16.dp).clip(Formas.Selo).background(Grafite.Ambar), contentAlignment = Alignment.Center) {
        Text(texto, style = Tipos.Selo.copy(fontSize = 11.sp, lineHeight = 14.sp))
    }
}

/** Linha de canal: selecionada na camada 3; não lida em negrito com selo. */
@Composable
fun LinhaDeCanal(nome: String, selecionado: Boolean = false, naoLidas: Int = 0, modifier: Modifier = Modifier) {
    val naoLido = naoLidas > 0
    Row(
        modifier
            .fillMaxWidth()
            .height(34.dp)
            .clip(Formas.Linha)
            .background(if (selecionado) Grafite.L3 else Color.Transparent)
            .padding(horizontal = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text("#", style = Tipos.Corpo.copy(color = Grafite.Apagado))
        Spacer(Modifier.width(10.dp))
        Text(
            nome,
            style = Tipos.Corpo.copy(
                color = if (selecionado || naoLido) Grafite.Texto else Grafite.Texto2,
                fontWeight = if (naoLido) FontWeight.SemiBold else FontWeight.Normal,
            ),
            modifier = Modifier.weight(1f),
        )
        if (naoLido) SeloNaoLido(naoLidas)
    }
}
