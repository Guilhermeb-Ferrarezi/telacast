package app.telacast.tela

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

private fun video(nome: String): ByteArray = TelaPuraTest::class.java.getResourceAsStream("/video/$nome")!!.use { it.readBytes() }

/** Separa um fluxo Annex B em "access units" (cada um começa no AUD, NAL tipo 9, que o x264 põe antes de cada quadro). */
private fun quadros(fluxo: ByteArray): List<ByteArray> {
    val nals = H264.nals(fluxo)
    val inicios = nals.filter { it.tipo == H264.NAL_AUD }.map { it.inicio - 4 }.map { maxOf(it, 0) }
    return inicios.indices.map { i -> fluxo.copyOfRange(inicios[i], if (i + 1 < inicios.size) inicios[i + 1] else fluxo.size) }
}

class TelaPuraTest {
    // ---------------------------------------------------------------- H.264 Annex B (fixtures geradas com x264, baseline)
    @Test fun achaOsNalsEOsTipos() {
        val q = quadros(video("320x240.h264"))
        assertEquals(12, q.size)
        val tipos0 = H264.nals(q[0]).map { it.tipo }
        assertTrue("o primeiro quadro leva SPS, PPS e IDR: $tipos0", tipos0.containsAll(listOf(H264.NAL_SPS, H264.NAL_PPS, H264.NAL_IDR)))
        assertEquals(listOf(H264.NAL_AUD, H264.NAL_NAO_IDR), H264.nals(q[1]).map { it.tipo }.distinct())
    }

    @Test fun reconheceQuadroChave() {
        val q = quadros(video("320x240.h264"))
        assertTrue(H264.ehQuadroChave(q[0]))
        assertFalse(H264.ehQuadroChave(q[1]))
        assertTrue("key-int-max=6: o sétimo quadro é chave de novo", H264.ehQuadroChave(q[6]))
        assertFalse(H264.ehQuadroChave(ByteArray(0)))
        assertFalse(H264.ehQuadroChave(byteArrayOf(1, 2, 3)))
    }

    @Test fun leOTamanhoDoSps() {
        assertEquals(320 to 240, H264.dimensoes(video("320x240.h264")))
        assertEquals(240 to 320, H264.dimensoes(video("240x320.h264")))
        assertNull(H264.dimensoes(byteArrayOf(0, 0, 0, 1, 0x65, 1, 2)))
        assertNull("SPS truncado não quebra", H264.dimensoes(byteArrayOf(0, 0, 0, 1, 0x67, 0x42)))
    }

    @Test fun extraiSpsPpsEPrependaNoQuadroChaveQueNaoTem() {
        val q = quadros(video("320x240.h264"))
        val sp = H264.spsPps(q[0])
        assertNotNull(sp)
        assertEquals(listOf(H264.NAL_SPS, H264.NAL_PPS), H264.nals(sp!!).map { it.tipo })
        // o codificador entrega o IDR sem SPS/PPS (que vêm no buffer de configuração): reconstrói
        val idrSemSps = q[0].let { b -> val n = H264.nals(b).first { it.tipo == H264.NAL_IDR }; byteArrayOf(0, 0, 0, 1) + b.copyOfRange(n.inicio, n.fim) }
        assertFalse(H264.nals(idrSemSps).any { it.tipo == H264.NAL_SPS })
        val completo = H264.comSpsPps(idrSemSps, sp)
        assertEquals(listOf(H264.NAL_SPS, H264.NAL_PPS, H264.NAL_IDR), H264.nals(completo).map { it.tipo })
        assertEquals("quadro P não leva", q[1].toList(), H264.comSpsPps(q[1], sp).toList())
        assertEquals("já tem: não duplica", q[0].toList(), H264.comSpsPps(q[0], sp).toList())
        assertEquals("sem SPS guardado, passa direto", idrSemSps.toList(), H264.comSpsPps(idrSemSps, null).toList())
    }

    @Test fun startCodeDeTresEDeQuatroBytes() {
        val d = byteArrayOf(0, 0, 1, 0x67, 1, 2, 0, 0, 0, 1, 0x68, 3, 4, 0, 0, 1, 0x65, 9)
        assertEquals(listOf(7, 8, 5), H264.nals(d).map { it.tipo })
        assertEquals(listOf(3, 3, 2), H264.nals(d).map { it.fim - it.inicio })
    }

    // ---------------------------------------------------------------- quadro no fio
    @Test fun quadroDeVideoVaiEVolta() {
        val b = QuadroDeVideo.codificar(true, byteArrayOf(0, 0, 0, 1, 0x65))
        assertEquals(1.toByte(), b[0]); assertEquals(6, b.size)
        val q = QuadroDeVideo.decodificar(b)!!
        assertTrue(q.chave); assertEquals(listOf<Byte>(0, 0, 0, 1, 0x65), q.h264.toList())
        assertFalse(QuadroDeVideo.decodificar(QuadroDeVideo.codificar(false, byteArrayOf(5)))!!.chave)
        assertNull("menos de 2 bytes o servidor também descarta", QuadroDeVideo.decodificar(byteArrayOf(1)))
        assertNull(QuadroDeVideo.decodificar(ByteArray(0)))
    }

    // ---------------------------------------------------------------- escolha de resolução
    @Test fun resolucaoGiraComATela() {
        val (w, h) = tamanhoDaCodificacao(720, 1560, 720)
        assertEquals("paisagem é o retrato girado", h to w, tamanhoDaCodificacao(1560, 720, 720))
        assertTrue(h > w)
    }

