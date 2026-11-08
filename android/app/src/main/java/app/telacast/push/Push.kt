package app.telacast.push

/** Tipos de push que o servidor manda (chave `kind` da data message do FCM). */
enum class TipoDePush(val etiqueta: String) {
    Dm("dm"), Mencao("mention"), Mensagem("message"), PedidoDeAmizade("friend_request");

    companion object {
        /** Valor que o app não conhece (servidor mais novo): vale como `message`, como manda `docs/PROTOCOL.md` 11.2. */
        fun de(etiqueta: String?) = entries.firstOrNull { it.etiqueta == etiqueta } ?: Mensagem
    }
}

/**
 * O que chega no FCM como data message: só strings. `canal` é o id do canal ou da DM; `servidor` é 0 em DM e em pedido de
 * amizade; `autor` é quem escreveu (ou quem pediu amizade); `ts` em milissegundos.
 */
data class DadosDePush(
    val tipo: TipoDePush,
    val canal: Long,
    val mensagem: Long,
    val servidor: Long,
    val titulo: String,
    val texto: String,
    val autor: Long,
    val ts: Long,
) {
    companion object {
        /**
         * Lê o mapa de strings do FCM. `null` se faltar o que o app precisa para abrir a tela certa: conforme o tipo, o
         * canal (dm, mention, message) ou o autor (friend_request). Ids que não são números invalidam; tipo novo vira `message`.
         */
        fun de(m: Map<String, String>): DadosDePush? {
            val tipo = TipoDePush.de(m["kind"])
            fun num(k: String): Long? = m[k]?.takeIf { it.isNotBlank() }?.toLongOrNull()
            if (m["channel_id"].orEmpty().isNotBlank() && num("channel_id") == null) return null
            if (m["author_id"].orEmpty().isNotBlank() && num("author_id") == null) return null
            val canal = num("channel_id") ?: 0L
            val autor = num("author_id") ?: 0L
            when (tipo) {
                TipoDePush.PedidoDeAmizade -> if (autor <= 0) return null
                else -> if (canal <= 0) return null
            }
            return DadosDePush(
                tipo, canal, num("message_id") ?: 0L, num("server_id") ?: 0L,
                limpar(m["title"].orEmpty(), 80), limpar(m["body"].orEmpty(), 500), autor, milissegundos(num("ts")),
            )
        }

        /** Sem controle nem quebra de linha repetida, no máximo `max` caracteres. */
        internal fun limpar(s: String, max: Int): String {
            val t = s.filter { it == '\n' || !it.isISOControl() }.replace(Regex("\n{3,}"), "\n\n").trim()
            return if (t.length <= max) t else t.take(max - 1).trimEnd() + "…"
        }

        /** O servidor pode mandar segundos ou milissegundos; 0 vira "agora" (quem chama decide). */
        internal fun milissegundos(ts: Long?): Long = when {
            ts == null || ts <= 0 -> 0L
            ts < 100_000_000_000L -> ts * 1000
            else -> ts
        }
    }
}

/** Para onde o toque na notificação leva. */
sealed interface DestinoDoToque {
    data class Dm(val canal: Long) : DestinoDoToque
    data class Canal(val servidor: Long, val canal: Long) : DestinoDoToque
    /** Início, aba Pendente (pedidos de amizade). */
    data object Pendentes : DestinoDoToque

    /** Vai nos extras do `PendingIntent` (só strings). */
    fun paraMapa(): Map<String, String> = when (this) {
        is Dm -> mapOf(CHAVE to "dm", "canal" to canal.toString())
        is Canal -> mapOf(CHAVE to "canal", "servidor" to servidor.toString(), "canal" to canal.toString())
        Pendentes -> mapOf(CHAVE to "pendentes")
    }

