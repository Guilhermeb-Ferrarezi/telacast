package app.telacast.voz

import app.telacast.data.Repository
import app.telacast.net.Credencial
import app.telacast.net.FakeConexao
import app.telacast.net.FakeTransporte
import app.telacast.net.Gateway
import app.telacast.proto.C2S
import app.telacast.proto.Fixtures
import app.telacast.proto.Proto
import app.telacast.proto.S2C
import app.telacast.proto.Snapshot
import app.telacast.proto.VoiceState
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
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

private class DispositivoFalso(val motor: MotorDeVoz, val aoFalhar: (String) -> Unit, val erroAoIniciar: String?, val lancaNaRota: Boolean = false) : DispositivoDeAudio {
    var iniciado = false; var parado = false
    var rota: RotaDeAudio? = null; var opcoes: OpcoesDeVoz? = null
    /** Quantas vezes qualquer método do dispositivo foi chamado (para provar que o mudo não o toca). */
    var chamadas = 0
    override fun iniciar(): String? { chamadas++; if (erroAoIniciar == null) iniciado = true; return erroAoIniciar }
    override fun parar() { chamadas++; parado = true }
    override fun definirRota(rota: RotaDeAudio) { chamadas++; if (lancaNaRota) throw IllegalStateException("rota recusada"); this.rota = rota }
    override fun aplicarOpcoes(opcoes: OpcoesDeVoz) { chamadas++; this.opcoes = opcoes }
}

private class ServicoFalso : ServicoDeVoz {
    var iniciadoCom: Pair<String, String>? = null; var paradas = 0
    override fun iniciar(titulo: String, texto: String) { iniciadoCom = titulo to texto }
    override fun parar() { paradas++; iniciadoCom = null }
}

@OptIn(ExperimentalCoroutinesApi::class)
class ControladorDeVozTest {
    private val snapshot: Snapshot = (Proto.lerS2C(Fixtures.texto("s2c", "Snapshot.json")) as S2C.Snapshot).snapshot
    private val eu = snapshot.me
    private val sala = 112623700000000014L
    private val marina = 112623612862660617L
    private val joao = 112623612862660633L

    private inner class Amb(val t: TestScope, val erroDoDispositivo: String? = null, val erroNaReabertura: String? = null, val lancaNaRota: Boolean = false) {
        val transporte = FakeTransporte()
        val gateway = Gateway(t.backgroundScope, transporte, relogio = { t.currentTime })
        val repo = Repository(t.backgroundScope, gateway, relogio = { t.currentTime })
        val dispositivos = ArrayList<DispositivoFalso>()
        val servico = ServicoFalso()
        val voz = ControladorDeVoz(repo, t.backgroundScope, ConcentusOpus(), { motor, _, _, aoFalhar -> DispositivoFalso(motor, aoFalhar, if (dispositivos.isEmpty()) erroDoDispositivo else erroNaReabertura ?: erroDoDispositivo, lancaNaRota).also { dispositivos += it } }, servico, { t.currentTime })
        val c: FakeConexao get() = transporte.ultima
        val enviados get() = transporte.conexoes.flatMap { it.c2s }

        init {
            repo.entrar("h", Credencial.Login("g", "s")); t.runCurrent()
            c.abre(); c.envia(S2C.Auth("tok", eu)); c.envia(S2C.Snapshot(snapshot)); t.runCurrent()
        }
        fun rodar() = t.runCurrent()
        /** O servidor confirma a entrada: manda o meu VoiceUpdate (e os outros continuam onde estavam). */
        fun confirmarEntrada(canal: Long = sala, mudo: Boolean = false, surdo: Boolean = false) { c.envia(S2C.VoiceUpdate(VoiceState(eu.id, canal, mudo, surdo))); rodar() }
        fun entrarEConfirmar() { voz.entrar(sala); rodar(); confirmarEntrada() }
    }

    private fun TestScope.amb(erro: String? = null, erroNaReabertura: String? = null, lancaNaRota: Boolean = false) = Amb(this, erro, erroNaReabertura, lancaNaRota)

