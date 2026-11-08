package app.telacast.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.Slider
import androidx.compose.material3.SliderDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.telacast.proto.VoiceState
import app.telacast.tema.Formas
import app.telacast.tema.Grafite
import app.telacast.tema.Tipos
import app.telacast.voz.EstadoDeVoz
import app.telacast.voz.FaseDaVoz
import app.telacast.voz.RotaDeAudio

/** O que as telas precisam saber da voz; `agoraMs == null` = relógio de verdade (os testes de imagem fixam um instante). */
class DadosDeVoz(val estado: EstadoDeVoz = EstadoDeVoz(), val agoraMs: Long? = null, val estatisticasAbertas: Boolean = false)

/** "sala de reunião · Turma do Telacast". */
fun localDaVoz(estado: app.telacast.data.AppState, canal: Long?): String {
    if (canal == null) return ""
    for (sv in estado.snapshot?.servers.orEmpty()) sv.channels.firstOrNull { it.id == canal }?.let { return "${it.name} · ${sv.name}" }
    return ""
}

/** Hora para o cronômetro da chamada: só corre (a cada segundo) enquanto há chamada; em teste vem fixa. */
@Composable
private fun agoraDaVoz(fixo: Long?, correndo: Boolean): Long {
    if (fixo != null) return fixo
    val v by androidx.compose.runtime.produceState(System.currentTimeMillis(), correndo) {
        value = System.currentTimeMillis()
        while (correndo) { kotlinx.coroutines.delay(1_000); value = System.currentTimeMillis() }
    }
    return v
}

class AcoesDeVoz(
    val aoEntrar: (canal: Long) -> Unit = {},
    val aoSair: () -> Unit = {},
    val aoAlternarMudo: () -> Unit = {},
    val aoAlternarSurdo: () -> Unit = {},
    val aoAlternarRota: () -> Unit = {},
    val aoAbrirTela: () -> Unit = {},
    val aoEscolherVolume: (usuario: Long) -> Unit = {},
    val aoMudarVolume: (usuario: Long, volume: Float) -> Unit = { _, _ -> },
    val aoFecharVolume: () -> Unit = {},
)

/** "00:42" / "1:02:03". */
fun cronometro(ms: Long): String {
    val s = maxOf(0L, ms) / 1000
    return if (s >= 3600) "%d:%02d:%02d".format(s / 3600, s % 3600 / 60, s % 60) else "%02d:%02d".format(s / 60, s % 60)
}

/** Texto e cor do estado da chamada. */
fun textoDoEstado(v: EstadoDeVoz, agoraMs: Long): Pair<String, Color> = when (v.fase) {
    FaseDaVoz.Fora -> "Desconectado" to Grafite.Apagado
    FaseDaVoz.Entrando -> "Conectando…" to Grafite.Ambar
    FaseDaVoz.Conectada -> ("Voz conectada" + (v.desdeMs?.let { " · ${cronometro(agoraMs - it)}" } ?: "")) to Grafite.Ok
    FaseDaVoz.Reconectando -> "Sem conexão. Reconectando…" to Grafite.Ambar
}

@Composable
private fun BotaoDaChamada(
    rotulo: String, aoClicar: () -> Unit, ativo: Boolean = false, perigo: Boolean = false, modifier: Modifier = Modifier,
    icone: @Composable (Color) -> Unit,
) {
    val fundo = if (perigo) Grafite.Vermelho else if (ativo) Grafite.Texto else Grafite.L3
    val tinta = if (perigo) Grafite.AmbarTinta else if (ativo) Grafite.L0 else Grafite.Texto
    Column(modifier, horizontalAlignment = Alignment.CenterHorizontally) {
        Toque(aoClicar, Modifier.size(56.dp).border(1.dp, if (perigo || ativo) Color.Transparent else Grafite.Linha2, CircleShape), CircleShape, fundo = fundo, fundoPressionado = if (perigo) Grafite.Vermelho.copy(alpha = 0.8f) else Grafite.L4) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { icone(tinta) }
        }
        Text(rotulo, style = Tipos.Pequeno.copy(fontSize = 11.5.sp, color = Grafite.Texto2), modifier = Modifier.padding(top = 6.dp))
    }
}

/**
 * Tela do canal de voz: participantes em grade (avatar com anel verde quando fala, ícones de mudo e surdo), estado da
 * chamada, medidor do microfone e os botões (mudo, surdo, rota do áudio, sair). Tocar num participante abre o volume.
 */
