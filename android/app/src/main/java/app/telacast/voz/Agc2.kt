package app.telacast.voz

import kotlin.math.abs
import kotlin.math.log10
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sqrt
import kotlin.math.tanh

/**
 * Controle de ganho adaptativo digital igual ao AGC2 do WebRTC que o Discord usa (m146), lido do `discord_voice.node`
 * (docs/discord/11-agc2-nativo.md, seção 5). Trabalha em quadros de 10 ms (480 amostras) com a probabilidade de fala `p`
 * de um VAD (aqui, a do RNNoise; sem RNNoise, 1 quando o [Vad] de energia vê voz):
 *
 * - estimador de nível de fala (só confirma rajadas de ≥ 12 quadros com `p` alto, média ponderada por `p`);
 * - margem entre o pico e o nível médio da fala (12 a 25 dB, começa em 20);
 * - estimador de ruído (mínimo de energia em janelas de 5 s);
 * - ganho-alvo: leva o pico da fala a -5 dBFS (headroom), no máximo +50 dB, e nunca acima do que deixaria o ruído de saída
 *   passando de -50 dBFS; sobe no máximo 6 dB/s (e só depois de 12 quadros de fala), desce a 6 dB/s;
 * - limitador suave no fim (transparente até -0,75 dBFS, nunca passa de 0 dBFS).
 *
 * Diferenças para o Discord: o ganho começa em 0 dB (ele começa em +15, o que estoura nos primeiros segundos) e o limiar de
 * fala é [LIMIAR_P] (0,95 no WebRTC, calibrado para o RNN VAD deles; o do RNNoise é menos "cravado").
 */
class Agc2 {
    @Volatile var ligado = true
    var ganhoDb = 0f
        private set
    val nivelDeFalaDb: Float get() = fala.toFloat()
    val margemDb: Float get() = margem.toFloat()
    val ruidoDb: Float get() = ruido.toFloat()

    // ---- estimador de nível de fala
    private var fala = INICIAL_FALA_DB
    private var num = 0.0; private var den = 0.0; private var confianca = 400
    private var tNum = 0.0; private var tDen = 0.0; private var tConf = 400; private var rajada = 0
    // ---- protetor de saturação (margem)
    private var margem = 20.0
    private var maxPico = DB_MIN
    private var quadrosNoPeriodo = 0
    private val fila = DoubleArray(4); private var nFila = 0
    // ---- ruído
    private var ruido = -84.29
    private var energiaMin = Double.MAX_VALUE
    private var quadrosJanela = 0
    // ---- ganho
    private var contador = ESPERA_QUADROS
    private var ganhoLinPrev = 1f

    fun reiniciar() {
        ganhoDb = 0f; fala = INICIAL_FALA_DB; num = 0.0; den = 0.0; confianca = 400; rajada = 0; tNum = 0.0; tDen = 0.0; tConf = 400
        margem = 20.0; maxPico = DB_MIN; quadrosNoPeriodo = 0; nFila = 0; ruido = -84.29; energiaMin = Double.MAX_VALUE; quadrosJanela = 0
        contador = ESPERA_QUADROS
    }

    /** Aplica o ganho e o limitador a [q] (20 ms = 2 quadros de 10 ms, no lugar). [p] é a probabilidade de fala (0..1). */
    fun processar(q: ShortArray, p: Float) {
        var i = 0
        while (i + SUB <= q.size) { quadro(q, i, if (p.isFinite()) p else 0f); i += SUB }
    }