    @Test fun entrarMandaVoiceJoinEFicaEntrandoAteOServidorConfirmar() = runTest {
        val a = amb()
        a.voz.entrar(sala); a.rodar()
        assertEquals(C2S.VoiceJoin(sala), a.enviados.last())
        assertEquals(FaseDaVoz.Entrando, a.voz.estado.value.fase)
        assertTrue("sem dispositivo antes da confirmação", a.dispositivos.isEmpty())
        a.confirmarEntrada()
        assertEquals(FaseDaVoz.Conectada, a.voz.estado.value.fase)
        assertTrue(a.dispositivos.single().iniciado)
        assertEquals("sala de reunião · Turma do Telacast", a.servico.iniciadoCom?.second)
        assertNotNull(a.voz.estado.value.desdeMs)
        assertNotNull("quadros binários vão para o motor", a.repo.ouvinteBinario)
    }

    @Test fun entrarDeNovoNoMesmoCanalNaoRepete() = runTest {
        val a = amb(); a.entrarEConfirmar()
        val n = a.enviados.count { it is C2S.VoiceJoin }
        a.voz.entrar(sala); a.rodar()
        assertEquals(n, a.enviados.count { it is C2S.VoiceJoin })
    }

    @Test fun servidorNaoRespondeEm8SegundosVoltaAForaComErro() = runTest {
        val a = amb()
        a.voz.entrar(sala); a.rodar()
        a.t.advanceTimeBy(7_999); a.rodar(); assertEquals(FaseDaVoz.Entrando, a.voz.estado.value.fase)
        a.t.advanceTimeBy(2); a.rodar()
        val s = a.voz.estado.value
        assertEquals(FaseDaVoz.Fora, s.fase); assertEquals("O servidor não respondeu à entrada na voz.", s.erro)
        assertEquals(C2S.VoiceLeave, a.enviados.last())
    }

    @Test fun falhaAoAbrirOAudioSaiDaVozEMostraOErro() = runTest {
        val a = amb(erro = "Microfone ocupado por outro app.")
        a.voz.entrar(sala); a.rodar(); a.confirmarEntrada()
        val s = a.voz.estado.value
        assertEquals(FaseDaVoz.Fora, s.fase); assertEquals("Microfone ocupado por outro app.", s.erro)
        assertEquals("avisa o servidor", C2S.VoiceLeave, a.enviados.last())
        assertNull(a.repo.ouvinteBinario)
        assertNull(a.servico.iniciadoCom)
    }

    @Test fun dispositivoQueQuebraDeNovoLogoDepoisDeReabrirSai() = runTest {
        val a = amb(); a.entrarEConfirmar()
        a.dispositivos[0].aoFalhar("O fone foi desconectado."); a.rodar()    // 1a falha: reabre
        a.dispositivos[1].aoFalhar("O fone foi desconectado."); a.rodar()    // 2a, dentro de 10 s: sai
        assertEquals(FaseDaVoz.Fora, a.voz.estado.value.fase)
        assertEquals("O fone foi desconectado.", a.voz.estado.value.erro)
        assertTrue(a.dispositivos.all { it.parado })
    }

    @Test fun sairMandaVoiceLeaveEFechaTudo() = runTest {
        val a = amb(); a.entrarEConfirmar()
        a.voz.sair(); a.rodar()
        assertEquals(C2S.VoiceLeave, a.enviados.last())
        assertEquals(FaseDaVoz.Fora, a.voz.estado.value.fase)
        assertTrue(a.dispositivos.single().parado); assertNull(a.repo.ouvinteBinario); assertEquals(1, a.servico.paradas)
        assertNull(a.voz.estado.value.erro)
    }

    @Test fun servidorTiraAPessoaDaVoz() = runTest {
        val a = amb(); a.entrarEConfirmar()
        a.c.envia(S2C.VoiceLeft(eu.id, sala)); a.rodar()
        assertEquals(FaseDaVoz.Fora, a.voz.estado.value.fase)
        assertEquals("Você saiu da chamada de voz.", a.voz.estado.value.erro)
    }

