@file:OptIn(ExperimentalTextApi::class)

package app.telacast.tema

import androidx.compose.material3.Typography
import androidx.compose.ui.text.ExperimentalTextApi
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontVariation
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.em
import androidx.compose.ui.unit.sp
import app.telacast.R

/**
 * Inter (texto) e Newsreader (títulos em serifa) são fontes variáveis: um arquivo só, e cada
 * peso é uma instância do eixo `wght` (FontVariation, exige API 26: é o nosso minSdk).
 */
private val pesosInter = listOf(300, 400, 500, 600, 700)
private val pesosSerifa = listOf(400, 500, 600)

val Inter: FontFamily = FontFamily(
    pesosInter.map { w ->
        Font(R.font.inter, FontWeight(w), FontStyle.Normal, variationSettings = FontVariation.Settings(FontVariation.weight(w)))
    }
)

val Newsreader: FontFamily = FontFamily(
    pesosSerifa.flatMap { w ->
        val s = FontVariation.Settings(FontVariation.weight(w))
        // não há arquivo itálico: o itálico é sintetizado (igual ao egui no desktop)
        listOf(
            Font(R.font.newsreader, FontWeight(w), FontStyle.Normal, variationSettings = s),
            Font(R.font.newsreader, FontWeight(w), FontStyle.Italic, variationSettings = s),
        )
    }
)

/** JetBrains Mono (variável) para código em mensagens e prévias de texto. */
val Mono: FontFamily = FontFamily(
    listOf(400, 600).map { w ->
        Font(R.font.jetbrainsmono, FontWeight(w), FontStyle.Normal, variationSettings = FontVariation.Settings(FontVariation.weight(w)))
    }
)

/** Estilos de texto com os tamanhos do desktop (Body 14.5, Button 13.5, Small 12, Heading 24 serifa). */
object Tipos {
    val Corpo = TextStyle(fontFamily = Inter, fontWeight = FontWeight.Normal, fontSize = 14.5.sp, lineHeight = 21.sp, color = Grafite.Texto)
    val CorpoApagado = Corpo.copy(color = Grafite.Apagado)
    val Botao = TextStyle(fontFamily = Inter, fontWeight = FontWeight.SemiBold, fontSize = 13.5.sp, lineHeight = 18.sp, color = Grafite.Texto2)
    val Pequeno = TextStyle(fontFamily = Inter, fontWeight = FontWeight.Normal, fontSize = 12.sp, lineHeight = 16.sp, color = Grafite.Texto2)
    val Selo = TextStyle(fontFamily = Inter, fontWeight = FontWeight.SemiBold, fontSize = 11.sp, lineHeight = 14.sp, color = Grafite.AmbarTinta)

    /** Rótulo de seção: pequeno, caixa alta, apagado (o texto vem em caixa alta de quem chama). */
    val Rotulo = TextStyle(fontFamily = Inter, fontWeight = FontWeight.SemiBold, fontSize = 11.sp, lineHeight = 14.sp, letterSpacing = 0.09.em, color = Grafite.Apagado)

    val Titulo = TextStyle(fontFamily = Newsreader, fontWeight = FontWeight.Medium, fontSize = 24.sp, lineHeight = 28.sp, letterSpacing = (-0.015).em, color = Grafite.Texto)
    val Subtitulo = Titulo.copy(fontSize = 22.sp, lineHeight = 26.sp, letterSpacing = (-0.01).em)
    val Divisor = TextStyle(fontFamily = Newsreader, fontWeight = FontWeight.Normal, fontStyle = FontStyle.Italic, fontSize = 14.sp, lineHeight = 18.sp, color = Grafite.Apagado)
}

/** Typography do Material3 só como base dos widgets que vierem dele; o visual real é o `Tipos`. */
val TipografiaMaterial = Typography(
    displayLarge = Tipos.Titulo, displayMedium = Tipos.Titulo, displaySmall = Tipos.Titulo,
    headlineLarge = Tipos.Titulo, headlineMedium = Tipos.Titulo, headlineSmall = Tipos.Subtitulo,
    titleLarge = Tipos.Subtitulo, titleMedium = Tipos.Corpo.copy(fontWeight = FontWeight.SemiBold), titleSmall = Tipos.Botao,
    bodyLarge = Tipos.Corpo, bodyMedium = Tipos.Corpo, bodySmall = Tipos.Pequeno,
    labelLarge = Tipos.Botao, labelMedium = Tipos.Botao, labelSmall = Tipos.Rotulo,
)
