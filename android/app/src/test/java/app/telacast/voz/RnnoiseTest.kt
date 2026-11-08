package app.telacast.voz

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Before
import org.junit.Test
import java.util.Random
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.log10
import kotlin.math.sin
import kotlin.math.sqrt

/**
 * RNNoise de verdade (a ponte nativa em `android/native/rnnoise`, o mesmo `nnnoiseless` do desktop), rodando na JVM com a
 * versão do PC. Ignorado se `scripts/rnnoise.sh --host` ainda não foi rodado.
 */
class RnnoiseTest {
    @Before fun precisaDaBiblioteca() = assumeTrue("falta a ponte nativa do PC: rode scripts/rnnoise.sh --host", RnnoiseNativo.disponivel)

    private val taxa = 48_000.0
    private fun db(x: Double) = 10 * log10(x + 1e-12)
    private fun energia(a: FloatArray, de: Int = 0, ate: Int = a.size): Double { var s = 0.0; for (i in de until ate) s += a[i].toDouble() * a[i]; return s / (ate - de) }

    /** Fala sintética: harmônicas de 140 Hz com envelope espectral decrescente, vibrato leve e modulação silábica de ~4 Hz. */
    private fun fala(n: Int, amp: Double = 6000.0): FloatArray = FloatArray(n) { i ->
        val t = i / taxa
        val f0 = 140.0 * (1 + 0.03 * sin(2 * PI * 5.0 * t))
        val silaba = 0.5 + 0.5 * sin(2 * PI * 4.0 * t - PI / 2)
        var v = 0.0
        for (h in 1..20) v += sin(2 * PI * f0 * h * t) / (h * 0.8)
        (amp * silaba * v / 2.5).toFloat()
    }

    /** Ruído rosa (1/f, filtro de Paul Kellet): parecido com ventilador, ar-condicionado e chiado de sala. */
    private fun ruidoRosa(n: Int, amp: Double, semente: Long = 1): FloatArray {
        val r = Random(semente)
        var b0 = 0.0; var b1 = 0.0; var b2 = 0.0; var b3 = 0.0; var b4 = 0.0; var b5 = 0.0; var b6 = 0.0
        return FloatArray(n) {
            val w = r.nextGaussian()
            b0 = 0.99886 * b0 + w * 0.0555179; b1 = 0.99332 * b1 + w * 0.0750759; b2 = 0.96900 * b2 + w * 0.1538520
            b3 = 0.86650 * b3 + w * 0.3104856; b4 = 0.55000 * b4 + w * 0.5329522; b5 = -0.7616 * b5 - w * 0.0168980
            val o = b0 + b1 + b2 + b3 + b4 + b5 + b6 + w * 0.5362; b6 = w * 0.115926
            (o * 0.11 * amp).toFloat()
        }
    }

    /** Passa o sinal inteiro pelo supressor em quadros de 480; devolve a saída e as probabilidades de voz. */
    private fun suprimir(x: FloatArray): Pair<FloatArray, FloatArray> {
        val s = SupressorRnnoise.criar()
        assertNotNull(s)
        val saida = FloatArray(x.size); val probs = FloatArray(x.size / 480)
        val e = FloatArray(480); val o = FloatArray(480)
        s!!.use {
            for (q in 0 until x.size / 480) {
                x.copyInto(e, 0, q * 480, q * 480 + 480)
                probs[q] = it.processar(e, o)
                o.copyInto(saida, q * 480)
            }
        }
        return saida to probs
    }

    /** Envelope silábico da [fala] (0 a 1) no instante da amostra `i`. */
    private fun envelope(i: Int) = 0.5 + 0.5 * sin(2 * PI * 4.0 * (i / taxa) - PI / 2)

    /**
     * Relação "pico da sílaba / intervalo entre sílabas" em dB: energia dos quadros de 10 ms em pleno pico contra a dos
     * quadros de intervalo (com um quadro de folga, porque o RNNoise atrasa a saída). É a SNR que importa: o que sobra de
     * ruído quando ninguém fala. (Comparar amostra a amostra com a fala limpa não serve: a saída tem atraso e fase própria.)
     */
    private fun picoSobreIntervalo(x: FloatArray, de: Int): Double {
        var pico = 0.0; var np = 0; var gap = 0.0; var ng = 0
        for (q in (de / 480 + 1) until x.size / 480 - 1) {
            val env = (q - 1..q + 1).map { envelope(it * 480 + 240) }
            val en = energia(x, q * 480, q * 480 + 480)
            if (env.all { it > 0.8 }) { pico += en; np++ } else if (env.all { it < 0.15 }) { gap += en; ng++ }
        }
        return db(pico / np) - db(gap / ng)
    }

    @Test fun ruidoDeFundoSozinhoEhMuitoAtenuado() {
        val x = ruidoRosa(48_000 * 4, 300.0) // ≈ -50 dBFS, como um ventilador distante
        val (y, probs) = suprimir(x)
        val atenuacao = db(energia(x, 48_000)) - db(energia(y, 48_000))
        println("ruído rosa -50 dBFS: atenuação ${"%.1f".format(atenuacao)} dB, prob de voz ${"%.2f".format(probs.drop(100).average())}")
        assertTrue("o ruído deveria cair pelo menos 30 dB (caiu $atenuacao)", atenuacao > 30)
    }

