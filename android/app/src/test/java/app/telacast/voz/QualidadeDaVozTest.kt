package app.telacast.voz

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.util.Random
import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.log10
import kotlin.math.sin

/** C6: passa-alta de 80 Hz, Opus a 48 kbps, time-stretch (emenda alinhada) e as estatísticas da recepção. */
class QualidadeDaVozTest {
    private fun db(x: Double) = 10 * log10(x + 1e-12)

    // ---------------------------------------------------------------- passa-alta 80 Hz
    private fun senoShort(freq: Double, quadros: Int, amp: Double = 8000.0) = ShortArray(quadros * QUADRO) { (amp * sin(2 * PI * freq * it / TAXA_DE_AMOSTRAGEM)).toInt().toShort() }

    private fun filtrar(x: ShortArray): ShortArray {
        val f = PassaAlta(); val y = ShortArray(x.size); val q = ShortArray(QUADRO); val o = ShortArray(QUADRO)
        for (k in 0 until x.size / QUADRO) { System.arraycopy(x, k * QUADRO, q, 0, QUADRO); f.aplicar(q, o); System.arraycopy(o, 0, y, k * QUADRO, QUADRO) }
        return y
    }

    private fun rms(x: ShortArray, de: Int) = Math.sqrt((de until x.size).sumOf { x[it].toDouble() * x[it] } / (x.size - de))

    @Test fun passaAltaTiraRonco() {
        val x = senoShort(20.0, 50); val y = filtrar(x)
        val atenuacao = db(rms(x, 9600) * rms(x, 9600)) - db(rms(y, 9600) * rms(y, 9600))
        println("passa-alta: 20 Hz atenuado ${"%.1f".format(atenuacao)} dB")
        assertTrue("20 Hz deveria cair mais de 20 dB (caiu $atenuacao)", atenuacao > 20)
    }

    @Test fun passaAltaDeixaAVozPassar() {
        for (f in listOf(300.0, 1000.0, 4000.0)) {
            val x = senoShort(f, 20); val y = filtrar(x)
            val perda = db(rms(x, 9600) * rms(x, 9600)) - db(rms(y, 9600) * rms(y, 9600))
            assertTrue("$f Hz perdeu $perda dB", abs(perda) < 0.6)
        }
        val corte = senoShort(80.0, 30).let { x -> val y = filtrar(x); db(rms(x, 9600).let { it * it }) - db(rms(y, 9600).let { it * it }) }
        assertTrue("em 80 Hz o corte é de ~3 dB (foi $corte)", corte in 2.0..4.5)
    }

    @Test fun passaAltaTiraNivelContinuoEMantemEstadoEntreQuadros() {
        val x = ShortArray(QUADRO * 40) { (3000 + 5000 * sin(2 * PI * 500.0 * it / TAXA_DE_AMOSTRAGEM)).toInt().toShort() }
        val y = filtrar(x)
        val media = (QUADRO * 20 until y.size).sumOf { y[it].toDouble() } / (y.size - QUADRO * 20)
        assertTrue("o nível contínuo some (sobrou $media)", abs(media) < 60)
        var maxBorda = 0; var maxInterno = 0
        for (i in QUADRO * 20 until y.size - 1) { val d = abs(y[i + 1] - y[i]); if ((i + 1) % QUADRO == 0) maxBorda = maxOf(maxBorda, d) else maxInterno = maxOf(maxInterno, d) }
        assertTrue("estalo na borda dos quadros ($maxBorda contra $maxInterno)", maxBorda <= maxInterno + 50)
    }

