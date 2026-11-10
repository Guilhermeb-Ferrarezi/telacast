package app.telacast.chamada

import app.telacast.data.AppState
import app.telacast.proto.Status
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/** Estado da ligação direta (DM) que está tocando. Não há mensagem própria no protocolo: a chamada é o `VoiceJoin` no canal da DM. */
sealed interface EstadoDaChamada {
    data object Nenhuma : EstadoDaChamada
    /** O outro entrou primeiro na chamada da DM e eu ainda não: está tocando para mim. */
    data class Recebendo(val canal: Long, val usuario: Long, val nome: String, val desdeMs: Long) : EstadoDaChamada
    /** Eu entrei primeiro e espero o outro atender. */
    data class Chamando(val canal: Long, val nome: String, val desdeMs: Long) : EstadoDaChamada
}

/** Toques da ligação (no Android: `Ringtone` do sistema e `ToneGenerator`). */
interface ToqueDeChamada {
    fun tocarRecebendo()
    fun tocarChamando()
    fun parar()
    /** Som curto de fim de chamada. */
    fun fim()
}

object ToqueNulo : ToqueDeChamada {
    override fun tocarRecebendo() = Unit
    override fun tocarChamando() = Unit
    override fun parar() = Unit
    override fun fim() = Unit
}

/** Notificações da ligação (no Android: canal de alta prioridade com "Atender" e "Recusar"). */
interface NotificadorDeChamada {
    fun chamadaRecebida(canal: Long, nome: String)
    fun chamadaPerdida(canal: Long, nome: String)
    fun limpar(canal: Long)
}

object NotificadorDeChamadaNulo : NotificadorDeChamada {
    override fun chamadaRecebida(canal: Long, nome: String) = Unit
    override fun chamadaPerdida(canal: Long, nome: String) = Unit
    override fun limpar(canal: Long) = Unit
}

/**
 * Chamadas em DM individual (o desktop faz igual; `app/src/main.rs`): a sala da chamada é o canal da DM e só os dois entram. Quem liga faz
 * `VoiceJoin` na DM; o outro percebe pelo `VoiceUpdate`. Regras do desktop:
 * - toca quando o outro entra numa chamada que estava vazia e eu não estou nela; para quando atendo (entro), o outro desliga ou acaba o tempo;
 * - 60 s sem atender: a que recebe vira "Chamada perdida"; a que liga sai da chamada sozinha ("A ligação expirou");
 * - "Recusar" só para o toque DESTE aparelho (não há mensagem de recusa no protocolo; quem liga continua tocando até o próprio limite de 60 s);
 * - não toca com status "Não perturbe" (a faixa na tela aparece do mesmo jeito).
 */
