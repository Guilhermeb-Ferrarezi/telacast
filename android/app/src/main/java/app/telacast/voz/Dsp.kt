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

/**
 * Controle automático de ganho por software (porte do `Agc` do desktop): leva a fala a -22 dBFS RMS com ganho de 0 a +24 dB
 * (nunca atenua), só adapta quando o VAD vê voz (congela no silêncio, para não subir o ruído), ataque de ~50 ms e liberação
 * de ~1 s, ganho interpolado dentro do quadro e limitador suave no fim. É o que faz a voz de quem está longe do microfone
 * chegar no mesmo volume de quem está perto (o AGC do sistema, quando existe, costuma ser fraco ou ausente).
 */
class Agc {
    @Volatile var ligado = true
    var ganhoDb = 0f
        private set

    fun reiniciar() { ganhoDb = 0f }

    /** Aplica o ganho ao [quadro] de 20 ms (no lugar). [nivelDb] é o RMS antes do AGC; [fala] diz se há voz neste quadro. */
    fun processar(quadro: ShortArray, nivelDb: Float, fala: Boolean) {
        val g0 = ganhoDb
        val ligado = ligado
        val desejado = if (!ligado) 0f else if (fala && nivelDb.isFinite()) (ALVO_DB - nivelDb).coerceIn(0f, MAX_DB) else g0
        val k = if (desejado < g0 || !ligado) ATAQUE else LIBERACAO
        val g1 = (g0 + (desejado - g0) * k).coerceIn(0f, MAX_DB)
        ganhoDb = g1
        if (g0 == 0f && g1 == 0f) return
        val l0 = Math.pow(10.0, g0 / 20.0).toFloat()
        val l1 = Math.pow(10.0, g1 / 20.0).toFloat()
        val n = max(quadro.size, 1).toFloat()
        for (i in quadro.indices) {
            val l = l0 + (l1 - l0) * ((i + 1) / n)
            val x = joelho(quadro[i] / 32768f * l)
            quadro[i] = (x * 32767f).toInt().coerceIn(-32768, 32767).toShort()
        }
    }

    private fun joelho(x: Float): Float {
        if (!x.isFinite()) return 0f
        val a = abs(x)
        if (a <= JOELHO) return x
        val y = min(JOELHO + (1f - JOELHO) * tanh((a - JOELHO) / (1f - JOELHO)), 1f)
        return if (x < 0) -y else y
    }

    companion object {
        const val ALVO_DB = -22f
        const val MAX_DB = 24f
        const val JOELHO = 0.85f
        const val ATAQUE = 0.3297f // 1 - exp(-20/50)
        const val LIBERACAO = 0.0198f // 1 - exp(-20/1000)
    }
}

/**
 * Ganho de entrada automático ANTES do supressor de ruído. O RNNoise trata fala baixa (ou longe do microfone) como ruído e a
 * corta, e o VAD que depende dele fecha: por isso a voz precisa chegar ao RNNoise num nível normal. Aqui não há alvo de
 * volume final (isso é do [Agc], depois): só se traz o sinal para perto de [ALVO_DB].
 *
 * - "evidência de fala" = nível acima do piso de ruído estimado + [MARGEM_DB] (e acima de [MINIMO_DB]); o piso desce rápido
 *   e sobe devagar, então o ruído constante de fundo não conta como fala;
 * - com evidência o ganho sobe depressa (~100 ms) até `ALVO_DB - nível` (no máximo [MAX_DB]); sem evidência desce devagar
 *   ([QUEDA_DB_POR_QUADRO]), para o ruído do silêncio não ser amplificado por muito tempo;
 * - o ganho muda de forma contínua dentro do quadro e termina num limitador suave.
 */
class PreAgc {
    @Volatile var ligado = true
    var ganhoDb = 0f
        private set
    var pisoDb = -70f
        private set
    // piso = mínimo do nível nos últimos 1,5 s (75 quadros): o ruído de fundo constante vira piso, a fala (com pausas) não
    private val janela = FloatArray(JANELA)
    private var preenchidos = 0
    private var pos = 0

    fun reiniciar() { ganhoDb = 0f; pisoDb = -70f; preenchidos = 0; pos = 0 }

    /** Aplica o ganho a [q] (20 ms, no lugar). Devolve o nível (dBFS) medido ANTES do ganho. */
    fun processar(q: ShortArray): Float {
        val db = rmsDb(q)
        if (!ligado) { ganhoDb = 0f; return db }
        janela[pos] = db; pos = (pos + 1) % JANELA; if (preenchidos < JANELA) preenchidos++
        var mn = Float.MAX_VALUE
        for (i in 0 until preenchidos) if (janela[i] < mn) mn = janela[i]
        pisoDb = mn.coerceIn(-90f, -35f)
        val evidencia = db > MINIMO_DB && db > pisoDb + MARGEM_DB
        val g0 = ganhoDb
        ganhoDb = if (evidencia) {
            val desejado = (ALVO_DB - db).coerceIn(0f, MAX_DB)
            val k = if (desejado > g0) SUBIDA else DESCIDA
            (g0 + (desejado - g0) * k).coerceIn(0f, MAX_DB)
        } else {
            max(0f, g0 - QUEDA_DB_POR_QUADRO)
        }
        val g1 = ganhoDb
        if (g0 == 0f && g1 == 0f) return db
        val l0 = Math.pow(10.0, g0 / 20.0).toFloat()
        val l1 = Math.pow(10.0, g1 / 20.0).toFloat()
        val n = max(q.size, 1).toFloat()
        for (i in q.indices) {
            val l = l0 + (l1 - l0) * ((i + 1) / n)
            val x = q[i] / 32768f * l
            val a = abs(x)
            val y = if (a <= 0.85f) x else { val s = min(0.85f + 0.15f * tanh((a - 0.85f) / 0.15f), 1f); if (x < 0) -s else s }
            q[i] = (y * 32767f).toInt().coerceIn(-32768, 32767).toShort()
        }
        return db
    }

