package app.telacast.voz

/** Resultado de pedir um quadro de 20 ms ao jitter buffer. */
sealed interface Reproducao {
    /** Pacote Opus a decodificar. */
    class Quadro(val dados: ByteArray) : Reproducao
    /** Pacote perdido ou atrasado: o decodificador deve fazer PLC. */
    data object Perdido : Reproducao
    /** Nada a tocar (usuário calado ou ainda enchendo o buffer). */
    data object Silencio : Reproducao
}

enum class ResultadoDoPush { Aceito, Duplicado, Atrasado, Ressincronizou }

/**
 * Jitter buffer de um usuário: porte fiel do `JitterBuffer` de `app/src/voice.rs`. Orientado a "ticks": cada
 * [tocar] é um quadro de 20 ms de reprodução, sem relógio, então é determinístico nos testes.
 *
 * - Ordena por `seq` (aritmética de 32 bits com virada), descarta duplicados e pacotes que chegaram depois do seu
 *   quadro já ter passado.
 * - Começa a tocar quando há `alvo` quadros; o alvo (2 a 5 quadros = 40 a 100 ms) sobe com subfluxo ou atraso e
 *   desce depois de 5 s sem problemas.
 * - Perda no meio da fala vira [Reproducao.Perdido] (PLC); buffer esvaziado vira 2 quadros de PLC e depois silêncio.
 */
class JitterBuffer(alvoInicial: Int = ALVO_INICIAL) {
    private val pacotes = ArrayList<Pair<Int, ByteArray>>(MAX_PACOTES + 1)
    private var proximo = 0
    private var iniciado = false
    var tocando = false
        private set
    private var virgem = true
    var alvo: Int = alvoInicial.coerceIn(MIN, MAX)
        private set
    private var espera = 0
    private var vazios = 0
    private var ocioso = Int.MAX_VALUE / 2
    private var atrasadosSeguidos = 0
    private var calma = 0
    private var descartes = 0

    /** Pacotes descartados desde a última chamada. */
    fun descartados(): Int { val d = descartes; descartes = 0; return d }

    /** Quadros de áudio à frente do ponto de reprodução. */
    fun profundidade(): Int = pacotes.maxOfOrNull { it.first - proximo }?.let { (maxOf(it, -1) + 1) } ?: 0

    private fun recomeca(seq: Int, dados: ByteArray) {
        pacotes.clear(); proximo = seq; iniciado = true; tocando = false; virgem = true
        espera = 0; vazios = 0; atrasadosSeguidos = 0
        pacotes.add(seq to dados)
    }

    private fun sobeAlvo() { alvo = minOf(alvo + 1, MAX); calma = 0 }

    fun entregar(seq: Int, dados: ByteArray): ResultadoDoPush {
        if (!iniciado) { recomeca(seq, dados); return ResultadoDoPush.Aceito }
        val dif = seq - proximo
        if (dif < -RESYNC || dif > RESYNC) { recomeca(seq, dados); return ResultadoDoPush.Ressincronizou }
        if (dif < 0) {
            if (virgem && dif >= -RETROCESSO_INICIAL) {
                proximo = seq // chegou fora de ordem antes de qualquer reprodução: recua o ponto de partida
            } else {
                atrasadosSeguidos++; descartes++
                if (atrasadosSeguidos >= ATRASADOS_RESYNC) { recomeca(seq, dados); return ResultadoDoPush.Ressincronizou }
                if (tocando) sobeAlvo()
                return ResultadoDoPush.Atrasado
            }
        }
        atrasadosSeguidos = 0
        if (pacotes.any { it.first == seq }) { descartes++; return ResultadoDoPush.Duplicado }
        pacotes.add(seq to dados)
        while (pacotes.size > MAX_PACOTES) pulaUm()
        return ResultadoDoPush.Aceito
    }

    /** Descarta o quadro do ponto de reprodução (existindo ou não) e avança. */
    private fun pulaUm() {
        val p = proximo
        val i = pacotes.indexOfFirst { it.first == p }
        if (i >= 0) { pacotes.removeAt(i); descartes++ }
        else {
            val j = pacotes.indexOfFirst { it.first - p < 0 }
            if (j >= 0) pacotes.removeAt(j)
        }
        proximo = p + 1
    }

    /** Próximo quadro de 20 ms de reprodução. */
    fun tocar(): Reproducao {
        if (!iniciado) return Reproducao.Silencio
        if (!tocando) {
            val d = pacotes.size
            if (d == 0) { ocioso = minOf(ocioso + 1, Int.MAX_VALUE); return Reproducao.Silencio }
            espera++
            if (d >= alvo || espera > MAX) {
                tocando = true; espera = 0
                if (ocioso <= SUBFLUXO_JANELA) sobeAlvo() // voltou logo depois de esvaziar: foi subfluxo, não pausa de fala
                ocioso = 0
            } else {
                ocioso = minOf(ocioso + 1, Int.MAX_VALUE)
                return Reproducao.Silencio
            }
        }
        // recupera latência acumulada (rajada, relógio do remetente mais rápido)
        while (pacotes.size > alvo + EXCESSO) pulaUm()

        val p = proximo
        val i = pacotes.indexOfFirst { it.first == p }
        if (i >= 0) {
            val dados = pacotes.removeAt(i).second
            proximo = p + 1
            virgem = false
            if (vazios > 0) { vazios = 0; sobeAlvo() }
            calma++
            if (calma >= CALMA && alvo > MIN) { alvo--; calma = 0 }
            return Reproducao.Quadro(dados)
        }
        if (pacotes.isEmpty()) {
            vazios++
            if (vazios > PLC_QUADROS) { tocando = false; vazios = 0; espera = 0; ocioso = 1; return Reproducao.Silencio }
            return Reproducao.Perdido
        }
        // há pacotes mais novos mas o do ponto de reprodução não veio: perdido
        vazios = 0; calma = 0
        val menor = pacotes.map { it.first - p }.filter { it > 0 }.minOrNull() ?: 1
        if (menor > BURACO_MAX) proximo = p + (menor - 1)
        proximo += 1
        return Reproducao.Perdido
    }

    companion object {
        const val MIN = 2
        const val MAX = 5
        const val ALVO_INICIAL = 3
        const val MAX_PACOTES = 32
        const val RESYNC = 200
        const val ATRASADOS_RESYNC = 8
        const val BURACO_MAX = 4
        const val EXCESSO = 4
        const val PLC_QUADROS = 2
        const val CALMA = 250
        const val SUBFLUXO_JANELA = 12
        const val RETROCESSO_INICIAL = 8
    }
}
