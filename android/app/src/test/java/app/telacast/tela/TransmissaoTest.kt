package app.telacast.tela

import app.telacast.data.AppState
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ConexaoDeVideoFalsa(val url: String, val ouvinte: OuvinteDeVideo) : ConexaoDeVideo {
    val enviados = ArrayList<ByteArray>()
    var pend = 0L
    var fechada = false
    var aceita = true
    override fun enviar(dados: ByteArray): Boolean { if (!aceita || fechada) return false; enviados += dados; return true }
    override fun pendente() = pend
    override fun fechar() { fechada = true }
    override fun cancelar() { fechada = true }
    fun abre() = ouvinte.aoAbrir()
    fun texto(t: String) = ouvinte.aoTexto(t)
    fun binario(b: ByteArray) = ouvinte.aoBinario(b)
    fun fim(motivo: String = "caiu", http: Int? = null) = ouvinte.aoFim(motivo, http)
}

class TransporteDeVideoFalso : TransporteDeVideo {
    val conexoes = ArrayList<ConexaoDeVideoFalsa>()
    val ultima get() = conexoes.last()
    override fun abrir(url: String, ouvinte: OuvinteDeVideo) = ConexaoDeVideoFalsa(url, ouvinte).also { conexoes += it }
}

class FonteFalsa(var erro: String? = null, val fps: List<Int> = listOf(15, 30)) : FonteDeTela {
    var ouvinte: OuvinteDaFonte? = null
    var iniciadaCom: Pair<Any, OpcoesDeTransmissao>? = null
    var chavesPedidas = 0; var paradas = 0
    var opcoesReconfiguradas = ArrayList<OpcoesDeTransmissao>(); var erroReconfigurar: String? = null
    override fun fpsSuportados() = fps
    override fun iniciar(concessao: Any, opcoes: OpcoesDeTransmissao, ouvinte: OuvinteDaFonte): String? { this.ouvinte = ouvinte; iniciadaCom = concessao to opcoes; return erro }
    override fun reconfigurar(opcoes: OpcoesDeTransmissao): String? { opcoesReconfiguradas += opcoes; return erroReconfigurar }
    override fun pedirQuadroChave() { chavesPedidas++ }
    override fun parar() { paradas++ }
}

@OptIn(ExperimentalCoroutinesApi::class)
class TransmissaoTest {
    private class Amb(val t: TestScope, canal: Long? = CANAL, val fonte: FonteFalsa = FonteFalsa(), token: String? = "T") {
        val app = MutableStateFlow(AppState())
        val voz = MutableStateFlow(canal)
        val transporte = TransporteDeVideoFalso()
        val c = ControladorDeTransmissao(
            t.backgroundScope, app, voz, transporte, { fonte }, { id -> token?.let { urlDeVideo("http://h", "publish", id, it) } },
            relogio = { t.currentTime }, despachante = UnconfinedTestDispatcher(t.testScheduler),
        )
        fun rodar() = t.runCurrent()
        val ws get() = transporte.ultima
        /** Permissão concedida e o socket aberto. */
        fun noAr() { c.pedir(CANAL, OpcoesDeTransmissao()); c.permissaoConcedida("concessao"); rodar(); ws.abre(); rodar() }
        fun chave() = fonte.ouvinte!!.aoQuadro(true, byteArrayOf(0, 0, 0, 1, 0x65))
        fun p() = fonte.ouvinte!!.aoQuadro(false, byteArrayOf(0, 0, 0, 1, 0x41))
    }

    private companion object { const val CANAL = 77L }

    private fun TestScope.amb(canal: Long? = CANAL, fonte: FonteFalsa = FonteFalsa()) = Amb(this, canal, fonte).also { it.rodar() }

