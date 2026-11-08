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
    fabricaDeSupressor: FabricaDeSupressor? = null,
) : AutoCloseable {
    val vad = Vad()
    /** AGC por software, depois do VAD (os limiares não dependem do ganho acumulado). */
    val agc = Agc()
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

    // Supressão de ruído (RNNoise), depois do ganho e antes do VAD, como no desktop. `null` = biblioteca ausente.
    private val supressor: SupressorDeRuido? = fabricaDeSupressor?.criar()
    /** Há RNNoise de verdade neste aparelho (o dispositivo desliga o NoiseSuppressor do sistema para não aplicar dois). */
    val supressorProprio: Boolean get() = supressor != null
    /** Opção "Supressão de ruído" ligada. Sem [supressorProprio] não faz nada aqui. */
    @Volatile var supressao = true
    private var supressaoAnterior = true // o supressor rodou no quadro anterior (crossfade ao alternar)
    private var processou = false
    private val entradaSup = FloatArray(QUADRO_DO_SUPRESSOR)
    private val saidaSup = FloatArray(QUADRO_DO_SUPRESSOR)
    private val limpo = ShortArray(QUADRO)
    /** Probabilidade de voz do RNNoise no último quadro (0 com a supressão desligada). */
    @Volatile var probDeVoz = 0f
        private set

    /** Esquece o estado (ao ficar mudo): fecha o VAD, zera o medidor e a pré-rolagem. */
    fun reiniciar() {
        vad.reiniciar(); agc.reiniciar(); medidor.reiniciar(); preroll.clear(); nivel = 0f; falando = false
        if (supressao) supressor?.reiniciar()
        supressaoAnterior = supressao; probDeVoz = 0f
    }

    override fun close() { supressor?.close() }

    /**
     * Roda o RNNoise (2 x 480) sobre [buf] e deixa o resultado em [limpo]; devolve a probabilidade de voz (média das duas
     * metades). Os valores já são "de 16 bits", que é a escala do RNNoise.
     */
    private fun suprimir(s: SupressorDeRuido): Float {
        var prob = 0f
        for (h in 0 until 2) {
            val ini = h * QUADRO_DO_SUPRESSOR
            for (i in 0 until QUADRO_DO_SUPRESSOR) entradaSup[i] = buf[ini + i].toFloat()
            val p = s.processar(entradaSup, saidaSup)
            prob += if (p.isFinite() && p >= 0f) p else 0f
            for (i in 0 until QUADRO_DO_SUPRESSOR) limpo[ini + i] = saidaSup[i].toInt().coerceIn(-32768, 32767).toShort()
        }
        return prob / 2f
    }

    /** Processa um quadro de [QUADRO] amostras mono a 48 kHz; chama `enviar(seq, pacote)` quando for o caso. */
    fun processar(quadro: ShortArray, enviar: (Int, ByteArray) -> Unit) {
        if (quadro.size != QUADRO) return
        val g = if (ganho.isFinite()) ganho.coerceIn(0f, 8f) else 1f
        for (i in 0 until QUADRO) buf[i] = (quadro[i] * g).toInt().coerceIn(-32768, 32767).toShort()
        // Supressão de ruído; ao alternar, crossfade linear de um quadro entre o sinal cru e o limpo (sem estalo)
        var prob: Float? = null
        val sup = supressor
        val ligada = supressao && sup != null
        if (sup != null && (supressao || supressaoAnterior)) {
            if (supressao && !supressaoAnterior && processou) sup.reiniciar()
            val p = suprimir(sup)
            val n = QUADRO.toFloat()
            when {
                supressao && supressaoAnterior -> limpo.copyInto(buf)
                supressao -> for (i in 0 until QUADRO) { val t = (i + 1) / n; buf[i] = (buf[i] * (1f - t) + limpo[i] * t).toInt().toShort() }
                else -> for (i in 0 until QUADRO) { val t = (i + 1) / n; buf[i] = (limpo[i] * (1f - t) + buf[i] * t).toInt().toShort() }
            }
            if (supressao) prob = p
            probDeVoz = if (supressao) p else 0f
        }
        supressaoAnterior = ligada
        processou = true
        // o VAD decide sobre o nível pós-RNNoise e ANTES do AGC: os limiares não dependem do ganho acumulado
        val db = rmsDb(buf)
        val estava = vad.aberto
        val aberto = vad.processarDbProb(db, prob) || forcarAberto
        nivel = medidor.atualizar(db)
        falando = aberto
        agc.processar(buf, db, vad.evidencia)
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
