package app.telacast.tema

import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Shapes
import androidx.compose.ui.unit.dp

/** Raios do desktop: linhas 8, botões e campos 9, menus 10, cartões e itens da barra 12, janelas 14. */
object Formas {
    val Linha = RoundedCornerShape(8.dp)
    val Botao = RoundedCornerShape(9.dp)
    val Menu = RoundedCornerShape(10.dp)
    val Cartao = RoundedCornerShape(12.dp)
    val Janela = RoundedCornerShape(14.dp)
    val Reacao = RoundedCornerShape(8.dp)
    val Selo = RoundedCornerShape(50)

    val ParaMaterial = Shapes(
        extraSmall = Linha, small = Botao, medium = Menu, large = Cartao, extraLarge = Janela,
    )
}