    @Test fun fluxoFeliz() = runTest {
        val a = amb()
        a.c.pedir(CANAL, OpcoesDeTransmissao(QualidadeDeTransmissao.P480, 15)); a.rodar()
        assertEquals(FaseDaTransmissao.PedindoPermissao, a.c.estado.value.fase)
        assertTrue("sem fonte aberta antes da permissão", a.fonte.iniciadaCom == null)
        a.c.permissaoConcedida("concessao"); a.rodar()
        assertEquals(FaseDaTransmissao.Conectando, a.c.estado.value.fase)
        assertEquals("concessao" to OpcoesDeTransmissao(QualidadeDeTransmissao.P480, 15), a.fonte.iniciadaCom)
        assertEquals("ws://h/publish/77?token=T", a.ws.url)
        a.ws.abre(); a.rodar()
        val s = a.c.estado.value
        assertEquals(FaseDaTransmissao.Transmitindo, s.fase); assertNotNull(s.desdeMs); assertEquals(listOf(15, 30), s.fpsDisponiveis)
        assertEquals("pede um quadro-chave ao abrir", 1, a.fonte.chavesPedidas)
        a.chave(); a.p(); a.p()
        assertEquals(listOf<Byte>(1, 0, 0, 0, 1, 0x65), a.ws.enviados[0].toList())
        assertEquals(0.toByte(), a.ws.enviados[1][0])
        assertEquals(3, a.ws.enviados.size)
    }

    @Test fun naoTransmiteForaDoCanalDeVoz() = runTest {
        val a = amb(canal = null)
        a.c.pedir(CANAL, OpcoesDeTransmissao()); a.rodar()
        assertEquals(FaseDaTransmissao.Erro, a.c.estado.value.fase)
        assertTrue(a.c.estado.value.mensagem!!.contains("canal de voz"))
        assertTrue(a.transporte.conexoes.isEmpty())
    }

    @Test fun permissaoNegadaVoltaAInativaComMensagem() = runTest {
        val a = amb()
        a.c.pedir(CANAL, OpcoesDeTransmissao()); a.c.permissaoNegada(); a.rodar()
        assertEquals(FaseDaTransmissao.Inativa, a.c.estado.value.fase)
        assertTrue(a.c.estado.value.mensagem!!.contains("permissão"))
        assertFalse(a.c.estado.value.ativa)
    }

    @Test fun fonteQueNaoAbreViraErroESeLimpa() = runTest {
        val f = FonteFalsa(erro = "O codificador de vídeo não abriu.")
        val a = amb(fonte = f)
        a.c.pedir(CANAL, OpcoesDeTransmissao()); a.c.permissaoConcedida("x"); a.rodar()
        assertEquals(FaseDaTransmissao.Erro, a.c.estado.value.fase)
        assertEquals("O codificador de vídeo não abriu.", a.c.estado.value.mensagem)
        assertTrue(f.paradas >= 1)
        assertTrue(a.transporte.conexoes.isEmpty())
    }

    @Test fun primeiroQuadroEnviadoEhChave() = runTest {
        val a = amb(); a.noAr()
        a.p(); a.p()
        assertTrue("P antes de qualquer quadro-chave não sai", a.ws.enviados.isEmpty())
        a.chave(); a.p()
        assertEquals(2, a.ws.enviados.size)
    }

    @Test fun socketLentoDescartaEPedeChaveSemAcumularFila() = runTest {
        val a = amb(); a.noAr(); a.chave()
        val pedidasAntes = a.fonte.chavesPedidas
        a.ws.pend = 10L * 1024 * 1024
        a.p(); a.p(); a.p()
        assertEquals("nada saiu com a fila cheia", 1, a.ws.enviados.size)
        assertEquals("um pedido de quadro-chave", pedidasAntes + 1, a.fonte.chavesPedidas)
        a.ws.pend = 0
        a.p()
        assertEquals("a fila esvaziou mas o P não decodifica: espera a chave", 1, a.ws.enviados.size)
        a.chave(); a.p()
        assertEquals(3, a.ws.enviados.size)
    }

