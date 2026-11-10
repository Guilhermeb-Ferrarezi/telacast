package app.telacast.voz

import app.telacast.data.AppState
import app.telacast.data.Repository
import app.telacast.net.EstadoConexao
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

enum class FaseDaVoz {
    /** Fora de qualquer canal de voz. */
    Fora,
    /** `VoiceJoin` enviado, esperando o servidor confirmar. */
    Entrando,
    /** Conectada: microfone e alto-falante abertos. */
    Conectada,
    /** A conexão com o servidor caiu: o servidor tirou da voz; entra de novo sozinho quando voltar. */
    Reconectando,
}

enum class RotaDeAudio { AltoFalante, Fone }

/** Opções de voz (Ajustes). Os efeitos do Android só valem onde o aparelho os oferece. */
data class OpcoesDeVoz(
    val supressaoDeRuido: Boolean = true,
    val cancelamentoDeEco: Boolean = true,
    val ganhoAutomatico: Boolean = true,
    /** Sensibilidade do VAD, 0 a 1 (padrão 0,7, igual ao desktop). */
    val sensibilidade: Float = Vad.SENSIBILIDADE_PADRAO_ANDROID,
    /** Ganho de entrada linear (0,5 a 4). */
    val ganhoDeEntrada: Float = 1f,
)

/** Painel de estatísticas da voz (atualizado 2 vezes por segundo enquanto a chamada está conectada). */
data class EstatisticasDeVoz(
    /** Perda de pacotes recebidos, em % (PLC e descartes). */
    val perdaPct: Float = 0f,
    /** Jitter de chegada (RFC 3550), em ms. */
    val jitterMs: Float = 0f,
    val alvoMs: Int = 0,
    val bufferMs: Float = 0f,
    /** Cortes audíveis (PLC ou buffer esvaziado) no último minuto. */
    val cortesPorMin: Int = 0,
    /** Vezes que o AudioTrack ficou sem áudio para tocar (underrun). */
    val subfluxos: Int = 0,
    val nivelMic: Float = 0f,
    val ganhoEntradaDb: Float = 0f,
    val agcDb: Float = 0f,
    val probDeVoz: Float = 0f,
    val esticados: Int = 0,
    val comprimidos: Int = 0,
    val pacotesEnviados: Int = 0,
    val usuarios: Int = 0,
)

/** Tudo o que a interface de voz mostra. */
data class EstadoDeVoz(
    val fase: FaseDaVoz = FaseDaVoz.Fora,
    val canal: Long? = null,
    val mudo: Boolean = false,
    val surdo: Boolean = false,
    /** Quem está falando agora (recebeu pacote nos últimos ~250 ms), sem mim. */
    val falando: Set<Long> = emptySet(),
    val euFalando: Boolean = false,
    /** Nível do microfone, 0 a 1. */
    val nivel: Float = 0f,
    /** Último problema (permissão negada, dispositivo falhou, caiu): a tela mostra e some ao entrar de novo. */
    val erro: String? = null,
    /** Instante (ms) em que a chamada ficou conectada, para o cronômetro. */
    val desdeMs: Long? = null,
    val rota: RotaDeAudio = RotaDeAudio.AltoFalante,
    /** Volume individual de quem ouço, 0 a 2 (padrão 1). */
    val volumes: Map<Long, Float> = emptyMap(),
    val opcoes: OpcoesDeVoz = OpcoesDeVoz(),
    val estatisticas: EstatisticasDeVoz = EstatisticasDeVoz(),
) {
    val naChamada: Boolean get() = fase != FaseDaVoz.Fora
    fun volumeDe(usuario: Long): Float = volumes[usuario] ?: 1f
}

/** O que depende do aparelho (AudioRecord, AudioTrack, efeitos, rota). Em produção é o Android; nos testes, um falso. */
interface DispositivoDeAudio {
    /** Abre microfone e alto-falante e começa a chamar `capturar`/`reproduzir` do motor. Devolve a mensagem de erro, se falhar. */
    fun iniciar(): String?
    fun parar()
    fun definirRota(rota: RotaDeAudio)
    fun aplicarOpcoes(opcoes: OpcoesDeVoz)
    /** Quantas vezes o alto-falante ficou sem áudio (`AudioTrack.getUnderrunCount`); 0 se não souber. */
    fun subfluxos(): Int = 0
}

