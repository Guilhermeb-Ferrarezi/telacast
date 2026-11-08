package app.telacast.voz

/**
 * Do quadro de 20 ms capturado ao pacote Opus: ganho, medidor, VAD, codificação e pré-rolagem. Porte de `SendPipeline`
 * (`app/src/voice.rs`): só gera pacotes com o VAD aberto, e ao abrir manda primeiro os [PREROLL] pacotes anteriores
 * para não cortar o começo da fala. O codificador roda em todo quadro (estado sempre aquecido, sem estalo).
 * O `seq` só avança quando um pacote sai (continua contíguo depois de uma pausa) e começa "aleatório": se o motor
 * for recriado, o receptor ressincroniza na hora.
 */
class PipelineDeEnvio(
    private val codificador: CodificadorOpus,
    semente: Int = (System.nanoTime() xor (System.currentTimeMillis() * 2_654_435_761L)).toInt(),
) {
    val vad = Vad()
    /** Ganho linear de entrada (1 = sem mudança). */
    @Volatile var ganho = 1f
    /** Só para testes com hardware. */
    @Volatile var forcarAberto = false
    private val medidor = MedidorDeNivel()
    private val preroll = ArrayDeque<ByteArray>(PREROLL + 1)
    private var seq = semente
    @Volatile var nivel = 0f
        private set
    @Volatile var falando = false
        private set
    private val buf = ShortArray(QUADRO)

    /** Esquece o estado (ao ficar mudo): fecha o VAD, zera o medidor e a pré-rolagem. */
    fun reiniciar() {
        vad.reiniciar(); medidor.reiniciar(); preroll.clear(); nivel = 0f; falando = false
    }

    /** Processa um quadro de [QUADRO] amostras mono a 48 kHz; chama `enviar(seq, pacote)` quando for o caso. */
    fun processar(quadro: ShortArray, enviar: (Int, ByteArray) -> Unit) {
        if (quadro.size != QUADRO) return
        val g = if (ganho.isFinite()) ganho.coerceIn(0f, 8f) else 1f
        for (i in 0 until QUADRO) buf[i] = (quadro[i] * g).toInt().coerceIn(-32768, 32767).toShort()
        val db = rmsDb(buf)
        val estava = vad.aberto
        val aberto = vad.processarDb(db) || forcarAberto
        nivel = medidor.atualizar(db)
        falando = aberto
        val pacote = codificador.codificar(buf) ?: return
        if (aberto) {
            if (!estava) while (preroll.isNotEmpty()) enviar(seq++, preroll.removeFirst())
            enviar(seq++, pacote)
        } else {
            if (preroll.size >= PREROLL) preroll.removeFirst()
            preroll.addLast(pacote)
        }
    }

    companion object { const val PREROLL = 5 }
}
