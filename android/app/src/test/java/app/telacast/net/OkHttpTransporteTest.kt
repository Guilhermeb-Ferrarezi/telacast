package app.telacast.net

import app.telacast.proto.C2S
import app.telacast.proto.ErrorCode
import app.telacast.proto.Proto
import app.telacast.proto.S2C
import app.telacast.proto.Status
import app.telacast.proto.User
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okio.ByteString
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.util.concurrent.LinkedBlockingQueue
import java.util.concurrent.TimeUnit

/** O OkHttp de verdade contra um servidor WebSocket de mentira (MockWebServer), em tempo real (esperas curtas). */
class OkHttpTransporteTest {
    private val eu = User(1, "ana", true, Status.Online)
    private lateinit var servidor: MockWebServer
    private lateinit var escopo: CoroutineScope

    @Before fun antes() {
        servidor = MockWebServer().also { it.start() }
        escopo = CoroutineScope(SupervisorJob() + Dispatchers.Default)
    }

    @After fun depois() {
        escopo.cancel()
        runCatching { servidor.shutdown() }
    }

    private fun gateway() = Gateway(escopo, OkHttpTransporte(), esperasMs = listOf(30L, 60L))

    private fun enderecoDoServidor() = "${servidor.hostName}:${servidor.port}"

    /** Aceita o upgrade; guarda o que o cliente manda e responde conforme `aoReceber`. */
    private class Roteiro(val aoReceber: (WebSocket, C2S) -> Unit) : WebSocketListener() {
        val recebidos = LinkedBlockingQueue<C2S>()
        @Volatile var ws: WebSocket? = null
        override fun onOpen(webSocket: WebSocket, response: Response) { ws = webSocket }
        override fun onMessage(webSocket: WebSocket, text: String) {
            val m = Proto.lerC2S(text)
            recebidos += m
            aoReceber(webSocket, m)
        }
        override fun onMessage(webSocket: WebSocket, bytes: ByteString) {}
    }

    private fun WebSocket.manda(m: S2C) { send(Proto.escreverS2C(m)) }

    @Test fun loginDeVerdadePeloWebSocket() = runBlocking<Unit> {
        val roteiro = Roteiro { ws, m -> if (m is C2S.Login) ws.manda(S2C.Auth("tok", eu)) }
        servidor.enqueue(MockResponse().withWebSocketUpgrade(roteiro))
        val g = gateway()
        g.conectar(enderecoDoServidor(), Credencial.Login("ana", "senha1234"))
        withTimeout(5_000) { g.estado.first { it == EstadoConexao.Conectado } }
        assertEquals(C2S.Login("ana", "senha1234", 1), roteiro.recebidos.poll(2, TimeUnit.SECONDS))
        assertEquals("/ws", servidor.takeRequest(2, TimeUnit.SECONDS)!!.path)
        assertEquals("tok", g.token)
        assertTrue(g.enviar(C2S.Typing(9)))
        assertEquals(C2S.Typing(9), roteiro.recebidos.poll(2, TimeUnit.SECONDS))
        g.desconectar()
    }

    @Test fun credencialRecusadaChegaComoFalha() = runBlocking<Unit> {
        val roteiro = Roteiro { ws, _ -> ws.manda(S2C.Error(ErrorCode.InvalidCredentials, "Usuário ou senha incorretos")) }
        servidor.enqueue(MockResponse().withWebSocketUpgrade(roteiro))
        val g = gateway()
        g.conectar(enderecoDoServidor(), Credencial.Login("ana", "errada123"))
        val f = withTimeout(5_000) { g.eventos.first() }
        assertEquals(EventoGateway.Falha(ErrorCode.InvalidCredentials, "Usuário ou senha incorretos", false), f)
        withTimeout(5_000) { g.estado.first { it == EstadoConexao.Desconectado } }
    }

    @Test fun quedaDoServidorReconectaComResume() = runBlocking<Unit> {
        val primeiro = Roteiro { ws, m ->
            if (m is C2S.Login) { ws.manda(S2C.Auth("tok", eu)); ws.close(1001, "reiniciando") }
        }
        val segundo = Roteiro { ws, m -> if (m is C2S.Resume) ws.manda(S2C.Auth("tok", eu)) }
        servidor.enqueue(MockResponse().withWebSocketUpgrade(primeiro))
        servidor.enqueue(MockResponse().withWebSocketUpgrade(segundo))
        val g = gateway()
        val eventos = mutableListOf<EventoGateway>()
        val coleta = escopo.launchColeta(g, eventos)
        g.conectar(enderecoDoServidor(), Credencial.Login("ana", "senha1234"))
        assertEquals(C2S.Resume("tok", 1), segundo.recebidos.poll(5, TimeUnit.SECONDS))
        withTimeout(5_000) { while (eventos.size < 2) kotlinx.coroutines.delay(10) }
        assertEquals(EventoGateway.Autenticado("tok", eu, false), eventos[0])
        assertEquals(EventoGateway.Autenticado("tok", eu, true), eventos[1])
        withTimeout(5_000) { g.estado.first { it == EstadoConexao.Conectado } }
        coleta.cancel()
        g.desconectar()
    }

    @Test fun servidorForaDoArFalhaNoLogin() = runBlocking<Unit> {
        val porta = servidor.port
        servidor.shutdown()
        val g = gateway()
        g.conectar("127.0.0.1:$porta", Credencial.Login("ana", "senha1234"))
        val f = withTimeout(10_000) { g.eventos.first() } as EventoGateway.Falha
        assertEquals(null, f.code)
        assertNotNull(f.msg)
    }

    @Test fun respostaHttpSemUpgradeEhFalhaDeRede() = runBlocking<Unit> {
        servidor.enqueue(MockResponse().setResponseCode(429))
        val g = gateway()
        g.conectar(enderecoDoServidor(), Credencial.Login("ana", "senha1234"))
        val f = withTimeout(10_000) { g.eventos.first() } as EventoGateway.Falha
        assertTrue(f.msg, f.msg.contains("429"))
    }

    private fun CoroutineScope.launchColeta(g: Gateway, destino: MutableList<EventoGateway>) =
        this.launch { g.eventos.collect { synchronized(destino) { destino += it } } }
}
