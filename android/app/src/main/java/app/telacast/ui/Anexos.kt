package app.telacast.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.gestures.detectTransformGestures
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.graphics.painter.BitmapPainter
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.telacast.arquivos.Arquivos
import app.telacast.arquivos.FonteDeArquivo
import app.telacast.arquivos.PreviaDeTexto
import app.telacast.data.Formatacao
import app.telacast.proto.Attachment
import app.telacast.tema.Formas
import app.telacast.tema.Grafite
import app.telacast.tema.Mono
import app.telacast.tema.Tipos

/**
 * De onde vêm os bitmaps e as prévias. As funções `*EmCache` são síncronas (o que já foi baixado aparece sem piscar,
 * e os testes de imagem entregam bitmaps gerados em código); as `suspend` baixam. Sem Coil.
 */
class ProvedorDeAnexos(
    val imagemEmCache: (Attachment) -> ImageBitmap? = { null },
    val imagem: suspend (Attachment) -> ImageBitmap? = { null },
    /** Miniatura (ou o original, se for o que há) para a lista do chat; a tela cheia usa [imagem]. */
    val miniaturaDoAnexoEmCache: (Attachment) -> ImageBitmap? = { null },
    val miniaturaDoAnexo: suspend (Attachment) -> ImageBitmap? = { null },
    val previaEmCache: (Attachment) -> PreviaDeTexto? = { null },
    val previa: suspend (Attachment) -> PreviaDeTexto? = { null },
    val miniaturaEmCache: (FonteDeArquivo) -> ImageBitmap? = { null },
    val miniatura: suspend (FonteDeArquivo) -> ImageBitmap? = { null },
)

val LocalProvedorDeAnexos = staticCompositionLocalOf { ProvedorDeAnexos() }

class AcoesDeAnexo(
    val aoAbrirImagem: (Attachment) -> Unit = {},
    val aoBaixar: (Attachment) -> Unit = {},
    val aoAbrirTexto: (Attachment) -> Unit = {},
    val aoAlternarTexto: (Long) -> Unit = {},
)

private sealed interface EstadoDeImagem {
    data object Carregando : EstadoDeImagem
    data class Pronta(val bmp: ImageBitmap) : EstadoDeImagem
    data object Falhou : EstadoDeImagem
}

/** Anexo de uma mensagem: imagem (até 400 x 300 dp), prévia de texto (6 linhas) ou cartão de arquivo. */
@Composable
fun AnexoView(a: Attachment, textoExpandido: Boolean, acoes: AcoesDeAnexo, modifier: Modifier = Modifier) {
    val prov = LocalProvedorDeAnexos.current
    if (Arquivos.ehImagem(a)) {
        // a lista usa a MINIATURA (até 1024 px); o original só é baixado na tela cheia
        var est by remember(a.id) { mutableStateOf(prov.miniaturaDoAnexoEmCache(a)?.let { EstadoDeImagem.Pronta(it) } ?: EstadoDeImagem.Carregando) }
        LaunchedEffect(a.id) {
            if (est is EstadoDeImagem.Carregando) est = prov.miniaturaDoAnexo(a)?.let { EstadoDeImagem.Pronta(it) } ?: EstadoDeImagem.Falhou
        }
        if (est !is EstadoDeImagem.Falhou) {
            BoxWithConstraints(modifier) {
                val forma = RoundedCornerShape(8.dp)
                // com o bitmap na mão, a proporção vem dele (foto com orientação EXIF 5 a 8 tem largura e altura trocadas no anexo)
                val dim = (est as? EstadoDeImagem.Pronta)?.let { Arquivos.dimensoesDeExibicao(a, it.bmp.width, it.bmp.height) } ?: (a.width to a.height)
                val (w, h) = Arquivos.tamanhoDaImagem(dim.first, dim.second, minOf(400f, maxWidth.value))
                when (val e = est) {
                    is EstadoDeImagem.Pronta -> Image(
                        BitmapPainter(e.bmp), a.name, Modifier.size(w.dp, h.dp).clip(forma).clickable { acoes.aoAbrirImagem(a) },
                        contentScale = ContentScale.Crop,
                    )
                    else -> Box(Modifier.size(w.dp, h.dp).background(Grafite.L3, forma), contentAlignment = Alignment.Center) {
                        Text("Carregando…", style = Tipos.Corpo.copy(fontSize = 13.sp, color = Grafite.Apagado))
                    }
                }
            }
            return
        }
    }
    if (Arquivos.ehTexto(a)) {
        var previa by remember(a.id) { mutableStateOf(prov.previaEmCache(a)) }
        var falhou by remember(a.id) { mutableStateOf(false) }
        LaunchedEffect(a.id) { if (previa == null) { previa = prov.previa(a); falhou = previa == null } }
        val p = previa
        if (p != null) { CartaoDeTexto(a, p, textoExpandido, acoes, modifier); return }
        if (!falhou) {
            Box(modifier.fillMaxWidth().height(96.dp).background(Grafite.L2, Formas.Linha).border(1.dp, Grafite.Linha2, Formas.Linha), contentAlignment = Alignment.Center) {
                Text("Carregando prévia…", style = Tipos.Corpo.copy(fontSize = 13.sp, color = Grafite.Apagado))
            }
            return
        }
    }
    CartaoDeArquivo(a, acoes, modifier)
}

