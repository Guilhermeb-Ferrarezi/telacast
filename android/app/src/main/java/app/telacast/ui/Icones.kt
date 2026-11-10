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

/** Balão de mensagem (botão "Mensagem" dos amigos). */
@Composable
fun IconeBalao(cor: Color, modifier: Modifier = Modifier, tamanho: Dp = 20.dp) {
    Canvas(modifier.size(tamanho)) {
        val s = size.width
        val u = density
        drawRoundRect(cor, Offset(0.14f * s, 0.18f * s), Size(0.72f * s, 0.50f * s), CornerRadius(0.12f * s), style = Stroke(1.7f * u, cap = StrokeCap.Round))
        val cauda = Path().apply {
            moveTo(0.30f * s, 0.68f * s); lineTo(0.30f * s, 0.84f * s); lineTo(0.48f * s, 0.68f * s)
        }
        drawPath(cauda, cor, style = Stroke(1.7f * u, cap = StrokeCap.Round, join = androidx.compose.ui.graphics.StrokeJoin.Round))
    }
}

/** Três pontos na vertical (mais ações). */
@Composable
fun IconeMais3(cor: Color, modifier: Modifier = Modifier, tamanho: Dp = 20.dp) {
    Canvas(modifier.size(tamanho)) {
        val s = size.width
        for (y in listOf(0.24f, 0.5f, 0.76f)) drawCircle(cor, 1.9f * density, Offset(s / 2, s * y))
    }
}

@Composable
fun IconeConfirmar(cor: Color, modifier: Modifier = Modifier, tamanho: Dp = 20.dp) {
    Canvas(modifier.size(tamanho)) {
        val s = size.width
        val u = density
        drawLine(cor, Offset(0.22f * s, 0.54f * s), Offset(0.42f * s, 0.72f * s), 2f * u, StrokeCap.Round)
        drawLine(cor, Offset(0.42f * s, 0.72f * s), Offset(0.78f * s, 0.30f * s), 2f * u, StrokeCap.Round)
    }
}

@Composable
fun IconeLupa(cor: Color, modifier: Modifier = Modifier, tamanho: Dp = 22.dp) {
    Canvas(modifier.size(tamanho)) {
        val s = size.width
        val u = density
        drawCircle(cor, 0.26f * s, Offset(0.43f * s, 0.43f * s), style = Stroke(1.8f * u))
        drawLine(cor, Offset(0.62f * s, 0.62f * s), Offset(0.84f * s, 0.84f * s), 1.9f * u, StrokeCap.Round)
    }
}

/** Alfinete (mensagens fixadas). */
@Composable
fun IconeAlfinete(cor: Color, modifier: Modifier = Modifier, tamanho: Dp = 22.dp) {
    Canvas(modifier.size(tamanho)) {
        val s = size.width
        val u = density
        drawCircle(cor, 0.17f * s, Offset(0.5f * s, 0.33f * s))
        drawLine(cor, Offset(0.5f * s, 0.46f * s), Offset(0.5f * s, 0.86f * s), 1.8f * u, StrokeCap.Round)
        drawLine(cor, Offset(0.32f * s, 0.50f * s), Offset(0.68f * s, 0.50f * s), 1.8f * u, StrokeCap.Round)
    }
}

/** Seta para a esquerda (voltar). */
@Composable
fun IconeVoltar(cor: Color, modifier: Modifier = Modifier, tamanho: Dp = 22.dp) {
    Canvas(modifier.size(tamanho)) {
        val s = size.width
        val u = density
        drawLine(cor, Offset(0.78f * s, 0.5f * s), Offset(0.22f * s, 0.5f * s), 1.9f * u, StrokeCap.Round)
        drawLine(cor, Offset(0.45f * s, 0.27f * s), Offset(0.22f * s, 0.5f * s), 1.9f * u, StrokeCap.Round)
        drawLine(cor, Offset(0.45f * s, 0.73f * s), Offset(0.22f * s, 0.5f * s), 1.9f * u, StrokeCap.Round)
    }
}

private fun androidx.compose.ui.graphics.drawscope.DrawScope.riscoDeMudo(s: Float) {
    drawLine(Grafite_Vermelho, Offset(0.12f * s, 0.12f * s), Offset(0.88f * s, 0.88f * s), maxOf(1.8f * density, 0.11f * s), StrokeCap.Round)
}

private val Grafite_Vermelho = app.telacast.tema.Grafite.Vermelho

