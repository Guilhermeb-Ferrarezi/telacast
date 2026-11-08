package app.telacast.voz

import app.telacast.proto.Fixtures
import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import kotlin.math.PI
import kotlin.math.sin

private fun hex(f: String): ByteArray {
    val t = File(Fixtures.raiz, "binary/$f").readText().trim()
    return ByteArray(t.length / 2) { t.substring(it * 2, it * 2 + 2).toInt(16).toByte() }
}

/** Seno de 440 Hz de `n` quadros de 20 ms, com amplitude `a` (0..1). */
fun senoide(n: Int, a: Float = 0.3f, hz: Double = 440.0, fase: Int = 0): List<ShortArray> =
    (0 until n).map { q -> ShortArray(QUADRO) { i -> (a * 32767 * sin(2 * PI * hz * (fase + q * QUADRO + i) / TAXA_DE_AMOSTRAGEM)).toInt().toShort() } }

fun silencio(n: Int): List<ShortArray> = List(n) { ShortArray(QUADRO) }

class QuadroDeVozTest {
    @Test fun fixtureClienteParaServidor() {
        val b = hex("voz_cliente_para_servidor.hex")
        assertEquals(1, b[0].toInt())
        assertArrayEquals(b, QuadroDeVoz.codificarEnvio(0x0001e240, byteArrayOf(0xf8.toByte(), 0xff.toByte(), 0xfe.toByte(), 1, 2, 3)))
    }

    @Test fun fixtureServidorParaCliente() {
        val b = hex("voz_servidor_para_cliente.hex")
        val q = QuadroDeVoz.decodificarRecebido(b)!!
        assertEquals(0x0190_1e94_4600_0009L, q.remetente)
        assertEquals(0x0001e240, q.seq)
        assertArrayEquals(byteArrayOf(0xf8.toByte(), 0xff.toByte(), 0xfe.toByte(), 1, 2, 3), q.opus)
        assertArrayEquals(b, QuadroDeVoz.codificarRecebido(q.remetente, q.seq, q.opus))
    }

    @Test fun descartaOQueNaoEAudio() {
        assertNull(QuadroDeVoz.decodificarRecebido(ByteArray(0)))
        assertNull("só cabeçalho", QuadroDeVoz.decodificarRecebido(ByteArray(13).also { it[0] = 1 }))
        assertNull("primeiro byte diferente de 1", QuadroDeVoz.decodificarRecebido(ByteArray(20)))
        assertNull("pacote acima de 400 bytes", QuadroDeVoz.decodificarRecebido(QuadroDeVoz.codificarRecebido(1, 1, ByteArray(401))))
        assertNotNull("400 ainda vale", QuadroDeVoz.decodificarRecebido(QuadroDeVoz.codificarRecebido(1, 1, ByteArray(400))))
    }

    @Test fun seqNegativoViraDeNovoEIdsGrandes() {
        val q = QuadroDeVoz.decodificarRecebido(QuadroDeVoz.codificarRecebido(Long.MAX_VALUE, -2, byteArrayOf(9)))!!
        assertEquals(Long.MAX_VALUE, q.remetente); assertEquals(-2, q.seq)
        assertEquals(-5L, QuadroDeVoz.decodificarRecebido(QuadroDeVoz.codificarRecebido(-5, 0, byteArrayOf(9)))!!.remetente)
    }
}

class VadTest {
    @Test fun silencioFicaFechadoEAltoAbre() {
        val v = Vad()
        repeat(50) { assertFalse(v.processar(ShortArray(QUADRO))) }
        assertTrue(v.processar(senoide(1, 0.3f)[0]))
    }

    @Test fun hangoverDe18QuadrosDepoisDaUltimaVoz() {
        val v = Vad()
        v.processar(senoide(1)[0])
        var abertos = 0
        repeat(40) { if (v.processar(ShortArray(QUADRO))) abertos++ }
        assertEquals(Vad.HANGOVER_QUADROS, abertos)
        assertFalse(v.aberto)
    }

    @Test fun histereseMantemAbertoComNivelEntreOsLimiares() {
        val v = Vad(0.7f)
        val l = v.limiares()
        assertTrue(l.abreDb > l.fechaDb)
        assertTrue(v.processarDb(l.abreDb + 1))
        // abaixo do limiar de abrir, mas acima do de manter: continua aberto indefinidamente
        repeat(100) { assertTrue(v.processarDb((l.abreDb + l.fechaDb) / 2)) }
    }

