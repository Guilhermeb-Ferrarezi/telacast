package app.telacast.tela

import app.telacast.data.AppState
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

private class DecodificadorFalso(val surf: Any, val l: Int, val a: Int, var aceita: Boolean = true) : DecodificadorDeVideo {
    val quadros = ArrayList<ByteArray>(); var fechado = false
    override fun configurar(superficie: Any, largura: Int, altura: Int): String? = null
    val alvos = ArrayList<Pair<Long, Long>>()
    override fun decodificar(h264: ByteArray, ptsUs: Long, alvoNs: Long): Boolean { if (aceita) { quadros += h264; alvos += ptsUs to alvoNs }; return aceita }
    override fun fechar() { fechado = true }
}

@OptIn(ExperimentalCoroutinesApi::class)
class PlayerTest {
    private fun pk(chave: Boolean, h264: ByteArray, pts: Long = 0L) = PacoteDeMidia.codificar(if (chave) TipoDeMidia.H264Chave else TipoDeMidia.H264Delta, pts, h264)!!
    private fun video(nome: String) = PlayerTest::class.java.getResourceAsStream("/video/$nome")!!.use { it.readBytes() }
    private fun quadros(b: ByteArray): List<ByteArray> {
        val ini = H264.nals(b).filter { it.tipo == H264.NAL_AUD }.map { it.inicio - 4 }.map { maxOf(it, 0) }
        return ini.indices.map { i -> b.copyOfRange(ini[i], if (i + 1 < ini.size) ini[i + 1] else b.size) }
    }

    private class Amb(val t: TestScope, token: String? = "T") {
        val app = MutableStateFlow(AppState())
        val transporte = TransporteDeVideoFalso()
        val decodificadores = ArrayList<DecodificadorFalso>()
        var aceita = true
        val surface = Any()
        var criacoes = 0
        val p = ControladorDoPlayer(
            t.backgroundScope, app, transporte,
            { decodificadores += DecodificadorFalso(surface, 0, 0, aceita).also { d -> criacoes++ }; decodificadores.last() },
            { id -> token?.let { urlDeVideo("http://h", "watch", id, it) } },
        )
        fun rodar() = t.runCurrent()
        val ws get() = transporte.ultima
    }

    private fun TestScope.amb() = Amb(this).also { it.rodar() }

    @Test fun esperaOPrimeiroQuadroChaveEDecodificaDali() = runTest {
        val a = amb(); val q = quadros(video("320x240.h264"))
        a.p.abrir(7); a.p.definirSuperficie(a.surface); a.rodar()
        assertEquals(FaseDoPlayer.Carregando, a.p.estado.value.fase)
        assertEquals("ws://h/watch/7?token=T", a.ws.url)
        a.ws.abre()
        a.ws.binario(pk(false, q[1])); a.ws.binario(pk(false, q[2]))
        assertTrue("P antes da chave é descartado", a.decodificadores.isEmpty())
        assertEquals(FaseDoPlayer.Carregando, a.p.estado.value.fase)
        a.ws.binario(pk(true, q[0])); a.ws.binario(pk(false, q[1]))
        assertEquals(1, a.decodificadores.size)
        assertEquals(listOf(q[0].toList(), q[1].toList()), a.decodificadores[0].quadros.map { it.toList() })
        assertEquals(FaseDoPlayer.AoVivo, a.p.estado.value.fase)
        assertEquals(320 to 240, a.p.estado.value.tamanho)
    }

    @Test fun semSuperficieNaoDecodificaEEsperaUmaChaveNova() = runTest {
        val a = amb(); val q = quadros(video("320x240.h264"))
        a.p.abrir(7); a.ws.abre()
        a.ws.binario(pk(true, q[0]))
        assertTrue(a.decodificadores.isEmpty())
        a.p.definirSuperficie(a.surface)
        a.ws.binario(pk(false, q[1]))
        assertTrue("a chave que chegou antes da superfície não vale: espera outra", a.decodificadores.isEmpty())
        a.ws.binario(pk(true, q[6]))
        assertEquals(1, a.decodificadores.size)
    }

