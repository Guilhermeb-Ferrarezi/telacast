package app.telacast.tela

/**
 * Backpressure da transmissão: o WebSocket nunca acumula fila. Se o que está esperando para sair passa do limite, descarta o
 * quadro e TODOS os quadros P seguintes (sem o anterior eles não decodificam) até o próximo quadro-chave, e pede um. Um quadro-chave
 * só é descartado se a fila estiver absurda. Puro; quem chama passa os bytes pendentes do socket.
 */
class PoliticaDeEnvio(
    private val maxPendente: Long = 512L * 1024,
    private val maxPendenteParaChave: Long = 3L * 1024 * 1024,
) {
    sealed interface Decisao {
        data object Enviar : Decisao
        /** `pedirChave`: peça um quadro-chave ao codificador (a primeira vez que começa a descartar). */
        data class Descartar(val pedirChave: Boolean) : Decisao
    }

    private var ateChave = true // começa exigindo um quadro-chave: o espectador precisa de um para decodificar
    @Volatile var descartados = 0
        private set

    /** Recomeça (nova conexão): só passa a partir do próximo quadro-chave. */
    @Synchronized fun exigirChave() { ateChave = true }

    @Synchronized fun decidir(chave: Boolean, pendente: Long): Decisao {
        if (chave) {
            if (pendente > maxPendenteParaChave) { descartados++; return Decisao.Descartar(pedirChave = true) }
            ateChave = false
            return Decisao.Enviar
        }
        if (ateChave) { descartados++; return Decisao.Descartar(pedirChave = false) }
        if (pendente > maxPendente) { ateChave = true; descartados++; return Decisao.Descartar(pedirChave = true) }
        return Decisao.Enviar
    }
}

/** Do lado de quem assiste: ignora tudo até o primeiro quadro-chave e volta a esperar um depois de qualquer falha de decodificação. */
class PoliticaDeDecodificacao {
    private var esperandoChave = true
    @Synchronized fun aceitar(chave: Boolean): Boolean {
        if (esperandoChave && !chave) return false
        if (chave) esperandoChave = false
        return true
    }
    @Synchronized fun exigirChave() { esperandoChave = true }
}
