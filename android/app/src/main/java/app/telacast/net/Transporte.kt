package app.telacast.net

/**
 * Camada fina sobre o WebSocket, para o [Gateway] rodar na JVM com um transporte de mentira nos testes
 * (relógio virtual) e com o OkHttp de verdade no app. Puro Kotlin, sem Android.
 */
interface Transporte {
    /** Abre a conexão e devolve na hora; o resultado chega em [OuvinteTransporte] (de qualquer thread). */
    fun abrir(url: String, ouvinte: OuvinteTransporte): ConexaoTransporte
}

interface ConexaoTransporte {
    /** Enfileira um quadro de texto. `false` se a conexão já não aceita. Pode ser chamado de qualquer thread. */
    fun enviar(texto: String): Boolean

    /** Fecha com educação (o que já foi enfileirado sai antes). */
    fun fechar()

    /** Derruba na hora, sem esperar ninguém. */
    fun cancelar()
}

/** Eventos de uma conexão. Chamados da thread de rede: só repasse, nunca bloqueie. */
interface OuvinteTransporte {
    fun aoAbrir()
    fun aoTexto(texto: String)
    fun aoBinario(dados: ByteArray)

    /** A conexão terminou (fechada por um dos lados, ou falhou); sempre o último evento. */
    fun aoFim(motivo: String)
}
