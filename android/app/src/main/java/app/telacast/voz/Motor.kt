package app.telacast.voz

/**
 * O motor de voz sem hardware: junta o envio (captura → VAD → Opus → quadros) e a recepção (quadros → jitter buffer →
 * Opus → mistura). O dispositivo (AudioRecord/AudioTrack) só precisa chamar [capturar] a cada 20 ms de microfone e
 * [reproduzir] a cada 20 ms de alto-falante; por isso tudo aqui roda e é testado na JVM.
 *
 * @param enviar recebe o quadro binário pronto (`[1][seq][opus]`) para mandar ao gateway; não pode bloquear.
 */
class MotorDeVoz(
    fabrica: FabricaDeOpus,
    private val enviar: (ByteArray) -> Unit,
    relogio: () -> Long = System::currentTimeMillis,
    alvoDoJitter: Int = JitterBuffer.ALVO_INICIAL,
    supressor: FabricaDeSupressor? = null,
) : AutoCloseable {
    private val envio = PipelineDeEnvio(fabrica.codificador(), fabricaDeSupressor = supressor)
    private val recepcao = RecepcaoDeVoz(fabrica, relogio, alvoDoJitter)
    @Volatile private var mudo = false
    @Volatile private var surdo = false

    /** Quadros que o servidor descartou ou que não eram áudio. */
    @Volatile var quadrosIgnorados = 0
        private set
    @Volatile var pacotesEnviados = 0
        private set

    /** Mudo: para de enviar (e esquece o VAD). */
    fun definirMudo(v: Boolean) { mudo = v; if (v) envio.reiniciar() }
    fun definirSurdo(v: Boolean) { surdo = v }
    fun definirSensibilidade(s: Float) { envio.vad.sensibilidade = s }
    fun definirGanho(g: Float) { envio.ganho = g }
    fun definirAgc(ligado: Boolean) { envio.agc.ligado = ligado }
    /** Opção "Supressão de ruído": liga o RNNoise (se a biblioteca nativa existe). */
    fun definirSupressao(ligado: Boolean) { envio.supressao = ligado }
    /** Há RNNoise próprio (então o NoiseSuppressor do sistema deve ficar desligado). */
    val supressorProprio: Boolean get() = envio.supressorProprio
    /** Probabilidade de voz do RNNoise no último quadro. */
    val probDeVoz: Float get() = envio.probDeVoz
    /** Libera o RNNoise nativo. Chame depois de parar a captura. */
    override fun close() { envio.close() }
    fun definirVolume(usuario: Long, volume: Float) = recepcao.definirVolume(usuario, volume)
    fun volume(usuario: Long) = recepcao.volume(usuario)

    /** Chamada pela thread do microfone com 20 ms de áudio (960 amostras mono a 48 kHz). */
    fun capturar(quadro: ShortArray) {
        if (mudo) return
        envio.processar(quadro) { seq, pacote -> pacotesEnviados++; enviar(QuadroDeVoz.codificarEnvio(seq, pacote)) }
    }

    /** Chamada pela thread de rede com um quadro binário do gateway. */
    fun receberQuadro(bytes: ByteArray) {
        val q = QuadroDeVoz.decodificarRecebido(bytes)
        if (q == null) { quadrosIgnorados++; return }
        recepcao.entregar(q)
    }

    /** Chamada pela thread do alto-falante: gera 20 ms misturados em `saida`. Surdo toca silêncio (e descarta o que chegou). */
    fun reproduzir(saida: ShortArray) {
        if (surdo) { recepcao.reproduzir(saida); saida.fill(0); return }
        recepcao.reproduzir(saida)
    }

    fun removerUsuario(usuario: Long) = recepcao.remover(usuario)
    fun falando(usuario: Long) = recepcao.falando(usuario)
    fun usuariosFalando(): Set<Long> = recepcao.usuariosAtivos()
    /** O VAD está aberto agora (eu estou falando). */
    val euFalando: Boolean get() = !mudo && envio.falando
    /** Nível do microfone 0..1 (medidor). */
    val nivelDoMicrofone: Float get() = if (mudo) 0f else envio.nivel
}