/** Microfone; `cortado` risca com a barra vermelha (mudo). */
@Composable
fun IconeMicrofone(cor: Color, cortado: Boolean = false, modifier: Modifier = Modifier, tamanho: Dp = 22.dp) {
    Canvas(modifier.size(tamanho)) {
        val s = size.width
        val traco = maxOf(1.6f * density, 0.085f * s)
        drawRoundRect(cor, Offset(0.36f * s, 0.14f * s), Size(0.28f * s, 0.46f * s), CornerRadius(0.14f * s))
        drawArc(cor, 0f, 180f, false, Offset(0.25f * s, 0.30f * s), Size(0.50f * s, 0.42f * s), style = Stroke(traco, cap = StrokeCap.Round))
        drawLine(cor, Offset(0.5f * s, 0.72f * s), Offset(0.5f * s, 0.86f * s), traco, StrokeCap.Round)
        drawLine(cor, Offset(0.38f * s, 0.86f * s), Offset(0.62f * s, 0.86f * s), traco, StrokeCap.Round)
        if (cortado) riscoDeMudo(s)
    }
}

/** Fones de ouvido; `cortado` = surdo. */
@Composable
fun IconeFones(cor: Color, cortado: Boolean = false, modifier: Modifier = Modifier, tamanho: Dp = 22.dp) {
    Canvas(modifier.size(tamanho)) {
        val s = size.width
        val traco = maxOf(1.6f * density, 0.085f * s)
        drawArc(cor, 180f, 180f, false, Offset(0.16f * s, 0.18f * s), Size(0.68f * s, 0.66f * s), style = Stroke(traco, cap = StrokeCap.Round))
        drawRoundRect(cor, Offset(0.12f * s, 0.52f * s), Size(0.17f * s, 0.30f * s), CornerRadius(0.05f * s))
        drawRoundRect(cor, Offset(0.71f * s, 0.52f * s), Size(0.17f * s, 0.30f * s), CornerRadius(0.05f * s))
        if (cortado) riscoDeMudo(s)
    }
}

/** Telefone desligado (sair da voz). */
@Composable
fun IconeDesligar(cor: Color, modifier: Modifier = Modifier, tamanho: Dp = 22.dp) {
    Canvas(modifier.size(tamanho)) {
        val s = size.width
        val traco = maxOf(2.2f * density, 0.17f * s)
        val p = Path().apply {
            moveTo(0.10f * s, 0.60f * s); lineTo(0.22f * s, 0.44f * s); lineTo(0.40f * s, 0.38f * s)
            lineTo(0.60f * s, 0.38f * s); lineTo(0.78f * s, 0.44f * s); lineTo(0.90f * s, 0.60f * s)
        }
        drawPath(p, cor, style = Stroke(traco, cap = StrokeCap.Round, join = androidx.compose.ui.graphics.StrokeJoin.Round))
    }
}

/** Sinal de conexão: três barras. */
@Composable
fun IconeSinal(cor: Color, modifier: Modifier = Modifier, tamanho: Dp = 18.dp) {
    Canvas(modifier.size(tamanho)) {
        val u = density
        val x0 = size.width / 2 - 7f * u
        for ((i, h) in listOf(5f, 9f, 13f).withIndex()) {
            val x = x0 + i * 5f * u
            drawRoundRect(cor, Offset(x, size.height / 2 + 6.5f * u - h * u), Size(3f * u, h * u), CornerRadius(1f * u))
        }
    }
}

/** Telefone (ligar). */
@Composable
fun IconeTelefone(cor: Color, modifier: Modifier = Modifier, tamanho: Dp = 22.dp) {
    Canvas(modifier.size(tamanho)) {
        val s = size.width
        val p = Path().apply {
            moveTo(0.22f * s, 0.16f * s); lineTo(0.38f * s, 0.16f * s); lineTo(0.45f * s, 0.34f * s); lineTo(0.34f * s, 0.42f * s)
            cubicTo(0.40f * s, 0.55f * s, 0.46f * s, 0.62f * s, 0.58f * s, 0.67f * s); lineTo(0.66f * s, 0.56f * s)
            lineTo(0.84f * s, 0.63f * s); lineTo(0.84f * s, 0.79f * s); cubicTo(0.84f * s, 0.84f * s, 0.80f * s, 0.86f * s, 0.76f * s, 0.86f * s)
            cubicTo(0.44f * s, 0.84f * s, 0.18f * s, 0.58f * s, 0.14f * s, 0.24f * s); cubicTo(0.14f * s, 0.20f * s, 0.18f * s, 0.16f * s, 0.22f * s, 0.16f * s); close()
        }
        drawPath(p, cor)
    }
}