    @Test fun mudoSurdoEOsVoiceSetDoDesktop() = runTest {
        val a = amb(); a.entrarEConfirmar()
        a.voz.alternarMudo()
        assertTrue(a.voz.estado.value.mudo); assertEquals(C2S.VoiceSet(true, false), a.enviados.last())
        a.voz.alternarSurdo()
        assertEquals(C2S.VoiceSet(true, true), a.enviados.last())
        a.voz.alternarMudo() // surdo: ligar o microfone liga o áudio também
        assertFalse(a.voz.estado.value.surdo); assertFalse(a.voz.estado.value.mudo)
        assertEquals(C2S.VoiceSet(false, false), a.enviados.last())
        a.voz.alternarSurdo(); assertEquals("surdo implica mudo no servidor", C2S.VoiceSet(true, true), a.enviados.last())
    }

    @Test fun mudoEscolhidoAntesDeEntrarVaiDepoisDaConfirmacao() = runTest {
        val a = amb()
        a.voz.alternarMudo()
        assertTrue("fora da voz não manda VoiceSet", a.enviados.none { it is C2S.VoiceSet })
        a.voz.entrar(sala); a.rodar(); a.confirmarEntrada()
        assertEquals(C2S.VoiceSet(true, false), a.enviados.last())
    }

    @Test fun mudoDeVerdadeNaoMandaQuadros() = runTest {
        val a = amb(); a.entrarEConfirmar()
        val motor = a.dispositivos.single().motor
        senoide(10).forEach { motor.capturar(it) }
        assertTrue(a.transporte.ultima.binarios.isNotEmpty())
        val antes = a.transporte.ultima.binarios.size
        a.voz.alternarMudo()
        senoide(10).forEach { motor.capturar(it) }
        assertEquals(antes, a.transporte.ultima.binarios.size)
        assertEquals(1, a.transporte.ultima.binarios.first()[0].toInt())
    }

    @Test fun quedaDaConexaoParaOAudioEReentraSozinha() = runTest {
        val a = amb(); a.entrarEConfirmar()
        a.c.cai(); a.rodar()
        assertEquals(FaseDaVoz.Reconectando, a.voz.estado.value.fase)
        assertTrue(a.dispositivos.single().parado)
        assertNull(a.repo.ouvinteBinario)
        a.t.advanceTimeBy(1_000); a.rodar()
        a.c.abre(); a.c.envia(S2C.Auth("tok", eu)); a.c.envia(S2C.Snapshot(snapshot)); a.rodar()
        assertEquals("o servidor tirou da voz: entra de novo", C2S.VoiceJoin(sala), a.transporte.ultima.c2s.last())
        assertEquals(FaseDaVoz.Entrando, a.voz.estado.value.fase)
        a.confirmarEntrada()
        assertEquals(FaseDaVoz.Conectada, a.voz.estado.value.fase)
        assertEquals("um dispositivo novo", 2, a.dispositivos.size)
        assertTrue(a.dispositivos[1].iniciado)
    }

    @Test fun mudoSobreviveAReconexao() = runTest {
        val a = amb(); a.entrarEConfirmar(); a.voz.alternarMudo()
        a.c.cai(); a.rodar(); a.t.advanceTimeBy(1_000); a.rodar()
        a.c.abre(); a.c.envia(S2C.Auth("tok", eu)); a.c.envia(S2C.Snapshot(snapshot)); a.rodar(); a.confirmarEntrada()
        assertTrue(a.voz.estado.value.mudo)
        assertEquals(C2S.VoiceSet(true, false), a.transporte.ultima.c2s.last())
    }

    @Test fun sairEnquantoReconectaCancelaTudo() = runTest {
        val a = amb(); a.entrarEConfirmar()
        a.c.cai(); a.rodar()
        a.voz.sair(); a.rodar()
        assertEquals(FaseDaVoz.Fora, a.voz.estado.value.fase)
        a.t.advanceTimeBy(5_000); a.rodar()
        assertEquals(FaseDaVoz.Fora, a.voz.estado.value.fase)
    }

