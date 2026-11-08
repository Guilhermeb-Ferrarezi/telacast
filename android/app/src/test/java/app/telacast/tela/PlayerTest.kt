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
    override fun decodificar(h264: ByteArray, instanteUs: Long): Boolean { if (aceita) quadros += h264; return aceita }
    override fun fechar() { fechado = true }
}

@OptIn(ExperimentalCoroutinesApi::class)
class PlayerTest {
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
        a.ws.binario(QuadroDeVideo.codificar(false, q[1])); a.ws.binario(QuadroDeVideo.codificar(false, q[2]))
        assertTrue("P antes da chave é descartado", a.decodificadores.isEmpty())
        assertEquals(FaseDoPlayer.Carregando, a.p.estado.value.fase)
        a.ws.binario(QuadroDeVideo.codificar(true, q[0])); a.ws.binario(QuadroDeVideo.codificar(false, q[1]))
        assertEquals(1, a.decodificadores.size)
        assertEquals(listOf(q[0].toList(), q[1].toList()), a.decodificadores[0].quadros.map { it.toList() })
        assertEquals(FaseDoPlayer.AoVivo, a.p.estado.value.fase)
        assertEquals(320 to 240, a.p.estado.value.tamanho)
    }

    @Test fun semSuperficieNaoDecodificaEEsperaUmaChaveNova() = runTest {
        val a = amb(); val q = quadros(video("320x240.h264"))
        a.p.abrir(7); a.ws.abre()
        a.ws.binario(QuadroDeVideo.codificar(true, q[0]))
        assertTrue(a.decodificadores.isEmpty())
        a.p.definirSuperficie(a.surface)
        a.ws.binario(QuadroDeVideo.codificar(false, q[1]))
        assertTrue("a chave que chegou antes da superfície não vale: espera outra", a.decodificadores.isEmpty())
        a.ws.binario(QuadroDeVideo.codificar(true, q[6]))
        assertEquals(1, a.decodificadores.size)
    }

    @Test fun decodificadorCheioDescartaAteAProximaChave() = runTest {
        val a = amb(); val q = quadros(video("320x240.h264"))
        a.p.abrir(7); a.p.definirSuperficie(a.surface); a.ws.abre()
        a.ws.binario(QuadroDeVideo.codificar(true, q[0]))
        val d = a.decodificadores[0]
        d.aceita = false
        a.ws.binario(QuadroDeVideo.codificar(false, q[1]))
        d.aceita = true
        a.ws.binario(QuadroDeVideo.codificar(false, q[2]))
        assertEquals("depois de uma recusa os P seguintes não valem", 1, d.quadros.size)
        a.ws.binario(QuadroDeVideo.codificar(true, q[6]))
        assertEquals(2, d.quadros.size)
    }

    @Test fun tamanhoNovoNoSpsRecriaODecodificador() = runTest {
        val a = amb(); val h = quadros(video("320x240.h264")); val v = quadros(video("240x320.h264"))
        a.p.abrir(7); a.p.definirSuperficie(a.surface); a.ws.abre()
        a.ws.binario(QuadroDeVideo.codificar(true, h[0]))
        a.ws.binario(QuadroDeVideo.codificar(true, v[0]))
        assertEquals(2, a.decodificadores.size)
        assertTrue(a.decodificadores[0].fechado)
        assertEquals(240 to 320, a.p.estado.value.tamanho)
        a.ws.binario(QuadroDeVideo.codificar(true, v[6]))
        assertEquals("mesmo tamanho: reaproveita", 2, a.decodificadores.size)
    }

    @Test fun perderASuperficieFechaODecodificador() = runTest {
        val a = amb(); val q = quadros(video("320x240.h264"))
        a.p.abrir(7); a.p.definirSuperficie(a.surface); a.ws.abre()
        a.ws.binario(QuadroDeVideo.codificar(true, q[0]))
        a.p.definirSuperficie(null)
        assertTrue(a.decodificadores[0].fechado)
        a.ws.binario(QuadroDeVideo.codificar(false, q[1]))
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
        a.ws.binario(QuadroDeVideo.codificar(true, q[0]))
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
        a.ws.binario(byteArrayOf(1)); a.ws.binario(ByteArray(0))
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
}