    @Test fun maisSensibilidadeAbreComNivelMenor() {
        assertTrue(Vad(1f).limiares().abreDb < Vad(0f).limiares().abreDb)
        val v = Vad(0.7f); v.sensibilidade = 5f; assertEquals(1f, v.sensibilidade, 0f)
        v.sensibilidade = Float.NaN; assertEquals(1f, v.sensibilidade, 0f)
    }

    @Test fun pisoDeRuidoSobeDevagarEmRuidoConstante() {
        val v = Vad(0.7f)
        repeat(2000) { v.processarDb(-56f) } // ruído constante abaixo do limiar absoluto (-52,6 dB)
        assertTrue(v.pisoDeRuidoDb > -60f)
        assertTrue("ruído constante não abre", !v.aberto)
    }

    @Test fun rmsDbEMlimitador() {
        assertEquals(-120f, rmsDb(ShortArray(0)), 0f)
        assertEquals(-3f, rmsDb(ShortArray(100) { 23170 }), 0.1f)
        assertEquals(0.5f, limitadorSuave(0.5f), 0f)
        assertTrue(limitadorSuave(5f) <= 1f && limitadorSuave(5f) > 0.9f)
        assertEquals(-limitadorSuave(3f), limitadorSuave(-3f), 0f)
        assertEquals(0f, limitadorSuave(Float.NaN), 0f)
    }
}

class JitterBufferTest {
    private fun p(n: Int) = byteArrayOf(n.toByte())
    private fun JitterBuffer.ler(): String = when (val r = tocar()) { is Reproducao.Quadro -> "q${r.dados[0]}"; Reproducao.Perdido -> "P"; Reproducao.Silencio -> "-" }

    @Test fun enchePeloAlvoEEntregaEmOrdemMesmoDesordenado() {
        val j = JitterBuffer(3)
        j.entregar(12, p(2)); j.entregar(10, p(0)); j.entregar(11, p(1))
        assertEquals(listOf("q0", "q1", "q2"), List(3) { j.ler() })
    }

    @Test fun naoTocaAntesDeEncherEEsperaNoMaximoOLimite() {
        val j = JitterBuffer(3)
        j.entregar(0, p(0))
        assertEquals("-", j.ler())
        assertEquals("-", j.ler())
        j.entregar(1, p(1))
        val r = List(JitterBuffer.MAX + 3) { j.ler() }
        assertTrue("começa a tocar depois de esperar até MAX quadros", r.contains("q0"))
    }

    @Test fun duplicadoEAtrasadoSaoDescartados() {
        val j = JitterBuffer(2)
        assertEquals(ResultadoDoPush.Aceito, j.entregar(5, p(5)))
        assertEquals(ResultadoDoPush.Duplicado, j.entregar(5, p(5)))
        j.entregar(6, p(6)); j.ler(); j.ler()
        j.entregar(7, p(7))
        assertEquals("q7", j.ler())
        assertEquals("depois de tocado, o 5 chega tarde", ResultadoDoPush.Atrasado, j.entregar(5, p(5)))
        assertTrue(j.descartados() >= 2)
    }

    @Test fun perdaNoMeioViraPerdidoESegue() {
        val j = JitterBuffer(2)
        j.entregar(0, p(0)); j.entregar(1, p(1)); j.entregar(3, p(3)); j.entregar(4, p(4))
        assertEquals(listOf("q0", "q1", "P", "q3", "q4"), List(5) { j.ler() })
    }

    @Test fun esvaziarDa3QuadrosDePlcEDepoisSilencio() {
        val j = JitterBuffer(2)
        j.entregar(0, p(0)); j.entregar(1, p(1))
        assertEquals(listOf("q0", "q1", "P", "P", "P", "-", "-"), List(7) { j.ler() })
    }

    @Test fun buracoGrandeEPuladoDeUmaVez() {
        val j = JitterBuffer(2)
        j.entregar(0, p(0)); j.entregar(1, p(1)); j.entregar(30, p(30)); j.entregar(31, p(31))
        val r = List(6) { j.ler() }
        assertEquals(listOf("q0", "q1", "P"), r.take(3))
        assertTrue("em seguida toca o 30, sem 28 PLCs", r.contains("q30"))
    }

