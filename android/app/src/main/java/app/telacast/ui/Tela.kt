package app.telacast.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBars
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.telacast.tela.EstadoDaTransmissao
import app.telacast.tela.EstadoDoPlayer
import app.telacast.tela.FaseDaTransmissao
import app.telacast.tela.FaseDoPlayer
import app.telacast.tela.OpcoesDeTransmissao
import app.telacast.tela.QualidadeDeTransmissao
import app.telacast.tela.tamanhoDaCodificacao
import app.telacast.tema.Formas
import app.telacast.tema.Grafite
import app.telacast.tema.Tipos

class AcoesDeTela(
    val aoAbrirOpcoes: () -> Unit = {},
    val aoFecharOpcoes: () -> Unit = {},
    /** Confirmou o diálogo: começa (ou, no ar, aplica). */
    val aoTransmitir: (OpcoesDeTransmissao) -> Unit = {},
    val aoPararTransmissao: () -> Unit = {},
    val aoAssistir: (usuario: Long) -> Unit = {},
    val aoFecharPlayer: () -> Unit = {},
    val aoTelaCheia: () -> Unit = {},
)

/** O que as telas precisam saber da tela compartilhada. */
class DadosDeTela(
    val transmissao: EstadoDaTransmissao = EstadoDaTransmissao(),
    val player: EstadoDoPlayer = EstadoDoPlayer(),
    /** Onde o vídeo é desenhado (no app, um `SurfaceView`; nas imagens de teste, um bloco liso). */
    val superficie: @Composable (Modifier) -> Unit = { m -> Box(m.background(Color(0xFF0B0B0C))) },
    val telaCheia: Boolean = false,
)

private fun textoDoEstado(t: EstadoDaTransmissao): Pair<String, Color> = when (t.fase) {
    FaseDaTransmissao.Inativa -> "" to Grafite.Apagado
    FaseDaTransmissao.PedindoPermissao -> "Aguardando a permissão…" to Grafite.Ambar
    FaseDaTransmissao.Conectando -> "Conectando a transmissão…" to Grafite.Ambar
    FaseDaTransmissao.Transmitindo -> "Você está transmitindo · ${if (t.espectadores == 1) "1 assistindo" else "${t.espectadores} assistindo"}" to Grafite.Ambar
    FaseDaTransmissao.Reconectando -> "Sem conexão. Reconectando a transmissão…" to Grafite.Ambar
    FaseDaTransmissao.Erro -> (t.mensagem ?: "A transmissão parou.") to Grafite.Vermelho
}

/** Faixa no topo do centro enquanto você transmite (e não está na tela da voz): estado, espectadores e Parar. */
@Composable
fun FaixaDeTransmissao(t: EstadoDaTransmissao, acoes: AcoesDeTela, aoAbrir: () -> Unit) {
    val (txt, cor) = textoDoEstado(t)
    Row(Modifier.fillMaxWidth().background(Grafite.L3).padding(start = 14.dp, end = 8.dp).height(44.dp), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.size(8.dp).background(cor, CircleShape))
        Spacer(Modifier.width(10.dp))
        Toque(aoAbrir, Modifier.weight(1f).height(44.dp)) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.CenterStart) {
                Text(txt, style = Tipos.Corpo.copy(fontSize = 13.sp, color = cor, fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
        }
        Botao("Parar", TipoBotao.Perigo, altura = 32.dp, aoClicar = acoes.aoPararTransmissao)
    }
}

/** Cartão da gaveta (acima do da voz): "Transmitindo · N assistindo" e Parar. */
@Composable
fun CartaoDeTransmissao(t: EstadoDaTransmissao, acoes: AcoesDeTela) {
    val (txt, cor) = textoDoEstado(t)
    Row(
        Modifier.padding(start = 8.dp, end = 8.dp, top = 4.dp).fillMaxWidth().background(Grafite.L2, Formas.Menu).border(1.dp, Grafite.Linha, Formas.Menu).padding(start = 12.dp, end = 8.dp, top = 8.dp, bottom = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text("AO VIVO", style = Tipos.Corpo.copy(fontSize = 10.sp, color = Grafite.Ambar, fontWeight = FontWeight.Bold, letterSpacing = 1.sp))
            Text(txt.removePrefix("Você está "), style = Tipos.Corpo.copy(fontSize = 12.5.sp, color = if (cor == Grafite.Ambar) Grafite.Texto2 else cor), maxLines = 2, overflow = TextOverflow.Ellipsis)
        }
        Botao("Parar", TipoBotao.Perigo, altura = 34.dp, aoClicar = acoes.aoPararTransmissao)
    }
}

