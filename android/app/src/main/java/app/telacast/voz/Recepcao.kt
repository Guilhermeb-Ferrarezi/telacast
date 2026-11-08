package app.telacast.voz

import kotlin.math.max
import kotlin.math.min

/** Contadores de um usuário remoto (para o painel de estatísticas da voz). */
class EstatisticasDoFluxo {
    @Volatile var pacotes = 0; @Volatile var perdidos = 0; @Volatile var descartados = 0
    @Volatile var esticados = 0; @Volatile var comprimidos = 0
    @Volatile var jitterMs = 0f
    /** Instantes (ms) dos cortes audíveis (PLC, esvaziar o buffer) nos últimos 60 s. */
    val cortes = ArrayDeque<Long>()
}

/**
 * Um usuário remoto: jitter buffer + decodificador Opus + time-stretch + suavização das bordas. O áudio decodificado entra numa fila
 * e cada tick de 20 ms tira [QUADRO] amostras dela; quando o buffer de rede está cheio demais o quadro sai ACELERADO (consome um
 * pouco mais da fila, puxando um pacote a mais de vez em quando) e quando está vazio demais sai DESACELERADO, só na fala e até
 * ±20% ([Esticador]); assim o atraso volta ao alvo sem pular nem repetir pedaço de palavra.
 */
class FluxoDeUsuario(private val decodificador: DecodificadorOpus, alvoInicial: Int = JitterBuffer.ALVO_INICIAL, private val relogio: () -> Long = System::currentTimeMillis) {
    val jitter = JitterBuffer(alvoInicial)
    val estat = EstatisticasDoFluxo()
    private val borda = SuavizadorDeBordas()
    private val pcm = FloatArray(QUADRO)
    private var fila = FloatArray(QUADRO * 4)
    private var tam = 0
    private var tocouAntes = false
    /** Desligar o time-stretch (os testes comparam com o comportamento sem ele). */
    @Volatile var esticar = true

    fun entregar(seq: Int, dados: ByteArray) = jitter.entregar(seq, dados)

    private fun empurrar(pacote: ByteArray?) {
        val s = decodificador.decodificar(pacote) ?: ShortArray(QUADRO)
        if (tam + QUADRO > fila.size) fila = fila.copyOf(fila.size * 2)
        for (i in 0 until QUADRO) { val x = s[i] / 32768f; fila[tam + i] = if (x.isFinite()) x else 0f }
        tam += QUADRO
    }

    private fun corte() { val agora = relogio(); estat.cortes.addLast(agora); while (estat.cortes.isNotEmpty() && agora - estat.cortes.first() > 60_000) estat.cortes.removeFirst() }

    /** Quanto acelerar (> 0) ou desacelerar (< 0) neste quadro, em amostras. */
    private fun pedirDelta(): Int {
        if (!esticar || !jitter.tocando) return 0
        if (tam >= QUADRO && Esticador.nivelDb(fila, QUADRO) < -52f) return 0 // silêncio: o buffer se ajusta sozinho
        val fundo = jitter.ocupacao() + tam / QUADRO
        val alvo = jitter.alvo
        return when {
            jitter.ocupacao() >= 2 && fundo >= alvo + 2 -> min(Esticador.MAX_DELTA, 48 * (fundo - alvo - 1))
            tam >= QUADRO && fundo <= alvo - 2 -> -min(Esticador.MAX_DELTA, 48 * (alvo - 1 - fundo))
            else -> 0
        }
    }

