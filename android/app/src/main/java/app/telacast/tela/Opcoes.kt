package app.telacast.tela

import kotlin.math.max
import kotlin.math.min
import kotlin.math.sqrt

/** Qualidade da transmissão: o lado MENOR do vídeo (a tela do celular é mais alta do que larga, ou o contrário). */
enum class QualidadeDeTransmissao(val rotulo: String, val ladoMenor: Int) {
    P480("480p", 480), P720("720p", 720), P1080("1080p", 1080);
}

/**
 * Escolhas do diálogo Transmitir. `kbps == null` é automático (a tabela de [kbpsSugerido]). O padrão é 720p a 30 fps, perto de
 * 2,5 Mbps: tela de celular tem pouco movimento e o upload móvel é o gargalo.
 */
data class OpcoesDeTransmissao(
    val qualidade: QualidadeDeTransmissao = QualidadeDeTransmissao.P720,
    val fps: Int = 30,
    val kbps: Int? = null,
) {
    fun kbpsEfetivo(): Int = kbps ?: kbpsSugerido(qualidade.ladoMenor, fps)

    companion object {
        val FPS = listOf(15, 30, 60)

        /** Tabela de partida (kbps) por lado menor e fps, interpolada pela taxa de pixels como a do desktop (`suggested_kbps`), mais enxuta. */
        fun kbpsSugerido(ladoMenor: Int, fps: Int): Int {
            val taxa = { h: Int, f: Int -> (h * 16.0 / 9.0) * h * f }
            val pts = listOf(Triple(480, 30, 1200), Triple(720, 30, 2500), Triple(720, 60, 4000), Triple(1080, 30, 4500), Triple(1080, 60, 7500)).map { taxa(it.first, it.second) to it.third.toDouble() }
            val r = taxa(max(ladoMenor, 1), max(fps, 1))
            val k = when {
                r <= pts.first().first -> pts.first().second * r / pts.first().first
                r >= pts.last().first -> pts.last().second * r / pts.last().first
                else -> { val i = pts.indexOfFirst { it.first >= r }; val a = pts[i - 1]; val b = pts[i]; a.second + (b.second - a.second) * (r - a.first) / (b.first - a.first) }
            }
            return max(Math.round(k).toInt(), 500)
        }
    }
}

/** Macroblocos (16 x 16) por quadro que o nível 4.1 do H.264 aceita; acima disso muitos codificadores de celular recusam. */
const val MAX_MACROBLOCOS = 8160

/**
 * Tamanho do vídeo para uma tela de `larguraTela` x `alturaTela` e o lado menor pedido: mantém a proporção, nunca amplia
 * além da tela, não passa de [MAX_MACROBLOCOS] e arredonda para baixo para múltiplos de 16 (o que os codificadores de
 * hardware esperam; o desvio de proporção é de no máximo ~1%). Gira junto com a tela: retrato dá vídeo em pé.
 */
fun tamanhoDaCodificacao(larguraTela: Int, alturaTela: Int, ladoMenorAlvo: Int): Pair<Int, Int> {
    val lt = max(larguraTela, 16); val at = max(alturaTela, 16)
    val menor = min(lt, at)
    var k = min(ladoMenorAlvo.toDouble() / menor, 1.0)
    var l = lt * k; var a = at * k
    val mb = (l / 16) * (a / 16)
    if (mb > MAX_MACROBLOCOS) { val f = sqrt(MAX_MACROBLOCOS / mb); l *= f; a *= f }
    fun p16(x: Double) = max((x / 16).toInt() * 16, 16)
    return p16(l) to p16(a)
}

/** `http://h:p` -> `ws://h:p/publish/12?token=T` (e `/watch/`); `https` vira `wss`. */
fun urlDeVideo(baseHttp: String, rota: String, canal: Long, token: String): String {
    val ws = when { baseHttp.startsWith("https://") -> "wss://" + baseHttp.removePrefix("https://"); baseHttp.startsWith("http://") -> "ws://" + baseHttp.removePrefix("http://"); else -> baseHttp }
    return "${ws.trimEnd('/')}/$rota/$canal?token=$token"
}