    @Test fun pedidoDeChaveTemIntervaloMinimo() = runTest {
        val a = amb(); a.noAr(); a.chave()
        val antes = a.fonte.chavesPedidas
        a.ws.pend = 10L * 1024 * 1024
        a.p(); a.t.advanceTimeBy(100); a.chave() // chave com fila absurda também cai e pede
        assertEquals(antes + 1, a.fonte.chavesPedidas)
        a.t.advanceTimeBy(600); a.chave()
        assertEquals(antes + 2, a.fonte.chavesPedidas)
    }

    @Test fun quedaDoSocketReconectaSozinhaEOPrimeiroQuadroDepoisEChave() = runTest {
        val a = amb(); a.noAr(); a.chave(); a.p()
        a.ws.fim("rede caiu"); a.rodar()
        assertEquals(FaseDaTransmissao.Reconectando, a.c.estado.value.fase); assertEquals(1, a.c.estado.value.tentativa)
        a.p(); a.p()
        assertEquals("durante a queda nada vai para o socket", 2, a.transporte.conexoes[0].enviados.size)
        a.t.advanceTimeBy(1_000); a.rodar()
        assertEquals(2, a.transporte.conexoes.size)
        a.ws.abre(); a.rodar()
        assertEquals(FaseDaTransmissao.Transmitindo, a.c.estado.value.fase); assertEquals(0, a.c.estado.value.tentativa)
        a.p()
        assertTrue("P logo depois de reconectar não sai", a.ws.enviados.isEmpty())
        a.chave()
        assertEquals(1, a.ws.enviados.size)
        assertEquals("quadro-chave pedido na reabertura", 2, a.fonte.chavesPedidas)
    }

    @Test fun desisteDepoisDeTentarMuitasVezes() = runTest {
        val a = amb(); a.noAr()
        repeat(8) { a.ws.fim(); a.rodar(); a.t.advanceTimeBy(9_000); a.rodar() }
        a.ws.fim(); a.rodar()
        assertEquals(FaseDaTransmissao.Erro, a.c.estado.value.fase)
        assertTrue(a.c.estado.value.mensagem!!.contains("reconectar"))
        assertTrue(a.fonte.paradas >= 1)
    }

    @Test fun servidorRecusarComTextoEErroSemReconectar() = runTest {
        val a = amb(); a.c.pedir(CANAL, OpcoesDeTransmissao()); a.c.permissaoConcedida("x"); a.rodar()
        a.ws.abre(); a.ws.texto("já há uma transmissão neste canal"); a.rodar()
        assertEquals(FaseDaTransmissao.Erro, a.c.estado.value.fase)
        assertEquals("já há uma transmissão neste canal", a.c.estado.value.mensagem)
        a.t.advanceTimeBy(20_000); a.rodar()
        assertEquals("não reconecta", 1, a.transporte.conexoes.size)
    }

    @Test fun http403EhErroDefinitivo() = runTest {
        val a = amb(); a.c.pedir(CANAL, OpcoesDeTransmissao()); a.c.permissaoConcedida("x"); a.rodar()
        a.ws.fim("falhou", http = 403); a.rodar()
        assertEquals(FaseDaTransmissao.Erro, a.c.estado.value.fase)
        assertTrue(a.c.estado.value.mensagem!!.contains("canal de voz"))
        assertEquals(1, a.transporte.conexoes.size)
    }

    @Test fun sairDaVozParaATransmissao() = runTest {
        val a = amb(); a.noAr(); a.chave()
        a.voz.value = null; a.rodar()
        assertEquals(FaseDaTransmissao.Inativa, a.c.estado.value.fase)
        assertTrue(a.ws.fechada); assertEquals(1, a.fonte.paradas)
        a.p(); a.chave()
        assertEquals("nada mais sai", 1, a.ws.enviados.size)
    }

    @Test fun mudarDeCanalDeVozTambemPara() = runTest {
        val a = amb(); a.noAr()
        a.voz.value = 999L; a.rodar()
        assertEquals(FaseDaTransmissao.Inativa, a.c.estado.value.fase)
    }

