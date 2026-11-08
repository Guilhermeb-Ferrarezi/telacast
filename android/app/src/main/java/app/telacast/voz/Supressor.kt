package app.telacast.voz

/**
 * Supressão de ruído por RNNoise, o mesmo do cliente desktop (`nnnoiseless`, Rust). Trabalha em quadros de 480 amostras
 * (10 ms a 48 kHz), na escala de 16 bits (-32768..32767) como o RNNoise espera, e devolve a probabilidade de voz (0 a 1).
 */
interface SupressorDeRuido : AutoCloseable {
    /** `entrada` e `saida` têm [QUADRO_DO_SUPRESSOR] amostras. Devolve a probabilidade de voz; `-1` se falhou (a saída vale a entrada). */
    fun processar(entrada: FloatArray, saida: FloatArray): Float
    /** Esquece o estado interno (começa de novo, como o desktop faz ao religar ou ficar mudo). */
    fun reiniciar()
}

const val QUADRO_DO_SUPRESSOR = 480

/** Cria supressores; `null` se não dá (biblioteca nativa ausente): o app segue sem RNNoise e usa o do sistema. */
fun interface FabricaDeSupressor {
    fun criar(): SupressorDeRuido?
}

/**
 * Ponte JNI para `libtelacast_rnnoise.so` (código em `android/native/rnnoise`, compilado por `scripts/rnnoise.sh`; a versão
 * arm64 fica no repositório em `jniLibs`). Os nomes das funções nativas dependem do pacote e do nome desta classe.
 */
object RnnoiseNativo {
    @Volatile private var carregada: Boolean? = null

    /** A biblioteca nativa carregou? (No Android: `jniLibs/arm64-v8a`; na JVM dos testes: `-Djava.library.path`.) */
    val disponivel: Boolean get() = carregada ?: carregar()

    @Synchronized private fun carregar(): Boolean {
        carregada?.let { return it }
        val ok = try { System.loadLibrary("telacast_rnnoise"); true } catch (_: UnsatisfiedLinkError) { false } catch (_: SecurityException) { false }
        carregada = ok
        return ok
    }

    @JvmStatic external fun criar(): Long
    @JvmStatic external fun processar(estado: Long, entrada: FloatArray, saida: FloatArray): Float
    @JvmStatic external fun destruir(estado: Long)
}

/** [SupressorDeRuido] sobre [RnnoiseNativo]. Seguro contra uso depois de fechado (o áudio pode parar durante um quadro). */
class SupressorRnnoise private constructor(private var estado: Long) : SupressorDeRuido {
    private val lock = Any()

    override fun processar(entrada: FloatArray, saida: FloatArray): Float = synchronized(lock) {
        if (estado == 0L || entrada.size != QUADRO_DO_SUPRESSOR || saida.size != QUADRO_DO_SUPRESSOR) { entrada.copyInto(saida, endIndex = minOf(entrada.size, saida.size)); return -1f }
        val p = RnnoiseNativo.processar(estado, entrada, saida)
        if (p < 0f) entrada.copyInto(saida)
        p
    }

    override fun reiniciar() = synchronized(lock) {
        if (estado == 0L) return
        val novo = RnnoiseNativo.criar()
        if (novo != 0L) { RnnoiseNativo.destruir(estado); estado = novo }
    }

    override fun close() = synchronized(lock) {
        if (estado != 0L) { RnnoiseNativo.destruir(estado); estado = 0L }
    }

    companion object : FabricaDeSupressor {
        /**
         * Mede o custo de um quadro de 10 ms (µs) com ruído sintético: vai para o logcat ao entrar na voz, para saber no
         * aparelho quanto da CPU o RNNoise usa (precisa ficar muito abaixo dos 10 ms). `null` sem a biblioteca.
         */
        fun medirCustoPorQuadroUs(quadros: Int = 300): Double? {
            val s = criar() ?: return null
            return s.use {
                val e = FloatArray(QUADRO_DO_SUPRESSOR) { i -> (((i * 7919) % 2000) - 1000).toFloat() }
                val o = FloatArray(QUADRO_DO_SUPRESSOR)
                repeat(50) { s.processar(e, o) }
                val t0 = System.nanoTime()
                repeat(quadros) { s.processar(e, o) }
                (System.nanoTime() - t0) / quadros / 1000.0
            }
        }

        override fun criar(): SupressorDeRuido? {
            if (!RnnoiseNativo.disponivel) return null
            val h = try { RnnoiseNativo.criar() } catch (_: UnsatisfiedLinkError) { 0L }
            return if (h == 0L) null else SupressorRnnoise(h)
        }
    }
}
