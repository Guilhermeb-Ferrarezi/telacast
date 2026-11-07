package app.telacast.tema

import androidx.compose.foundation.text.selection.LocalTextSelectionColors
import androidx.compose.foundation.text.selection.TextSelectionColors
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.graphics.Color

/** Só existe o tema escuro. O Material3 entra apenas como base de widgets, mapeado para o Grafite. */
private val EsquemaMaterial = darkColorScheme(
    primary = Grafite.Ambar, onPrimary = Grafite.AmbarTinta,
    secondary = Grafite.Ambar, onSecondary = Grafite.AmbarTinta,
    tertiary = Grafite.Ambar, onTertiary = Grafite.AmbarTinta,
    background = Grafite.L2, onBackground = Grafite.Texto,
    surface = Grafite.L2, onSurface = Grafite.Texto,
    surfaceVariant = Grafite.L3, onSurfaceVariant = Grafite.Texto2,
    surfaceTint = Color.Transparent, // sem tingimento de elevação
    surfaceContainerLowest = Grafite.L0, surfaceContainerLow = Grafite.L1, surfaceContainer = Grafite.L2,
    surfaceContainerHigh = Grafite.L3, surfaceContainerHighest = Grafite.L4,
    outline = Grafite.Linha2, outlineVariant = Grafite.Linha,
    error = Grafite.Vermelho, onError = Grafite.AmbarTinta,
)

private val SelecaoDeTexto = TextSelectionColors(
    handleColor = Grafite.Ambar,
    backgroundColor = Grafite.Ambar.copy(alpha = 0.28f),
)

@Composable
fun GrafiteTheme(content: @Composable () -> Unit) {
    MaterialTheme(colorScheme = EsquemaMaterial, typography = TipografiaMaterial, shapes = Formas.ParaMaterial) {
        CompositionLocalProvider(LocalTextSelectionColors provides SelecaoDeTexto, content = content)
    }
}