    // ---------------------------------------------------------------- Opus 48 kbps, banda total, pacote <= 400 bytes
    @Test fun opusA48kbpsNuncaPassaDe400Bytes() {
        assertEquals(48_000, BITRATE)
        val enc = ConcentusOpus().codificador()
        val r = Random(3)
        var maior = 0; var soma = 0L; var n = 0
        // fala sintética, ruído alto e silêncio: o pior caso de tamanho é o ruído forte
        for (tipo in 0..2) for (q in 0 until 200) {
            val pcm = ShortArray(QUADRO) { i -> when (tipo) {
                0 -> (6000 * sin(2 * PI * 150.0 * (q * QUADRO + i) / TAXA_DE_AMOSTRAGEM) + 3000 * sin(2 * PI * 450.0 * (q * QUADRO + i) / TAXA_DE_AMOSTRAGEM)).toInt().toShort()
                1 -> (r.nextGaussian() * 9000).toInt().coerceIn(-32768, 32767).toShort()
                else -> 0
            } }
            val p = enc.codificar(pcm)!!
            maior = maxOf(maior, p.size); if (tipo == 0) { soma += p.size; n++ }
        }
        println("Opus 48 kbps: maior pacote $maior bytes, média da fala ${soma / n} bytes")
        assertTrue("pacote de $maior bytes passa do limite do servidor", maior <= MAX_PACOTE)
    }

    @Test fun opusDeVerdadeIdaEVoltaAindaSoaComoOSinal() {
        val f = ConcentusOpus(); val enc = f.codificador(); val dec = f.decodificador()
        var erro = 0.0; var sinal = 0.0
        for (q in 0 until 100) {
            val pcm = ShortArray(QUADRO) { (7000 * sin(2 * PI * 440.0 * (q * QUADRO + it) / TAXA_DE_AMOSTRAGEM) + 2500 * sin(2 * PI * 5000.0 * (q * QUADRO + it) / TAXA_DE_AMOSTRAGEM)).toInt().toShort() }
            val saida = dec.decodificar(enc.codificar(pcm)!!)!!
            if (q > 10) { for (i in 0 until QUADRO) sinal += pcm[i].toDouble() * pcm[i] ; for (i in 0 until QUADRO) { /* energia do que saiu */ erro += saida[i].toDouble() * saida[i] } }
        }
        val dif = abs(db(erro) - db(sinal))
        assertTrue("a energia na saída (incluindo 5 kHz, banda larga) bate com a da entrada (difere $dif dB)", dif < 2.0)
    }

    // ---------------------------------------------------------------- time-stretch (emenda alinhada)
    private fun voz(n: Int, inicio: Int = 0): FloatArray = FloatArray(n) { val t = (inicio + it) / TAXA_DE_AMOSTRAGEM.toDouble(); (0.25 * sin(2 * PI * 140.0 * t) + 0.15 * sin(2 * PI * 280.0 * t) + 0.1 * sin(2 * PI * 420.0 * t)).toFloat() }

    private fun maiorSalto(x: FloatArray) = (1 until x.size).maxOf { abs(x[it] - x[it - 1]) }

    @Test fun esticadorSemDeltaCopiaOQuadroInteiro() {
        val e = voz(QUADRO * 3); val s = FloatArray(QUADRO)
        val r = Esticador.produzir(e, e.size, 0, s)
        assertEquals(QUADRO, r.consumidas); assertTrue(s.contentEquals(e.copyOf(QUADRO)))
        assertEquals("delta pequeno demais não vale a emenda", QUADRO, Esticador.produzir(e, e.size, 20, s).consumidas)
    }

    @Test fun aceleraConsumindoMaisEntradaSemEstalo() {
        val e = voz(QUADRO * 3); val s = FloatArray(QUADRO)
        val r = Esticador.produzir(e, e.size, 150, s)
        assertTrue("consumiu ${r.consumidas}", r.consumidas in QUADRO + Esticador.MIN_DELTA..QUADRO + Esticador.MAX_DELTA)
        assertEquals(r.consumidas, QUADRO + r.usado); assertTrue("escolheu d=${r.usado} perto de 150", abs(r.usado - 150) <= 64)
        assertTrue("salto de ${maiorSalto(s)} contra ${maiorSalto(e.copyOf(QUADRO))} no original", maiorSalto(s) <= maiorSalto(e.copyOf(QUADRO)) * 1.6f)
    }