    @Test fun trocarDeCanalEnquantoConectada() = runTest {
        val a = amb(); a.entrarEConfirmar()
        val outro = 999L
        a.voz.entrar(outro); a.rodar()
        assertEquals(C2S.VoiceJoin(outro), a.enviados.last())
        assertEquals("o áudio continua aberto durante a troca", FaseDaVoz.Conectada, a.voz.estado.value.fase)
        a.c.envia(S2C.VoiceUpdate(VoiceState(eu.id, outro, false, false))); a.rodar()
        assertEquals(outro, a.voz.estado.value.canal)
        assertEquals(1, a.dispositivos.size)
    }

    @Test fun quemEstaFalandoVemDosQuadrosRecebidos() = runTest {
        val a = amb(); a.entrarEConfirmar()
        val enc = ConcentusOpus().codificador()
        senoide(4).forEachIndexed { i, q -> a.c.enviaBinario(QuadroDeVoz.codificarRecebido(marina, i, enc.codificar(q)!!)) }
        a.rodar(); a.t.advanceTimeBy(150); a.rodar()
        val s = a.voz.estado.value
        assertEquals(setOf(marina), s.falando)
        assertFalse(joao in s.falando)
        a.t.advanceTimeBy(500); a.rodar()
        assertTrue("depois de ~250 ms sem pacotes, para de falar", a.voz.estado.value.falando.isEmpty())
    }

    @Test fun euFalandoENivelDoMicrofone() = runTest {
        val a = amb(); a.entrarEConfirmar()
        val motor = a.dispositivos.single().motor
        senoide(5, 0.5f).forEach { motor.capturar(it) }
        a.t.advanceTimeBy(150); a.rodar()
        assertTrue(a.voz.estado.value.euFalando); assertTrue(a.voz.estado.value.nivel > 0.3f)
    }

    @Test fun volumeRotaEOpcoesChegamAoDispositivoEAoMotor() = runTest {
        val a = amb(); a.entrarEConfirmar()
        a.voz.definirVolume(marina, 1.5f); a.voz.definirVolume(joao, 9f)
        assertEquals(1.5f, a.voz.estado.value.volumeDe(marina), 0f); assertEquals(2f, a.voz.estado.value.volumeDe(joao), 0f)
        assertEquals(1f, a.voz.estado.value.volumeDe(12345), 0f)
        assertEquals(1.5f, a.dispositivos.single().motor.volume(marina), 0f)
        a.voz.definirRota(RotaDeAudio.Fone); assertEquals(RotaDeAudio.Fone, a.dispositivos.single().rota)
        a.voz.definirOpcoes(OpcoesDeVoz(supressaoDeRuido = false, sensibilidade = 0.2f))
        assertFalse(a.dispositivos.single().opcoes!!.supressaoDeRuido)
    }

    @Test fun volumeEscolhidoAntesDeEntrarValeDepois() = runTest {
        val a = amb()
        a.voz.definirVolume(marina, 0.4f)
        a.entrarEConfirmar()
        assertEquals(0.4f, a.dispositivos.single().motor.volume(marina), 0f)
    }

    @Test fun recusarPermissaoMostraErroESai() = runTest {
        val a = amb()
        a.voz.entrar(sala); a.rodar()
        a.voz.recusar("Sem a permissão do microfone não dá para entrar na voz.")
        assertEquals(FaseDaVoz.Fora, a.voz.estado.value.fase)
        assertEquals("Sem a permissão do microfone não dá para entrar na voz.", a.voz.estado.value.erro)
    }

    @Test fun semConexaoNaoEntra() = runTest {
        val a = amb()
        a.c.cai(); a.rodar()
        a.voz.entrar(sala); a.rodar()
        assertEquals(FaseDaVoz.Fora, a.voz.estado.value.fase)
        assertEquals("Sem conexão com o servidor.", a.voz.estado.value.erro)
    }

    // ---------------------------------------------------------------- áudio que falha no meio da chamada (Bluetooth, rota, foco)

