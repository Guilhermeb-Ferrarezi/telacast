package app.telacast.net

import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

/**
 * [Transporte] de verdade: OkHttp. O `ping` do OkHttp (20 s) derruba a conexão se o servidor não responder,
 * o que cobre rede morta sem aviso; o servidor, por sua vez, encerra quem fica 60 s em silêncio.
 */
class OkHttpTransporte(
    private val cliente: OkHttpClient = clientePadrao(),
) : Transporte {
    override fun abrir(url: String, ouvinte: OuvinteTransporte): ConexaoTransporte {
        val fim = AtomicBoolean(false)
        fun terminou(motivo: String) {
            if (fim.compareAndSet(false, true)) ouvinte.aoFim(motivo)
        }
        val ws = cliente.newWebSocket(Request.Builder().url(url).build(), object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) = ouvinte.aoAbrir()
            override fun onMessage(webSocket: WebSocket, text: String) = ouvinte.aoTexto(text)
            override fun onMessage(webSocket: WebSocket, bytes: ByteString) = ouvinte.aoBinario(bytes.toByteArray())
            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                webSocket.close(code, null) // completa o aperto de mão de fechamento
            }
            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) = terminou("fechada ($code)")
            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                val http = response?.let { " HTTP ${it.code}" } ?: ""
                terminou((t.message ?: t.javaClass.simpleName) + http)
            }
        })
        return object : ConexaoTransporte {
            override fun enviar(texto: String): Boolean = ws.send(texto)
            override fun fechar() { ws.close(1000, null) }
            override fun cancelar() { ws.cancel() }
        }
    }

    companion object {
        fun clientePadrao(): OkHttpClient = OkHttpClient.Builder()
            .pingInterval(20, TimeUnit.SECONDS)
            .readTimeout(0, TimeUnit.MILLISECONDS) // WebSocket: sem limite de leitura (quem vigia é o ping)
            .connectTimeout(10, TimeUnit.SECONDS)
            .build()
    }
}