    @Test fun desaceleraConsumindoMenosEntradaSemEstalo() {
        val e = voz(QUADRO * 3); val s = FloatArray(QUADRO)
        val r = Esticador.produzir(e, e.size, -150, s)
        assertTrue("consumiu ${r.consumidas}", r.consumidas in QUADRO - Esticador.MAX_DELTA..QUADRO - Esticador.MIN_DELTA)
        assertTrue("salto de ${maiorSalto(s)} contra ${maiorSalto(e.copyOf(QUADRO))}", maiorSalto(s) <= maiorSalto(e.copyOf(QUADRO)) * 1.6f)
    }

    @Test fun esticadorEmSequenciaMantemAOndaContinuaENoMaximo20PorCento() {
        // 40 quadros acelerados em 20%: sem nenhum estalo na saída inteira
        var pos = 0; val total = voz(QUADRO * 80); val saida = FloatArray(QUADRO * 40); val q = FloatArray(QUADRO)
        for (k in 0 until 40) {
            val janela = total.copyOfRange(pos, pos + QUADRO * 2)
            val r = Esticador.produzir(janela, janela.size, Esticador.MAX_DELTA, q)
            System.arraycopy(q, 0, saida, k * QUADRO, QUADRO); pos += r.consumidas
        }
        val razao = pos.toDouble() / saida.size
        println("aceleração obtida: ${"%.3f".format(razao)}x; maior salto ${maiorSalto(saida)} (original ${maiorSalto(total)})")
        assertTrue("razão $razao fora de 1,1 a 1,35", razao in 1.1..1.35)
        assertTrue("estalo na saída acelerada", maiorSalto(saida) <= maiorSalto(total) * 1.8f)
    }

    // ---------------------------------------------------------------- no fluxo de um usuário (jitter buffer + decodificador de mentira)
    /** O "pacote" carrega o número do quadro; o decodificador devolve a onda contínua daquele quadro (PLC repete o anterior). */
    private class DecodificadorDeOnda : DecodificadorOpus {
        var ultimo = 0
        override fun decodificar(pacote: ByteArray?): ShortArray? {
            val k = if (pacote == null) ultimo else java.nio.ByteBuffer.wrap(pacote).int.also { ultimo = it }
            return ShortArray(QUADRO) { i -> val t = (k * QUADRO + i) / TAXA_DE_AMOSTRAGEM.toDouble(); (32768 * (0.25 * sin(2 * PI * 140.0 * t) + 0.15 * sin(2 * PI * 280.0 * t) + 0.1 * sin(2 * PI * 420.0 * t))).toInt().toShort() }
        }
    }
    private fun pacote(k: Int) = java.nio.ByteBuffer.allocate(4).putInt(k).array()

    @Test fun fluxoSemDesvioNaoMexeNoAudio() {
        val f = FluxoDeUsuario(DecodificadorDeOnda(), alvoInicial = 3)
        val saida = ArrayList<Float>(); val acc = FloatArray(QUADRO)
        for (k in 0 until 60) {
            f.entregar(k, pacote(k))
            acc.fill(0f); f.misturarEm(acc, 1f); saida.addAll(acc.toList())
        }
        assertEquals(0, f.estat.esticados + f.estat.comprimidos)
        assertEquals(0, f.estat.perdidos)
    }

