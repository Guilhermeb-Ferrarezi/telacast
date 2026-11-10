package app.telacast.net

import app.telacast.proto.PROTO_VERSION

import app.telacast.proto.C2S
import app.telacast.proto.ErrorCode
import app.telacast.proto.S2C
import app.telacast.proto.Status
import app.telacast.proto.User
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class GatewayTest {
    private val eu = User(1, "ana", true, Status.Online)

    /** Gateway sobre o transporte de mentira, com relógio virtual; coleta os eventos numa lista. */
    private class Montagem(val teste: TestScope) {
        val transporte = FakeTransporte()
        val gateway = Gateway(teste.backgroundScope, transporte, relogio = { teste.currentTime })
        val eventos = mutableListOf<EventoGateway>()
        init { teste.backgroundScope.launch { gateway.eventos.collect { eventos += it } } }
    }

    private fun TestScope.montar() = Montagem(this)

    @Test fun versionMismatchNoResumeEhDefinitivoMasNaoInvalidaASessao() = runTest {
        val m = montar()
        m.gateway.conectar("h", Credencial.Retomar("tok")); runCurrent()
        m.transporte.ultima.abre(); runCurrent()
        m.transporte.ultima.envia(S2C.Error(ErrorCode.VersionMismatch, "protocolo")); runCurrent()
        assertEquals(EventoGateway.Falha(ErrorCode.VersionMismatch, "protocolo", false), m.eventos.single())
        advanceTimeBy(60_000); runCurrent()
        assertEquals("não reconecta sozinho", 1, m.transporte.conexoes.size)
    }

    @Test fun urlDoGatewayNormalizaOServidor() {
        assertEquals("ws://10.0.2.2:8080/ws", Gateway.urlDoGateway("10.0.2.2:8080"))
        assertEquals("ws://host:8080/ws", Gateway.urlDoGateway("ws://host:8080/"))
        assertEquals("wss://exemplo.com/ws", Gateway.urlDoGateway("https://exemplo.com"))
        assertEquals("ws://x/ws", Gateway.urlDoGateway(" http://x/ws "))
        assertEquals("wss://x/ws", Gateway.urlDoGateway("wss://x/ws"))
        assertEquals("domínio público sem esquema é seguro", "wss://telecast.guilhermebf.dev/ws", Gateway.urlDoGateway("telecast.guilhermebf.dev"))
        assertEquals("ws://192.168.0.5:8080/ws", Gateway.urlDoGateway("192.168.0.5:8080"))
        assertEquals("ws://localhost/ws", Gateway.urlDoGateway("localhost"))
        assertEquals("ws://meu-servidor/ws", Gateway.urlDoGateway("meu-servidor"))
        assertEquals("ws://exemplo.com:8080/ws", Gateway.urlDoGateway("exemplo.com:8080"))
    }

    @Test fun fluxoDeLogin() = runTest {
        val m = montar()
        m.gateway.conectar("localhost:8080", Credencial.Login("ana", "senha1234"))
        runCurrent()
        assertEquals(EstadoConexao.Conectando, m.gateway.estado.value)
        assertEquals("ws://localhost:8080/ws", m.transporte.ultima.url)
        assertFalse("antes de autenticar não manda nada", m.gateway.enviar(C2S.Typing(5)))

        m.transporte.ultima.abre()
        runCurrent()
        assertEquals(listOf<C2S>(C2S.Login("ana", "senha1234", PROTO_VERSION)), m.transporte.ultima.c2s)

        m.transporte.ultima.envia(S2C.Auth("tok", eu))
        runCurrent()
        assertEquals(EstadoConexao.Conectado, m.gateway.estado.value)
        assertEquals("tok", m.gateway.token)
        assertEquals(listOf<EventoGateway>(EventoGateway.Autenticado("tok", eu, reconexao = false)), m.eventos)
        assertTrue(m.gateway.enviar(C2S.Typing(5)))
        assertEquals(C2S.Typing(5), m.transporte.ultima.c2s.last())
    }

    @Test fun cadastroMandaRegisterComVersaoDoProtocolo() = runTest {
        val m = montar()
        m.gateway.conectar("h", Credencial.Registro("ana", "senha1234", "ABCD-1234"))
        runCurrent(); m.transporte.ultima.abre(); runCurrent()
        assertEquals(listOf<C2S>(C2S.Register("ana", "senha1234", "ABCD-1234", PROTO_VERSION)), m.transporte.ultima.c2s)
    }

    @Test fun erroDeCredencialEhDefinitivo() = runTest {
        val m = montar()
        m.gateway.conectar("h", Credencial.Login("ana", "errada123"))
        runCurrent(); m.transporte.ultima.abre(); runCurrent()
        m.transporte.ultima.envia(S2C.Error(ErrorCode.InvalidCredentials, "Usuário ou senha incorretos"))
        runCurrent()
        assertEquals(listOf<EventoGateway>(EventoGateway.Falha(ErrorCode.InvalidCredentials, "Usuário ou senha incorretos", false)), m.eventos)
        assertEquals(EstadoConexao.Desconectado, m.gateway.estado.value)
        advanceTimeBy(60_000)
        assertEquals("não reconecta com senha errada", 1, m.transporte.conexoes.size)
        assertTrue(m.transporte.ultima.fechada)
    }

    @Test fun loginSemRedeAvisaEmVezDeInsistir() = runTest {
        val m = montar()
        m.gateway.conectar("h", Credencial.Login("ana", "senha1234"))
        runCurrent()
        m.transporte.ultima.cai("Connection refused")
        runCurrent()
        val f = m.eventos.single() as EventoGateway.Falha
        assertEquals(null, f.code)
        assertTrue(f.msg.contains("Connection refused"))
        assertEquals(EstadoConexao.Desconectado, m.gateway.estado.value)
    }

    @Test fun retomadaComSessaoExpiradaVaiParaOLogin() = runTest {
        val m = montar()
        m.gateway.conectar("h", Credencial.Retomar("velho"))
        runCurrent(); m.transporte.ultima.abre(); runCurrent()
        assertEquals(listOf<C2S>(C2S.Resume("velho", PROTO_VERSION)), m.transporte.ultima.c2s)
        m.transporte.ultima.envia(S2C.Error(ErrorCode.SessionExpired, "Sessão expirada"))
        runCurrent()
        assertEquals(EventoGateway.Falha(ErrorCode.SessionExpired, "Sessão expirada", true), m.eventos.single())
        assertEquals(EstadoConexao.Desconectado, m.gateway.estado.value)
        advanceTimeBy(60_000)
        assertEquals(1, m.transporte.conexoes.size)
    }

    @Test fun sessaoExpiradaDepoisDeAutenticarTambemVaiParaOLogin() = runTest {
        val m = montar()
        m.gateway.conectar("h", Credencial.Login("ana", "senha1234"))
        runCurrent(); m.transporte.ultima.abre(); m.transporte.ultima.envia(S2C.Auth("t", eu)); runCurrent()
        m.transporte.ultima.envia(S2C.Error(ErrorCode.SessionExpired, "revogada"))
        runCurrent()
        assertEquals(EventoGateway.Falha(ErrorCode.SessionExpired, "revogada", true), m.eventos.last())
        assertEquals(EstadoConexao.Desconectado, m.gateway.estado.value)
    }

    @Test fun recuoDeReconexaoEhUmDoisQuatroOitoQuinze() = runTest {
        val m = montar()
        m.gateway.conectar("h", Credencial.Login("ana", "senha1234"))
        runCurrent(); m.transporte.ultima.abre(); m.transporte.ultima.envia(S2C.Auth("tok", eu)); runCurrent()
        assertEquals(1, m.transporte.conexoes.size)

        // cai depois de autenticar: 1 s, e a nova conexão retoma com Resume
        m.transporte.ultima.cai(); runCurrent()
        assertEquals(EstadoConexao.Reconectando(1, currentTime + 1_000), m.gateway.estado.value)
        advanceTimeBy(999); assertEquals(1, m.transporte.conexoes.size)
        advanceTimeBy(1); runCurrent(); assertEquals(2, m.transporte.conexoes.size)
        m.transporte.ultima.abre(); runCurrent()
        assertEquals(listOf<C2S>(C2S.Resume("tok", PROTO_VERSION)), m.transporte.ultima.c2s)

        // o servidor segue fora: falha antes do Auth, esperas 2, 4, 8, 15, 15
        for (espera in listOf(2_000L, 4_000L, 8_000L, 15_000L, 15_000L)) {
            val n = m.transporte.conexoes.size
            m.transporte.ultima.cai(); runCurrent()
            val est = m.gateway.estado.value as EstadoConexao.Reconectando
            assertEquals(currentTime + espera, est.quandoMs)
            advanceTimeBy(espera - 1); assertEquals(n, m.transporte.conexoes.size)
            advanceTimeBy(1); runCurrent(); assertEquals(n + 1, m.transporte.conexoes.size)
            m.transporte.ultima.abre(); runCurrent()
        }

        // volta: Auth zera o recuo, e o evento avisa que é reconexão
        m.transporte.ultima.envia(S2C.Auth("tok", eu)); runCurrent()
        assertEquals(EstadoConexao.Conectado, m.gateway.estado.value)
        assertEquals(EventoGateway.Autenticado("tok", eu, reconexao = true), m.eventos.last())
        m.transporte.ultima.cai(); runCurrent()
        assertEquals("recuo recomeça em 1 s", EstadoConexao.Reconectando(1, currentTime + 1_000), m.gateway.estado.value)
    }

    @Test fun erroPassageiroNaRetomadaTentaDeNovo() = runTest {
        val m = montar()
        m.gateway.conectar("h", Credencial.Retomar("tok"))
        runCurrent(); m.transporte.ultima.abre(); runCurrent()
        m.transporte.ultima.envia(S2C.Error(ErrorCode.RateLimited, "calma"))
        runCurrent()
        assertTrue(m.eventos.isEmpty())
        assertEquals(EstadoConexao.Reconectando(1, currentTime + 1_000), m.gateway.estado.value)
        advanceTimeBy(1_000); runCurrent()
        assertEquals(2, m.transporte.conexoes.size)
    }

    @Test fun descartaQuadrosRuinsSemDerrubar() = runTest {
        val m = montar()
        m.gateway.conectar("h", Credencial.Login("ana", "senha1234"))
        runCurrent(); m.transporte.ultima.abre(); m.transporte.ultima.envia(S2C.Auth("t", eu)); runCurrent()
        m.transporte.ultima.enviaTexto("""{"ThreadCreated":{"id":1}}""")
        m.transporte.ultima.enviaTexto("lixo")
        m.transporte.ultima.enviaTexto("""{"Snapshot":{"me":1}}""")
        m.transporte.ultima.envia(S2C.Typing(7, 8))
        runCurrent()
        assertEquals(3, m.gateway.descartados)
        assertEquals(EstadoConexao.Conectado, m.gateway.estado.value)
        assertEquals(EventoGateway.Recebido(S2C.Typing(7, 8)), m.eventos.last())
    }

    @Test fun quadrosBinariosChegamComoBytes() = runTest {
        val m = montar()
        m.gateway.conectar("h", Credencial.Login("ana", "senha1234"))
        runCurrent(); m.transporte.ultima.abre(); m.transporte.ultima.envia(S2C.Auth("t", eu)); runCurrent()
        m.transporte.ultima.enviaBinario(byteArrayOf(1, 2, 3))
        runCurrent()
        val b = m.eventos.last() as EventoGateway.Binario
        assertEquals(listOf<Byte>(1, 2, 3), b.bytes.toList())
    }

    @Test fun sairMandaLogoutEFecha() = runTest {
        val m = montar()
        m.gateway.conectar("h", Credencial.Login("ana", "senha1234"))
        runCurrent(); m.transporte.ultima.abre(); m.transporte.ultima.envia(S2C.Auth("t", eu)); runCurrent()
        m.gateway.sair()
        runCurrent()
        assertEquals(C2S.Logout, m.transporte.ultima.c2s.last())
        assertTrue(m.transporte.ultima.fechada)
        assertEquals(EstadoConexao.Desconectado, m.gateway.estado.value)
        assertEquals(null, m.gateway.token)
        advanceTimeBy(60_000)
        assertEquals("não reconecta depois de sair", 1, m.transporte.conexoes.size)
    }

    @Test fun sairEnquantoReconectaCancelaATentativa() = runTest {
        val m = montar()
        m.gateway.conectar("h", Credencial.Login("ana", "senha1234"))
        runCurrent(); m.transporte.ultima.abre(); m.transporte.ultima.envia(S2C.Auth("t", eu)); runCurrent()
        m.transporte.ultima.cai(); runCurrent()
        assertTrue(m.gateway.estado.value is EstadoConexao.Reconectando)
        m.gateway.sair(); runCurrent()
        assertEquals(EstadoConexao.Desconectado, m.gateway.estado.value)
        advanceTimeBy(60_000)
        assertEquals(1, m.transporte.conexoes.size)
    }
}