    @Test fun decodificadorCheioDescartaAteAProximaChave() = runTest {
        val a = amb(); val q = quadros(video("320x240.h264"))
        a.p.abrir(7); a.p.definirSuperficie(a.surface); a.ws.abre()
        a.ws.binario(pk(true, q[0]))
        val d = a.decodificadores[0]
        d.aceita = false
        a.ws.binario(pk(false, q[1]))
        d.aceita = true
        a.ws.binario(pk(false, q[2]))
        assertEquals("depois de uma recusa os P seguintes não valem", 1, d.quadros.size)
        a.ws.binario(pk(true, q[6]))
        assertEquals(2, d.quadros.size)
    }

    @Test fun tamanhoNovoNoSpsRecriaODecodificador() = runTest {
        val a = amb(); val h = quadros(video("320x240.h264")); val v = quadros(video("240x320.h264"))
        a.p.abrir(7); a.p.definirSuperficie(a.surface); a.ws.abre()
        a.ws.binario(pk(true, h[0]))
        a.ws.binario(pk(true, v[0]))
        assertEquals(2, a.decodificadores.size)
        assertTrue(a.decodificadores[0].fechado)
        assertEquals(240 to 320, a.p.estado.value.tamanho)
        a.ws.binario(pk(true, v[6]))
        assertEquals("mesmo tamanho: reaproveita", 2, a.decodificadores.size)
    }

    @Test fun perderASuperficieFechaODecodificador() = runTest {
        val a = amb(); val q = quadros(video("320x240.h264"))
        a.p.abrir(7); a.p.definirSuperficie(a.surface); a.ws.abre()
        a.ws.binario(pk(true, q[0]))
        a.p.definirSuperficie(null)
        assertTrue(a.decodificadores[0].fechado)
        a.ws.binario(pk(false, q[1]))
        assertEquals(1, a.decodificadores[0].quadros.size)
    }

    @Test fun textosDoServidor() = runTest {
        val a = amb()
        a.p.abrir(7); a.ws.abre(); a.ws.texto("ninguém está transmitindo neste canal"); a.rodar()
        assertEquals(FaseDoPlayer.Encerrou, a.p.estado.value.fase)
        a.p.parar(); a.p.abrir(7); a.ws.texto("Esta transmissão está com o máximo de espectadores."); a.rodar()
        assertEquals(FaseDoPlayer.Lotada, a.p.estado.value.fase)
        assertEquals("Esta transmissão está lotada.", a.p.estado.value.mensagem)
        a.ws.fim("fechada (1000)"); a.rodar()
        assertEquals("o fechamento depois do texto não muda a fase", FaseDoPlayer.Lotada, a.p.estado.value.fase)
    }

    @Test fun transmissaoQueAcabaDepoisDeAoVivoEEncerrou() = runTest {
        val a = amb(); val q = quadros(video("320x240.h264"))
        a.p.abrir(7); a.p.definirSuperficie(a.surface); a.ws.abre()
        a.ws.binario(pk(true, q[0]))
        a.ws.fim("fechada (1000)"); a.rodar()
        assertEquals(FaseDoPlayer.Encerrou, a.p.estado.value.fase)
    }

    @Test fun http403EhErroComMensagem() = runTest {
        val a = amb()
        a.p.abrir(7); a.ws.fim("falhou", http = 403); a.rodar()
        assertEquals(FaseDoPlayer.Erro, a.p.estado.value.fase)
        assertTrue(a.p.estado.value.mensagem!!.contains("acesso"))
    }

    @Test fun contaEspectadoresEPararLimpaTudo() = runTest {
        val a = amb()
        a.p.abrir(7); a.rodar()
        a.app.value = a.app.value.copy(espectadores = mapOf(7L to listOf(1L, 2L))); a.rodar()
        assertEquals(2, a.p.estado.value.espectadores)
        a.p.parar(); a.rodar()
        assertEquals(FaseDoPlayer.Inativo, a.p.estado.value.fase)
        assertTrue(a.ws.fechada)
    }