class ControladorDeChamadasEmDm(
    private val escopo: CoroutineScope,
    estadoDoApp: StateFlow<AppState>,
    /** Sai da chamada de voz atual (a tentativa de ligar que ninguém atendeu). */
    private val sairDaVoz: () -> Unit,
    private val avisar: (String) -> Unit,
    private val toque: ToqueDeChamada = ToqueNulo,
    private val notificador: NotificadorDeChamada = NotificadorDeChamadaNulo,
    private val emPrimeiroPlano: () -> Boolean = { true },
    private val relogio: () -> Long = System::currentTimeMillis,
    private val tempoLimiteMs: Long = 60_000,
) {
    private val _estado = MutableStateFlow<EstadoDaChamada>(EstadoDaChamada.Nenhuma)
    val estado: StateFlow<EstadoDaChamada> = _estado.asStateFlow()
    private val lock = Any()
    private var anterior: Map<Long, Set<Long>> = emptyMap()
    private var iniciou = false
    /** Chamadas que recusei neste aparelho: não voltam a tocar até a sala esvaziar. */
    private val recusadas = HashSet<Long>()
    private var jobTempo: Job? = null

    init {
        escopo.launch { estadoDoApp.collect { synchronized(lock) { aoMudar(it) } } }
    }

    private fun aoMudar(s: AppState) {
        val euUsuario = s.eu ?: return
        val eu = euUsuario.id
        val dms = s.snapshot?.dms.orEmpty()
        val agora = dms.associate { dm -> dm.channelId to s.voz.filter { it.channelId == dm.channelId }.map { it.userId }.toSet() }
        val antes = anterior
        anterior = agora
        if (!iniciou) { iniciou = true; return } // o primeiro estado da conexão não é uma chamada nova (quem já estava na sala não "liga" agora)
        if (!s.autenticado) { encerrar(); return }
        val atual = _estado.value
        for ((canal, usuarios) in agora) {
            val tinha = antes[canal].orEmpty()
            val outro = usuarios.firstOrNull { it != eu }
            val euNaSala = eu in usuarios
            if (usuarios.isEmpty()) recusadas.remove(canal)
            val nome = dms.firstOrNull { it.channelId == canal }?.user?.name.orEmpty()
            when {
                // o outro entrou numa chamada vazia: está ligando para mim
                tinha.isEmpty() && outro != null && !euNaSala && canal !in recusadas && atual !is EstadoDaChamada.Recebendo ->
                    receber(canal, outro, nome, euUsuario.status == Status.Dnd)
                // eu entrei primeiro: espero atender
                eu !in tinha && euNaSala && outro == null && atual !is EstadoDaChamada.Chamando -> chamar(canal, nome)
                // os dois estão na sala: atendida
                euNaSala && outro != null && (atual.canalOuNulo() == canal) -> { encerrarSemSom(); }
            }
            // o outro desligou antes de eu atender
            val a = _estado.value
            if (a is EstadoDaChamada.Recebendo && a.canal == canal && outro == null && !euNaSala) { perdida(a); }
            // eu saí enquanto ligava
            if (a is EstadoDaChamada.Chamando && a.canal == canal && !euNaSala) encerrar()
        }
    }

    private fun EstadoDaChamada.canalOuNulo(): Long? = when (this) { is EstadoDaChamada.Recebendo -> canal; is EstadoDaChamada.Chamando -> canal; else -> null }

    private fun receber(canal: Long, usuario: Long, nome: String, naoPerturbe: Boolean) {
        val e = EstadoDaChamada.Recebendo(canal, usuario, nome, relogio())
        _estado.value = e
        if (!naoPerturbe) {
            toque.tocarRecebendo()
            if (!emPrimeiroPlano()) notificador.chamadaRecebida(canal, nome)
        }
        agendar(e)
    }

    private fun chamar(canal: Long, nome: String) {
        val e = EstadoDaChamada.Chamando(canal, nome, relogio())
        _estado.value = e
        toque.tocarChamando()
        agendar(e)
    }

    private fun agendar(e: EstadoDaChamada) {
        jobTempo?.cancel()
        jobTempo = escopo.launch {
            delay(tempoLimiteMs)
            synchronized(lock) {
                if (_estado.value != e) return@synchronized
                when (e) {
                    is EstadoDaChamada.Recebendo -> perdida(e)
                    is EstadoDaChamada.Chamando -> { encerrar(); sairDaVoz(); toque.fim(); avisar("A ligação expirou: ninguém atendeu em 1 minuto.") }
                    EstadoDaChamada.Nenhuma -> Unit
                }
            }
        }
    }

    private fun perdida(e: EstadoDaChamada.Recebendo) {
        encerrar()
        notificador.chamadaPerdida(e.canal, e.nome)
        avisar("Chamada perdida de ${e.nome}.")
    }

    private fun encerrarSemSom() {
        val c = _estado.value.canalOuNulo()
        jobTempo?.cancel(); jobTempo = null
        toque.parar(); c?.let(notificador::limpar)
        _estado.value = EstadoDaChamada.Nenhuma
    }

    private fun encerrar() = encerrarSemSom()

    /** Recusar neste aparelho: para o toque e a notificação. */
    fun recusar(canal: Long) = synchronized(lock) {
        val a = _estado.value
        if (a is EstadoDaChamada.Recebendo && a.canal == canal) { recusadas += canal; encerrar() }
    }

    /** Eu atendi (ou liguei) pela interface: o toque para na hora, sem esperar o servidor confirmar. */
    fun aoEntrarNaChamada(canal: Long) = synchronized(lock) {
        val a = _estado.value
        if (a is EstadoDaChamada.Recebendo && a.canal == canal) encerrar()
    }
}