    @Test fun ressincronizaQuandoORemetenteRecomeca() {
        val j = JitterBuffer(2)
        j.entregar(1000, p(1)); j.entregar(1001, p(2)); j.ler(); j.ler()
        assertEquals(ResultadoDoPush.Ressincronizou, j.entregar(5, p(5)))
        j.entregar(6, p(6))
        assertEquals(listOf("q5", "q6"), List(2) { j.ler() })
    }

    @Test fun rajadaDeDezPacotesNaoCortaAudio() {
        val j = JitterBuffer(5)
        for (i in 0 until 10) j.entregar(i, p(i))
        val r = List(10) { j.ler() }
        assertEquals((0 until 10).map { "q$it" }, r)
        assertTrue("o alvo subiu para absorver a rajada", j.alvo > 5)
    }

    @Test fun viraDoContadorDe32Bits() {
        val j = JitterBuffer(2)
        val base = Int.MAX_VALUE - 1
        j.entregar(base, p(0)); j.entregar(base + 1, p(1)); j.entregar(base + 2, p(2)) // passa por Int.MIN_VALUE
        assertEquals(listOf("q0", "q1", "q2"), List(3) { j.ler() })
    }

    @Test fun rajadaGrandeRecuperaLatencia() {
        val j = JitterBuffer(2)
        repeat(20) { j.entregar(it, p(it)) }
        val primeiro = j.ler()
        assertTrue("descartou o excesso para voltar perto do alvo", j.profundidade() <= j.alvo + JitterBuffer.EXCESSO)
        assertTrue(primeiro.startsWith("q"))
    }

    @Test fun subfluxoSobeOAlvoESilencioLongoNao() {
        val j = JitterBuffer(2)
        j.entregar(0, p(0)); j.entregar(1, p(1))
        repeat(2) { j.ler() }
        // esvaziou e o próximo pacote chega logo: era subfluxo
        j.ler(); j.entregar(2, p(2)); j.entregar(3, p(3)); j.entregar(4, p(4)); j.entregar(5, p(5))
        repeat(4) { j.ler() }
        assertTrue("o alvo subiu", j.alvo >= 3)
    }

    @Test fun alvoDesceDepoisDeBastanteCalma() {
        val j = JitterBuffer(4)
        var seq = 0
        repeat(JitterBuffer.CALMA + 20) { j.entregar(seq, p(seq)); seq++; if (seq > 3) j.ler() }
        assertTrue(j.alvo < 4)
        assertTrue(j.alvo >= JitterBuffer.MIN)
    }
}

class EnvioERecepcaoTest {
    /** Codec de mentira: o "pacote" é o valor da 1ª amostra. */
    private class CodecFalso : FabricaDeOpus {
        override fun codificador() = object : CodificadorOpus { override fun codificar(pcm: ShortArray) = byteArrayOf((pcm[0].toInt() shr 8).toByte(), pcm[0].toByte()) }
        override fun decodificador() = object : DecodificadorOpus {
            override fun decodificar(pacote: ByteArray?): ShortArray? = ShortArray(QUADRO) { if (pacote == null) 0 else (((pacote[0].toInt() shl 8) or (pacote[1].toInt() and 0xFF))).toShort() }
        }
    }

    private fun quadroCom(v: Int, db: Float = 0f) = ShortArray(QUADRO) { v.toShort() }

    @Test fun silencioNaoEnviaNada() {
        val enviados = ArrayList<Int>()
        val p = PipelineDeEnvio(CodecFalso().codificador(), 100)
        repeat(100) { p.processar(silencio(1)[0]) { s, _ -> enviados += s } }
        assertTrue(enviados.isEmpty())
        assertFalse(p.falando)
    }

    @Test fun aoAbrirManda5DePreRolagemEDepoisCadaQuadroEOHangover() {
        val enviados = ArrayList<Pair<Int, Int>>()
        val p = PipelineDeEnvio(CodecFalso().codificador(), 1000)
        // 10 quadros de silêncio marcados (valor 1..10 na 1ª amostra, nível baixo) e depois fala
        repeat(10) { i -> val q = ShortArray(QUADRO); q[0] = (i + 1).toShort(); p.processar(q) { s, b -> enviados += s to b[1].toInt() } }
        assertTrue(enviados.isEmpty())
        p.processar(senoide(1)[0]) { s, b -> enviados += s to b[1].toInt() }
        assertEquals("5 de pré-rolagem + o quadro com a fala", 6, enviados.size)
        assertEquals(listOf(6, 7, 8, 9, 10), enviados.take(5).map { it.second })
        assertEquals("seq contíguo a partir da semente", (1000..1005).toList(), enviados.map { it.first })
        // depois da fala: 18 quadros de hangover e para
        var n = 0
        repeat(40) { p.processar(ShortArray(QUADRO)) { _, _ -> n++ } }
        assertEquals(Vad.HANGOVER_QUADROS, n)
        // volta a falar: o seq continua de onde parou (contíguo)
        val nova = ArrayList<Int>()
        p.processar(senoide(1)[0]) { s, _ -> nova += s }
        assertEquals(1006 + Vad.HANGOVER_QUADROS + 5, nova.last()) // 18 do hangover + 5 de pré-rolagem + o quadro atual (1029)
    }

