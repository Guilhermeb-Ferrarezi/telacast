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
) {
    /** O que a tela mostra no canal: as reais em ordem e, no fim, as provisórias na ordem de envio. */
    fun lista(canal: Long): List<Message> {
        val reais = mensagens[canal].orEmpty()
        val eu = eu ?: return reais
        val prov = pendentes.filter { it.canal == canal }.map { it.provisoria(eu) }
        return if (prov.isEmpty()) reais else reais + prov
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

/** Avisos de uma vez só (a tela reage e esquece): navegação e erros. */
sealed interface Aviso {
    data class Erro(val codigo: app.telacast.proto.ErrorCode, val msg: String) : Aviso
    data class DmAberta(val canal: Long) : Aviso
    data class ServidorEntrou(val servidor: Long) : Aviso
    data object SenhaTrocada : Aviso
    /** Mensagem informativa curta (por exemplo, "A mensagem não está mais disponível"). */
    data class Texto(val msg: String) : Aviso
}