    @Test fun rajadaEhAbsorvidaAcelerandoEmVezDeCortar() {
        val a = FluxoDeUsuario(DecodificadorDeOnda(), alvoInicial = 3)
        // a rede segurou 12 pacotes e soltou de uma vez, depois voltou ao ritmo
        for (k in 0 until 12) a.entregar(k, pacote(k))
        val saida = ArrayList<Float>(); val acc = FloatArray(QUADRO)
        val profundidades = ArrayList<Float>()
        for (t in 0 until 50) {
            if (t >= 1) a.entregar(11 + t, pacote(11 + t))
            acc.fill(0f); a.misturarEm(acc, 1f); saida.addAll(acc.toList()); profundidades.add(a.profundidadeEmQuadros())
        }
        println("rajada: comprimidos=${a.estat.comprimidos} esticados=${a.estat.esticados} perdidos=${a.estat.perdidos} descartados=${a.estat.descartados}; profundidade ${"%.1f".format(profundidades.first())} -> ${"%.1f".format(profundidades.last())} quadros")
        assertTrue("o atraso acumulado volta ao alvo acelerando", a.estat.comprimidos > 0)
        assertTrue("profundidade final menor que a inicial", profundidades.last() < profundidades.first())
        assertEquals("nada foi tratado como perda", 0, a.estat.perdidos)
        val x = saida.toFloatArray()
        var maiorOriginal = 0f; for (i in 1 until x.size) maiorOriginal = maxOf(maiorOriginal, abs(x[i] - x[i - 1]))
        assertTrue("estalo no áudio acelerado: salto $maiorOriginal", maiorOriginal < 0.1f)
    }

    @Test fun desligarOEsticadorDeixaOAtrasoQuieto() {
        val a = FluxoDeUsuario(DecodificadorDeOnda(), alvoInicial = 3).also { it.esticar = false }
        for (k in 0 until 12) a.entregar(k, pacote(k))
        val acc = FloatArray(QUADRO)
        for (t in 0 until 30) { if (t >= 1) a.entregar(11 + t, pacote(11 + t)); acc.fill(0f); a.misturarEm(acc, 1f) }
        assertEquals(0, a.estat.comprimidos)
    }

    @Test fun naoEsticaNoSilencio() {
        val silencioso = object : DecodificadorOpus { override fun decodificar(pacote: ByteArray?) = ShortArray(QUADRO) }
        val a = FluxoDeUsuario(silencioso, alvoInicial = 3)
        for (k in 0 until 12) a.entregar(k, pacote(k))
        val acc = FloatArray(QUADRO)
        for (t in 0 until 30) { if (t >= 1) a.entregar(11 + t, pacote(11 + t)); acc.fill(0f); a.misturarEm(acc, 1f) }
        assertEquals("silêncio não é esticado (o buffer se ajusta sozinho)", 0, a.estat.comprimidos + a.estat.esticados)
    }

    // ---------------------------------------------------------------- estatísticas da recepção
    @Test fun jitterEPerdaNoResumo() {
        var t = 0L
        val r = RecepcaoDeVoz(object : FabricaDeOpus { override fun codificador() = throw UnsupportedOperationException(); override fun decodificador() = DecodificadorDeOnda() }, { t }, alvoInicial = 3)
        val rnd = Random(5)
        var seq = 0
        val saida = ShortArray(QUADRO)
        for (k in 0 until 200) {
            t += 20
            if (k % 10 == 5) seq++                  // some um pacote a cada 10
            if (rnd.nextInt(4) != 0 || k < 4) { r.entregar(QuadroRecebido(1, seq, pacote(seq))); }  // chegadas com folga irregular
            seq++
            r.reproduzir(saida)
        }
        val s = r.resumo()
        println("resumo: perda ${s.perdaPct}% jitter ${s.jitterMs} ms alvo ${s.alvoMs} ms buffer ${s.bufferMs} ms cortes/min ${s.cortesPorMin}")
        assertEquals(1, s.usuarios)
        assertTrue("jitter medido (${s.jitterMs})", s.jitterMs in 2f..30f)
        assertTrue("perda medida (${s.perdaPct}%) tem que ser positiva e não absurda", s.perdaPct in 4f..60f)
        assertTrue(s.alvoMs >= 40)
    }

    @Test fun motorMonta() {
        val m = MotorDeVoz(ConcentusOpus(), {}, { 0L })
        val e = m.estatisticas(3)
        assertEquals(3, e.subfluxos); assertEquals(0, e.usuarios)
        assertEquals("sem ganho de entrada o ganho é 0 dB", 0f, e.ganhoEntradaDb, 0.01f)
    }
}