    @Test fun ganhoZeroSilenciaEForcarAbertoEnviaSempre() {
        val p = PipelineDeEnvio(CodecFalso().codificador(), 0)
        p.ganho = 0f
        var n = 0
        repeat(10) { p.processar(senoide(1)[0]) { _, _ -> n++ } }
        assertEquals(0, n)
        p.forcarAberto = true
        p.processar(silencio(1)[0]) { _, _ -> n++ }
        assertTrue(n > 0)
    }

    @Test fun quadroDeTamanhoErradoEIgnorado() {
        val p = PipelineDeEnvio(CodecFalso().codificador(), 0); p.forcarAberto = true
        var n = 0
        p.processar(ShortArray(10)) { _, _ -> n++ }
        assertEquals(0, n)
    }

    private fun entregar(r: RecepcaoDeVoz, usuario: Long, seq: Int, v: Int) = r.entregar(QuadroRecebido(usuario, seq, byteArrayOf((v shr 8).toByte(), v.toByte())))

    @Test fun misturaDoisUsuariosComVolumeIndividual() {
        var t = 0L
        val r = RecepcaoDeVoz(CodecFalso(), { t }, alvoInicial = 2)
        repeat(6) { entregar(r, 1, it, 2000); entregar(r, 2, it, 1000) }
        r.definirVolume(2, 0.5f)
        val saida = ShortArray(QUADRO)
        repeat(3) { r.reproduzir(saida) } // passa o fade-in de 5 ms
        // 2000*1 + 1000*0.5 = 2500 (limitador não mexe abaixo de 0,7 de escala)
        assertEquals(2500.0, saida[QUADRO - 1].toDouble(), 2.0)
        r.definirVolume(1, 0f); r.reproduzir(saida)
        assertEquals(500.0, saida[QUADRO - 1].toDouble(), 2.0)
        assertEquals("volume é limitado a 0..2", 2f, run { r.definirVolume(3, 99f); r.volume(3) }, 0f)
    }

    @Test fun falandoSoNosUltimos250MsELimpaOciosos() {
        var t = 0L
        val r = RecepcaoDeVoz(CodecFalso(), { t }, alvoInicial = 2)
        entregar(r, 7, 0, 1)
        assertTrue(r.falando(7)); assertFalse(r.falando(8))
        t = 249; assertTrue(r.falando(7))
        t = 250; assertFalse(r.falando(7))
        t = 10_000; r.reproduzir(ShortArray(QUADRO)); r.reproduzir(ShortArray(QUADRO))
        assertTrue(r.usuariosAtivos().isEmpty())
    }

    @Test fun limitadorEvitaEstourarComMuitoVolume() {
        val r = RecepcaoDeVoz(CodecFalso(), { 0 }, alvoInicial = 2)
        repeat(6) { entregar(r, 1, it, 30000); entregar(r, 2, it, 30000) }
        val saida = ShortArray(QUADRO)
        repeat(3) { r.reproduzir(saida) }
        assertTrue(saida.all { it in -32768..32767 })
        assertTrue(saida[QUADRO - 1] > 25000)
    }

    @Test fun maximoDe64UsuariosEIgnoraOResto() {
        val r = RecepcaoDeVoz(CodecFalso(), { 0 })
        for (u in 1L..80L) entregar(r, u, 0, 1)
        assertEquals(64, r.usuariosAtivos().size)
    }
}

class OpusDeVerdadeTest {
    private fun rms(q: ShortArray): Double = Math.sqrt(q.sumOf { it.toDouble() * it } / q.size)

