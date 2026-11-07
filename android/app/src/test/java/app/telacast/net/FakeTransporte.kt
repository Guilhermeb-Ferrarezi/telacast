package app.telacast.net

import app.telacast.proto.C2S
import app.telacast.proto.Proto
import app.telacast.proto.S2C
import java.util.Collections

/** Servidor de mentira: o teste decide quando abrir, o que enviar e quando derrubar. */
class FakeTransporte : Transporte {
    val conexoes: MutableList<FakeConexao> = Collections.synchronizedList(mutableListOf())
    val ultima: FakeConexao get() = conexoes.last()

    override fun abrir(url: String, ouvinte: OuvinteTransporte): ConexaoTransporte =
        FakeConexao(url, ouvinte).also { conexoes += it }
}

class FakeConexao(val url: String, private val ouvinte: OuvinteTransporte) : ConexaoTransporte {
    private val enviados = Collections.synchronizedList(mutableListOf<String>())
    @Volatile var fechada = false
        private set

    /** Tudo o que o cliente mandou, decodificado. */
    val c2s: List<C2S> get() = synchronized(enviados) { enviados.map { Proto.lerC2S(it) } }
    val textos: List<String> get() = synchronized(enviados) { enviados.toList() }

    // ---- lado "servidor"
    fun abre() = ouvinte.aoAbrir()
    fun envia(m: S2C) = ouvinte.aoTexto(Proto.escreverS2C(m))
    fun enviaTexto(t: String) = ouvinte.aoTexto(t)
    fun enviaBinario(b: ByteArray) = ouvinte.aoBinario(b)
    fun cai(motivo: String = "rede caiu") { fechada = true; ouvinte.aoFim(motivo) }

    // ---- lado "cliente"
    override fun enviar(texto: String): Boolean {
        if (fechada) return false
        enviados += texto
        return true
    }

    override fun fechar() {
        if (!fechada) { fechada = true; ouvinte.aoFim("fechada (1000)") }
    }

    override fun cancelar() { fechada = true }
}