@Composable
fun TelaDeVoz(
    voz: EstadoDeVoz,
    canalNome: String,
    servidorNome: String,
    canalId: Long,
    participantes: List<VoiceState>,
    nomeDe: (Long) -> String,
    euId: Long,
    agoraMs: Long?,
    aoMenu: () -> Unit,
    acoes: AcoesDeVoz,
    modifier: Modifier = Modifier,
    novidade: Boolean = false,
    aoVivo: Set<Long> = emptySet(),
    transmissao: app.telacast.tela.EstadoDaTransmissao = app.telacast.tela.EstadoDaTransmissao(),
    acoesTela: AcoesDeTela = AcoesDeTela(),
    estatisticasAbertas: Boolean = false,
) {
    val naChamadaDeste = voz.naChamada && voz.canal == canalId
    Column(modifier.fillMaxSize().background(Grafite.L2)) {
        BarraSuperior(aoMenu, novidade) {
            IconeAltoFalante(Grafite.Apagado, tamanho = 22.dp)
            Spacer(Modifier.width(8.dp))
            Column {
                Text(canalNome, style = Tipos.Titulo.copy(fontSize = 20.sp, lineHeight = 24.sp), maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(servidorNome, style = Tipos.Pequeno.copy(fontSize = 11.5.sp, color = Grafite.Apagado), maxLines = 1)
            }
        }
        val (txt, cor) = if (naChamadaDeste) textoDoEstado(voz, agoraDaVoz(agoraMs, voz.fase == FaseDaVoz.Conectada)) else ("Você não está neste canal" to Grafite.Apagado)
        Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(8.dp).background(cor, CircleShape))
            Spacer(Modifier.width(8.dp))
            Text(txt, style = Tipos.Corpo.copy(fontSize = 13.5.sp, color = cor, fontWeight = FontWeight.Medium))
        }
        if (naChamadaDeste) PainelDeEstatisticas(voz.estatisticas, estatisticasAbertas)
        if (voz.erro != null) {
            Text(voz.erro, style = Tipos.Corpo.copy(fontSize = 13.sp, color = Grafite.Vermelho, lineHeight = 18.sp), modifier = Modifier.padding(horizontal = 16.dp).padding(bottom = 8.dp))
        }
        if (participantes.isEmpty()) {
            Box(Modifier.weight(1f).fillMaxWidth(), contentAlignment = Alignment.Center) {
                Text("Ninguém está neste canal de voz.", style = Tipos.CorpoApagado)
            }
        } else {
            LazyVerticalGrid(
                GridCells.Fixed(2), Modifier.weight(1f).fillMaxWidth(), contentPadding = PaddingValues(12.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                items(participantes, key = { it.userId }) { p ->
                    val falando = if (p.userId == euId) voz.euFalando && !voz.mudo && !voz.surdo else p.userId in voz.falando
                    // o meu estado vem do controlador (instantâneo); o dos outros, do servidor
                    val mudo = if (p.userId == euId) voz.mudo || voz.surdo else p.muted
                    val surdo = if (p.userId == euId) voz.surdo else p.deafened
                    TileDeParticipante(nomeDe(p.userId), p.userId, falando, mudo, surdo, voz.volumeDe(p.userId), p.userId == euId, p.userId in aoVivo, { acoesTela.aoAssistir(p.userId) }) { if (p.userId != euId) acoes.aoEscolherVolume(p.userId) }
                }
            }
        }
        if (naChamadaDeste) {
            Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                if (transmissao.ativa) {
                    Botao("Parar transmissão", TipoBotao.Perigo, Modifier.weight(1f), 44.dp, aoClicar = acoesTela.aoPararTransmissao)
                    Botao("Qualidade", TipoBotao.Quieto, Modifier.weight(1f), 44.dp, aoClicar = acoesTela.aoAbrirOpcoes)
                } else {
                    Botao("Transmitir minha tela", TipoBotao.Quieto, Modifier.fillMaxWidth(), 44.dp, aoClicar = acoesTela.aoAbrirOpcoes)
                }
            }
            if (transmissao.fase != app.telacast.tela.FaseDaTransmissao.Inativa) {
                val (t, c) = textoDaTransmissao(transmissao)
                Text(t, style = Tipos.Pequeno.copy(fontSize = 12.sp, color = c), modifier = Modifier.padding(horizontal = 20.dp, vertical = 6.dp))
            } else if (transmissao.mensagem != null) {
                Text(transmissao.mensagem, style = Tipos.Pequeno.copy(fontSize = 12.sp, color = Grafite.Apagado), modifier = Modifier.padding(horizontal = 20.dp, vertical = 6.dp))
            }
            Box(Modifier.padding(horizontal = 24.dp, vertical = 6.dp).fillMaxWidth().height(4.dp).background(Grafite.L4, Formas.Selo)) {
                Box(Modifier.fillMaxWidth(if (voz.mudo || voz.surdo) 0f else voz.nivel.coerceIn(0f, 1f)).height(4.dp).background(Grafite.Ambar, Formas.Selo))
            }
            Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 12.dp), horizontalArrangement = Arrangement.SpaceEvenly) {
                BotaoDaChamada(if (voz.mudo || voz.surdo) "Ativar mic" else "Mudo", acoes.aoAlternarMudo, ativo = voz.mudo || voz.surdo) { IconeMicrofone(it, cortado = voz.mudo || voz.surdo) }
                BotaoDaChamada(if (voz.surdo) "Ativar áudio" else "Surdo", acoes.aoAlternarSurdo, ativo = voz.surdo) { IconeFones(it, cortado = voz.surdo) }
                BotaoDaChamada(if (voz.rota == RotaDeAudio.AltoFalante) "Alto-falante" else "Fone", acoes.aoAlternarRota) { if (voz.rota == RotaDeAudio.AltoFalante) IconeAltoFalante(it, tamanho = 24.dp) else IconeFones(it) }
                BotaoDaChamada("Sair", acoes.aoSair, perigo = true) { IconeDesligar(it, tamanho = 24.dp) }
            }
        } else {
            Box(Modifier.fillMaxWidth().padding(16.dp)) {
                Botao(if (voz.fase == FaseDaVoz.Fora) "Entrar na voz" else "Mudar para este canal", TipoBotao.Primario, Modifier.fillMaxWidth(), altura = 48.dp, aoClicar = { acoes.aoEntrar(canalId) })
            }
        }
    }
}