fun interface FabricaDeDispositivo {
    /** `aoFalhar`: o dispositivo quebrou depois de iniciado (fone arrancado, permissão revogada...). */
    fun criar(motor: MotorDeVoz, opcoes: OpcoesDeVoz, rota: RotaDeAudio, aoFalhar: (String) -> Unit): DispositivoDeAudio
}

/** Serviço em primeiro plano: mantém a voz viva com o app em segundo plano e mostra a notificação. */
interface ServicoDeVoz {
    fun iniciar(titulo: String, texto: String)
    fun parar()
}

object ServicoDeVozNulo : ServicoDeVoz {
    override fun iniciar(titulo: String, texto: String) = Unit
    override fun parar() = Unit
}

/**
 * Máquina de estados da voz (entrar, conectada, caiu, reconectando, sair), ligada ao [Repository]. Puro Kotlin: o
 * aparelho entra por [FabricaDeDispositivo] e [ServicoDeVoz], então tudo aqui é testado na JVM.
 *
 * Regras (do desktop, `main.rs`): o motor só existe depois que o servidor confirma a entrada (chega o meu
 * `VoiceUpdate`); ligar o microfone com o áudio desligado liga o áudio também; surdo implica mudo; o mudo e o surdo
 * vão ao servidor (`VoiceSet`). Diferença para o desktop: se a conexão cair, o servidor tira da voz e aqui a entrada
 * é refeita sozinha quando a conexão volta (estado [FaseDaVoz.Reconectando]).
 */
