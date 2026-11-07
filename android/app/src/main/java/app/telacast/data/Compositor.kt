package app.telacast.data

import app.telacast.armazenamento.Preferencias
import app.telacast.proto.Attachment
import app.telacast.proto.Message
import app.telacast.proto.Reply
import app.telacast.proto.User
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/** O que o campo de mensagem mostra. `editando` = id da mensagem sendo editada (o texto dela está no campo). */
data class EstadoCompositor(
    val canal: Long = 0,
    val valor: TextFieldValue = TextFieldValue(""),
    val respondendo: Reply? = null,
    val editando: Long? = null,
)

/** Uma sugestão do autocompletar de `@`: um membro ou `@everyone`/`@here` (`usuario == null`). */
data class Sugestao(val nome: String, val usuario: User?, val descricao: String? = null)

data class SugestoesDeMencao(val inicio: Int, val consulta: String, val itens: List<Sugestao>)

object Autocompletar {
    private fun parteDeNome(c: Char) = c.isLetterOrDigit() || c == '_' || c == '.' || c == '-'

    /** Acha o `@token` colado ao cursor e lista quem casa (membros online primeiro, depois por nome; no máx. 6). */
    fun sugestoes(texto: String, cursor: Int, membros: List<User>, eu: Long): SugestoesDeMencao? {
        if (cursor < 1 || cursor > texto.length) return null
        var p = cursor
        while (p > 0 && parteDeNome(texto[p - 1])) p--
        if (p == 0 || texto[p - 1] != '@') return null
        val arroba = p - 1
        if (arroba > 0 && texto[arroba - 1].isLetterOrDigit()) return null // e-mail, não menção
        val consulta = texto.substring(p, cursor)
        val itens = membros.filter { it.id != eu && it.name.startsWith(consulta, ignoreCase = true) }
            .sortedWith(compareByDescending<User> { it.online }.thenBy { it.name.lowercase() })
            .take(6).map { Sugestao(it.name, it) } +
            listOf("everyone" to "Notifica todo mundo do canal", "here" to "Notifica quem está online")
                .filter { it.first.startsWith(consulta, ignoreCase = true) }.map { Sugestao(it.first, null, it.second) }
        return if (itens.isEmpty()) null else SugestoesDeMencao(arroba, consulta, itens.take(7))
    }

    /** Troca o `@token` por `@nome ` e devolve o novo texto e a posição do cursor. */
    fun aplicar(texto: String, cursor: Int, s: SugestoesDeMencao, nome: String): Pair<String, Int> {
        val novo = texto.substring(0, s.inicio) + "@$nome " + texto.substring(cursor)
        return novo to s.inicio + nome.length + 2
    }
}

/**
 * Lógica do compositor, sem Compose: texto, resposta, edição, rascunho por canal e envio. Fala com o [Repository];
 * o `Typing` já é limitado a um a cada 4 s por ele.
 */
class ControladorDoCompositor(
    private val repo: Repository,
    private val prefs: Preferencias,
) {
    private val _estado = MutableStateFlow(EstadoCompositor())
    val estado: StateFlow<EstadoCompositor> = _estado.asStateFlow()

    /** Rascunho de quem estava escrevendo antes de entrar em modo de edição. */
    private var rascunhoDaEdicao: String? = null

    /** Troca de canal: guarda o rascunho do atual e carrega o do novo (a edição é cancelada). */
    fun abrirCanal(canal: Long) = _estado.update { atual ->
        if (atual.canal == canal) return@update atual
        if (atual.editando == null && atual.canal != 0L) prefs.salvarRascunho(atual.canal, atual.valor.text)
        rascunhoDaEdicao = null
        val texto = prefs.rascunho(canal)
        EstadoCompositor(canal = canal, valor = TextFieldValue(texto, TextRange(texto.length)))
    }

    fun mudar(valor: TextFieldValue) {
        val antes = _estado.value
        _estado.value = antes.copy(valor = valor)
        if (antes.canal == 0L) return
        if (antes.editando == null) prefs.salvarRascunho(antes.canal, valor.text)
        if (valor.text != antes.valor.text && valor.text.isNotBlank() && antes.editando == null) repo.avisarDigitando(antes.canal)
    }

    fun responder(m: Message) = _estado.update {
        it.copy(respondendo = Reply(m.id, m.author, m.content.take(120)), editando = null)
    }

    fun cancelarResposta() = _estado.update { it.copy(respondendo = null) }

    fun editar(m: Message) = _estado.update { atual ->
        if (atual.editando == null) rascunhoDaEdicao = atual.valor.text
        atual.copy(valor = TextFieldValue(m.content, TextRange(m.content.length)), editando = m.id, respondendo = null)
    }

    fun cancelarEdicao() = _estado.update { atual ->
        if (atual.editando == null) return@update atual
        val texto = rascunhoDaEdicao.orEmpty()
        rascunhoDaEdicao = null
        atual.copy(valor = TextFieldValue(texto, TextRange(texto.length)), editando = null)
    }

    /** Aplica uma sugestão do autocompletar na posição do cursor. */
    fun completarMencao(s: SugestoesDeMencao, nome: String) = _estado.update { atual ->
        val (texto, cursor) = Autocompletar.aplicar(atual.valor.text, atual.valor.selection.end, s, nome)
        atual.copy(valor = TextFieldValue(texto, TextRange(cursor)))
    }

    /**
     * Envia (ou salva a edição). Devolve `false` se não havia o que enviar. Os `anexos` são os já prontos; quem chama
     * limpa a bandeja quando isto devolve `true`.
     */
    fun enviar(anexos: List<Attachment> = emptyList()): Boolean {
        val atual = _estado.value
        if (atual.canal == 0L) return false
        val edicao = atual.editando
        if (edicao != null) {
            val texto = atual.valor.text.trim()
            if (texto.isEmpty() || texto.length > Repository.LIMITE_TEXTO) return false
            repo.editar(edicao, texto)
            cancelarEdicao()
            return true
        }
        repo.enviarMensagem(atual.canal, atual.valor.text, atual.respondendo, anexos) ?: return false
        prefs.salvarRascunho(atual.canal, "")
        _estado.value = atual.copy(valor = TextFieldValue(""), respondendo = null)
        return true
    }

    val podeEnviar: Boolean get() = _estado.value.valor.text.isNotBlank()
}

/** "fulano está digitando…" (como no desktop); `null` se ninguém. */
fun textoDigitando(nomes: List<String>): String? = when (nomes.size) {
    0 -> null
    1 -> "${nomes[0]} está digitando…"
    2 -> "${nomes[0]} e ${nomes[1]} estão digitando…"
    3 -> "${nomes[0]}, ${nomes[1]} e ${nomes[2]} estão digitando…"
    else -> "Várias pessoas estão digitando…"
}
