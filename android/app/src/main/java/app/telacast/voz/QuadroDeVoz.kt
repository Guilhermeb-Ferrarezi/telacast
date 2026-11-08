package app.telacast.voz

import app.telacast.proto.VOICE_FRAME

/** Um quadro de áudio vindo do servidor: `[1][remetente i64 BE][seq u32 BE][opus]`. */
class QuadroRecebido(val remetente: Long, val seq: Int, val opus: ByteArray) {
    override fun equals(other: Any?) = other is QuadroRecebido && remetente == other.remetente && seq == other.seq && opus.contentEquals(other.opus)
    override fun hashCode() = (remetente.hashCode() * 31 + seq) * 31 + opus.contentHashCode()
}

/** Formato dos quadros binários de voz no WebSocket do gateway (`docs/VOICE.md`, `docs/PROTOCOL.md` seção 6). */
object QuadroDeVoz {
    const val CABECALHO_ENVIO = 1 + 4
    const val CABECALHO_RECEBIDO = 1 + 8 + 4

    /** Cliente para servidor: `[1][seq u32 BE][opus]`. */
    fun codificarEnvio(seq: Int, opus: ByteArray): ByteArray {
        val b = ByteArray(CABECALHO_ENVIO + opus.size)
        b[0] = VOICE_FRAME.toByte()
        escreverInt(b, 1, seq)
        System.arraycopy(opus, 0, b, CABECALHO_ENVIO, opus.size)
        return b
    }

    /**
     * Lê um quadro do servidor. `null` se não for áudio (outro primeiro byte), for curto demais, ou o pacote for vazio
     * ou maior que [MAX_PACOTE]: nesse caso o quadro é simplesmente descartado.
     */
    fun decodificarRecebido(b: ByteArray): QuadroRecebido? {
        if (b.size <= CABECALHO_RECEBIDO || b[0].toInt() != VOICE_FRAME) return null
        val tamanho = b.size - CABECALHO_RECEBIDO
        if (tamanho > MAX_PACOTE) return null
        var remetente = 0L
        for (i in 1..8) remetente = (remetente shl 8) or (b[i].toLong() and 0xFF)
        return QuadroRecebido(remetente, lerInt(b, 9), b.copyOfRange(CABECALHO_RECEBIDO, b.size))
    }

    /** Servidor para clientes (só para testes e para o relé): `[1][remetente i64 BE][seq u32 BE][opus]`. */
    fun codificarRecebido(remetente: Long, seq: Int, opus: ByteArray): ByteArray {
        val b = ByteArray(CABECALHO_RECEBIDO + opus.size)
        b[0] = VOICE_FRAME.toByte()
        for (i in 0 until 8) b[1 + i] = (remetente shr (56 - 8 * i)).toByte()
        escreverInt(b, 9, seq)
        System.arraycopy(opus, 0, b, CABECALHO_RECEBIDO, opus.size)
        return b
    }

    private fun escreverInt(b: ByteArray, p: Int, v: Int) {
        b[p] = (v ushr 24).toByte(); b[p + 1] = (v ushr 16).toByte(); b[p + 2] = (v ushr 8).toByte(); b[p + 3] = v.toByte()
    }

    private fun lerInt(b: ByteArray, p: Int): Int =
        ((b[p].toInt() and 0xFF) shl 24) or ((b[p + 1].toInt() and 0xFF) shl 16) or ((b[p + 2].toInt() and 0xFF) shl 8) or (b[p + 3].toInt() and 0xFF)
}