    @Test fun mudoESurdoNuncaTocamNoDispositivoNemNaRota() = runTest {
        val a = amb(); a.entrarEConfirmar()
        val d = a.dispositivos.single()
        val antes = d.chamadas
        a.voz.alternarMudo(); a.voz.alternarMudo(); a.voz.alternarMudo(); a.voz.alternarSurdo(); a.voz.alternarSurdo(); a.rodar()
        assertEquals("nenhuma chamada ao dispositivo", antes, d.chamadas)
        assertEquals(1, a.dispositivos.size)
        assertEquals(FaseDaVoz.Conectada, a.voz.estado.value.fase)
        assertTrue(a.enviados.none { it == C2S.VoiceLeave })
        // e o eco do servidor (VoiceUpdate do próprio usuário mudo) também não derruba nada
        a.c.envia(S2C.VoiceUpdate(VoiceState(eu.id, sala, muted = true, deafened = false))); a.rodar()
        assertEquals(FaseDaVoz.Conectada, a.voz.estado.value.fase)
        assertEquals(1, a.dispositivos.size)
    }

    @Test fun falhaDoAudioReabreODispositivoEAChamadaContinua() = runTest {
        val a = amb(); a.entrarEConfirmar(); a.voz.alternarMudo()
        a.dispositivos[0].aoFalhar("O microfone parou de responder."); a.rodar()
        assertEquals("a chamada não cai", FaseDaVoz.Conectada, a.voz.estado.value.fase)
        assertNull(a.voz.estado.value.erro)
        assertEquals(2, a.dispositivos.size)
        assertTrue(a.dispositivos[0].parado); assertTrue(a.dispositivos[1].iniciado)
        assertTrue("o servidor não foi avisado de saída", a.enviados.none { it == C2S.VoiceLeave })
        assertTrue("o mudo sobrevive", a.voz.estado.value.mudo)
        assertNotNull(a.repo.ouvinteBinario)
    }

    @Test fun segundaFalhaLogoDepoisDesisteEPrimeiraAposAJanelaReabreDeNovo() = runTest {
        val a = amb(); a.entrarEConfirmar()
        a.dispositivos[0].aoFalhar("x"); a.rodar()
        a.t.advanceTimeBy(11_000); a.rodar()
        a.dispositivos[1].aoFalhar("y"); a.rodar()
        assertEquals("fora da janela de 10 s reabre de novo", FaseDaVoz.Conectada, a.voz.estado.value.fase)
        assertEquals(3, a.dispositivos.size)
        a.t.advanceTimeBy(2_000); a.rodar()
        a.dispositivos[2].aoFalhar("z"); a.rodar()
        assertEquals("falhou de novo dentro da janela: sai", FaseDaVoz.Fora, a.voz.estado.value.fase)
        assertEquals("z", a.voz.estado.value.erro)
        assertEquals(C2S.VoiceLeave, a.enviados.last())
    }

    @Test fun seAReaberturaFalharSaiComAMensagemDela() = runTest {
        val a = amb(erroNaReabertura = "Não consegui abrir o microfone.")
        // o primeiro dispositivo abre bem, o segundo não
        a.voz.entrar(sala); a.rodar(); a.confirmarEntrada()
        a.dispositivos[0].aoFalhar("caiu"); a.rodar()
        assertEquals(FaseDaVoz.Fora, a.voz.estado.value.fase)
        assertEquals("Não consegui abrir o microfone.", a.voz.estado.value.erro)
        assertEquals(C2S.VoiceLeave, a.enviados.last())
    }

    @Test fun avisoDeFalhaDeDispositivoAntigoEhIgnorado() = runTest {
        val a = amb(); a.entrarEConfirmar()
        val antigo = a.dispositivos[0]
        antigo.aoFalhar("x"); a.rodar()
        antigo.aoFalhar("de novo, do velho"); a.rodar() // já foi trocado: não conta
        assertEquals(FaseDaVoz.Conectada, a.voz.estado.value.fase)
        assertEquals(2, a.dispositivos.size)
    }

    @Test fun trocarARotaQueODispositivoRecusaNaoDerrubaAChamada() = runTest {
        val a = amb(lancaNaRota = true); a.entrarEConfirmar()
        a.voz.definirRota(RotaDeAudio.Fone); a.rodar()
        assertEquals(FaseDaVoz.Conectada, a.voz.estado.value.fase)
        assertEquals(RotaDeAudio.Fone, a.voz.estado.value.rota)
    }
}