    @Test fun pararPeloBotaoFechaTudoEDeixaIniciarDeNovo() = runTest {
        val a = amb(); a.noAr()
        a.c.parar(); a.rodar()
        assertEquals(FaseDaTransmissao.Inativa, a.c.estado.value.fase); assertTrue(a.ws.fechada); assertEquals(1, a.fonte.paradas)
        a.c.pedir(CANAL, OpcoesDeTransmissao()); assertEquals(FaseDaTransmissao.PedindoPermissao, a.c.estado.value.fase)
    }

    @Test fun contaEspectadoresPeloViewers() = runTest {
        val a = amb(); a.noAr()
        assertEquals(0, a.c.estado.value.espectadores)
        a.app.value = a.app.value.copy(espectadores = mapOf(CANAL to listOf(1L, 2L, 3L), 5L to listOf(9L))); a.rodar()
        assertEquals(3, a.c.estado.value.espectadores)
        a.app.value = a.app.value.copy(espectadores = emptyMap()); a.rodar()
        assertEquals(0, a.c.estado.value.espectadores)
    }

    @Test fun contaOFpsRealEnviado() = runTest {
        val a = amb(); a.noAr(); a.chave()
        repeat(20) { a.p() }
        a.t.advanceTimeBy(1_100); a.rodar()
        assertEquals(21, a.c.estado.value.fpsReal)
    }

    @Test fun aplicarOpcoesNoArReconfiguraAFonteSemDerrubar() = runTest {
        val a = amb(); a.noAr(); a.chave()
        val novas = OpcoesDeTransmissao(QualidadeDeTransmissao.P1080, 30)
        a.c.aplicarOpcoes(novas); a.rodar()
        assertEquals(listOf(novas), a.fonte.opcoesReconfiguradas)
        assertEquals(novas, a.c.estado.value.opcoes)
        assertEquals(FaseDaTransmissao.Transmitindo, a.c.estado.value.fase)
        a.p()
        assertEquals("depois de reconfigurar espera o quadro-chave novo", 1, a.ws.enviados.size)
        a.chave(); assertEquals(2, a.ws.enviados.size)
    }

    @Test fun reconfigurarQueFalhaMostraAMensagemEMantemATransmissao() = runTest {
        val f = FonteFalsa().also { it.erroReconfigurar = "O codificador não aceita 60 fps." }
        val a = amb(fonte = f); a.noAr()
        a.c.aplicarOpcoes(OpcoesDeTransmissao(fps = 60)); a.rodar()
        assertEquals(FaseDaTransmissao.Transmitindo, a.c.estado.value.fase)
        assertEquals("O codificador não aceita 60 fps.", a.c.estado.value.mensagem)
        assertEquals(30, a.c.estado.value.opcoes.fps)
    }

    @Test fun falhaDaCapturaEncerraComAMensagem() = runTest {
        val a = amb(); a.noAr()
        a.fonte.ouvinte!!.aoFalhar("A captura de tela foi encerrada pelo sistema."); a.rodar()
        assertEquals(FaseDaTransmissao.Erro, a.c.estado.value.fase)
        assertEquals("A captura de tela foi encerrada pelo sistema.", a.c.estado.value.mensagem)
        assertTrue(a.ws.fechada)
    }

    @Test fun mudancaDeTamanhoDaTelaAparecenoEstadoSemDerrubar() = runTest {
        val a = amb(); a.noAr()
        a.fonte.ouvinte!!.aoTamanho(1552, 720); a.rodar()
        assertEquals(1552 to 720, a.c.estado.value.tamanho)
        assertEquals(FaseDaTransmissao.Transmitindo, a.c.estado.value.fase)
    }

    @Test fun semSessaoNaoAbreSocket() = runTest {
        val a = Amb(this, token = null).also { it.rodar() }
        a.c.pedir(CANAL, OpcoesDeTransmissao()); a.c.permissaoConcedida("x"); a.rodar()
        assertEquals(FaseDaTransmissao.Erro, a.c.estado.value.fase)
        assertTrue(a.transporte.conexoes.isEmpty())
    }
}