/** Selo "AO VIVO" (âmbar, sem gradiente nem brilho). */
@Composable
fun SeloAoVivo(modifier: Modifier = Modifier, texto: String = "AO VIVO") {
    Box(modifier.background(Grafite.Ambar, Formas.Selo).padding(horizontal = 6.dp, vertical = 2.dp)) {
        Text(texto, style = Tipos.Corpo.copy(fontSize = 10.sp, color = Grafite.AmbarTinta, fontWeight = FontWeight.Bold, letterSpacing = 0.8.sp))
    }
}

@Composable
private fun Escolhas(rotulos: List<String>, selecionado: Int, aoEscolher: (Int) -> Unit, habilitados: List<Boolean> = rotulos.map { true }) {
    Row(Modifier.fillMaxWidth().background(Grafite.L3, Formas.Botao).padding(3.dp), horizontalArrangement = Arrangement.spacedBy(3.dp)) {
        for ((i, r) in rotulos.withIndex()) {
            val ok = habilitados[i]
            Toque({ if (ok) aoEscolher(i) }, Modifier.weight(1f).height(40.dp), Formas.Linha, fundo = if (selecionado == i) Grafite.L4 else Color.Transparent, fundoPressionado = Grafite.L4) {
                Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                    Text(r, style = Tipos.Botao.copy(color = if (!ok) Grafite.Linha2 else if (selecionado == i) Grafite.Texto else Grafite.Apagado))
                }
            }
        }
    }
}

/**
 * Diálogo "Transmitir" (folha inferior), como o do desktop: qualidade (480p, 720p, 1080p), taxa de quadros (só as que o codec aceita) e
 * bitrate (Automático ou um valor). Sem áudio do sistema (o desktop também não envia). No ar, vira "Aplicar" e "Parar de transmitir".
 */
