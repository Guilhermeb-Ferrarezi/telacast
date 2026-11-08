package app.telacast.voz

/** Um usuário remoto: jitter buffer + decodificador Opus + suavização das bordas. */
class FluxoDeUsuario(private val decodificador: DecodificadorOpus, alvoInicial: Int = JitterBuffer.ALVO_INICIAL) {
    val jitter = JitterBuffer(alvoInicial)
    private val borda = SuavizadorDeBordas()
    private val pcm = FloatArray(QUADRO)

    fun entregar(seq: Int, dados: ByteArray) = jitter.entregar(seq, dados)

    /** Os próximos 20 ms em `saida` (960 amostras, soma ao que já estiver lá com `volume`). `true` se havia áudio. */
    fun misturarEm(saida: FloatArray, volume: Float): Boolean {
        val dados = when (val r = jitter.tocar()) {
            Reproducao.Silencio -> return somar(saida, volume, borda.silencio(pcm))
            is Reproducao.Quadro -> r.dados
            Reproducao.Perdido -> null
        }
        val s = decodificador.decodificar(dados) ?: return somar(saida, volume, borda.silencio(pcm))
        for (i in 0 until QUADRO) { val x = s[i] / 32768f; pcm[i] = if (x.isFinite()) x else 0f }
        borda.audio(pcm)
        return somar(saida, volume, true)
    }

    private fun somar(saida: FloatArray, volume: Float, tem: Boolean): Boolean {
        if (tem) for (i in 0 until QUADRO) saida[i] += pcm[i] * volume
        return tem
    }

    val tocando: Boolean get() = jitter.tocando || borda.ativo
}

/**
 * Recepção de todos os usuários: um [FluxoDeUsuario] por remetente, volume individual (0 a 2), soma com limitador suave
 * e a regra de "falando" (mandou pacote nos últimos ~250 ms). Seguro para chamar de várias threads.
 */
class RecepcaoDeVoz(
    private val fabrica: FabricaDeOpus,
    private val relogio: () -> Long,
    private val alvoInicial: Int = JitterBuffer.ALVO_INICIAL,
) {
    private class Entrada(val fluxo: FluxoDeUsuario, var ultimoPacote: Long)

    private val usuarios = LinkedHashMap<Long, Entrada>()
    private val volumes = HashMap<Long, Float>()
    private val acumulado = FloatArray(QUADRO)

    @Synchronized fun definirVolume(usuario: Long, volume: Float) {
        volumes[usuario] = if (volume.isFinite()) volume.coerceIn(0f, 2f) else 1f
    }

    @Synchronized fun volume(usuario: Long): Float = volumes[usuario] ?: 1f

    @Synchronized fun entregar(q: QuadroRecebido) {
        var e = usuarios[q.remetente]
        if (e == null) {
            if (usuarios.size >= MAX_USUARIOS) return
            e = Entrada(FluxoDeUsuario(fabrica.decodificador(), alvoInicial), relogio())
            usuarios[q.remetente] = e
        }
        e.ultimoPacote = relogio()
        e.fluxo.entregar(q.seq, q.opus)
    }

    @Synchronized fun remover(usuario: Long) { usuarios.remove(usuario) }

    @Synchronized fun falando(usuario: Long): Boolean = usuarios[usuario]?.let { relogio() - it.ultimoPacote < JANELA_DE_FALA_MS } == true

    @Synchronized fun usuariosAtivos(): Set<Long> = usuarios.filterValues { relogio() - it.ultimoPacote < JANELA_DE_FALA_MS }.keys.toSet()

    /** Gera 20 ms misturados em `saida` (960 amostras de 16 bits). Devolve `false` se tudo estava em silêncio. */
    @Synchronized fun reproduzir(saida: ShortArray): Boolean {
        acumulado.fill(0f)
        var algum = false
        val agora = relogio()
        val it = usuarios.entries.iterator()
        while (it.hasNext()) {
            val (id, e) = it.next()
            if (e.fluxo.misturarEm(acumulado, volumes[id] ?: 1f)) algum = true
            // sumiu há muito e não toca mais: libera
            if (agora - e.ultimoPacote > OCIOSO_MS && !e.fluxo.tocando) it.remove()
        }
        for (i in 0 until QUADRO) {
            val x = limitadorSuave(acumulado[i])
            saida[i] = (x * 32767f).toInt().coerceIn(-32768, 32767).toShort()
        }
        return algum
    }

    companion object {
        const val JANELA_DE_FALA_MS = 250L
        const val OCIOSO_MS = 5_000L
        const val MAX_USUARIOS = 64
    }
}
