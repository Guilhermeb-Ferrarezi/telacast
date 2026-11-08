package app.telacast.tela

/**
 * Utilidades de H.264 em Annex B (start codes `00 00 01` / `00 00 00 01`), puras e testadas na JVM. O servidor só olha o
 * primeiro byte de cada quadro (a flag de quadro-chave); quem entende o H.264 são o codificador, o decodificador e aqui.
 */
object H264 {
    const val NAL_NAO_IDR = 1
    const val NAL_IDR = 5
    const val NAL_SEI = 6
    const val NAL_SPS = 7
    const val NAL_PPS = 8
    const val NAL_AUD = 9

    private val START_CODE = byteArrayOf(0, 0, 0, 1)

    /** Um NAL: [inicio, fim) são os bytes do NAL (cabeçalho e dados), sem o start code. */
    class Nal(val inicio: Int, val fim: Int, val tipo: Int)

    fun nals(d: ByteArray): List<Nal> {
        val achados = ArrayList<Nal>()
        var i = 0
        var inicioAtual = -1
        while (i + 2 < d.size) {
            if (d[i] == 0.toByte() && d[i + 1] == 0.toByte() && d[i + 2] == 1.toByte()) {
                if (inicioAtual >= 0) achados += nal(d, inicioAtual, fimSemZeros(d, inicioAtual, i))
                inicioAtual = i + 3
                i += 3
            } else i++
        }
        if (inicioAtual >= 0 && inicioAtual < d.size) achados += nal(d, inicioAtual, d.size)
        return achados
    }

    /** Os zeros finais de um NAL pertencem ao start code de 4 bytes do próximo (`00 00 00 01`). */
    private fun fimSemZeros(d: ByteArray, inicio: Int, fim: Int): Int { var f = fim; while (f > inicio && d[f - 1] == 0.toByte()) f--; return f }
    private fun nal(d: ByteArray, inicio: Int, fim: Int) = Nal(inicio, fim, if (fim > inicio) d[inicio].toInt() and 0x1F else -1)

    /** Tem NAL de quadro IDR (quadro-chave)? */
    fun ehQuadroChave(d: ByteArray): Boolean = nals(d).any { it.tipo == NAL_IDR }

    /** SPS e PPS do começo de um quadro ou do buffer de configuração do codificador (com start codes), ou `null` se faltar um dos dois. */
    fun spsPps(d: ByteArray): ByteArray? {
        val ns = nals(d)
        val sps = ns.firstOrNull { it.tipo == NAL_SPS } ?: return null
        val pps = ns.firstOrNull { it.tipo == NAL_PPS } ?: return null
        return START_CODE + d.copyOfRange(sps.inicio, sps.fim) + START_CODE + d.copyOfRange(pps.inicio, pps.fim)
    }

    /** Garante SPS/PPS antes de um quadro-chave (quem entra no meio da transmissão só consegue decodificar a partir de um). */
    fun comSpsPps(quadro: ByteArray, spsPps: ByteArray?): ByteArray {
        if (spsPps == null) return quadro
        val ns = nals(quadro)
        if (ns.none { it.tipo == NAL_IDR } || ns.any { it.tipo == NAL_SPS }) return quadro
        return spsPps + quadro
    }

    /** Largura e altura (em pixels, já descontado o recorte) do primeiro SPS, ou `null` se não achar ou não entender. */
    fun dimensoes(d: ByteArray): Pair<Int, Int>? {
        val sps = nals(d).firstOrNull { it.tipo == NAL_SPS } ?: return null
        return runCatching { lerSps(semEmulacao(d.copyOfRange(sps.inicio + 1, sps.fim))) }.getOrNull()
    }

    /** Tira os bytes de prevenção de emulação (`00 00 03` -> `00 00`). */
    private fun semEmulacao(d: ByteArray): ByteArray {
        val s = java.io.ByteArrayOutputStream(d.size)
        var zeros = 0
        for (b in d) {
            if (zeros >= 2 && b == 3.toByte()) { zeros = 0; continue }
            s.write(b.toInt())
            zeros = if (b == 0.toByte()) zeros + 1 else 0
        }
        return s.toByteArray()
    }

    private class Bits(val d: ByteArray) {
        var pos = 0
        fun bit(): Int { val b = (d[pos shr 3].toInt() shr (7 - (pos and 7))) and 1; pos++; return b }
        fun bits(n: Int): Int { var v = 0; repeat(n) { v = (v shl 1) or bit() }; return v }
        fun ue(): Int { var z = 0; while (bit() == 0) { z++; if (z > 31) throw IllegalStateException("ue") }; return (1 shl z) - 1 + bits(z) }
        fun se(): Int { val k = ue(); return if (k and 1 == 1) (k + 1) / 2 else -(k / 2) }
    }

    private fun lerSps(d: ByteArray): Pair<Int, Int> {
        val b = Bits(d)
        val perfil = b.bits(8); b.bits(8); b.bits(8) // profile_idc, constraint flags, level_idc
        b.ue() // seq_parameter_set_id
        var croma = 1
        if (perfil in setOf(100, 110, 122, 244, 44, 83, 86, 118, 128, 138, 139, 134, 135)) {
            croma = b.ue()
            if (croma == 3) b.bit()
            b.ue(); b.ue(); b.bit()
            if (b.bit() == 1) repeat(if (croma != 3) 8 else 12) { i ->
                if (b.bit() == 1) { var ultimo = 8; var prox = 8; repeat(if (i < 6) 16 else 64) { if (prox != 0) { prox = (ultimo + b.se() + 256) % 256 }; ultimo = if (prox == 0) ultimo else prox } }
            }
        }
        b.ue() // log2_max_frame_num_minus4
        when (b.ue()) { // pic_order_cnt_type
            0 -> b.ue()
            1 -> { b.bit(); b.se(); b.se(); repeat(b.ue()) { b.se() } }
        }
        b.ue(); b.bit() // max_num_ref_frames, gaps_in_frame_num
        val larguraMb = b.ue() + 1
        val alturaMapa = b.ue() + 1
        val soQuadros = b.bit()
        if (soQuadros == 0) b.bit()
        b.bit() // direct_8x8_inference
        var esq = 0; var dir = 0; var topo = 0; var base = 0
        if (b.bit() == 1) { esq = b.ue(); dir = b.ue(); topo = b.ue(); base = b.ue() }
        val unidadeX = if (croma == 0 || croma == 3) 1 else 2
        val unidadeY = (if (croma == 1) 2 else 1) * (2 - soQuadros)
        return (larguraMb * 16 - (esq + dir) * unidadeX) to ((2 - soQuadros) * alturaMapa * 16 - (topo + base) * unidadeY)
    }
}

/** Quadro de vídeo no fio (`/publish` e `/watch`): `[flag: 1 byte][H.264 Annex B]`, flag 1 = quadro-chave. */
class QuadroDeVideo(val chave: Boolean, val h264: ByteArray) {
    companion object {
        fun codificar(chave: Boolean, h264: ByteArray): ByteArray {
            val s = ByteArray(h264.size + 1)
            s[0] = if (chave) 1 else 0
            System.arraycopy(h264, 0, s, 1, h264.size)
            return s
        }

        /** `null` se tiver menos de 2 bytes (o servidor também descarta). */
        fun decodificar(b: ByteArray): QuadroDeVideo? = if (b.size < 2) null else QuadroDeVideo(b[0] == 1.toByte(), b.copyOfRange(1, b.size))
    }
}
