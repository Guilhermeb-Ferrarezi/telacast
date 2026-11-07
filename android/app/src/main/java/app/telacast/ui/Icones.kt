package app.telacast.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import app.telacast.proto.Status

/** Ícones desenhados com traços simples (sem fonte de ícones nem imagens), no espírito do `theme.rs` do desktop. */

/** Seta de categoria: aponta para baixo (aberta) ou para a direita (recolhida). 12 dp, traço de 1,5. */
@Composable
fun IconeSeta(aberta: Boolean, cor: Color, modifier: Modifier = Modifier, tamanho: Dp = 12.dp) {
    Canvas(modifier.size(tamanho)) {
        val u = density
        val c = Offset(size.width / 2, size.height / 2)
        val (a, b, d) = if (aberta) {
            Triple(Offset(-3.5f, -1.8f), Offset(0f, 1.8f), Offset(3.5f, -1.8f))
        } else {
            Triple(Offset(-1.8f, -3.5f), Offset(1.8f, 0f), Offset(-1.8f, 3.5f))
        }
        val st = Stroke(width = 1.5f * u, cap = StrokeCap.Round)
        drawLine(cor, c + a * u, c + b * u, st.width, StrokeCap.Round)
        drawLine(cor, c + b * u, c + d * u, st.width, StrokeCap.Round)
    }
}

/** Alto-falante com duas ondas (canal de voz). */
@Composable
fun IconeAltoFalante(cor: Color, modifier: Modifier = Modifier, tamanho: Dp = 18.dp) {
    Canvas(modifier.size(tamanho)) {
        val s = size.width
        val c = Offset(s / 2, s / 2)
        val traco = (s * 0.085f).coerceAtLeast(1.3f * density)
        drawRoundRect(cor, c + Offset(-0.27f * s - 0.09f * s, -0.13f * s), Size(0.18f * s, 0.26f * s), CornerRadius(0.02f * s))
        val cone = Path().apply {
            moveTo(c.x - 0.18f * s, c.y - 0.13f * s)
            lineTo(c.x + 0.10f * s, c.y - 0.36f * s)
            lineTo(c.x + 0.10f * s, c.y + 0.36f * s)
            lineTo(c.x - 0.18f * s, c.y + 0.13f * s)
            close()
        }
        drawPath(cone, cor)
        for (r in listOf(0.22f, 0.40f)) {
            drawArc(
                cor, startAngle = -50f, sweepAngle = 100f, useCenter = false,
                topLeft = Offset(c.x + 0.10f * s - r * s, c.y - r * s), size = Size(2 * r * s, 2 * r * s),
                style = Stroke(traco, cap = StrokeCap.Round),
            )
        }
    }
}

/** Três linhas (abre a gaveta). */
@Composable
fun IconeMenu(cor: Color, modifier: Modifier = Modifier, tamanho: Dp = 22.dp) {
    Canvas(modifier.size(tamanho)) {
        val w = size.width
        val x0 = w * 0.16f
        val x1 = w * 0.84f
        for (y in listOf(0.28f, 0.5f, 0.72f)) drawLine(cor, Offset(x0, w * y), Offset(x1, w * y), 1.8f * density, StrokeCap.Round)
    }
}

/** Duas pessoas (painel de membros). */
@Composable
fun IconeMembros(cor: Color, modifier: Modifier = Modifier, tamanho: Dp = 22.dp) {
    Canvas(modifier.size(tamanho)) {
        val s = size.width
        fun pessoa(cx: Float, cor: Color) {
            drawCircle(cor, 0.14f * s, Offset(cx, 0.32f * s))
            val corpo = Path().apply {
                addRoundRect(androidx.compose.ui.geometry.RoundRect(cx - 0.22f * s, 0.52f * s, cx + 0.22f * s, 0.84f * s, topLeftCornerRadius = CornerRadius(0.22f * s), topRightCornerRadius = CornerRadius(0.22f * s), bottomLeftCornerRadius = CornerRadius(0.04f * s), bottomRightCornerRadius = CornerRadius(0.04f * s)))
            }
            drawPath(corpo, cor)
        }
        pessoa(0.66f * s, cor.copy(alpha = 0.55f))
        pessoa(0.38f * s, cor)
    }
}

@Composable
fun IconeMais(cor: Color, modifier: Modifier = Modifier, tamanho: Dp = 18.dp) {
    Canvas(modifier.size(tamanho)) {
        val s = size.width
        val c = s / 2
        val d = s * 0.32f
        drawLine(cor, Offset(c - d, c), Offset(c + d, c), 1.8f * density, StrokeCap.Round)
        drawLine(cor, Offset(c, c - d), Offset(c, c + d), 1.8f * density, StrokeCap.Round)
    }
}

@Composable
fun IconeFechar(cor: Color, modifier: Modifier = Modifier, tamanho: Dp = 20.dp) {
    Canvas(modifier.size(tamanho)) {
        val s = size.width
        val c = s / 2
        val d = s * 0.25f
        drawLine(cor, Offset(c - d, c - d), Offset(c + d, c + d), 1.8f * density, StrokeCap.Round)
        drawLine(cor, Offset(c - d, c + d), Offset(c + d, c - d), 1.8f * density, StrokeCap.Round)
    }
}

/** Seta para cima (indica que o cartão do usuário abre um menu). */
@Composable
fun IconeSetaParaCima(cor: Color, modifier: Modifier = Modifier, tamanho: Dp = 14.dp) {
    Canvas(modifier.size(tamanho)) {
        val u = density
        val c = Offset(size.width / 2, size.height / 2)
        drawLine(cor, c + Offset(-3.5f, 1.8f) * u, c + Offset(0f, -1.8f) * u, 1.5f * u, StrokeCap.Round)
        drawLine(cor, c + Offset(0f, -1.8f) * u, c + Offset(3.5f, 1.8f) * u, 1.5f * u, StrokeCap.Round)
    }
}

fun Status.rotulo(): String = when (this) {
    Status.Online -> "Online"
    Status.Idle -> "Ausente"
    Status.Dnd -> "Não perturbe"
    Status.Invisible -> "Invisível"
    Status.Offline -> "Offline"
}

fun Status.presenca(): Presenca = when (this) {
    Status.Online -> Presenca.Online
    Status.Idle -> Presenca.Ausente
    Status.Dnd -> Presenca.NaoPerturbe
    Status.Invisible, Status.Offline -> Presenca.Offline
}

/** Sigla do servidor: primeira letra de cada palavra (Unicode), como `initials` do desktop. */
fun siglaDe(nome: String): String {
    val s = nome.replace("'s ", " ").split(Regex("[^\\p{L}\\p{N}]+")).filter { it.isNotEmpty() }.joinToString("") { it.first().toString() }
    return s.ifEmpty { "?" }
}