    companion object {
        const val CHAVE = "destino"

        fun deMapa(m: Map<String, String>): DestinoDoToque? = when (m[CHAVE]) {
            "dm" -> m["canal"]?.toLongOrNull()?.takeIf { it > 0 }?.let(::Dm)
            "canal" -> {
                val s = m["servidor"]?.toLongOrNull(); val c = m["canal"]?.toLongOrNull()
                if (s != null && c != null && s > 0 && c > 0) Canal(s, c) else null
            }
            "pendentes" -> Pendentes
            else -> null
        }

        fun de(d: DadosDePush): DestinoDoToque = when (d.tipo) {
            TipoDePush.Dm -> Dm(d.canal)
            TipoDePush.Mencao, TipoDePush.Mensagem -> if (d.servidor > 0) Canal(d.servidor, d.canal) else Dm(d.canal)
            TipoDePush.PedidoDeAmizade -> Pendentes
        }
    }
}

/** Canais de notificação do Android (o usuário liga e desliga cada um nos Ajustes do sistema). */
enum class CanalDeNotificacao(val id: String, val nome: String, val descricao: String) {
    Dms("dms", "Mensagens diretas", "Mensagens que pessoas mandam só para você."),
    Mencoes("mencoes", "Menções", "Quando alguém menciona você num canal."),
    Mensagens("mensagens", "Mensagens de canais", "Mensagens novas nos canais dos seus servidores."),
    Amizades("amizades", "Pedidos de amizade", "Quando alguém quer ser seu amigo.");

    /** Importância pensada para cada um: DMs e menções chamam a atenção; o resto é discreto. */
    val importancia: Importancia get() = when (this) {
        Dms, Mencoes -> Importancia.Alta
        Amizades -> Importancia.Padrao
        Mensagens -> Importancia.Baixa
    }
}

enum class Importancia { Alta, Padrao, Baixa }

/** Uma linha do histórico que aparece empilhada na notificação do canal. */
data class LinhaDeMensagem(val autor: String, val texto: String, val ts: Long, val mensagem: Long = 0)

/** Notificação já decidida: o Android só desenha. */
data class ModeloDeNotificacao(
    val canal: CanalDeNotificacao,
    /** `tag` da notificação: uma por canal/DM (`c:ID`), uma por pessoa nos pedidos de amizade (`f:ID`). Colapsa por canal. */
    val tag: String,
    val titulo: String,
    val texto: String,
    val ts: Long,
    val destino: DestinoDoToque,
    /** Conversas (dm, mention, message) viram `MessagingStyle` com até [MAX_LINHAS] linhas; pedido de amizade é texto simples. */
    val conversa: Boolean,
    val linha: LinhaDeMensagem,
    /** Atualização dentro da janela anti-spam (notificação local): troca o texto sem tocar o som de novo. */
    val silenciosa: Boolean = false,
    /** `#canal (servidor)` nas menções (o título do servidor é `autor em #canal (servidor)`); `null` em DM e pedido. */
    val tituloDaConversa: String? = null,
) {
    companion object {
        const val MAX_LINHAS = 5

        private val TITULO_DE_CANAL = Regex("^(.+?) em (#.+)$")

        /** `marina em #geral (Turma)` -> (`marina`, `#geral (Turma)`); título fora desse formato fica inteiro como autor. */
        fun partirTitulo(titulo: String): Pair<String, String?> =
            TITULO_DE_CANAL.matchEntire(titulo)?.let { it.groupValues[1] to it.groupValues[2] } ?: (titulo to null)

        fun tagDoCanal(canal: Long) = "c:$canal"
        fun tagDoPedido(autor: Long) = "f:$autor"

        /** Monta o modelo; `agora` preenche o `ts` quando o servidor não mandou. */
        fun de(d: DadosDePush, agora: Long): ModeloDeNotificacao {
            val ts = if (d.ts > 0) d.ts else agora
            val (canal, tituloPadrao, textoPadrao) = when (d.tipo) {
                TipoDePush.Dm -> Triple(CanalDeNotificacao.Dms, "Nova mensagem direta", "Toque para abrir a conversa.")
                TipoDePush.Mencao -> Triple(CanalDeNotificacao.Mencoes, "Você foi mencionado", "Toque para abrir o canal.")
                TipoDePush.Mensagem -> Triple(CanalDeNotificacao.Mensagens, "Nova mensagem", "Toque para abrir o canal.")
                TipoDePush.PedidoDeAmizade -> Triple(CanalDeNotificacao.Amizades, "Pedido de amizade", "Alguém quer ser seu amigo.")
            }
            val titulo = d.titulo.ifBlank { tituloPadrao }
            val texto = d.texto.ifBlank { textoPadrao }
            val conversa = d.tipo != TipoDePush.PedidoDeAmizade
            val (autor, local) = if (d.tipo == TipoDePush.Mencao || d.tipo == TipoDePush.Mensagem) partirTitulo(titulo) else titulo to null
            return ModeloDeNotificacao(
                canal, if (conversa) tagDoCanal(d.canal) else tagDoPedido(d.autor), titulo, texto, ts, DestinoDoToque.de(d), conversa,
                LinhaDeMensagem(autor, texto, ts, d.mensagem), tituloDaConversa = local,
            )
        }
    }
}

