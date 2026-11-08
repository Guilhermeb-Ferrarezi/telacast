package app.telacast.voz

import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.log10
import kotlin.math.max
import kotlin.math.min
import kotlin.math.tanh

/** Nível RMS do quadro em dBFS (silêncio digital ≈ -120). Amostras de 16 bits. */
fun rmsDb(quadro: ShortArray): Float {
    if (quadro.isEmpty()) return -120f
    var soma = 0.0
    for (s in quadro) { val x = s / 32768.0; soma += x * x }
    return (10.0 * log10(soma / quadro.size + 1e-12)).toFloat()
}

/** Limitador suave (igual ao desktop): abaixo do joelho (0,7) não mexe; acima, curva em tanh que nunca passa de 1,0. */
fun limitadorSuave(x: Float): Float {
    if (!x.isFinite()) return 0f
    val a = abs(x)
    if (a <= 0.7f) return x
    val y = min(0.7f + 0.3f * tanh((a - 0.7f) / 0.3f), 1f)
    return if (x < 0) -y else y
}

private fun lerp(a: Float, b: Float, t: Float) = a + (b - a) * t.coerceIn(0f, 1f)

/**
 * Detector de voz por energia, com histerese, *hangover* e piso de ruído adaptativo: o mesmo do desktop (`Vad` em
 * `app/src/voice.rs`) no modo sem RNNoise. `sensibilidade` 0 (menos sensível) a 1 (mais), padrão 0,7.
 *
 * - abre quando o nível passa de `max(mínimo absoluto, piso + margem)`; mantém aberto acima do limiar de fechar
 *   (5 dB abaixo); depois da última evidência de voz ainda fica aberto por [HANGOVER_QUADROS] (360 ms);
 * - o piso de ruído sobe devagar e desce rápido enquanto está fechado.
 */
class Vad(sensibilidade: Float = SENSIBILIDADE_PADRAO) {
    var sensibilidade: Float = if (sensibilidade.isFinite()) sensibilidade.coerceIn(0f, 1f) else SENSIBILIDADE_PADRAO
        set(v) { if (v.isFinite()) field = v.coerceIn(0f, 1f) }
    var aberto = false
        private set
    /** O último quadro tinha voz de fato (acima do limiar de manter). */
    var evidencia = false
        private set
    var pisoDeRuidoDb = -60f
        private set
    private var resto = 0

    class Limiares(val abreDb: Float, val fechaDb: Float)

    fun limiares(): Limiares {
        val s = sensibilidade
        val abs = lerp(-40f, -58f, s)
        val m = lerp(12f, 7f, s)
        return Limiares(max(abs, pisoDeRuidoDb + m), max(abs - 5f, pisoDeRuidoDb + m - 5f))
    }

    fun reiniciar() { aberto = false; resto = 0; evidencia = false }

    fun processar(quadro: ShortArray): Boolean = processarDb(rmsDb(quadro))

    fun processarDb(dbEntrada: Float): Boolean {
        val db = if (dbEntrada.isFinite()) dbEntrada else -120f
        val l = limiares()
        if (!aberto) {
            if (db >= l.abreDb) {
                aberto = true; resto = HANGOVER_QUADROS; evidencia = true
            } else {
                evidencia = false
                val k = if (db < pisoDeRuidoDb) 0.25f else 0.005f
                pisoDeRuidoDb = (pisoDeRuidoDb + (db - pisoDeRuidoDb) * k).coerceIn(-90f, -35f)
            }
        } else if (db >= l.fechaDb) {
            resto = HANGOVER_QUADROS; evidencia = true
        } else {
            evidencia = false
            if (resto > 0) resto-- else aberto = false
        }
        return aberto
    }

    companion object {
        const val HANGOVER_QUADROS = 18
        const val SENSIBILIDADE_PADRAO = 0.7f
    }
}

/** Suaviza as bordas de uma fala: fade-in de 5 ms no primeiro quadro depois do silêncio e fade-out a partir do último valor. */
class SuavizadorDeBordas {
    var ativo = false
        private set
    private var ultimo = 0f

    fun reiniciar() { ativo = false; ultimo = 0f }

    /** Há áudio neste quadro (modifica `q` com o fade-in, se for o primeiro). */
    fun audio(q: FloatArray) {
        if (!ativo) {
            ativo = true
            val n = min(FADE, q.size)
            for (i in 0 until n) { val t = (i + 1f) / n; q[i] *= 0.5f - 0.5f * cos(PI.toFloat() * t) }
        }
        if (q.isNotEmpty()) ultimo = if (q.last().isFinite()) q.last() else 0f
    }

    /** Sem áudio: zera `q`; devolve `true` se escreveu o fade-out (precisa ser somado). */
    fun silencio(q: FloatArray): Boolean {
        q.fill(0f)
        if (!ativo) return false
        ativo = false
        val n = min(FADE, q.size)
        for (i in 0 until n) { val t = (i + 1f) / n; q[i] = ultimo * (0.5f + 0.5f * cos(PI.toFloat() * t)) }
        ultimo = 0f
        return true
    }

    private companion object { const val FADE = 240 }
}

/** Medidor de nível 0..1 com ataque rápido e queda lenta (para a barra do microfone). */
class MedidorDeNivel {
    var nivel = 0f
        private set

    fun atualizar(db: Float): Float {
        val alvo = ((db + 60f) / 50f).coerceIn(0f, 1f)
        nivel = if (alvo > nivel) alvo else nivel + (alvo - nivel) * 0.2f
        return nivel
    }

    fun reiniciar() { nivel = 0f }
}