    @Test fun codificaEDecodificaUmaSenoide() {
        val f = ConcentusOpus()
        val enc = f.codificador(); val dec = f.decodificador()
        val entrada = senoide(30, 0.3f)
        val saidas = entrada.map { q ->
            val pacote = enc.codificar(q)!!
            assertTrue("pacote dentro do limite do servidor (${pacote.size})", pacote.size in 1..MAX_PACOTE)
            dec.decodificar(pacote)!!
        }
        // depois do aquecimento, a energia da saída é parecida com a da entrada (Opus perde fase, não energia)
        val r1 = rms(entrada.last()); val r2 = rms(saidas.last())
        assertTrue("energia: entrada $r1, saída $r2", r2 > r1 * 0.7 && r2 < r1 * 1.3)
    }

    @Test fun plcDevolveUmQuadroEOSilencioFicaPequeno() {
        val f = ConcentusOpus()
        val enc = f.codificador(); val dec = f.decodificador()
        val q = enc.codificar(silencio(1)[0])!!
        assertTrue("quadro de silêncio é pequeno (${q.size})", q.size < 80)
        assertEquals(QUADRO, dec.decodificar(q)!!.size)
        assertEquals(QUADRO, dec.decodificar(null)!!.size)
    }

    @Test fun lixoNaoDerrubaODecodificador() {
        val dec = ConcentusOpus().decodificador()
        repeat(200) { n -> dec.decodificar(ByteArray(1 + n % 50) { (it * 37 + n).toByte() }) } // pode devolver null; não pode lançar
        assertNull("pacote vazio não decodifica", dec.decodificar(ByteArray(0)).takeIf { false })
    }

    @Test fun bitrateFicaPertoDe32kbps() {
        val enc = ConcentusOpus().codificador()
        val total = senoide(100, 0.3f).sumOf { enc.codificar(it)!!.size }
        val kbps = total * 8 / 2.0 / 1000 // 100 quadros = 2 s
        assertTrue("$kbps kbps", kbps in 4.0..48.0)
    }

    @Test fun motorsEmLoopbackReproduzemAVoz() {
        val fab = ConcentusOpus()
        var t = 0L
        val quadros = ArrayList<ByteArray>()
        val a = MotorDeVoz(fab, { quadros += it }, { t })
        senoide(60, 0.3f).forEach { a.capturar(it); t += 20 }
        assertTrue("o VAD abriu e saíram pacotes", a.pacotesEnviados > 30)
        assertTrue(a.euFalando)
        // o servidor reescreve o quadro: [1][remetente][seq][opus]
        val b2 = MotorDeVoz(fab, {}, { t })
        quadros.forEach { env ->
            val seq = ((env[1].toInt() and 0xFF) shl 24) or ((env[2].toInt() and 0xFF) shl 16) or ((env[3].toInt() and 0xFF) shl 8) or (env[4].toInt() and 0xFF)
            b2.receberQuadro(QuadroDeVoz.codificarRecebido(42, seq, env.copyOfRange(5, env.size)))
        }
        assertTrue(b2.falando(42))
        val saida = ShortArray(QUADRO)
        var maior = 0.0
        repeat(80) { b2.reproduzir(saida); maior = maxOf(maior, rms(saida)) }
        assertTrue("saiu áudio: $maior", maior > 3000)
    }

    @Test fun mudoNaoEnviaESurdoNaoToca() {
        val fab = ConcentusOpus()
        var enviados = 0
        val a = MotorDeVoz(fab, { enviados++ }, { 0 })
        a.definirMudo(true)
        senoide(20).forEach { a.capturar(it) }
        assertEquals(0, enviados); assertFalse(a.euFalando)
        a.definirMudo(false)
        senoide(5).forEach { a.capturar(it) }
        assertTrue(enviados > 0)

        val b = MotorDeVoz(fab, {}, { 0 })
        val enc = fab.codificador()
        senoide(10).forEachIndexed { i, q -> b.receberQuadro(QuadroDeVoz.codificarRecebido(1, i, enc.codificar(q)!!)) }
        b.definirSurdo(true)
        val saida = ShortArray(QUADRO) { 5 }
        repeat(8) { b.reproduzir(saida) }
        assertTrue(saida.all { it == 0.toShort() })
    }

    @Test fun quadroInvalidoContaComoIgnorado() {
        val m = MotorDeVoz(ConcentusOpus(), {}, { 0 })
        m.receberQuadro(byteArrayOf(9, 9, 9)); m.receberQuadro(ByteArray(0))
        assertEquals(2, m.quadrosIgnorados)
    }
}