    @Test fun ruidoMaisForteTambemEhAtenuadoEAFalaFicaMaisLimpa() {
        val n = 48_000 * 6
        val v = fala(n)
        for (amp in listOf(1000.0, 3000.0)) {
            val r = ruidoRosa(n, amp, 9)
            val x = FloatArray(n) { v[it] + r[it] }
            val (y, _) = suprimir(x)
            val antes = picoSobreIntervalo(x, 48_000); val depois = picoSobreIntervalo(y, 48_000)
            println("fala + ruído rosa $amp: pico/intervalo ${"%.1f".format(antes)} -> ${"%.1f".format(depois)} dB")
            assertTrue("a SNR deveria melhorar pelo menos 6 dB (de $antes para $depois, ruído $amp)", depois > antes + 6)
        }
    }

    @Test fun falaSinteticaSemRuidoContinuaAudivel() {
        val n = 48_000 * 3
        val v = fala(n)
        val (y, probs) = suprimir(v)
        val perda = db(energia(v, 24_000)) - db(energia(y, 24_000))
        println("fala limpa: perda ${"%.1f".format(perda)} dB, prob média ${"%.2f".format(probs.drop(50).average())}")
        assertTrue("a fala limpa não pode ser engolida (perdeu $perda dB)", perda < 3)
    }

    @Test fun probabilidadeDeVozDistingueFalaDeRuido() {
        val n = 48_000 * 3
        val (_, pf) = suprimir(fala(n)); val (_, pr) = suprimir(ruidoRosa(n, 300.0))
        val mf = pf.drop(100).average(); val mr = pr.drop(100).average()
        println("prob de voz: fala ${"%.2f".format(mf)}, ruído ${"%.2f".format(mr)}")
        assertTrue("fala ($mf) deveria ter bem mais probabilidade que ruído ($mr)", mf > mr + 0.5)
    }

    @Test fun semEstalosNasBordasDosQuadros() {
        val n = 48_000 * 2
        val v = fala(n); val r = ruidoRosa(n, 1000.0, 3)
        val (y, _) = suprimir(FloatArray(n) { v[it] + r[it] })
        var maxInterno = 0f; var maxBorda = 0f
        for (i in 49_000 until n - 1) {
            val d = abs(y[i + 1] - y[i])
            if ((i + 1) % 480 == 0) maxBorda = maxOf(maxBorda, d) else maxInterno = maxOf(maxInterno, d)
        }
        println("degrau máximo: dentro do quadro $maxInterno, na borda $maxBorda")
        assertTrue("estalo na borda: $maxBorda contra $maxInterno dentro do quadro", maxBorda <= maxInterno * 1.5f + 50f)
    }

    /** A saída da ponte JNI é a do `nnnoiseless` puro (0.5.2, o mesmo do desktop): vetores gerados por `examples/ruido.rs vetores`. */
    @Test fun ponteJniIgualAoNnnoiselessDoDesktop() {
        fun f32(nome: String): FloatArray {
            val b = java.nio.ByteBuffer.wrap(RnnoiseTest::class.java.getResourceAsStream("/rnnoise/$nome")!!.use { it.readBytes() }).order(java.nio.ByteOrder.LITTLE_ENDIAN)
            return FloatArray(b.remaining() / 4) { b.getFloat() }
        }
        val entrada = f32("entrada.f32"); val esperada = f32("saida.f32"); val probEsperada = f32("prob.f32")
        val (saida, probs) = suprimir(entrada)
        assertEquals(esperada.size, saida.size)
        var maxErro = 0f
        for (i in saida.indices) maxErro = maxOf(maxErro, abs(saida[i] - esperada[i]))
        var maxErroProb = 0f
        for (i in probs.indices) maxErroProb = maxOf(maxErroProb, abs(probs[i] - probEsperada[i]))
        println("ponte JNI x nnnoiseless: erro máximo da saída $maxErro (escala de 16 bits), das probabilidades $maxErroProb")
        assertTrue("saída difere do nnnoiseless puro em $maxErro", maxErro < 1e-3f)
        assertTrue("probabilidades diferem em $maxErroProb", maxErroProb < 1e-3f)
    }

    @Test fun reiniciarEEstadoNovo() {
        val s = SupressorRnnoise.criar()!!
        val e = ruidoRosa(480, 1000.0); val a = FloatArray(480); val b = FloatArray(480)
        s.use { for (k in 0 until 20) s.processar(e, a); s.reiniciar(); val p1 = s.processar(e, a)
            val novo = SupressorRnnoise.criar()!!; val p2 = novo.use { novo.processar(e, b) }
            assertEquals(p2, p1, 1e-6f); assertTrue(a.contentEquals(b)) }
    }

