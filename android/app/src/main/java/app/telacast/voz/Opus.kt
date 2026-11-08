package app.telacast.voz

import io.github.jaredmdobson.concentus.OpusApplication
import io.github.jaredmdobson.concentus.OpusBandwidth
import io.github.jaredmdobson.concentus.OpusDecoder
import io.github.jaredmdobson.concentus.OpusEncoder
import io.github.jaredmdobson.concentus.OpusSignal

/** Formato do áudio do Telacast: Opus mono, 48 kHz, quadros de 20 ms (iguais ao desktop, `app/src/voice.rs`). */
const val TAXA_DE_AMOSTRAGEM = 48_000
/** Amostras por quadro de 20 ms. */
const val QUADRO = 960
/** Maior pacote Opus aceito pelo servidor (`VOICE_MAX_PACKET`). */
const val MAX_PACOTE = 400
/** 48 kbps (VBR restrito, banda total): ~120 bytes por pacote em média e nunca mais que [MAX_PACOTE] (o servidor descarta quadros maiores). */
const val BITRATE = 48_000

/** Codifica um quadro de 20 ms (960 amostras de 16 bits) em um pacote Opus. */
interface CodificadorOpus {
    /** `null` se falhar (o quadro é descartado). */
    fun codificar(pcm: ShortArray): ByteArray?
}

/** Decodifica um pacote Opus em 960 amostras. `pacote == null` pede PLC (esconde a perda). */
interface DecodificadorOpus {
    fun decodificar(pacote: ByteArray?): ShortArray?
}

/** Fábrica de codecs. Em produção é o [ConcentusOpus]; os testes de lógica usam codecs de mentira. */
interface FabricaDeOpus {
    fun codificador(): CodificadorOpus
    fun decodificador(): DecodificadorOpus
}

/**
 * Opus em Java puro (Concentus): é o MESMO código na JVM e no aparelho, então os testes de verdade rodam no PC.
 * Escolhido em vez de `MediaCodec` ("audio/opus") porque o codificador do sistema só existe do Android 10 em diante,
 * muda de um fabricante para outro (o aparelho de teste roda um port) e não dá para testar na JVM; e em vez da
 * libopus nativa porque dispensa NDK. O custo é CPU: complexidade 5 e um só codificador por chamada.
 */
class ConcentusOpus(private val complexidade: Int = 5) : FabricaDeOpus {
    override fun codificador(): CodificadorOpus {
        val enc = OpusEncoder(TAXA_DE_AMOSTRAGEM, 1, OpusApplication.OPUS_APPLICATION_VOIP)
        enc.bitrate = BITRATE
        enc.useVBR = true
        enc.useConstrainedVBR = true
        enc.signalType = OpusSignal.OPUS_SIGNAL_VOICE
        enc.bandwidth = OpusBandwidth.OPUS_BANDWIDTH_FULLBAND // banda total (até 20 kHz): voz com ar, em vez de abafada
        enc.maxBandwidth = OpusBandwidth.OPUS_BANDWIDTH_FULLBAND
        enc.useInbandFEC = false
        enc.useDTX = false
        enc.complexity = complexidade
        val saida = ByteArray(MAX_PACOTE)
        return object : CodificadorOpus {
            override fun codificar(pcm: ShortArray): ByteArray? = try {
                val n = enc.encode(pcm, 0, QUADRO, saida, 0, saida.size)
                if (n > 0) saida.copyOf(n) else null
            } catch (_: Exception) { null }
        }
    }

    override fun decodificador(): DecodificadorOpus {
        val dec = OpusDecoder(TAXA_DE_AMOSTRAGEM, 1)
        return object : DecodificadorOpus {
            override fun decodificar(pacote: ByteArray?): ShortArray? = try {
                val saida = ShortArray(QUADRO)
                val n = if (pacote == null) dec.decode(null, 0, 0, saida, 0, QUADRO, false) else dec.decode(pacote, 0, pacote.size, saida, 0, QUADRO, false)
                if (n < QUADRO) saida.fill(0, maxOf(n, 0), QUADRO)
                saida
            } catch (_: Exception) { null }
        }
    }
}