    companion object {
        const val JANELA = 75
        const val ALVO_DB = -30f
        const val MAX_DB = 30f
        const val MINIMO_DB = -68f
        const val MARGEM_DB = 6f
        const val SUBIDA = 0.18f // ~100 ms
        const val DESCIDA = 0.3f
        const val QUEDA_DB_POR_QUADRO = 0.08f // ~4 dB/s
    }
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

    /** `probAbre`/`probFecha`: probabilidade de voz do RNNoise para abrir/manter (0 sem RNNoise). */
    class Limiares(val abreDb: Float, val fechaDb: Float, val probAbre: Float = 0f, val probFecha: Float = 0f)

    /**
     * Limiares de agora. Com RNNoise ([comProb]) a decisão principal é a probabilidade de voz e o piso de energia é bem
     * mais baixo (≈ -61 dBFS na sensibilidade 0,7), porque o ruído já foi tirado; sem ele, só energia.
     */
    fun limiares(comProb: Boolean = false): Limiares {
        val s = sensibilidade
        return if (comProb) {
            val abs = lerp(-50f, -66f, s)
            val m = lerp(12f, 6f, s)
            Limiares(max(abs, pisoDeRuidoDb + m), max(abs - 4f, pisoDeRuidoDb + m - 4f), lerp(0.60f, 0.25f, s), lerp(0.30f, 0.10f, s))
        } else {
            val abs = lerp(-40f, -58f, s)
            val m = lerp(12f, 7f, s)
            Limiares(max(abs, pisoDeRuidoDb + m), max(abs - 5f, pisoDeRuidoDb + m - 5f))
        }
    }

    fun reiniciar() { aberto = false; resto = 0; evidencia = false }

    fun processar(quadro: ShortArray): Boolean = processarDb(rmsDb(quadro))

    fun processarDb(dbEntrada: Float): Boolean = processarDbProb(dbEntrada, null)

    /** [prob] = probabilidade de voz do RNNoise neste quadro (0 a 1); `null` = sem RNNoise, só energia. */
    fun processarDbProb(dbEntrada: Float, prob: Float?): Boolean {
        val db = if (dbEntrada.isFinite()) dbEntrada else -120f
        val l = limiares(prob != null)
        val okAbre = prob == null || prob >= l.probAbre
        val apoio = db >= l.fechaDb && (prob == null || prob >= l.probFecha)
        if (!aberto) {
            if (db >= l.abreDb && okAbre) {
                aberto = true; resto = HANGOVER_QUADROS; evidencia = true
            } else {
                evidencia = false
                val k = if (db < pisoDeRuidoDb) 0.25f else 0.005f
                pisoDeRuidoDb = (pisoDeRuidoDb + (db - pisoDeRuidoDb) * k).coerceIn(-90f, -35f)
            }
        } else if (apoio) {
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
        /** No celular o microfone costuma estar mais longe da boca: abre com menos energia (limiar absoluto ≈ -55 dBFS). */
        const val SENSIBILIDADE_PADRAO_ANDROID = 0.85f
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

/**
 * Filtro passa-alta de 80 Hz (biquad Butterworth de 2ª ordem) na captura, ANTES do ganho de entrada: tira ronco, vento e o "pop" de
 * manusear o celular, que gastam bitrate do Opus e enganam o VAD e o AGC. Guarda o estado entre quadros (sem estalo na emenda).
 */
class PassaAlta(frequencia: Double = 80.0, taxa: Double = TAXA_DE_AMOSTRAGEM.toDouble()) {
    private val b0: Float; private val b1: Float; private val b2: Float; private val a1: Float; private val a2: Float
    private var x1 = 0f; private var x2 = 0f; private var y1 = 0f; private var y2 = 0f

    init {
        val w0 = 2 * PI * frequencia / taxa
        val alfa = Math.sin(w0) / (2 * 0.70710678)
        val c = cos(w0)
        val a0 = 1 + alfa
        b0 = ((1 + c) / 2 / a0).toFloat(); b1 = (-(1 + c) / a0).toFloat(); b2 = b0
        a1 = (-2 * c / a0).toFloat(); a2 = ((1 - alfa) / a0).toFloat()
    }

    fun reiniciar() { x1 = 0f; x2 = 0f; y1 = 0f; y2 = 0f }

    /** Filtra `entrada` para `saida` (podem ser o mesmo array). */
    fun aplicar(entrada: ShortArray, saida: ShortArray) {
        for (i in entrada.indices) {
            val x = entrada[i].toFloat()
            val y = b0 * x + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2
            x2 = x1; x1 = x; y2 = y1; y1 = if (y.isFinite()) y else 0f
            saida[i] = y1.toInt().coerceIn(-32768, 32767).toShort()
        }
    }
}