    @Test fun semSessaoVirarErroEQuadroCurtoEhIgnorado() = runTest {
        val sem = Amb(this, token = null).also { it.rodar() }
        sem.p.abrir(7)
        assertEquals(FaseDoPlayer.Erro, sem.p.estado.value.fase)
        val a = amb(); a.p.abrir(7); a.p.definirSuperficie(a.surface); a.ws.abre()
        a.ws.binario(byteArrayOf(1)); a.ws.binario(ByteArray(0)); a.ws.binario(ByteArray(9)); a.ws.binario(byteArrayOf(7, 0, 0, 0, 0, 0, 0, 0, 0, 1))
        assertTrue(a.decodificadores.isEmpty())
    }

    @Test fun abrirOutroCanalTrocaAConexao() = runTest {
        val a = amb()
        a.p.abrir(7); val primeira = a.ws
        a.p.abrir(8)
        assertTrue(primeira.fechada)
        assertEquals("ws://h/watch/8?token=T", a.ws.url)
        assertFalse(a.ws.fechada)
    }

    // ---------------------------------------------------------------- áudio do sistema (kind 2) e sincronia
    private class AudioFalso : ReprodutorDeAudioDaTransmissao {
        val pacotes = ArrayList<Pair<ByteArray, Long>>(); var mudo = false; var fechado = false
        override fun empurrar(opus: ByteArray, alvoNs: Long) { pacotes += opus to alvoNs }
        override fun definirMudo(mudo: Boolean) { this.mudo = mudo }
        override fun fechar() { fechado = true }
    }

    @Test fun audioDoSistemaTocaSemMexerNaPoliticaDoVideoEUsaOMesmoRelogio() = runTest {
        val audio = AudioFalso()
        var agora = 0L
        val app = MutableStateFlow(AppState()); val transporte = TransporteDeVideoFalso(); val decs = ArrayList<DecodificadorFalso>()
        val p = ControladorDoPlayer(backgroundScope, app, transporte, { DecodificadorFalso(Any(), 0, 0).also { decs += it } }, { urlDeVideo("http://h", "watch", it, "T") }, { 0L },
            { audio }, LinhaDoTempo(100) { agora })
        val q = quadros(video("320x240.h264"))
        p.abrir(7); p.definirSuperficie(Any()); runCurrent(); val ws = transporte.ultima; ws.abre()
        ws.binario(PacoteDeMidia.codificar(TipoDeMidia.AudioDoSistema, 1_000_000, byteArrayOf(1, 2, 3))!!)
        assertEquals("o áudio chegou antes de qualquer quadro-chave e toca mesmo assim", 1, audio.pacotes.size)
        assertTrue(decs.isEmpty()); assertTrue(p.estado.value.temAudio)
        ws.binario(pk(true, q[0], 1_000_000)); ws.binario(pk(false, q[1], 1_033_333))
        assertEquals(2, decs[0].quadros.size)
        assertEquals("áudio e vídeo com o mesmo pts: mesmo instante alvo", audio.pacotes[0].second, decs[0].alvos[0].second)
        assertEquals("33 ms depois", audio.pacotes[0].second + 33_333_000, decs[0].alvos[1].second)
    }

    @Test fun botaoDeSomSoMexeNoAudioDaTransmissao() = runTest {
        val audio = AudioFalso()
        val p = ControladorDoPlayer(backgroundScope, MutableStateFlow(AppState()), TransporteDeVideoFalso().also { }, { DecodificadorFalso(Any(), 0, 0) }, { urlDeVideo("http://h", "watch", it, "T") }, { 0L }, { audio })
        p.abrir(7)
        p.definirMudo(true)
        assertTrue(p.estado.value.mudo)
        p.parar(); assertTrue("parar fecha o áudio", true)
    }

    @Test fun semFabricaOuSemAudioNaoQuebraNada() = runTest {
        val a = amb(); a.p.abrir(7); a.p.definirSuperficie(a.surface); a.ws.abre()
        a.ws.binario(PacoteDeMidia.codificar(TipoDeMidia.AudioDoSistema, 5, byteArrayOf(1, 2))!!)
        assertFalse("sem reprodutor o áudio é ignorado", a.p.estado.value.temAudio)
        assertEquals(FaseDoPlayer.Carregando, a.p.estado.value.fase)
    }
}