/** Empilha mensagens do mesmo canal: sem repetir a mesma (mesmo `mensagem`), em ordem de hora, só as [max] mais novas. */
fun acumularLinhas(existentes: List<LinhaDeMensagem>, nova: LinhaDeMensagem, max: Int = ModeloDeNotificacao.MAX_LINHAS): List<LinhaDeMensagem> {
    val sem = if (nova.mensagem != 0L) existentes.filter { it.mensagem != nova.mensagem } else existentes
    return (sem + nova).sortedBy { it.ts }.takeLast(max)
}

/** Mostra ou não: com o app na frente a pessoa já está vendo; `forcar` é só do receiver de teste. */
object PoliticaDePush {
    fun mostrar(emPrimeiroPlano: Boolean, forcar: Boolean = false) = forcar || !emPrimeiroPlano
}

/** O que o Android faz com o modelo (`NotificadorAndroid`); nos testes, um falso. */
interface Notificador {
    fun mostrar(modelo: ModeloDeNotificacao)
    /** Tira a notificação do canal/DM (a pessoa abriu a conversa). */
    fun limparCanal(canal: Long)
    /** Tira os avisos de pedido de amizade (a pessoa abriu a aba Pendente). */
    fun limparPedidos()
}

object NotificadorNulo : Notificador {
    override fun mostrar(modelo: ModeloDeNotificacao) = Unit
    override fun limparCanal(canal: Long) = Unit
    override fun limparPedidos() = Unit
}

/** Resultado de [ReceptorDePush.processar] (o receiver de teste imprime). */
enum class ResultadoDoPush { Mostrada, AppEmPrimeiroPlano, Invalida, Duplicada }

/**
 * Junta as decisões puras: valida, aplica a política, tira duplicatas e manda para o [Notificador]. É o MESMO caminho para o FCM e para as
 * notificações locais (WebSocket vivo): a mesma mensagem (`message_id`; pedido de amizade por autor) só vira uma notificação, venha por onde vier.
 */
class ReceptorDePush(
    private val notificador: Notificador,
    private val emPrimeiroPlano: () -> Boolean,
    private val relogio: () -> Long = System::currentTimeMillis,
) {
    private val vistos = object : LinkedHashMap<String, Unit>(256, 0.75f, false) { override fun removeEldestEntry(e: MutableMap.MutableEntry<String, Unit>?) = size > 300 }

    fun processar(dados: Map<String, String>, forcar: Boolean = false): ResultadoDoPush {
        val d = DadosDePush.de(dados) ?: return ResultadoDoPush.Invalida
        return entregar(d, forcar)
    }

    @Synchronized fun entregar(d: DadosDePush, forcar: Boolean = false, silenciosa: Boolean = false): ResultadoDoPush {
        if (!PoliticaDePush.mostrar(emPrimeiroPlano(), forcar)) return ResultadoDoPush.AppEmPrimeiroPlano
        val chave = if (d.mensagem != 0L) "m:${d.mensagem}" else "f:${d.autor}"
        if (!forcar && vistos.containsKey(chave)) return ResultadoDoPush.Duplicada
        vistos[chave] = Unit
        notificador.mostrar(ModeloDeNotificacao.de(d, relogio()).copy(silenciosa = silenciosa))
        return ResultadoDoPush.Mostrada
    }
}