    @Test fun resolucaoNuncaAmplia() {
        val (w, h) = tamanhoDaCodificacao(720, 1560, 1080)
        assertTrue("$w x $h", w <= 720 && h <= 1560)
    }

    @Test fun resolucaoRespeitaMultiplosDe16EOLimiteDeMacroblocos() {
        for ((lt, at) in listOf(1080 to 2400, 1440 to 3200, 1080 to 1920, 800 to 1280, 2160 to 3840)) for (q in QualidadeDeTransmissao.entries) {
            val (w, h) = tamanhoDaCodificacao(lt, at, q.ladoMenor)
            assertEquals("largura múltipla de 16 ($w)", 0, w % 16); assertEquals("altura múltipla de 16 ($h)", 0, h % 16)
            assertTrue("${w / 16 * (h / 16)} macroblocos acima do limite ($lt x $at, $q)", w / 16 * (h / 16) <= MAX_MACROBLOCOS)
            val desvio = Math.abs(w.toDouble() / h - lt.toDouble() / at) / (lt.toDouble() / at)
            assertTrue("proporção desviou ${desvio * 100}% ($lt x $at -> $w x $h)", desvio < 0.05)
        }
    }

    @Test fun resolucaoDeExemplos() {
        assertEquals(720 to 1552, tamanhoDaCodificacao(720, 1560, 720))
        assertEquals(480 to 1040, tamanhoDaCodificacao(1080, 2340, 480))
        assertEquals(720 to 1552, tamanhoDaCodificacao(1080, 2340, 720))
        assertEquals(1280 to 720, tamanhoDaCodificacao(1920, 1080, 720))
        assertEquals(1920 to 1072, tamanhoDaCodificacao(1920, 1080, 1080)) // 1072 = 67 x 16
    }

    @Test fun bitrateSugerido() {
        assertEquals(2500, OpcoesDeTransmissao().kbpsEfetivo())
        assertEquals(1200, OpcoesDeTransmissao(QualidadeDeTransmissao.P480, 30).kbpsEfetivo())
        assertEquals(4500, OpcoesDeTransmissao(QualidadeDeTransmissao.P1080, 30).kbpsEfetivo())
        assertTrue(OpcoesDeTransmissao(QualidadeDeTransmissao.P720, 60).kbpsEfetivo() > OpcoesDeTransmissao(QualidadeDeTransmissao.P720, 30).kbpsEfetivo())
        assertTrue(OpcoesDeTransmissao(QualidadeDeTransmissao.P720, 15).kbpsEfetivo() < OpcoesDeTransmissao(QualidadeDeTransmissao.P720, 30).kbpsEfetivo())
        assertEquals("o usuário manda", 3333, OpcoesDeTransmissao(kbps = 3333).kbpsEfetivo())
    }

    @Test fun urlsDeVideo() {
        assertEquals("ws://h:8080/publish/12?token=T", urlDeVideo("http://h:8080", "publish", 12, "T"))
        assertEquals("wss://h/watch/12?token=T", urlDeVideo("https://h/", "watch", 12, "T"))
    }

    // ---------------------------------------------------------------- política de envio (backpressure)
    @Test fun naoEnviaPAntesDoPrimeiroQuadroChave() {
        val p = PoliticaDeEnvio()
        assertEquals(PoliticaDeEnvio.Decisao.Descartar(false), p.decidir(false, 0))
        assertEquals(PoliticaDeEnvio.Decisao.Enviar, p.decidir(true, 0))
        assertEquals(PoliticaDeEnvio.Decisao.Enviar, p.decidir(false, 0))
    }

    @Test fun filaGrandeDescartaEPedeChaveUmaVezEEsperaOProximoQuadroChave() {
        val p = PoliticaDeEnvio(maxPendente = 1000)
        p.decidir(true, 0)
        assertEquals(PoliticaDeEnvio.Decisao.Enviar, p.decidir(false, 900))
        assertEquals("passou do limite: descarta e pede chave", PoliticaDeEnvio.Decisao.Descartar(true), p.decidir(false, 1500))
        assertEquals("a fila esvaziou mas o P seguinte não decodifica sem o anterior", PoliticaDeEnvio.Decisao.Descartar(false), p.decidir(false, 0))
        assertEquals(PoliticaDeEnvio.Decisao.Enviar, p.decidir(true, 0))
        assertEquals(PoliticaDeEnvio.Decisao.Enviar, p.decidir(false, 0))
        assertEquals(2, p.descartados)
    }

    @Test fun quadroChaveSoCaiComFilaAbsurda() {
        val p = PoliticaDeEnvio(maxPendente = 1000, maxPendenteParaChave = 5000)
        assertEquals(PoliticaDeEnvio.Decisao.Enviar, p.decidir(true, 4000))
        assertEquals(PoliticaDeEnvio.Decisao.Descartar(true), p.decidir(true, 9000))
    }

    @Test fun exigirChaveReiniciaAPolitica() {
        val p = PoliticaDeEnvio(); p.decidir(true, 0)
        p.exigirChave()
        assertEquals(PoliticaDeEnvio.Decisao.Descartar(false), p.decidir(false, 0))
    }

    @Test fun decodificacaoEsperaOPrimeiroQuadroChave() {
        val p = PoliticaDeDecodificacao()
        assertFalse(p.aceitar(false)); assertFalse(p.aceitar(false))
        assertTrue(p.aceitar(true)); assertTrue(p.aceitar(false))
        p.exigirChave()
        assertFalse(p.aceitar(false)); assertTrue(p.aceitar(true))
    }
}