class ControladorDeVoz(
    private val repo: Repository,
    private val escopo: CoroutineScope,
    private val opus: FabricaDeOpus,
    private val dispositivos: FabricaDeDispositivo,
    private val servico: ServicoDeVoz = ServicoDeVozNulo,
    private val relogio: () -> Long = System::currentTimeMillis,
    private val tempoLimiteDaEntradaMs: Long = 8_000,
    private val intervaloDoMedidorMs: Long = 100,
    opcoesIniciais: OpcoesDeVoz = OpcoesDeVoz(),
    rotaInicial: RotaDeAudio = RotaDeAudio.AltoFalante,
    /** RNNoise (no app: [SupressorRnnoise]); `null` ou sem a biblioteca nativa = sem supressor próprio, vale o do sistema. */
    private val supressores: FabricaDeSupressor? = null,
) {
    private val lock = Any()
    private val _estado = MutableStateFlow(EstadoDeVoz(opcoes = opcoesIniciais, rota = rotaInicial))
    val estado: StateFlow<EstadoDeVoz> = _estado.asStateFlow()

    private var motor: MotorDeVoz? = null
    private var dispositivo: DispositivoDeAudio? = null
    private var jobMedidor: Job? = null
    private var jobLimite: Job? = null
    /** Canal que quero estar (entrando, conectada ou reconectando). */
    private var alvo: Long? = null

    init { escopo.launch { repo.estado.collect { synchronized(lock) { aoMudar(it) } } } }

    private inline fun atualizar(f: (EstadoDeVoz) -> EstadoDeVoz) { _estado.value = f(_estado.value) }

    // ------------------------------------------------------------------ comandos

    /** Entra num canal de voz (mover de canal também). A permissão do microfone já deve ter sido concedida. */
    fun entrar(canal: Long) = synchronized(lock) {
        val s = _estado.value
        if (alvo == canal && s.fase != FaseDaVoz.Fora) return
        alvo = canal
        atualizar { it.copy(fase = if (it.fase == FaseDaVoz.Conectada) FaseDaVoz.Conectada else FaseDaVoz.Entrando, canal = it.canal ?: canal, erro = null) }
        if (!repo.entrarNaVoz(canal)) { falhar("Sem conexão com o servidor."); return }
        iniciarLimite(canal)
    }

    /** Sai da chamada (`VoiceLeave`) e fecha o áudio. */
    fun sair() = synchronized(lock) { repo.sairDaVoz(); encerrar(null) }

    /** Como o desktop: se está surdo, ligar o microfone também liga o áudio. */
    fun alternarMudo() = synchronized(lock) {
        val s = _estado.value
        if (s.surdo) atualizar { it.copy(surdo = false, mudo = false) } else atualizar { it.copy(mudo = !it.mudo) }
        aplicarMudoSurdo()
    }

    fun alternarSurdo() = synchronized(lock) { atualizar { it.copy(surdo = !it.surdo) }; aplicarMudoSurdo() }

    fun definirVolume(usuario: Long, volume: Float) = synchronized(lock) {
        val v = if (volume.isFinite()) volume.coerceIn(0f, 2f) else 1f
        atualizar { it.copy(volumes = it.volumes + (usuario to v)) }
        motor?.definirVolume(usuario, v)
    }

    fun definirRota(rota: RotaDeAudio) = synchronized(lock) {
        atualizar { it.copy(rota = rota) }
        // trocar a rota nunca derruba a chamada: se o dispositivo recusar, o próximo `read` falho reabre o áudio
        runCatching { dispositivo?.definirRota(rota) }
    }

    fun definirOpcoes(o: OpcoesDeVoz) = synchronized(lock) {
        atualizar { it.copy(opcoes = o) }
        motor?.definirSensibilidade(o.sensibilidade); motor?.definirGanho(o.ganhoDeEntrada); motor?.definirAgc(o.ganhoAutomatico); motor?.definirSupressao(o.supressaoDeRuido)
        dispositivo?.aplicarOpcoes(o)
    }

    /** Mostra o erro e sai (permissão negada, por exemplo). */
    fun recusar(mensagem: String) = synchronized(lock) { encerrar(mensagem) }

    // ------------------------------------------------------------------ eventos do servidor

    private fun aoMudar(s: AppState) {
        val eu = s.eu?.id ?: return
        val meu = s.voz.firstOrNull { it.userId == eu }
        val fase = _estado.value.fase
        val conectado = s.conexao == EstadoConexao.Conectado && s.autenticado

        if (fase != FaseDaVoz.Fora && !conectado && fase != FaseDaVoz.Reconectando) {
            // a conexão caiu: o servidor tira da voz; paro o áudio e espero voltar
            pararAudio()
            atualizar { it.copy(fase = FaseDaVoz.Reconectando, falando = emptySet(), euFalando = false, nivel = 0f) }
            return
        }
        when (_estado.value.fase) {
            FaseDaVoz.Fora -> Unit
            FaseDaVoz.Entrando -> if (meu != null && meu.channelId == alvo) conectar(meu.channelId)
            FaseDaVoz.Conectada -> when {
                meu == null -> { runCatching { android.util.Log.w("TelacastVoz", "servidor tirou da voz: voz=${s.voz.size} eu=$eu") }; encerrar("Você saiu da chamada de voz.") }
                meu.channelId != _estado.value.canal -> { alvo = meu.channelId; atualizar { it.copy(canal = meu.channelId) } }
                else -> limparQuemSaiu(s, meu.channelId)
            }
            FaseDaVoz.Reconectando -> if (conectado) {
                val canal = alvo
                if (canal == null) encerrar(null) else {
                    atualizar { it.copy(fase = FaseDaVoz.Entrando) }
                    if (repo.entrarNaVoz(canal)) iniciarLimite(canal) else atualizar { it.copy(fase = FaseDaVoz.Reconectando) }
                }
            }
        }
    }

    private fun limparQuemSaiu(s: AppState, canal: Long) {
        val m = motor ?: return
        val presentes = s.voz.filter { it.channelId == canal }.map { it.userId }.toSet()
        for (u in _estado.value.falando) if (u !in presentes) m.removerUsuario(u)
    }

    // ------------------------------------------------------------------ ciclo da chamada

    private fun conectar(canal: Long) {
        jobLimite?.cancel()
        if (motor != null) { atualizar { it.copy(fase = FaseDaVoz.Conectada, canal = canal, desdeMs = it.desdeMs ?: relogio()) }; return }
        val st = _estado.value
        val m = MotorDeVoz(opus, { repo.enviarBinario(it) }, relogio, supressor = supressores)
        m.definirSupressao(st.opcoes.supressaoDeRuido)
        if (m.supressorProprio && !custoMedido) {
            custoMedido = true
            runCatching { android.util.Log.i("TelacastVoz", "RNNoise ativo: ${SupressorRnnoise.medirCustoPorQuadroUs()?.toInt()} µs por quadro de 10 ms (limite 10000)") }
        } else if (!m.supressorProprio) runCatching { android.util.Log.i("TelacastVoz", "RNNoise indisponível (biblioteca nativa ausente): usando o NoiseSuppressor do sistema") }
        m.definirMudo(st.mudo || st.surdo); m.definirSurdo(st.surdo)
        m.definirSensibilidade(st.opcoes.sensibilidade); m.definirGanho(st.opcoes.ganhoDeEntrada); m.definirAgc(st.opcoes.ganhoAutomatico)
        for ((u, v) in st.volumes) m.definirVolume(u, v)
        repo.ouvinteBinario = m::receberQuadro
        val d = criarDispositivo(m, st)
        val erro = d.iniciar()
        if (erro != null) { repo.ouvinteBinario = null; encerrar(erro, avisarServidor = true); return }
        motor = m; dispositivo = d
        atualizar { it.copy(fase = FaseDaVoz.Conectada, canal = canal, erro = null, desdeMs = relogio()) }
        if (st.mudo || st.surdo) repo.definirVoz(st.mudo || st.surdo, st.surdo)
        val nome = textoDoCanal(canal)
        servico.iniciar("Voz conectada", nome)
        iniciarMedidor()
    }

    /** Geração do dispositivo atual: avisos de falha de um dispositivo que já foi trocado ou parado são ignorados. */
    private var geracaoDoDispositivo = 0
    private var ultimaReaberturaMs = Long.MIN_VALUE
    private var custoMedido = false

    private fun criarDispositivo(m: MotorDeVoz, st: EstadoDeVoz): DispositivoDeAudio {
        val minha = ++geracaoDoDispositivo
        // a falha chega da thread de áudio do dispositivo: trata fora dela (reabrir para e junta essas threads)
        return dispositivos.criar(m, st.opcoes, st.rota) { msg -> escopo.launch { synchronized(lock) { aoFalharODispositivo(minha, msg) } } }
    }

    /**
     * O áudio falhou no meio da chamada (rota trocada, Bluetooth entrando ou saindo, microfone tomado, foco). Antes de sair da
     * chamada tenta reabrir o dispositivo UMA vez (o motor, o jitter buffer e o estado de mudo/surdo ficam); se falhar de novo
     * dentro de [JANELA_DA_REABERTURA_MS], aí sim encerra com a mensagem.
     */
    private fun aoFalharODispositivo(geracao: Int, msg: String) {
        val m = motor
        if (geracao != geracaoDoDispositivo || m == null || _estado.value.fase != FaseDaVoz.Conectada) return
        val agora = relogio()
        val recente = ultimaReaberturaMs != Long.MIN_VALUE && agora - ultimaReaberturaMs < JANELA_DA_REABERTURA_MS
        if (recente) { runCatching { android.util.Log.w("TelacastVoz", "áudio falhou de novo ($msg): saindo da chamada") }; encerrar(msg, avisarServidor = true); return }
        ultimaReaberturaMs = agora
        runCatching { android.util.Log.w("TelacastVoz", "áudio falhou ($msg): reabrindo o dispositivo") }
        dispositivo?.parar(); dispositivo = null
        val st = _estado.value
        val novo = criarDispositivo(m, st)
        val erro = runCatching { novo.iniciar() }.getOrElse { it.message ?: "falha ao reabrir o áudio" }
        if (erro != null) { encerrar(erro, avisarServidor = true); return }
        dispositivo = novo
    }

    private fun textoDoCanal(canal: Long): String {
        val sn = repo.estado.value.snapshot ?: return ""
        sn.dms.firstOrNull { it.channelId == canal }?.let { return "Chamada com ${it.user.name}" }
        for (sv in sn.servers) sv.channels.firstOrNull { it.id == canal }?.let { return "${it.name} · ${sv.name}" }
        return ""
    }

    private fun aplicarMudoSurdo() {
        val s = _estado.value
        motor?.definirMudo(s.mudo || s.surdo); motor?.definirSurdo(s.surdo)
        if (s.fase != FaseDaVoz.Fora) repo.definirVoz(s.mudo || s.surdo, s.surdo)
    }

    private fun iniciarLimite(canal: Long) {
        jobLimite?.cancel()
        jobLimite = escopo.launch {
            delay(tempoLimiteDaEntradaMs)
            synchronized(lock) {
                if (_estado.value.fase == FaseDaVoz.Entrando && alvo == canal) { repo.sairDaVoz(); encerrar("O servidor não respondeu à entrada na voz.") }
            }
        }
    }

    private var ciclos = 0

    private fun iniciarMedidor() {
        ciclos = 0
        jobMedidor?.cancel()
        jobMedidor = escopo.launch {
            while (isActive) {
                delay(intervaloDoMedidorMs)
                synchronized(lock) {
                    val m = motor ?: return@synchronized
                    val eu = repo.estado.value.eu?.id
                    val outros = repo.estado.value.voz.filter { it.channelId == _estado.value.canal && it.userId != eu }.map { it.userId }
                    val falando = outros.filter { m.falando(it) }.toSet()
                    val euFalando = m.euFalando
                    val nivel = m.nivelDoMicrofone
                    val atual = _estado.value
                    val novasStats = if (++ciclos % 5 == 0) m.estatisticas(dispositivo?.subfluxos() ?: 0) else atual.estatisticas
                    if (atual.falando != falando || atual.euFalando != euFalando || Math.abs(atual.nivel - nivel) > 0.02f || novasStats != atual.estatisticas) {
                        _estado.value = atual.copy(falando = falando, euFalando = euFalando, nivel = nivel, estatisticas = novasStats)
                    }
                }
            }
        }
    }

    private fun pararAudio() {
        jobMedidor?.cancel(); jobMedidor = null
        geracaoDoDispositivo++ // invalida avisos de falha que ainda estejam a caminho
        dispositivo?.parar(); dispositivo = null
        motor?.close(); motor = null
        repo.ouvinteBinario = null
    }

    private companion object {
        /** Segunda falha do áudio dentro desta janela depois de uma reabertura: desiste e sai da chamada. */
        const val JANELA_DA_REABERTURA_MS = 10_000L
    }

    /** Fecha tudo e volta a [FaseDaVoz.Fora]. */
    private fun encerrar(erro: String?, avisarServidor: Boolean = false) {
        // diagnóstico: de onde veio a saída da chamada (adb logcat -s TelacastVoz)
        runCatching { android.util.Log.w("TelacastVoz", "encerrar: erro=$erro avisarServidor=$avisarServidor fase=${_estado.value.fase}", Throwable("origem")) }
        if (avisarServidor) repo.sairDaVoz()
        jobLimite?.cancel(); jobLimite = null
        pararAudio()
        servico.parar()
        alvo = null
        atualizar { it.copy(fase = FaseDaVoz.Fora, canal = null, falando = emptySet(), euFalando = false, nivel = 0f, desdeMs = null, erro = erro) }
    }

    private fun falhar(mensagem: String) = encerrar(mensagem)
}

/** Fábrica para ambientes sem áudio (testes e prévias): abrir o dispositivo sempre falha com uma mensagem clara. */
object FabricaSemAudio : FabricaDeDispositivo {
    override fun criar(motor: MotorDeVoz, opcoes: OpcoesDeVoz, rota: RotaDeAudio, aoFalhar: (String) -> Unit) = object : DispositivoDeAudio {
        override fun iniciar() = "Este ambiente não tem áudio."
        override fun parar() = Unit
        override fun definirRota(rota: RotaDeAudio) = Unit
        override fun aplicarOpcoes(opcoes: OpcoesDeVoz) = Unit
    }
}