    @Test fun usarDepoisDeFecharNaoQuebraEPassaOSinal() {
        val s = SupressorRnnoise.criar()!!
        s.close()
        val e = FloatArray(480) { it.toFloat() }; val o = FloatArray(480)
        assertEquals(-1f, s.processar(e, o), 0f)
        assertTrue(e.contentEquals(o))
        s.close() // duplo close é inofensivo
    }

    @Test fun quadroDeTamanhoErradoPassaDireto() {
        SupressorRnnoise.criar()!!.use { s ->
            val e = FloatArray(100) { 1f }; val o = FloatArray(100)
            assertEquals(-1f, s.processar(e, o), 0f)
        }
    }

    // ---------------------------------------------------------------- na cadeia de envio (ganho -> RNNoise -> VAD -> AGC -> Opus)

    private class CodificadorGravador : CodificadorOpus {
        val pcm = ArrayList<ShortArray>()
        override fun codificar(quadro: ShortArray): ByteArray { pcm += quadro.copyOf(); return byteArrayOf(1, 2, 3) }
    }

    private fun quadros(x: FloatArray) = (0 until x.size / QUADRO).map { q -> ShortArray(QUADRO) { x[q * QUADRO + it].toInt().coerceIn(-32768, 32767).toShort() } }

    @Test fun ruidoDeFundoSozinhoNaoAbreOVadComRnnoise() {
        val cod = CodificadorGravador()
        PipelineDeEnvio(cod, 1, SupressorRnnoise).use { p ->
            assertTrue(p.supressorProprio)
            var enviados = 0
            for ((i, q) in quadros(ruidoRosa(48_000 * 4, 300.0)).withIndex()) { p.processar(q) { _, _ -> if (i > 50) enviados++ } }
            assertEquals("depois de aquecer, ruído sozinho não passa", 0, enviados)
        }
    }

    @Test fun falaComRuidoAbreOVadEPassaPeloRnnoise() {
        val cod = CodificadorGravador()
        PipelineDeEnvio(cod, 1, SupressorRnnoise).use { p ->
            val n = 48_000 * 4
            val v = fala(n, 9000.0); val r = ruidoRosa(n, 500.0, 5)
            var enviados = 0
            for (q in quadros(FloatArray(n) { v[it] + r[it] })) p.processar(q) { _, _ -> enviados++ }
            assertTrue("a fala tem que passar o VAD pós-RNNoise (enviados $enviados de ${n / QUADRO})", enviados > n / QUADRO / 4)
            assertTrue("a probabilidade de voz foi lida", p.probDeVoz in 0f..1f)
        }
    }

    @Test fun ligarEDesligarASupressaoNoMeioNaoFazDegrau() {
        val cod = CodificadorGravador()
        PipelineDeEnvio(cod, 1, SupressorRnnoise).use { p ->
            val v = fala(48_000 * 3, 4000.0)
            val qs = quadros(v)
            for ((i, q) in qs.withIndex()) {
                if (i == 60) p.supressao = false
                if (i == 90) p.supressao = true
                p.processar(q) { _, _ -> }
            }
            // salto entre o último ponto de um quadro e o primeiro do próximo, comparado ao maior passo interno (fala suave)
            val pcm = cod.pcm
            var maxInterno = 0; var maxBorda = 0
            for (k in 40 until pcm.size - 1) {
                for (i in 0 until QUADRO - 1) maxInterno = maxOf(maxInterno, abs(pcm[k][i + 1] - pcm[k][i]))
                maxBorda = maxOf(maxBorda, abs(pcm[k + 1][0] - pcm[k][QUADRO - 1]))
            }
            println("liga/desliga: degrau máximo dentro $maxInterno, na borda $maxBorda")
            assertTrue("degrau de $maxBorda nas bordas contra $maxInterno dentro dos quadros", maxBorda <= maxInterno * 2 + 200)
        }
    }

    @Test fun semAFabricaOuSemABibliotecaOPipelineSegueSemSupressor() {
        val sem = PipelineDeEnvio(CodificadorGravador(), 1, null)
        assertFalse(sem.supressorProprio)
        var n = 0
        for (q in quadros(fala(48_000, 9000.0))) sem.processar(q) { _, _ -> n++ }
        assertTrue(n > 0)
        val nula = PipelineDeEnvio(CodificadorGravador(), 1) { null }
        assertFalse(nula.supressorProprio)
    }

    @Test fun custoPorQuadroDe20ms() {
        val s = SupressorRnnoise.criar()!!
        val e = ruidoRosa(480, 1000.0); val o = FloatArray(480)
        s.use {
            repeat(500) { s.processar(e, o) } // aquecimento do JIT
            val t0 = System.nanoTime(); val n = 2000
            repeat(n) { s.processar(e, o) }
            val porQuadro10ms = (System.nanoTime() - t0) / n / 1000.0
            println("RNNoise (JNI, PC): ${"%.1f".format(porQuadro10ms)} µs por quadro de 10 ms (${"%.1f".format(porQuadro10ms * 2)} µs por quadro de 20 ms)")
            assertTrue("deveria levar bem menos que os 10 ms do quadro", porQuadro10ms < 5000)
        }
    }
}
