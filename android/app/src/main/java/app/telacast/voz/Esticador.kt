package app.telacast.voz

import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min
import kotlin.math.sqrt

/**
 * Time-stretch por emenda alinhada (a ideia do WSOLA, uma emenda por quadro de 20 ms): em vez de repetir um quadro (som de robô)
 * ou pular um (corte), o áudio é ACELERADO (consome mais entrada do que produz) ou DESACELERADO (menos) em até ±20%, repetindo ou
 * tirando uns poucos milissegundos escolhidos onde a onda se parece com ela mesma (correlação), com um fade cruzado curto. Só deve
 * ser usado na fala; no silêncio o buffer se ajusta sozinho.
 *
 * Puro e sem estado: `entrada` tem as amostras decodificadas à frente, `saida` recebe [QUADRO] amostras e o resultado diz quantas amostras
 * de `entrada` foram consumidas.
 */
object Esticador {
    /** Janela do fade cruzado (3,3 ms). */
    const val JANELA = 160
    /** Ponto de emenda dentro do quadro. */
    const val PONTO = 480
    /** Mudança máxima por quadro: 20% de 960. */
    const val MAX_DELTA = 192
    const val MIN_DELTA = 32
    private const val BUSCA = 64

    /** Amostras de entrada de que `produzir` precisa para um `delta` pedido (positivo: acelera). */
    fun precisa(delta: Int): Int = QUADRO + if (delta >= MIN_DELTA) min(MAX_DELTA, delta + BUSCA) else 0

    /**
     * Gera [QUADRO] amostras de `saida` a partir de `entrada[0 until n]`. `delta` > 0 acelera (consome QUADRO + d), < 0 desacelera
     * (consome QUADRO - d), 0 copia. Devolve `consumidas` e o `d` realmente usado (a correlação escolhe perto do pedido).
     */
    fun produzir(entrada: FloatArray, n: Int, delta: Int, saida: FloatArray): Resultado {
        val alvo = abs(delta.coerceIn(-MAX_DELTA, MAX_DELTA))
        if (alvo < MIN_DELTA || n < QUADRO) { System.arraycopy(entrada, 0, saida, 0, QUADRO); return Resultado(QUADRO, 0) }
        val acelera = delta > 0
        val de = max(MIN_DELTA, alvo - BUSCA); val ate = min(MAX_DELTA, alvo + BUSCA)
        if (acelera && n < QUADRO + ate) { System.arraycopy(entrada, 0, saida, 0, QUADRO); return Resultado(QUADRO, 0) }
        var melhor = alvo; var melhorPontos = -2.0
        for (d in de..ate) {
            val b = if (acelera) PONTO + d else PONTO - d
            var dot = 0.0; var ea = 1e-9; var eb = 1e-9
            for (i in 0 until JANELA) { val x = entrada[PONTO + i].toDouble(); val y = entrada[b + i].toDouble(); dot += x * y; ea += x * x; eb += y * y }
            val pontos = dot / sqrt(ea * eb) - 0.0005 * abs(d - alvo) // desempata a favor do pedido
            if (pontos > melhorPontos) { melhorPontos = pontos; melhor = d }
        }
        val d = melhor
        System.arraycopy(entrada, 0, saida, 0, PONTO)
        if (acelera) {
            for (i in 0 until JANELA) { val t = (i + 1f) / (JANELA + 1); saida[PONTO + i] = entrada[PONTO + i] * (1 - t) + entrada[PONTO + d + i] * t }
            val resto = QUADRO - PONTO - JANELA
            System.arraycopy(entrada, PONTO + d + JANELA, saida, PONTO + JANELA, resto)
            return Resultado(QUADRO + d, d)
        }
        for (i in 0 until JANELA) { val t = (i + 1f) / (JANELA + 1); saida[PONTO + i] = entrada[PONTO + i] * (1 - t) + entrada[PONTO - d + i] * t }
        val resto = QUADRO - PONTO - JANELA
        System.arraycopy(entrada, PONTO - d + JANELA, saida, PONTO + JANELA, resto)
        return Resultado(QUADRO - d, -d)
    }

    class Resultado(val consumidas: Int, val usado: Int)

    /** Energia do trecho em dBFS (para decidir se há fala). */
    fun nivelDb(x: FloatArray, n: Int): Float { var s = 0.0; for (i in 0 until n) s += x[i].toDouble() * x[i]; return (10 * kotlin.math.log10(s / max(n, 1) + 1e-12)).toFloat() }
}
