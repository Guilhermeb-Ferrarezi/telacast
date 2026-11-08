package app.telacast.push

import app.telacast.armazenamento.PreferenciasEmMemoria
import app.telacast.data.Repository
import app.telacast.net.FakeConexao
import app.telacast.net.FakeTransporte
import app.telacast.net.Gateway
import app.telacast.net.Credencial
import app.telacast.proto.C2S
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
import org.junit.Assert.assertTrue
import org.junit.Test

private class TokenFalso(var token: String? = "tok-1") : ProvedorDeToken {
    var apagados = 0; var pedidos = 0
    override suspend fun obter(): String? { pedidos++; return token }
    override suspend fun apagar() { apagados++ }
}

@OptIn(ExperimentalCoroutinesApi::class)
class RegistroTest {
    private val snapshot: Snapshot = (Proto.lerS2C(Fixtures.texto("s2c", "Snapshot.json")) as S2C.Snapshot).snapshot

    private inner class Amb(val t: TestScope, token: String? = "tok-1", logar: Boolean = true, val salvo: String = "") {
        val transporte = FakeTransporte()
        val gateway = Gateway(t.backgroundScope, transporte, relogio = { t.currentTime })
        val repo = Repository(t.backgroundScope, gateway, relogio = { t.currentTime })
        val prefs = PreferenciasEmMemoria(tokenDePush = salvo)
        val provedor = TokenFalso(token)
        val push = GerenciadorDePush(repo, t.backgroundScope, provedor, prefs)
        val foco = ControladorDeFoco(repo, t.backgroundScope)
        val c: FakeConexao get() = transporte.ultima
        val enviados get() = transporte.conexoes.flatMap { it.c2s }

        init { t.runCurrent(); if (logar) { repo.entrar("h", Credencial.Login("g", "s")); t.runCurrent(); autenticar() } }
        fun autenticar() { c.abre(); c.envia(S2C.Auth("tok", snapshot.me)); c.envia(S2C.Snapshot(snapshot)); t.runCurrent() }
        fun rodar() = t.runCurrent()
        fun reconectar() { c.cai(); rodar(); t.advanceTimeBy(1_000); rodar(); autenticar() }
    }

    private fun TestScope.amb(token: String? = "tok-1", logar: Boolean = true, salvo: String = "") = Amb(this, token, logar, salvo)

    // ---------------------------------------------------------------- registro do token
    @Test fun registraOTokenAoFicarConectadoESalva() = runTest {
        val a = amb()
        assertEquals(C2S.PushRegister("tok-1", "fcm"), a.enviados.single { it is C2S.PushRegister })
        assertEquals("tok-1", a.prefs.tokenDePush)
    }

    @Test fun semCredenciaisDoFirebaseNaoRegistraNada() = runTest {
        val a = amb(token = null)
        assertTrue(a.enviados.none { it is C2S.PushRegister })
        assertEquals("", a.prefs.tokenDePush)
    }

    @Test fun tokenSalvoNaoPedeDeNovoAoFirebase() = runTest {
        val a = amb(salvo = "antigo")
        assertEquals(C2S.PushRegister("antigo", "fcm"), a.enviados.single { it is C2S.PushRegister })
        assertEquals(0, a.provedor.pedidos)
    }

    @Test fun registraDeNovoEmCadaReconexao() = runTest {
        val a = amb()
        a.reconectar()
        assertEquals(2, a.enviados.count { it is C2S.PushRegister })
    }

    @Test fun naoRegistraAntesDeAutenticar() = runTest {
        val a = amb(logar = false)
        a.push.aoNovoToken("novo")
        assertEquals("guarda para depois", "novo", a.prefs.tokenDePush)
        assertTrue(a.transporte.conexoes.isEmpty())
        a.repo.entrar("h", Credencial.Login("g", "s")); a.rodar(); a.autenticar()
        assertEquals(C2S.PushRegister("novo", "fcm"), a.enviados.single { it is C2S.PushRegister })
    }

    @Test fun tokenNovoComConexaoCancelaOAntigoERegistraONovo() = runTest {
        val a = amb()
        a.push.aoNovoToken("tok-2"); a.rodar()
        val fim = a.enviados.takeLast(2)
        assertEquals(listOf(C2S.PushUnregister("tok-1"), C2S.PushRegister("tok-2", "fcm")), fim)
        assertEquals("tok-2", a.prefs.tokenDePush)
    }

    @Test fun sairCancelaOTokenAntesDoLogoutEApagaDoFcm() = runTest {
        val a = amb()
        a.push.aoSair(); a.repo.sair(); a.rodar()
        val c2s = a.enviados
        assertEquals(C2S.PushUnregister("tok-1"), c2s[c2s.lastIndex - 1])
        assertEquals(C2S.Logout, c2s.last())
        assertEquals("", a.prefs.tokenDePush)
        assertEquals(1, a.provedor.apagados)
    }

    // ---------------------------------------------------------------- foco (SetFocus)
    @Test fun focoSegueAsActivities() = runTest {
        val a = amb()
        a.foco.atividadeIniciou(); a.rodar()
        assertEquals(C2S.SetFocus(true), a.enviados.last())
        assertTrue(a.foco.emPrimeiroPlano)
        a.foco.atividadeParou(); a.rodar()
        assertEquals(C2S.SetFocus(false), a.enviados.last())
        assertTrue(!a.foco.emPrimeiroPlano)
    }

    @Test fun focoNaoRepeteNemPiscaNaRotacao() = runTest {
        val a = amb()
        a.foco.atividadeIniciou()
        val n = a.enviados.count { it is C2S.SetFocus }
        a.foco.atividadeParou(mudandoConfiguracao = true); a.foco.atividadeIniciou(); a.rodar()
        assertEquals("rotação não manda nada", n, a.enviados.count { it is C2S.SetFocus })
        a.foco.atividadeIniciou() // segunda activity
        assertEquals(n, a.enviados.count { it is C2S.SetFocus })
        a.foco.atividadeParou(); assertEquals("ainda há uma na frente", n, a.enviados.count { it is C2S.SetFocus })
    }

    @Test fun focoReenviadoNaReconexaoComOValorAtual() = runTest {
        val a = amb()
        a.foco.atividadeIniciou()
        a.reconectar()
        assertEquals(C2S.SetFocus(true), a.transporte.ultima.c2s.last { it is C2S.SetFocus })
        a.foco.atividadeParou(); a.rodar()
        a.reconectar()
        assertEquals("em segundo plano a conexão nova também diz false", C2S.SetFocus(false), a.transporte.ultima.c2s.last { it is C2S.SetFocus })
    }

    @Test fun focoEsperaAConexaoENaoMandaDesconectado() = runTest {
        val a = amb(logar = false)
        a.foco.atividadeIniciou(); a.rodar()
        assertTrue(a.transporte.conexoes.isEmpty())
        a.repo.entrar("h", Credencial.Login("g", "s")); a.rodar(); a.autenticar()
        assertEquals(C2S.SetFocus(true), a.enviados.last { it is C2S.SetFocus })
    }

    @Test fun aoCairAConexaoOServidorJaNaoTemFocoENaoMandamosNada() = runTest {
        val a = amb()
        a.foco.atividadeIniciou()
        val n = a.enviados.count { it is C2S.SetFocus }
        a.c.cai(); a.rodar()
        a.foco.atividadeParou(); a.rodar()
        assertEquals("desconectado: nada a enviar", n, a.enviados.count { it is C2S.SetFocus })
    }
}