@Composable
fun CartaoDeArquivo(a: Attachment, acoes: AcoesDeAnexo, modifier: Modifier = Modifier) {
    Row(
        modifier.fillMaxWidth().background(Grafite.L3, Formas.Menu).border(1.dp, Grafite.Linha2, Formas.Menu).padding(horizontal = 12.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(a.name, style = Tipos.Corpo.copy(color = Grafite.Ambar, fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(Formatacao.tamanho(a.size), style = Tipos.Pequeno.copy(color = Grafite.Apagado))
        }
        Spacer(Modifier.width(12.dp))
        Botao("Baixar", TipoBotao.Quieto, altura = 34.dp, aoClicar = { acoes.aoBaixar(a) })
    }
}

/** Prévia de texto/código: 6 linhas (ou 100 expandido), tamanho e nº de linhas, Expandir/Abrir/Baixar. */
@Composable
fun CartaoDeTexto(a: Attachment, p: PreviaDeTexto, expandido: Boolean, acoes: AcoesDeAnexo, modifier: Modifier = Modifier) {
    val maxLinhas = if (expandido) 100 else 6
    val inteiro = p.bytesRestantes == 0L
    val mostrado = p.texto.split('\n').take(maxLinhas).joinToString("\n")
    val aviso = when {
        inteiro && expandido && p.linhas > maxLinhas -> "… (${p.linhas - maxLinhas} linhas a mais)"
        !inteiro -> "… (${Formatacao.tamanho(p.bytesRestantes)} restantes)"
        else -> ""
    }
    val podeAlternar = expandido || p.linhas > 6 || !inteiro
    Column(modifier.fillMaxWidth().background(Grafite.L2, Formas.Linha).border(1.dp, Grafite.Linha2, Formas.Linha)) {
        Box(Modifier.padding(horizontal = 12.dp, vertical = 10.dp)) {
            Text(
                mostrado + if (aviso.isNotEmpty() && expandido) "\n$aviso" else "",
                style = Tipos.Corpo.copy(fontFamily = Mono, fontSize = 12.5.sp, lineHeight = 18.sp, color = Grafite.Texto),
                maxLines = if (expandido) 102 else 6, overflow = TextOverflow.Ellipsis,
            )
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(Grafite.Linha2))
        Column(Modifier.padding(horizontal = 10.dp, vertical = 8.dp)) {
            Text(a.name, style = Tipos.Corpo.copy(fontSize = 13.sp, color = Grafite.Texto), maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text(
                if (inteiro) "${Formatacao.tamanho(a.size)} · ${p.linhas} linhas" else Formatacao.tamanho(a.size),
                style = Tipos.Pequeno.copy(fontSize = 11.5.sp, color = Grafite.Apagado),
            )
            Spacer(Modifier.height(8.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                if (podeAlternar) Botao(if (expandido) "Recolher" else "Expandir", TipoBotao.Quieto, altura = 32.dp, aoClicar = { acoes.aoAlternarTexto(a.id) })
                Botao("Abrir", TipoBotao.Quieto, altura = 32.dp, aoClicar = { acoes.aoAbrirTexto(a) })
                Botao("Baixar", TipoBotao.Quieto, altura = 32.dp, aoClicar = { acoes.aoBaixar(a) })
            }
        }
    }
}

/**
 * Imagem em tela cheia: fundo quase preto, zoom por pinça (1x a 5x), toque duplo alterna, Baixar e Fechar.
 * Mostra logo a miniatura (que costuma estar na memória por causa do chat) e troca pelo original quando ele chega,
 * sem piscar; se o original falhar (grande demais, rede), fica com a miniatura. "Baixar" baixa sempre o original.
 */
@Composable
fun VisualizadorDeImagem(a: Attachment, aoFechar: () -> Unit, aoBaixar: () -> Unit) {
    val prov = LocalProvedorDeAnexos.current
    var bmp by remember(a.id) { mutableStateOf(prov.imagemEmCache(a) ?: prov.miniaturaDoAnexoEmCache(a)) }
    var original by remember(a.id) { mutableStateOf(prov.imagemEmCache(a) != null) }
    var falhouOriginal by remember(a.id) { mutableStateOf(false) }
    LaunchedEffect(a.id) {
        if (bmp == null) bmp = prov.miniaturaDoAnexo(a)
        if (!original) {
            val o = prov.imagem(a)
            if (o != null) { bmp = o; original = true } else falhouOriginal = true
        }
    }
    var escala by remember { mutableFloatStateOf(1f) }
    var deslocamento by remember { mutableStateOf(Offset.Zero) }
    Box(Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.92f))) {
        val b = bmp
        if (b != null) {
            Image(
                BitmapPainter(b), a.name,
                Modifier.fillMaxSize().padding(horizontal = 8.dp, vertical = 72.dp)
                    .pointerInput(Unit) {
                        detectTransformGestures { _, pan, zoom, _ ->
                            escala = (escala * zoom).coerceIn(1f, 5f)
                            deslocamento = if (escala <= 1f) Offset.Zero else deslocamento + pan
                        }
                    }
                    .pointerInput(Unit) {
                        detectTapGestures(onDoubleTap = { if (escala > 1f) { escala = 1f; deslocamento = Offset.Zero } else escala = 2.5f })
                    }
                    .graphicsLayer(scaleX = escala, scaleY = escala, translationX = deslocamento.x, translationY = deslocamento.y),
                contentScale = ContentScale.Fit,
            )
        } else {
            Text("Carregando…", style = Tipos.CorpoApagado, modifier = Modifier.align(Alignment.Center))
        }
        if (bmp != null && !original && !falhouOriginal) {
            Text("Carregando a imagem original…", style = Tipos.Pequeno.copy(color = Grafite.Texto2), modifier = Modifier.align(Alignment.BottomCenter).windowInsetsPadding(WindowInsets.navigationBars).padding(bottom = 16.dp))
        }
        Row(
            Modifier.fillMaxWidth().windowInsetsPadding(WindowInsets.statusBars).padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(a.name, style = Tipos.Corpo.copy(fontSize = 13.sp, color = Grafite.Texto2), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
            Spacer(Modifier.width(8.dp))
            Botao("Baixar", TipoBotao.Quieto, altura = 36.dp, aoClicar = aoBaixar)
            Spacer(Modifier.width(8.dp))
            Toque(aoFechar, Modifier.size(40.dp), Formas.Cartao, fundoPressionado = Grafite.L3) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { IconeFechar(Grafite.Texto) }
            }
        }
    }
}

/** Texto do anexo em tela cheia (o começo, até 50 KB), rolável. */
@Composable
fun VisualizadorDeTexto(a: Attachment, aoFechar: () -> Unit, aoBaixar: () -> Unit) {
    val prov = LocalProvedorDeAnexos.current
    var previa by remember(a.id) { mutableStateOf(prov.previaEmCache(a)) }
    LaunchedEffect(a.id) { if (previa == null) previa = prov.previa(a) }
    Column(Modifier.fillMaxSize().background(Grafite.L2).windowInsetsPadding(WindowInsets.statusBars)) {
        Row(Modifier.fillMaxWidth().height(56.dp).padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(a.name, style = Tipos.Corpo.copy(fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(Formatacao.tamanho(a.size) + (previa?.let { " · ${it.linhas} linhas" } ?: ""), style = Tipos.Pequeno.copy(color = Grafite.Apagado))
            }
            Botao("Baixar", TipoBotao.Quieto, altura = 36.dp, aoClicar = aoBaixar)
            Spacer(Modifier.width(8.dp))
            Toque(aoFechar, Modifier.size(40.dp), Formas.Cartao, fundoPressionado = Grafite.L3) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { IconeFechar(Grafite.Texto) }
            }
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(Grafite.Linha))
        val p = previa
        if (p == null) {
            Text("Carregando…", style = Tipos.CorpoApagado, modifier = Modifier.padding(16.dp))
        } else {
            androidx.compose.foundation.text.selection.SelectionContainer {
                Text(
                    p.texto + if (p.bytesRestantes > 0) "\n\n… (${Formatacao.tamanho(p.bytesRestantes)} restantes)" else "",
                    style = Tipos.Corpo.copy(fontFamily = Mono, fontSize = 12.5.sp, lineHeight = 18.sp, color = Grafite.Texto),
                    modifier = Modifier.verticalScroll(rememberScrollState()).padding(16.dp),
                )
            }
        }
    }
}
