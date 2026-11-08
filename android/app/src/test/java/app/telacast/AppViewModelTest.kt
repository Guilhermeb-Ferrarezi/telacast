package app.telacast

import app.telacast.armazenamento.CofreEmMemoria
import app.telacast.armazenamento.PreferenciasEmMemoria
import app.telacast.estado.Destino
import app.telacast.estado.Dialogo
import app.telacast.net.FakeConexao
import app.telacast.net.FakeTransporte
import app.telacast.proto.C2S
import app.telacast.proto.ErrorCode
import app.telacast.proto.Fixtures
import app.telacast.proto.Proto
import app.telacast.proto.S2C
import app.telacast.proto.Snapshot
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class AppViewModelTest {
    private val snapshot: Snapshot = (Proto.lerS2C(Fixtures.texto("s2c", "Snapshot.json")) as S2C.Snapshot).snapshot
    private val eu = snapshot.me
    private val servidor = snapshot.servers.first()
    private val geral = 112623700000000012L
    private val memes = 112623700000000013L
    private val categoria = 112623700000000010L

    private inner class Amb(val t: TestScope, token: String? = null, servidorSalvo: String = "") {
        val prefs = PreferenciasEmMemoria(servidor = servidorSalvo)
        val cofre = CofreEmMemoria(token)
        val transporte = FakeTransporte()
        val container = Container(prefs, cofre, servidorPadrao = "10.0.2.2:8080", transporte = transporte, escopo = t.backgroundScope, relogio = { t.currentTime })
        val vm = AppViewModel(container)
        val c: FakeConexao get() = transporte.ultima
        fun rodar() = t.runCurrent()
        fun autenticar() { c.abre(); c.envia(S2C.Auth("tok-novo", eu)); c.envia(S2C.Snapshot(snapshot)); rodar() }
    }

    private fun TestScope.amb(token: String? = null, servidorSalvo: String = "") = Amb(this, token, servidorSalvo).also { it.rodar() }

    @Test fun comecaNoLoginComOServidorPadrao() = runTest {
        val a = amb()
        assertEquals(Fase.Login, a.vm.fase.value)
        assertEquals("10.0.2.2:8080", a.vm.form.value.servidor)
        assertTrue(a.transporte.conexoes.isEmpty())
    }

    @Test fun formularioInvalidoNaoAbreConexao() = runTest {
        val a = amb()
        a.vm.entrar(); a.rodar()
        assertEquals("Informe o nome de usuário.", a.vm.form.value.erro)
        assertTrue(a.transporte.conexoes.isEmpty())
        a.vm.editarForm { it.copy(usuario = "ana") }
        assertNull("editar limpa o erro", a.vm.form.value.erro)
    }

    @Test fun loginCompletoGuardaTokenEUsuarioEVaiParaAPrincipal() = runTest {
        val a = amb()
        a.vm.editarForm { it.copy(usuario = "guilherme", senha = "senha1234") }
        a.vm.entrar(); a.rodar()
        assertTrue(a.vm.form.value.entrando)
        a.c.abre(); a.rodar()
        assertEquals(C2S.Login("guilherme", "senha1234", 1), a.c.c2s.single())
        a.autenticarSemAbrir()
        assertEquals(Fase.Principal, a.vm.fase.value)
        assertEquals("tok-novo", a.cofre.ler())
        assertEquals("guilherme", a.prefs.usuario)
        assertEquals("10.0.2.2:8080", a.prefs.servidor)
        assertFalse(a.vm.form.value.entrando)
        assertEquals("a senha não fica na memória do formulário", "", a.vm.form.value.senha)
    }

    private fun Amb.autenticarSemAbrir() { c.envia(S2C.Auth("tok-novo", eu)); c.envia(S2C.Snapshot(snapshot)); rodar() }

    @Test fun senhaErradaMostraErroEmPortugues() = runTest {
        val a = amb()
        a.vm.editarForm { it.copy(usuario = "ana", senha = "errada123") }
        a.vm.entrar(); a.rodar(); a.c.abre(); a.rodar()
        a.c.envia(S2C.Error(ErrorCode.InvalidCredentials, "Usuário ou senha incorretos")); a.rodar()
        assertEquals(Fase.Login, a.vm.fase.value)
        assertEquals("Usuário ou senha incorretos.", a.vm.form.value.erro)
        assertFalse(a.vm.form.value.entrando)
        assertNull(a.cofre.ler())
    }

    @Test fun cadastroMandaRegister() = runTest {
        val a = amb()
        a.vm.alternarModo()
        a.vm.editarForm { it.copy(usuario = "nova", senha = "12345678", convite = " ABCD-1234 ") }
        a.vm.entrar(); a.rodar(); a.c.abre(); a.rodar()
        assertEquals(C2S.Register("nova", "12345678", "ABCD-1234", 1), a.c.c2s.single())
    }

    @Test fun retomaASessaoSalvaSemMostrarOFormulario() = runTest {
        val a = amb(token = "salvo", servidorSalvo = "meu.servidor:9")
        assertEquals(Fase.Retomando, a.vm.fase.value)
        a.c.abre(); a.rodar()
        assertEquals("ws://meu.servidor:9/ws", a.c.url)
        assertEquals(C2S.Resume("salvo", 1), a.c.c2s.single())
        a.c.envia(S2C.Auth("salvo", eu)); a.c.envia(S2C.Snapshot(snapshot)); a.rodar()
        assertEquals(Fase.Principal, a.vm.fase.value)
    }

    @Test fun sessaoExpiradaApagaOTokenEVoltaAoLogin() = runTest {
        val a = amb(token = "velho", servidorSalvo = "h:1")
        a.c.abre(); a.rodar()
        a.c.envia(S2C.Error(ErrorCode.SessionExpired, "expirou")); a.rodar()
        assertEquals(Fase.Login, a.vm.fase.value)
        assertNull(a.cofre.ler())
        assertEquals("Sua sessão expirou. Entre de novo.", a.vm.form.value.erro)
    }

    @Test fun abreOPrimeiroCanalDeTextoAoEntrar() = runTest {
        val a = amb(); a.vm.editarForm { it.copy(usuario = "g", senha = "s") }; a.vm.entrar(); a.rodar(); a.autenticar()
        assertEquals(Destino.Canal(servidor.id, geral), a.vm.nav.value.destino)
        assertEquals(servidor.id, a.vm.nav.value.servidorNoPainel)
        assertEquals(geral, a.vm.estado.value.canalAberto)
        assertTrue("abrir o canal pede o histórico", a.c.c2s.contains(C2S.History(geral, null)))
        assertEquals(geral, a.prefs.ultimoCanal)
    }

    @Test fun reabreOUltimoCanalSalvo() = runTest {
        val a = amb(); a.prefs.ultimoServidor = servidor.id; a.prefs.ultimoCanal = memes
        a.vm.editarForm { it.copy(usuario = "g", senha = "s") }; a.vm.entrar(); a.rodar(); a.autenticar()
        assertEquals(Destino.Canal(servidor.id, memes), a.vm.nav.value.destino)
    }

    @Test fun categoriaRecolhidaFicaSalva() = runTest {
        val a = amb()
        a.vm.alternarCategoria(categoria)
        assertEquals(setOf(categoria), a.vm.recolhidas.value)
        assertEquals(setOf(categoria), a.prefs.categoriasRecolhidas)
        a.vm.alternarCategoria(categoria)
        assertTrue(a.prefs.categoriasRecolhidas.isEmpty())
    }

    @Test fun abrirCanalFechaAGavetaEMarcaComoLido() = runTest {
        val a = amb(); a.vm.editarForm { it.copy(usuario = "g", senha = "s") }; a.vm.entrar(); a.rodar(); a.autenticar()
        a.vm.abrirGaveta(); assertTrue(a.vm.nav.value.gavetaAberta)
        a.vm.abrirCanal(servidor.id, memes); a.rodar()
        assertFalse(a.vm.nav.value.gavetaAberta)
        assertEquals(memes, a.vm.estado.value.canalAberto)
        a.vm.abrirAmigos(); a.rodar()
        assertEquals(Destino.Amigos, a.vm.nav.value.destino)
        assertNull(a.vm.estado.value.canalAberto)
    }

    @Test fun servidorCriadoAbreOPrimeiroCanalEDmAbertaNavega() = runTest {
        val a = amb(); a.vm.editarForm { it.copy(usuario = "g", senha = "s") }; a.vm.entrar(); a.rodar(); a.autenticar()
        a.vm.abrirDialogo(Dialogo.AdicionarServidor(true))
        a.vm.criarServidor("Outro"); a.rodar()
        assertEquals(null, a.vm.nav.value.dialogo)
        assertEquals(C2S.CreateServer("Outro"), a.c.c2s.last())
        a.c.envia(S2C.DmOpened(112623800000000001L)); a.rodar()
        assertEquals(Destino.Dm(112623800000000001L), a.vm.nav.value.destino)
    }

    @Test fun canalQueSomeDoSnapshotVoltaAoInicio() = runTest {
        val a = amb(); a.vm.editarForm { it.copy(usuario = "g", senha = "s") }; a.vm.entrar(); a.rodar(); a.autenticar()
        val semGeral = snapshot.copy(servers = snapshot.servers.map { s -> s.copy(channels = s.channels.filter { it.id != geral }) })
        a.c.envia(S2C.Snapshot(semGeral)); a.rodar()
        assertEquals(Destino.Amigos, a.vm.nav.value.destino)
    }

    @Test fun sairApagaOTokenELimpaTudo() = runTest {
        val a = amb(); a.vm.editarForm { it.copy(usuario = "g", senha = "s") }; a.vm.entrar(); a.rodar(); a.autenticar()
        a.vm.sair(); a.rodar()
        assertEquals(C2S.Logout, a.c.c2s.last())
        assertNull(a.cofre.ler())
        assertEquals(Fase.Login, a.vm.fase.value)
        assertEquals("g", a.vm.form.value.usuario)
    }

    @Test fun avisoSomeSozinho() = runTest {
        val a = amb()
        a.vm.mostrarAviso("Teste"); a.rodar()
        assertEquals("Teste", a.vm.aviso.value)
        a.t.advanceTimeBy(3_600); a.rodar()
        assertNull(a.vm.aviso.value)
    }

    private val salaDeVoz = 112623700000000014L

    private fun TestScope.ambLogado() = amb().also { a -> a.vm.editarForm { it.copy(usuario = "g", senha = "s") }; a.vm.entrar(); a.rodar(); a.autenticar() }

    @Test fun abrirVozVaiParaATelaDoCanalENaoMarcaTextoComoAberto() = runTest {
        val a = ambLogado()
        a.vm.abrirVoz(servidor.id, salaDeVoz); a.rodar()
        assertEquals(Destino.Voz(servidor.id, salaDeVoz), a.vm.nav.value.destino)
        assertEquals(servidor.id, a.vm.nav.value.servidorNoPainel)
        assertNull("sem canal de texto aberto, nada é marcado como lido", a.vm.estado.value.canalAberto)
    }

    @Test fun destinoDeVozSobreviveEUmCanalApagadoVoltaAoInicio() = runTest {
        val a = ambLogado()
        a.vm.abrirVoz(servidor.id, salaDeVoz); a.rodar()
        assertEquals(Destino.Voz(servidor.id, salaDeVoz), a.vm.nav.value.destino)
        a.c.envia(S2C.Snapshot(snapshot.copy(servers = listOf(servidor.copy(channels = servidor.channels.filter { it.id != salaDeVoz }))))); a.rodar()
        assertEquals(Destino.Amigos, a.vm.nav.value.destino)
    }

    @Test fun entrarNaVozMandaVoiceJoinEConfirmacaoDoServidorConecta() = runTest {
        val a = ambLogado()
        a.vm.entrarNaVoz(salaDeVoz); a.rodar()
        assertTrue(a.c.c2s.contains(C2S.VoiceJoin(salaDeVoz)))
        assertEquals(app.telacast.voz.FaseDaVoz.Entrando, a.vm.voz.value.fase)
        // sem dispositivo de áudio (o contêiner dos testes usa FabricaSemAudio) a entrada falha e o erro vira aviso
        a.c.envia(S2C.VoiceUpdate(app.telacast.proto.VoiceState(eu.id, salaDeVoz, false, false))); a.rodar()
        assertEquals(app.telacast.voz.FaseDaVoz.Fora, a.vm.voz.value.fase)
        assertTrue(a.vm.aviso.value!!.contains("áudio"))
        assertEquals(C2S.VoiceLeave, a.c.c2s.last())
    }

    @Test fun abrirTelaDaVozLevaAoCanalDaChamada() = runTest {
        val a = ambLogado()
        a.vm.entrarNaVoz(salaDeVoz); a.rodar()
        a.vm.abrirTelaDaVoz(); a.rodar()
        assertEquals(Destino.Voz(servidor.id, salaDeVoz), a.vm.nav.value.destino)
    }

    @Test fun permissaoNegadaMostraAvisoENaoEntra() = runTest {
        val a = ambLogado()
        a.vm.recusarMicrofone(); a.rodar()
        assertTrue(a.vm.aviso.value!!.contains("microfone"))
        assertEquals(app.telacast.voz.FaseDaVoz.Fora, a.vm.voz.value.fase)
        assertTrue(a.c.c2s.none { it is C2S.VoiceJoin })
    }

    @Test fun opcoesDeVozSaoGuardadasENoControlador() = runTest {
        val a = amb()
        a.vm.definirOpcoesDeVoz { it.copy(supressaoDeRuido = false, ganhoDeEntrada = 2f) }; a.rodar()
        assertFalse(a.vm.voz.value.opcoes.supressaoDeRuido)
        assertEquals(2f, a.vm.voz.value.opcoes.ganhoDeEntrada, 0.001f)
        assertEquals(2f, a.prefs.opcoesDeVoz.ganhoDeEntrada, 0.001f)
    }

    // ---------------------------------------------------------------- push (D1): toque na notificação
    private fun TestScope.ambComToque(d: app.telacast.push.DestinoDoToque, logado: Boolean): Amb {
        val a = amb()
        a.vm.abrirPorNotificacao(d); a.rodar()
        if (!logado) return a
        a.vm.editarForm { it.copy(usuario = "g", senha = "s") }; a.vm.entrar(); a.rodar(); a.autenticar()
        return a
    }

    @Test fun toqueEmDmAbreAConversaDepoisDeEntrar() = runTest {
        val a = ambComToque(app.telacast.push.DestinoDoToque.Dm(112623800000000001L), logado = true)
        assertEquals(Destino.Dm(112623800000000001L), a.vm.nav.value.destino)
        assertEquals(112623800000000001L, a.vm.estado.value.canalAberto)
    }

    @Test fun toqueEmCanalAbreOCanalEMarcaComoAberto() = runTest {
        val a = ambComToque(app.telacast.push.DestinoDoToque.Canal(servidor.id, memes), logado = true)
        assertEquals(Destino.Canal(servidor.id, memes), a.vm.nav.value.destino)
        assertEquals(memes, a.vm.estado.value.canalAberto)
    }

    @Test fun toqueEmCanalQueNaoExisteMaisVaiParaOInicio() = runTest {
        val a = ambComToque(app.telacast.push.DestinoDoToque.Canal(servidor.id, 424242L), logado = true)
        assertEquals(Destino.Amigos, a.vm.nav.value.destino)
    }

    @Test fun toqueEmPedidoDeAmizadeAbreAbaPendente() = runTest {
        val a = ambComToque(app.telacast.push.DestinoDoToque.Pendentes, logado = true)
        assertEquals(Destino.Amigos, a.vm.nav.value.destino)
        assertEquals(app.telacast.estado.AbaInicio.Pendente, a.vm.nav.value.abaInicio)
    }

    @Test fun toqueComOAppJaAbertoNavegaNaHora() = runTest {
        val a = amb(); a.vm.editarForm { it.copy(usuario = "g", senha = "s") }; a.vm.entrar(); a.rodar(); a.autenticar()
        a.vm.abrirPorNotificacao(app.telacast.push.DestinoDoToque.Canal(servidor.id, memes)); a.rodar()
        assertEquals(Destino.Canal(servidor.id, memes), a.vm.nav.value.destino)
        a.vm.abrirPorNotificacao(app.telacast.push.DestinoDoToque.Dm(112623800000000002L)); a.rodar()
        assertEquals(Destino.Dm(112623800000000002L), a.vm.nav.value.destino)
    }

    @Test fun sairDescartaToquePendenteEPermissaoDeNotificacoesPedeUmaVezSo() = runTest {
        val a = amb(); a.vm.editarForm { it.copy(usuario = "g", senha = "s") }; a.vm.entrar(); a.rodar()
        a.vm.abrirPorNotificacao(app.telacast.push.DestinoDoToque.Canal(servidor.id, memes)); a.vm.sair(); a.rodar()
        a.vm.editarForm { it.copy(usuario = "g", senha = "s") }; a.vm.entrar(); a.rodar(); a.autenticar()
        assertEquals("o toque antigo não vale na conta nova: abre o destino inicial", Destino.Canal(servidor.id, geral), a.vm.nav.value.destino)
        assertTrue(a.vm.devePedirNotificacoes())
        a.vm.marcarPediuNotificacoes()
        assertFalse(a.vm.devePedirNotificacoes())
    }

    @Test fun sairMandaPushUnregisterAntesDoLogout() = runTest {
        val a = amb(); a.prefs.tokenDePush = "tok-1"
        a.vm.editarForm { it.copy(usuario = "g", senha = "s") }; a.vm.entrar(); a.rodar(); a.autenticar()
        a.vm.sair(); a.rodar()
        val c2s = a.c.c2s
        assertEquals(C2S.PushUnregister("tok-1"), c2s[c2s.lastIndex - 1])
        assertEquals(C2S.Logout, c2s.last())
        assertEquals("", a.prefs.tokenDePush)
    }
}