@Composable
private fun TileDeParticipante(nome: String, id: Long, falando: Boolean, mudo: Boolean, surdo: Boolean, volume: Float, eu: Boolean, aoVivo: Boolean, aoAssistir: () -> Unit, aoClicar: () -> Unit) {
    Toque(aoClicar, Modifier.fillMaxWidth().height(if (aoVivo && !eu) 176.dp else 132.dp).border(if (falando) 2.dp else 1.dp, if (falando) Grafite.Ok else Grafite.Linha2, Formas.Cartao), Formas.Cartao, fundo = Grafite.L3, fundoPressionado = Grafite.L4) {
        Box(Modifier.fillMaxSize()) {
            Column(Modifier.fillMaxSize().padding(10.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
                Avatar(nome, id, 64.dp, null, Grafite.L3, falando = falando)
                Spacer(Modifier.height(8.dp))
                Text(nome + if (eu) " (você)" else "", style = Tipos.Corpo.copy(fontSize = 14.sp, fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis, textAlign = TextAlign.Center)
                if (aoVivo) { Spacer(Modifier.height(6.dp)); if (eu) SeloAoVivo(texto = "VOCÊ ESTÁ AO VIVO") else Botao("Assistir", TipoBotao.Primario, altura = 30.dp, aoClicar = aoAssistir) }
                if (!eu && volume != 1f) Text("${(volume * 100).toInt()}%", style = Tipos.Pequeno.copy(fontSize = 11.sp, color = Grafite.Apagado))
            }
            if (aoVivo && !eu) SeloAoVivo(Modifier.align(Alignment.TopStart).padding(8.dp))
            Row(Modifier.align(Alignment.TopEnd).padding(8.dp), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                if (surdo) IconeFones(Grafite.Apagado, cortado = true, tamanho = 18.dp) else if (mudo) IconeMicrofone(Grafite.Apagado, cortado = true, tamanho = 18.dp)
            }
        }
    }
}

/** Folha com o volume de uma pessoa (0 a 200%). */
@Composable
fun FolhaDeVolume(nome: String, volume: Float, aoMudar: (Float) -> Unit, aoFechar: () -> Unit) {
    FolhaBase(aoFechar) {
        Column(Modifier.padding(horizontal = 20.dp, vertical = 8.dp)) {
            Text(nome, style = Tipos.Subtitulo.copy(fontSize = 22.sp))
            Text("Volume desta pessoa para você: ${(volume * 100).toInt()}%", style = Tipos.CorpoApagado, modifier = Modifier.padding(top = 4.dp, bottom = 8.dp))
            Slider(
                volume, aoMudar, valueRange = 0f..2f,
                colors = SliderDefaults.colors(thumbColor = Grafite.Ambar, activeTrackColor = Grafite.Ambar, inactiveTrackColor = Grafite.L4),
            )
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
                Text("0%", style = Tipos.Pequeno.copy(color = Grafite.Apagado)); Text("100%", style = Tipos.Pequeno.copy(color = Grafite.Apagado)); Text("200%", style = Tipos.Pequeno.copy(color = Grafite.Apagado))
            }
            Spacer(Modifier.height(12.dp))
            Botao("Voltar ao normal", TipoBotao.Quieto, Modifier.fillMaxWidth(), aoClicar = { aoMudar(1f) })
            Spacer(Modifier.height(8.dp))
        }
    }
}

/** Faixa no topo do centro enquanto você está na voz e não está na tela dela: estado, mudo e sair; tocar abre a tela. */
@Composable
fun FaixaDeVoz(voz: EstadoDeVoz, local: String, agoraMs: Long?, acoes: AcoesDeVoz) {
    val (txt, cor) = textoDoEstado(voz, agoraDaVoz(agoraMs, voz.fase == FaseDaVoz.Conectada))
    Row(
        Modifier.fillMaxWidth().background(Grafite.L3).padding(start = 14.dp, end = 4.dp).height(48.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Toque(acoes.aoAbrirTela, Modifier.weight(1f).fillMaxHeight48()) {
            Row(Modifier.fillMaxSize(), verticalAlignment = Alignment.CenterVertically) {
                IconeSinal(cor)
                Spacer(Modifier.width(10.dp))
                Column {
                    Text(txt, style = Tipos.Corpo.copy(fontSize = 13.sp, color = cor, fontWeight = FontWeight.SemiBold), maxLines = 1)
                    if (local.isNotEmpty()) Text(local, style = Tipos.Pequeno.copy(fontSize = 11.sp, color = Grafite.Apagado), maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        }
        Toque(acoes.aoAlternarMudo, Modifier.size(44.dp), CircleShape, fundoPressionado = Grafite.L4) { Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { IconeMicrofone(if (voz.mudo || voz.surdo) Grafite.Apagado else Grafite.Texto2, cortado = voz.mudo || voz.surdo, tamanho = 20.dp) } }
        Toque(acoes.aoSair, Modifier.size(44.dp), CircleShape, fundoPressionado = Grafite.L4) { Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { IconeDesligar(Grafite.Vermelho, tamanho = 20.dp) } }
    }
}

private fun Modifier.fillMaxHeight48() = this.then(Modifier.height(48.dp))

/** Cartão "Voz conectada" da gaveta (acima do cartão do usuário): estado, canal, mudo, surdo e desligar. */
@Composable
fun CartaoDeVoz(voz: EstadoDeVoz, local: String, agoraMs: Long?, acoes: AcoesDeVoz) {
    val (txtCompleto, cor) = textoDoEstado(voz, agoraDaVoz(agoraMs, voz.fase == FaseDaVoz.Conectada))
    // o cartão é estreito: estado curto na primeira linha, "canal · 00:42" na segunda
    val txt = if (voz.fase == FaseDaVoz.Reconectando) "Reconectando…" else txtCompleto.substringBefore(" · ")
    val tempo = txtCompleto.substringAfter(" · ", "")
    val linha2 = local.substringBefore(" · ") + if (tempo.isNotEmpty()) " · $tempo" else ""
    Column(Modifier.padding(start = 8.dp, end = 8.dp, top = 4.dp).fillMaxWidth().background(Grafite.L2, Formas.Menu).border(1.dp, Grafite.Linha, Formas.Menu).padding(8.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Toque(acoes.aoAbrirTela, Modifier.weight(1f), Formas.Linha, fundoPressionado = Grafite.L3) {
                Row(Modifier.padding(vertical = 2.dp), verticalAlignment = Alignment.CenterVertically) {
                    IconeSinal(cor)
                    Spacer(Modifier.width(8.dp))
                    Column {
                        Text(txt, style = Tipos.Corpo.copy(fontSize = 13.sp, color = cor, fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text(linha2, style = Tipos.Pequeno.copy(fontSize = 11.sp, color = Grafite.Apagado), maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
            Toque(acoes.aoSair, Modifier.size(36.dp), Formas.Linha, fundo = Grafite.L3, fundoPressionado = Grafite.L4) { Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { IconeDesligar(Grafite.Vermelho, tamanho = 18.dp) } }
        }
        Row(Modifier.padding(top = 6.dp), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            for ((rot, ativo, clique, icone) in listOf<Quad>(
                Quad("Mudo", voz.mudo || voz.surdo, acoes.aoAlternarMudo) { IconeMicrofone(it, cortado = voz.mudo || voz.surdo, tamanho = 18.dp) },
                Quad("Surdo", voz.surdo, acoes.aoAlternarSurdo) { IconeFones(it, cortado = voz.surdo, tamanho = 18.dp) },
            )) {
                Toque(clique, Modifier.weight(1f).height(34.dp), Formas.Linha, fundo = if (ativo) Grafite.L4 else Grafite.L3, fundoPressionado = Grafite.L4) {
                    Row(Modifier.fillMaxSize(), horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) {
                        icone(if (ativo) Grafite.Vermelho else Grafite.Texto2)
                        Spacer(Modifier.width(6.dp))
                        Text(rot, style = Tipos.Botao.copy(fontSize = 12.5.sp, color = if (ativo) Grafite.Vermelho else Grafite.Texto2))
                    }
                }
            }
        }
    }
}

private data class Quad(val rotulo: String, val ativo: Boolean, val clique: () -> Unit, val icone: @Composable (Color) -> Unit)

private fun textoDaTransmissao(t: app.telacast.tela.EstadoDaTransmissao): Pair<String, Color> = when (t.fase) {
    app.telacast.tela.FaseDaTransmissao.PedindoPermissao -> "Aguardando a permissão do Android…" to Grafite.Ambar
    app.telacast.tela.FaseDaTransmissao.Conectando -> "Conectando a transmissão…" to Grafite.Ambar
    app.telacast.tela.FaseDaTransmissao.Transmitindo -> "Você está transmitindo · ${if (t.espectadores == 1) "1 assistindo" else "${t.espectadores} assistindo"}${t.tamanho?.let { " · ${it.first}x${it.second}" } ?: ""}" to Grafite.Ambar
    app.telacast.tela.FaseDaTransmissao.Reconectando -> "Sem conexão. Reconectando a transmissão…" to Grafite.Ambar
    app.telacast.tela.FaseDaTransmissao.Erro -> (t.mensagem ?: "A transmissão parou.") to Grafite.Vermelho
    app.telacast.tela.FaseDaTransmissao.Inativa -> "" to Grafite.Apagado
}

/** "Estatísticas" da chamada (2 vezes por segundo): rede, buffer, cortes, subfluxos e o que a captura está fazendo. Recolhido por padrão. */
@Composable
fun PainelDeEstatisticas(e: app.telacast.voz.EstatisticasDeVoz, abertoInicial: Boolean = false) {
    var aberto by androidx.compose.runtime.remember { androidx.compose.runtime.mutableStateOf(abertoInicial) }
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp)) {
        Toque({ aberto = !aberto }, Modifier.fillMaxWidth().height(32.dp)) {
            Row(Modifier.fillMaxSize(), verticalAlignment = Alignment.CenterVertically) {
                Text("Estatísticas da chamada", style = Tipos.Pequeno.copy(fontSize = 12.sp, color = Grafite.Apagado), modifier = Modifier.weight(1f))
                Text(if (aberto) "ocultar" else "mostrar", style = Tipos.Pequeno.copy(fontSize = 12.sp, color = Grafite.Ambar))
            }
        }
        if (aberto) {
            val linhas = listOf(
                "Perda" to "${"%.1f".format(e.perdaPct).replace('.', ',')}%",
                "Jitter" to "${e.jitterMs.toInt()} ms",
                "Buffer (alvo / agora)" to "${e.alvoMs} / ${e.bufferMs.toInt()} ms",
                "Cortes por minuto" to "${e.cortesPorMin}",
                "Subfluxos do alto-falante" to "${e.subfluxos}",
                "Ajustes de ritmo (−/+)" to "${e.esticados} / ${e.comprimidos}",
                "Nível do microfone" to "${(e.nivelMic * 100).toInt()}%",
                "Ganho de entrada" to "${"%+.1f".format(e.ganhoEntradaDb).replace('.', ',')} dB",
                "Ganho automático (AGC)" to "${"%+.1f".format(e.agcDb).replace('.', ',')} dB",
                "Probabilidade de voz (RNNoise)" to "${(e.probDeVoz * 100).toInt()}%",
                "Pacotes enviados" to "${e.pacotesEnviados}",
            )
            Column(Modifier.fillMaxWidth().background(Grafite.L3, Formas.Botao).border(1.dp, Grafite.Linha2, Formas.Botao).padding(horizontal = 12.dp, vertical = 8.dp)) {
                for ((k, v) in linhas) Row(Modifier.fillMaxWidth().padding(vertical = 2.dp)) {
                    Text(k, style = Tipos.Pequeno.copy(fontSize = 12.sp, color = Grafite.Texto2), modifier = Modifier.weight(1f))
                    Text(v, style = Tipos.Pequeno.copy(fontSize = 12.sp, color = Grafite.Texto, fontFamily = app.telacast.tema.Mono))
                }
            }
        }
    }
}
