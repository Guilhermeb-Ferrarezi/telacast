package app.telacast.push

import app.telacast.armazenamento.Preferencias
import app.telacast.data.Repository
import app.telacast.net.EstadoConexao
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

/** De onde vem o token do FCM. Sem credenciais do Firebase (`google-services.json`), [SemToken]: o app não registra push. */
interface ProvedorDeToken {
    /** `null` se não há como obter (sem Firebase, sem Play Services, sem rede). */
    suspend fun obter(): String?
    /** Invalida o token no FCM (ao sair da conta). */
    suspend fun apagar()
}

object SemToken : ProvedorDeToken {
    override suspend fun obter(): String? = null
    override suspend fun apagar() = Unit
}

private fun AppStateConectado(s: app.telacast.data.AppState) = s.autenticado && s.conexao is EstadoConexao.Conectado

/**
 * Registra o token do FCM no servidor: a cada vez que a conexão fica autenticada (o servidor guarda o token por conta, então
 * repetir é inofensivo), quando o FCM troca o token (`aoNovoToken`) e tira o registro ao sair da conta. O token fica nas
 * preferências para o registro não depender de pedir ao Firebase toda vez.
 */
class GerenciadorDePush(
    private val repo: Repository,
    private val escopo: CoroutineScope,
    private val provedor: ProvedorDeToken,
    private val prefs: Preferencias,
) {
    init {
        escopo.launch {
            repo.estado.map(::AppStateConectado).distinctUntilChanged().collect { conectado -> if (conectado) registrar() }
        }
    }

    private suspend fun registrar() {
        val token = prefs.tokenDePush.ifEmpty { provedor.obter() ?: return }
        prefs.tokenDePush = token
        repo.registrarPush(token)
    }

    /** O FCM trocou (ou criou) o token. Chamado pelo `FirebaseMessagingService`, em qualquer thread. */
    fun aoNovoToken(novo: String) {
        val antigo = prefs.tokenDePush
        prefs.tokenDePush = novo
        if (AppStateConectado(repo.estado.value)) {
            if (antigo.isNotEmpty() && antigo != novo) repo.cancelarPush(antigo)
            repo.registrarPush(novo)
        }
    }

    /** Antes do `Logout`: o servidor para de mandar push a este aparelho e o token some daqui e do FCM. */
    fun aoSair() {
        val token = prefs.tokenDePush
        if (token.isNotEmpty()) repo.cancelarPush(token)
        prefs.tokenDePush = ""
        escopo.launch { runCatching { provedor.apagar() } }
    }
}

/**
 * Diz ao servidor se o app está na frente (`SetFocus`): o servidor só manda push a quem não tem conexão com `focused = true`.
 * Estado por conexão: ao conectar de novo (queda de rede, retomada) o valor atual é reenviado; ao desconectar, esquecemos
 * o que tínhamos mandado. [atividadeIniciou]/[atividadeParou] vêm do ciclo de vida das activities (a rotação não conta).
 */
class ControladorDeFoco(private val repo: Repository, escopo: CoroutineScope) {
    private val lock = Any()
    private var visiveis = 0
    private var enviado: Boolean? = null

    val emPrimeiroPlano: Boolean get() = synchronized(lock) { visiveis > 0 }

    init {
        escopo.launch {
            repo.estado.map(::AppStateConectado).distinctUntilChanged().collect { conectado ->
                synchronized(lock) { if (conectado) sincronizar() else enviado = null }
            }
        }
    }

    fun atividadeIniciou() = synchronized(lock) { visiveis++; sincronizar() }

    /** `mudandoConfiguracao`: a activity vai ser recriada (rotação); o app continua na frente. */
    fun atividadeParou(mudandoConfiguracao: Boolean = false) = synchronized(lock) {
        visiveis = (visiveis - 1).coerceAtLeast(0)
        if (!mudandoConfiguracao) sincronizar()
    }

    private fun sincronizar() {
        if (!AppStateConectado(repo.estado.value)) return
        val foco = visiveis > 0
        if (enviado == foco) return
        if (repo.definirFoco(foco)) enviado = foco
    }
}