    private fun quadro(q: ShortArray, ini: Int, p: Float) {
        var soma = 0.0; var pico = 0
        for (k in ini until ini + SUB) { val x = q[k].toInt(); soma += x.toDouble() * x; pico = max(pico, abs(x)) }
        val rmsDb = dbfs(sqrt(soma / SUB)); val picoDb = dbfs(pico.toDouble())
        val ehFala = p >= LIMIAR_P

        // ruído: mínimo de energia por janela de 500 quadros (piso = RMS 2 em escala de 16 bits)
        energiaMin = min(energiaMin, max(soma, PISO_ENERGIA))
        if (++quadrosJanela >= 500) {
            val prel = 10 * log10(energiaMin / SUB) - 90.309
            ruido = if (ruido < prel) 0.5 * (ruido + prel) else prel
            energiaMin = Double.MAX_VALUE; quadrosJanela = 0
        }
        // nível de fala (só confirma rajadas de 12+ quadros)
        if (ehFala) {
            if (rajada == 0) { tNum = num; tDen = den; tConf = confianca }
            rajada++
            val vaza = if (tConf > 0) 1.0 else 0.9975
            tConf = max(0, tConf - 10)
            tNum = tNum * vaza + rmsDb * p; tDen = tDen * vaza + p
            if (rajada >= MIN_RAJADA) {
                num = tNum; den = tDen; confianca = tConf
                if (den > 0) fala = (num / den).coerceIn(-90.0, 30.0)
            }
        } else rajada = 0
        // margem (protetor de saturação), só com fala confirmada
        if (ehFala && rajada >= MIN_RAJADA) {
            maxPico = max(maxPico, picoDb)
            if (++quadrosNoPeriodo > 40) {
                if (nFila == 4) { for (j in 0 until 3) fila[j] = fila[j + 1]; nFila = 3 }
                fila[nFila++] = maxPico; maxPico = DB_MIN; quadrosNoPeriodo = 0
            }
            val ref = if (nFila == 0) maxPico else fila[0]
            val delta = ref - fala
            margem = if (delta > margem) 0.998849 * margem + 0.001151 * delta else 0.99977 * margem + 0.00023 * delta
            margem = margem.coerceIn(12.0, 25.0)
        }
        // ganho-alvo
        val entrada = fala + margem
        var alvo = when {
            entrada < -(HEADROOM + MAX_GANHO) -> MAX_GANHO
            entrada < -HEADROOM -> -HEADROOM - entrada
            else -> 0.0
        }
        alvo = min(alvo, max(MAX_RUIDO - ruido, 0.0))
        if (!ligado) alvo = 0.0
        // passo: só sobe depois de 12 quadros de fala seguidos; nunca mais que 6 dB/s
        val antes = contador
        if (!ehFala) contador = ESPERA_QUADROS else if (contador > 0) contador--
        val maxSobe = if (contador == 0) (if (antes > 0) ESPERA_QUADROS * PASSO_DB else PASSO_DB) else 0.0
        var d = alvo - ganhoDb
        if (maxSobe == 0.0) d = min(d, 0.0)
        d = d.coerceIn(-PASSO_DB, maxSobe)
        val g0 = ganhoDb
        ganhoDb = (ganhoDb + d).toFloat().coerceIn(0f, MAX_GANHO.toFloat())
        aplicar(q, ini, g0, ganhoDb)
    }

    private fun aplicar(q: ShortArray, ini: Int, g0: Float, g1: Float) {
        val l0 = Math.pow(10.0, g0 / 20.0).toFloat(); val l1 = Math.pow(10.0, g1 / 20.0).toFloat()
        if (l0 == 1f && l1 == 1f) return
        for (k in 0 until SUB) {
            val l = l0 + (l1 - l0) * ((k + 1f) / SUB)
            val x = q[ini + k] / 32768f * l
            val a = abs(x)
            val y = if (a <= LIMITE) a else min(LIMITE + (1f - LIMITE) * tanh((a - LIMITE) / (1f - LIMITE)), 1f)
            q[ini + k] = ((if (x < 0) -y else y) * 32767f).toInt().coerceIn(-32768, 32767).toShort()
        }
    }

    companion object {
        const val SUB = 480
        const val HEADROOM = 5.0
        const val MAX_GANHO = 50.0
        const val MAX_RUIDO = -50.0
        const val PASSO_DB = 0.06
        const val ESPERA_QUADROS = 12
        const val MIN_RAJADA = 12
        const val INICIAL_FALA_DB = -40.0
        const val LIMIAR_P = 0.9f
        const val DB_MIN = -90.309
        const val LIMITE = 0.917f // -0,75 dBFS
        const val PISO_ENERGIA = 4.0 * SUB
        fun dbfs(x: Double) = if (x > 1.0) 20 * log10(x) - 90.309 else -90.309
    }
}