    /** Os próximos 20 ms em `saida` (960 amostras, soma ao que já estiver lá com `volume`). `true` se havia áudio. */
    fun misturarEm(saida: FloatArray, volume: Float): Boolean {
        val delta = pedirDelta()
        val precisa = max(QUADRO, Esticador.precisa(delta))
        var semMais = false
        while (tam < precisa && !semMais) {
            when (val r = jitter.tocar()) {
                Reproducao.Silencio -> semMais = true
                is Reproducao.Quadro -> { empurrar(r.dados); estat.pacotes++ }
                Reproducao.Perdido -> { empurrar(null); estat.perdidos++; corte() }
            }
        }
        if (tam < QUADRO) {
            if (tocouAntes) corte()
            tocouAntes = false; tam = 0
            return somar(saida, volume, borda.silencio(pcm))
        }
        tocouAntes = true
        // só se há fala de verdade no quadro (a decisão acima podia vir de uma fila ainda vazia)
        val d = if (tam >= precisa && Esticador.nivelDb(fila, QUADRO) >= -52f) delta else 0
        val r = Esticador.produzir(fila, tam, d, pcm)
        if (r.usado > 0) estat.comprimidos++ else if (r.usado < 0) estat.esticados++
        System.arraycopy(fila, r.consumidas, fila, 0, tam - r.consumidas); tam -= r.consumidas
        borda.audio(pcm)
        estat.descartados += jitter.descartados()
        return somar(saida, volume, true)
    }

    private fun somar(saida: FloatArray, volume: Float, tem: Boolean): Boolean {
        if (tem) for (i in 0 until QUADRO) saida[i] += pcm[i] * volume
        return tem
    }

    val tocando: Boolean get() = jitter.tocando || borda.ativo || tam > 0

    /** Atraso total deste usuário em quadros: o que está no buffer de rede mais o que já foi decodificado e espera. */
    fun profundidadeEmQuadros(): Float = jitter.ocupacao() + tam.toFloat() / QUADRO
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
    private class Entrada(val fluxo: FluxoDeUsuario, var ultimoPacote: Long, var ultimoSeq: Int = 0, var temSeq: Boolean = false)

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
            e = Entrada(FluxoDeUsuario(fabrica.decodificador(), alvoInicial, relogio), relogio())
            usuarios[q.remetente] = e
        }
        val agora = relogio()
        // jitter de chegada (RFC 3550): desvio entre o intervalo de chegada e o que o `seq` prevê (20 ms por pacote)
        val ds = q.seq - e.ultimoSeq
        if (e.temSeq && ds in 1..50) {
            val d = Math.abs((agora - e.ultimoPacote) - 20L * ds).toFloat()
            e.fluxo.estat.jitterMs += (d - e.fluxo.estat.jitterMs) / 16f
        }
        e.ultimoSeq = q.seq; e.temSeq = true
        e.ultimoPacote = agora
        e.fluxo.entregar(q.seq, q.opus)
    }

    @Synchronized fun remover(usuario: Long) { usuarios.remove(usuario) }

    /** Resumo da recepção (todos os usuários) para o painel de estatísticas. */
    @Synchronized fun resumo(): ResumoDaRecepcao {
        var pacotes = 0; var perdidos = 0; var jitter = 0f; var alvo = 0; var buffer = 0f; var cortes = 0; var esticados = 0; var comprimidos = 0; var n = 0
        val agora = relogio()
        for (e in usuarios.values) {
            val s = e.fluxo.estat
            pacotes += s.pacotes; perdidos += s.perdidos + s.descartados; jitter = maxOf(jitter, s.jitterMs)
            alvo = maxOf(alvo, e.fluxo.jitter.alvo); buffer = maxOf(buffer, e.fluxo.profundidadeEmQuadros())
            cortes += s.cortes.count { agora - it <= 60_000 }; esticados += s.esticados; comprimidos += s.comprimidos; n++
        }
        val total = pacotes + perdidos
        return ResumoDaRecepcao(n, if (total > 0) 100f * perdidos / total else 0f, jitter, alvo * 20, buffer * 20f, cortes, esticados, comprimidos)
    }

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

/** Estatísticas agregadas da recepção: perda em %, jitter de chegada (ms), buffer alvo e atual (ms), cortes audíveis no último minuto e quantas vezes o time-stretch agiu. */
data class ResumoDaRecepcao(
    val usuarios: Int = 0, val perdaPct: Float = 0f, val jitterMs: Float = 0f, val alvoMs: Int = 0, val bufferMs: Float = 0f,
    val cortesPorMin: Int = 0, val esticados: Int = 0, val comprimidos: Int = 0,
)
