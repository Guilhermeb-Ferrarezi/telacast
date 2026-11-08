package app.telacast.notificacao

import app.telacast.Container
import app.telacast.armazenamento.CofreEmMemoria
import app.telacast.armazenamento.PreferenciasEmMemoria
import app.telacast.data.AppState
import app.telacast.data.Repository
import app.telacast.net.Credencial
import app.telacast.net.FakeConexao
import app.telacast.net.FakeTransporte
import app.telacast.net.Gateway
import app.telacast.proto.C2S
import app.telacast.proto.Fixtures
import app.telacast.proto.Friend
import app.telacast.proto.FriendStatus
import app.telacast.proto.Message
import app.telacast.proto.Proto
import app.telacast.proto.S2C
import app.telacast.proto.Snapshot
import app.telacast.proto.Status
import app.telacast.proto.User
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class RepositoryLocalTest {
    private val snapshot: Snapshot = (Proto.lerS2C(Fixtures.texto("s2c", "Snapshot.json")) as S2C.Snapshot).snapshot
    private val eu = snapshot.me
    private val geral = 112623700000000012L

    private class Amb(val t: TestScope, val snapshot: Snapshot) {
        val transporte = FakeTransporte()
        val gateway = Gateway(t.backgroundScope, transporte, relogio = { t.currentTime })
        val repo = Repository(t.backgroundScope, gateway, relogio = { t.currentTime })
        val mensagens = ArrayList<Message>(); val pedidos = ArrayList<User>()
        val c: FakeConexao get() = transporte.ultima
        init {
            repo.aoMensagemAoVivo = { m, _ -> mensagens += m }; repo.aoPedidoDeAmizade = { u, _ -> pedidos += u }
            repo.entrar("h", Credencial.Login("g", "s")); t.runCurrent()
            c.abre(); c.envia(S2C.Auth("tok", snapshot.me)); c.envia(S2C.Snapshot(snapshot)); t.runCurrent()
        }
        fun rodar() = t.runCurrent()
    }

    private fun msg(id: Long, autor: Long = 2, texto: String = "oi") = Message(id, 112623700000000012L, autor, "marina.souza", texto, 1_794_000_000_000, false)

    @Test fun mensagemAoVivoDeOutraPessoaChamaOOuvinteUmaVez() = runTest {
        val a = Amb(this, snapshot)
        a.c.envia(S2C.Message(msg(1))); a.rodar()
        assertEquals(listOf(1L), a.mensagens.map { it.id })
    }

    @Test fun minhaMensagemEHistoricoNaoChamam() = runTest {
        val a = Amb(this, snapshot)
        a.c.envia(S2C.Message(msg(1, autor = eu.id))); a.rodar()
        a.c.envia(S2C.History(geral, listOf(msg(2), msg(3)))); a.rodar()
        assertTrue("histórico e eco próprio não notificam", a.mensagens.isEmpty())
    }

    @Test fun pedidoDeAmizadeNovoChamaMasOdaPrimeiraConexaoNao() = runTest {
        val ana = User(900, "ana.nova", true, Status.Online)
        val comPedido = snapshot.copy(friends = snapshot.friends + Friend(ana, FriendStatus.Incoming))
        val a = Amb(this, comPedido) // o pedido já estava no primeiro snapshot: não é novidade
        assertTrue(a.pedidos.isEmpty())
        val outra = User(901, "bia", true, Status.Online)
        a.c.envia(S2C.Snapshot(comPedido.copy(friends = comPedido.friends + Friend(outra, FriendStatus.Incoming)))); a.rodar()
        assertEquals(listOf("bia"), a.pedidos.map { it.name })
    }

    // ---------------------------------------------------------------- contêiner: retomar a sessão sem a interface
    @Test fun retomarSessaoSalvaSemInterface() = runTest {
        val transporte = FakeTransporte()
        val prefs = PreferenciasEmMemoria(servidor = "h:1")
        val c = Container(prefs, CofreEmMemoria("tok-salvo"), "x:2", transporte, backgroundScope, { currentTime })
        assertTrue(c.retomarSessaoSalva()); runCurrent()
        transporte.ultima.abre(); runCurrent()
        assertEquals(C2S.Resume("tok-salvo", 1), transporte.ultima.c2s.first())
        assertFalse("já está conectando: não abre outra conexão", c.retomarSessaoSalva())
        assertEquals(1, transporte.conexoes.size)
    }

    @Test fun semTokenOuSemServidorNaoRetoma() = runTest {
        val transporte = FakeTransporte()
        assertFalse(Container(PreferenciasEmMemoria(servidor = "h:1"), CofreEmMemoria(null), "", transporte, backgroundScope, { currentTime }).retomarSessaoSalva())
        assertFalse(Container(PreferenciasEmMemoria(), CofreEmMemoria("t"), "", transporte, backgroundScope, { currentTime }).retomarSessaoSalva())
        assertTrue(transporte.conexoes.isEmpty())
    }

    @Test fun manterConectadoComecaLigadoEMudaNoContainer() = runTest {
        val prefs = PreferenciasEmMemoria()
        val c = Container(prefs, CofreEmMemoria(), "", FakeTransporte(), backgroundScope, { currentTime })
        assertTrue(c.manterConectado.value)
        c.definirManterConectado(false)
        assertFalse(c.manterConectado.value); assertFalse(prefs.manterConectado)
    }
}
