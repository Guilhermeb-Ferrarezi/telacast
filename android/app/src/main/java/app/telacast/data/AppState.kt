package app.telacast.data

import app.telacast.net.EstadoConexao
import app.telacast.proto.Attachment
import app.telacast.proto.Channel
import app.telacast.proto.Message
import app.telacast.proto.Reply
import app.telacast.proto.SessionInfo
import app.telacast.proto.Snapshot
import app.telacast.proto.User
import app.telacast.proto.VoiceState

/**
 * Mensagem enviada que o servidor ainda não confirmou. Aparece na lista como provisória
 * (`id = -nonce`, ver [provisoria]) e sai quando chega o eco com o mesmo `nonce`.
 */
data class Pendente(
    val canal: Long,
    val nonce: Long,
    val conteudo: String,
    val responderA: Reply?,
    val anexos: List<Attachment>,
    val ts: Long,
    /** Maior id real que o canal tinha ao enviar: só mensagem MAIS NOVA que isto pode ser a nossa (ver `conciliar`). */
    val depoisDe: Long,
) {
    fun provisoria(eu: User) = Message(
        id = -nonce, channelId = canal, authorId = eu.id, author = eu.name, content = conteudo, ts = ts,
        edited = false, reply = responderA, nonce = nonce, attachments = anexos,
    )
}

/** Por que a entrada (login/cadastro/retomada) falhou; a tela mostra `msg` e, se `sessaoInvalida`, volta ao login. */
data class ErroDeEntrada(val codigo: app.telacast.proto.ErrorCode?, val msg: String, val sessaoInvalida: Boolean)

/**
 * Tudo o que as telas precisam, num valor só e imutável. Fluxo de dados em um sentido:
 * `Gateway` -> `Repository` -> `StateFlow<AppState>` -> telas (sem lógica).
 */
data class AppState(
    val conexao: EstadoConexao = EstadoConexao.Desconectado,
    /** Já recebeu `Auth` nesta execução (a tela de login some). */
    val autenticado: Boolean = false,
    val erroDeEntrada: ErroDeEntrada? = null,
    /** Maior `live_revision` de snapshot já aplicado nesta conexão (snapshot mais velho que ele chegou atrasado e é ignorado). */
    val liveRevisionVisto: Long = 0,
    /** Tráfego de saída do servidor no mês UTC atual: (`2026-11`, bytes). */
    val trafegoDoMes: Pair<String, Long>? = null,
    val eu: User? = null,
    val snapshot: Snapshot? = null,
    val canalAberto: Long? = null,
    /** Mensagens REAIS por canal, em ordem de id (cronológica). As provisórias vêm de [pendentes]; use [lista]. */
    val mensagens: Map<Long, List<Message>> = emptyMap(),
    val pendentes: List<Pendente> = emptyList(),
    /** Canais cujo histórico já foi pedido nesta conexão. */
    val carregados: Set<Long> = emptySet(),
    val buscandoAntigas: Set<Long> = emptySet(),
    /** Há páginas mais antigas no servidor (a última veio cheia, 50). */
    val temMais: Map<Long, Boolean> = emptyMap(),
    val naoLidos: Set<Long> = emptySet(),
    val mencoes: Map<Long, Int> = emptyMap(),
    /** Quem está digitando, por canal (some sozinho em ~8 s). */
    val digitando: Map<Long, Set<Long>> = emptyMap(),
    val fixadas: Map<Long, List<Message>> = emptyMap(),
    val voz: List<VoiceState> = emptyList(),
    val espectadores: Map<Long, List<Long>> = emptyMap(),
    val sessoes: List<SessionInfo> = emptyList(),
    /** Divisor "novas mensagens": canal -> último id lido quando o canal foi aberto (só se havia o que ler). */
    val marcadores: Map<Long, Long> = emptyMap(),
    /** Salto até uma mensagem antiga em andamento (buscando páginas). */
    val salto: Salto? = null,
    /** Mensagem a rolar até e realçar em âmbar por ~1,5 s. */
    val destaque: Destaque? = null,
    /** Busca de mensagens (painel de busca). */
    val busca: EstadoBusca = EstadoBusca(),
    /** Salto pedido para um canal que ainda não estava aberto: sai quando o histórico dele chegar. */
    val saltoAgendado: Pair<Long, Long>? = null,
) {
    /** O que a tela mostra no canal: as reais em ordem e, no fim, as provisórias na ordem de envio. */
    fun lista(canal: Long): List<Message> {
        val reais = mensagens[canal].orEmpty()
        val eu = eu ?: return reais
        val prov = pendentes.filter { it.canal == canal }.map { it.provisoria(eu) }
        return if (prov.isEmpty()) reais else reais + prov
    }

    /** Procura a pessoa em tudo o que o snapshot traz (eu, membros, amigos, DMs, bloqueados). */
    fun usuario(id: Long): User? {
        val sn = snapshot ?: return eu?.takeIf { it.id == id }
        if (sn.me.id == id) return sn.me
        return sn.servers.firstNotNullOfOrNull { s -> s.members.firstOrNull { it.id == id } }
            ?: sn.friends.firstOrNull { it.user.id == id }?.user
            ?: sn.dms.firstOrNull { it.user.id == id }?.user
            ?: sn.blocked.firstOrNull { it.id == id }
    }

    fun canal(id: Long): Channel? = snapshot?.servers?.firstNotNullOfOrNull { s -> s.channels.firstOrNull { it.id == id } }

    /** Maior id conhecido do canal: o do snapshot ou o da última mensagem carregada, o que for maior. */
    fun ultimoId(canal: Long): Long {
        val doSnapshot = canal(canal)?.lastMessageId ?: snapshot?.dms?.firstOrNull { it.channelId == canal }?.lastMessageId ?: 0L
        val carregada = mensagens[canal]?.lastOrNull()?.id ?: 0L
        return maxOf(doSnapshot, carregada)
    }
}

/** Salto até uma resposta antiga: busca no máximo `paginas` páginas de histórico (como o desktop). */
data class Salto(val canal: Long, val id: Long, val paginas: Int)

data class Destaque(val canal: Long, val id: Long, val desde: Long)

/** Resultado da busca de mensagens (do desktop: `SearchState`). `mais` = há outra página. */
data class EstadoBusca(
    val consulta: String = "",
    val nonce: Long = 0,
    val hits: List<app.telacast.proto.SearchHit> = emptyList(),
    val mais: Boolean = false,
    val carregando: Boolean = false,
    val anexando: Boolean = false,
    /** Já buscou ao menos uma vez (distingue "sem resultados" de "ainda não buscou"). */
    val buscou: Boolean = false,
)

/** Avisos de uma vez só (a tela reage e esquece): navegação e erros. */
sealed interface Aviso {
    data class Erro(val codigo: app.telacast.proto.ErrorCode, val msg: String) : Aviso
    data class DmAberta(val canal: Long) : Aviso
    data class ServidorEntrou(val servidor: Long) : Aviso
    data object SenhaTrocada : Aviso
    /** Mensagem informativa curta (por exemplo, "A mensagem não está mais disponível"). */
    data class Texto(val msg: String) : Aviso
}

/** O id é um snowflake: carrega a data de criação da conta (época do servidor: 2026-01-01). Em milissegundos. */
fun contaCriadaEm(id: Long): Long = (id shr 22) + 1_767_225_600_000L