@Composable
fun FolhaDeTransmissao(inicial: OpcoesDeTransmissao, fpsDisponiveis: List<Int>, tela: Pair<Int, Int>?, noAr: Boolean, acoes: AcoesDeTela) {
    var o by remember { mutableStateOf(inicial) }
    FolhaBase(acoes.aoFecharOpcoes) {
        Column(Modifier.padding(horizontal = 20.dp, vertical = 4.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Transmitir a tela", style = Tipos.Subtitulo.copy(fontSize = 22.sp))
            Text("Quem está neste canal de voz poderá assistir ao que aparece no seu celular. O áudio do celular não é enviado.", style = Tipos.CorpoApagado)
            Spacer(Modifier.height(4.dp))
            Text("QUALIDADE", style = Tipos.Rotulo)
            Escolhas(QualidadeDeTransmissao.entries.map { it.rotulo }, QualidadeDeTransmissao.entries.indexOf(o.qualidade), { o = o.copy(qualidade = QualidadeDeTransmissao.entries[it]) })
            Text("TAXA DE QUADROS", style = Tipos.Rotulo)
            Escolhas(OpcoesDeTransmissao.FPS.map { "$it fps" }, OpcoesDeTransmissao.FPS.indexOf(o.fps), { o = o.copy(fps = OpcoesDeTransmissao.FPS[it]) }, OpcoesDeTransmissao.FPS.map { it in fpsDisponiveis })
            Text("BITRATE", style = Tipos.Rotulo)
            val auto = o.kbps == null
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(if (auto) "Automático · ${"%.1f".format(o.kbpsEfetivo() / 1000.0).replace('.', ',')} Mbps" else "${"%.1f".format(o.kbpsEfetivo() / 1000.0).replace('.', ',')} Mbps", style = Tipos.Corpo.copy(fontSize = 14.sp), modifier = Modifier.weight(1f))
                if (!auto) Botao("Automático", TipoBotao.Quieto, altura = 32.dp, aoClicar = { o = o.copy(kbps = null) })
            }
            Slider(
                o.kbpsEfetivo() / 1000f, { o = o.copy(kbps = (it * 10).toInt() * 100) }, valueRange = 0.5f..10f,
                colors = SliderDefaults.colors(thumbColor = Grafite.Ambar, activeTrackColor = Grafite.Ambar, inactiveTrackColor = Grafite.L4),
            )
            val (w, h) = if (tela != null) tamanhoDaCodificacao(tela.first, tela.second, o.qualidade.ladoMenor) else (0 to 0)
            if (w > 0) Text("${w}x$h · ${o.fps} fps · H.264 · ~${"%.1f".format(o.kbpsEfetivo() / 1000.0).replace('.', ',')} Mbps", style = Tipos.Pequeno.copy(color = Grafite.Apagado))
            Spacer(Modifier.height(4.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                Botao("Cancelar", TipoBotao.Quieto, Modifier.weight(1f), 44.dp, aoClicar = acoes.aoFecharOpcoes)
                if (noAr) Botao("Parar", TipoBotao.Perigo, Modifier.weight(1f), 44.dp, aoClicar = acoes.aoPararTransmissao)
                Botao(if (noAr) "Aplicar" else "Transmitir", TipoBotao.Primario, Modifier.weight(1f), 44.dp, aoClicar = { acoes.aoTransmitir(o) })
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}

/**
 * Player de uma transmissão: o vídeo no centro (proporção do SPS), contagem de espectadores, tela cheia (gira para paisagem) e fechar.
 * Estados: carregando ("Aguardando vídeo…"), ao vivo, terminou, lotada, erro.
 */
@Composable
fun TelaDoPlayer(nome: String, p: EstadoDoPlayer, dados: DadosDeTela, acoes: AcoesDeTela) {
    Box(Modifier.fillMaxSize().background(Color.Black)) {
        val (l, a) = p.tamanho ?: (16 to 9)
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            dados.superficie(Modifier.fillMaxWidth().aspectRatio(l.toFloat() / a))
        }
        when (p.fase) {
            FaseDoPlayer.AoVivo, FaseDoPlayer.Inativo -> Unit
            FaseDoPlayer.Carregando -> CartaoDoPlayer("Aguardando vídeo…", "", false, acoes)
            FaseDoPlayer.Encerrou -> CartaoDoPlayer(p.mensagem ?: "Esta transmissão terminou.", "", true, acoes)
            FaseDoPlayer.Lotada -> CartaoDoPlayer("Esta transmissão está lotada.", "Desculpe, ela está com o máximo de espectadores.", true, acoes)
            FaseDoPlayer.Erro -> CartaoDoPlayer("A transmissão não iniciou.", p.mensagem ?: "", true, acoes)
        }
        Row(
            Modifier.fillMaxWidth().background(Color.Black.copy(alpha = 0.6f)).windowInsetsPadding(WindowInsets.statusBars).padding(horizontal = 12.dp, vertical = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (p.fase == FaseDoPlayer.AoVivo) { SeloAoVivo(); Spacer(Modifier.width(8.dp)) }
            Column(Modifier.weight(1f)) {
                Text(nome, style = Tipos.Corpo.copy(fontSize = 14.sp, fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis)
                if (p.fase == FaseDoPlayer.AoVivo) Text(if (p.espectadores == 1) "1 assistindo" else "${p.espectadores} assistindo", style = Tipos.Pequeno.copy(color = Grafite.Texto2))
            }
            Botao(if (dados.telaCheia) "Sair da tela cheia" else "Tela cheia", TipoBotao.Quieto, altura = 34.dp, aoClicar = acoes.aoTelaCheia)
            Spacer(Modifier.width(8.dp))
            Toque(acoes.aoFecharPlayer, Modifier.size(40.dp), CircleShape, fundoPressionado = Grafite.L3) { Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { IconeFechar(Grafite.Texto) } }
        }
    }
}

@Composable
private fun CartaoDoPlayer(titulo: String, detalhe: String, fechar: Boolean, acoes: AcoesDeTela) {
    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Column(Modifier.padding(32.dp).background(Grafite.L2, Formas.Cartao).border(1.dp, Grafite.Linha2, Formas.Cartao).padding(20.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(titulo, style = Tipos.Subtitulo.copy(fontSize = 20.sp), textAlign = TextAlign.Center)
            if (detalhe.isNotEmpty()) Text(detalhe, style = Tipos.CorpoApagado, textAlign = TextAlign.Center)
            if (fechar) { Spacer(Modifier.height(6.dp)); Botao("Fechar transmissão", TipoBotao.Quieto, aoClicar = acoes.aoFecharPlayer) }
        }
    }
}
