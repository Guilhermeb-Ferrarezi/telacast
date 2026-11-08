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
    val sensibilidade: Float = Vad.SENSIBILIDADE_PADRAO,
    /** Ganho de entrada linear (0,5 a 4). */
    val ganhoDeEntrada: Float = 1f,
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

    fun definirRota(rota: RotaDeAudio) = synchronized(lock) { atualizar { it.copy(rota = rota) }; dispositivo?.definirRota(rota) }

    fun definirOpcoes(o: OpcoesDeVoz) = synchronized(lock) {
        atualizar { it.copy(opcoes = o) }
        motor?.definirSensibilidade(o.sensibilidade); motor?.definirGanho(o.ganhoDeEntrada)
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
                meu == null -> encerrar("Você saiu da chamada de voz.")
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
        val m = MotorDeVoz(opus, { repo.enviarBinario(it) }, relogio)
        m.definirMudo(st.mudo || st.surdo); m.definirSurdo(st.surdo)
        m.definirSensibilidade(st.opcoes.sensibilidade); m.definirGanho(st.opcoes.ganhoDeEntrada)
        for ((u, v) in st.volumes) m.definirVolume(u, v)
        repo.ouvinteBinario = m::receberQuadro
        val d = dispositivos.criar(m, st.opcoes, st.rota) { msg -> synchronized(lock) { encerrar(msg, avisarServidor = true) } }
        val erro = d.iniciar()
        if (erro != null) { repo.ouvinteBinario = null; encerrar(erro, avisarServidor = true); return }
        motor = m; dispositivo = d
        atualizar { it.copy(fase = FaseDaVoz.Conectada, canal = canal, erro = null, desdeMs = relogio()) }
        if (st.mudo || st.surdo) repo.definirVoz(st.mudo || st.surdo, st.surdo)
        val nome = textoDoCanal(canal)
        servico.iniciar("Voz conectada", nome)
        iniciarMedidor()
    }

    private fun textoDoCanal(canal: Long): String {
        val sn = repo.estado.value.snapshot ?: return ""
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

    private fun iniciarMedidor() {
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
                    if (atual.falando != falando || atual.euFalando != euFalando || Math.abs(atual.nivel - nivel) > 0.02f) {
                        _estado.value = atual.copy(falando = falando, euFalando = euFalando, nivel = nivel)
                    }
                }
            }
        }
    }

    private fun pararAudio() {
        jobMedidor?.cancel(); jobMedidor = null
        dispositivo?.parar(); dispositivo = null
        motor = null
        repo.ouvinteBinario = null
    }

    /** Fecha tudo e volta a [FaseDaVoz.Fora]. */
    private fun encerrar(erro: String?, avisarServidor: Boolean = false) {
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
