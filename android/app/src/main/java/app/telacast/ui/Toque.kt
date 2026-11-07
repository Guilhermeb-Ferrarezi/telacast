package app.telacast.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.collectIsPressedAsState
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.RectangleShape
import androidx.compose.ui.graphics.Shape
import app.telacast.tema.Grafite

/** Área tocável sem ondulação (ripple) do Material: o retorno ao toque é só a troca de camada (L1 -> L2, etc.). */
@Composable
fun Toque(
    aoClicar: () -> Unit,
    modifier: Modifier = Modifier,
    forma: Shape = RectangleShape,
    fundo: Color = Color.Transparent,
    fundoPressionado: Color = Grafite.L2,
    conteudo: @Composable BoxScope.() -> Unit,
) {
    val fonte = remember { MutableInteractionSource() }
    val tocado by fonte.collectIsPressedAsState()
    Box(
        modifier
            .clip(forma)
            .background(if (tocado) fundoPressionado else fundo)
            .clickable(interactionSource = fonte, indication = null, onClick = aoClicar),
        content = conteudo,
    )
}
