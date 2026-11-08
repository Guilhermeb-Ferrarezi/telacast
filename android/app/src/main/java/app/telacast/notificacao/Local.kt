package app.telacast.notificacao

import app.telacast.data.AppState
import app.telacast.md.Bloco
import app.telacast.md.Inline
import app.telacast.md.Markdown
import app.telacast.proto.Friend
import app.telacast.proto.FriendStatus
import app.telacast.proto.Message
import app.telacast.proto.SnapshotDados
import app.telacast.proto.Status
import app.telacast.proto.User
import app.telacast.push.DadosDePush
import app.telacast.push.ReceptorDePush
import app.telacast.push.TipoDePush

/**
 * Notificações LOCAIS: as mesmas regras e a mesma formatação do push do servidor (`docs/PROTOCOL.md` 11, `server/src/push.rs`), mas
 * decididas no próprio app a partir do WebSocket vivo. Assim a notificação chega com o app em segundo plano sem Firebase. Puro e testado na JVM.
 */
object RegrasLocais {
    const val MAX_CORPO = 180

    /** Corta em [max] caracteres (não bytes); se cortou, o último é `…` (igual a `truncate_chars` do servidor). */
    fun cortar(s: String, max: Int = MAX_CORPO): String {
        if (s.codePointCount(0, s.length) <= max) return s
        val fim = s.offsetByCodePoints(0, max - 1)
        return s.substring(0, fim).trimEnd() + "…"
    }

    /** Texto simples da mensagem (`push_body`): sem marcas, spoiler vira "spoiler oculto", `<@id>` vira `@nome`, anexos viram "📎 nome (+N)", até 180 caracteres. */
    fun corpo(conteudo: String, nomeDe: (Long) -> String?, anexos: List<String>): String {
        val sb = StringBuilder()
        for (b in Markdown.parse(conteudo)) { bloco(b, nomeDe, sb); sb.append(' ') }
        var out = sb.toString().split(Regex("\\s+")).filter { it.isNotEmpty() }.joinToString(" ")
        if (anexos.isNotEmpty()) {
            if (out.isNotEmpty()) out += " "
            out += "📎 " + anexos[0] + if (anexos.size > 1) " (+${anexos.size - 1})" else ""
        }
        return cortar(out)
    }

    private fun bloco(b: Bloco, nomeDe: (Long) -> String?, o: StringBuilder) {
        when (b) {
            is Bloco.Paragrafo -> inline(b.k, nomeDe, o)
            is Bloco.Titulo -> inline(b.k, nomeDe, o)
            is Bloco.Subtexto -> inline(b.k, nomeDe, o)
            is Bloco.Citacao -> for (x in b.blocos) { bloco(x, nomeDe, o); o.append(' ') }
            is Bloco.Lista -> for (it in b.itens) { inline(it.k, nomeDe, o); o.append(' ') }
            is Bloco.Codigo -> o.append(b.texto)
        }
    }

    private fun inline(ks: List<Inline>, nomeDe: (Long) -> String?, o: StringBuilder) {
        for (i in ks) when (i) {
            is Inline.Texto -> o.append(i.s)
            is Inline.Codigo -> o.append(i.s)
            is Inline.Negrito -> inline(i.k, nomeDe, o)
            is Inline.Italico -> inline(i.k, nomeDe, o)
            is Inline.Sublinhado -> inline(i.k, nomeDe, o)
            is Inline.Tachado -> inline(i.k, nomeDe, o)
            is Inline.Spoiler -> o.append("spoiler oculto")
            is Inline.Link -> if (i.k.isEmpty()) o.append(i.url) else inline(i.k, nomeDe, o)
            is Inline.Usuario -> o.append('@').append(nomeDe(i.id) ?: "usuário")
            is Inline.Canal -> o.append("#canal")
            is Inline.Tempo -> o.append("horário")
            is Inline.Todos -> o.append(i.nome)
        }
    }

    /** Título do aviso: autor na DM; "autor em #canal (servidor)" nos canais. */
    fun titulo(dm: Boolean, autor: String, canal: String, servidor: String?): String =
        if (dm) autor else if (servidor != null) "$autor em #$canal ($servidor)" else "$autor em #$canal"

    /**
     * A mensagem ao vivo [m] gera notificação para mim? `decide` do servidor: nunca a minha, nunca de quem bloqueei, nunca em "Não perturbar";
     * DM sempre; canal só se menciona (`<@eu>`), responde a algo meu, ou `@everyone`/`@here` (estou conectado, então `@here` vale). Mensagem de
     * sistema (entrou, fixou) não conta. `null` = não notifica.
     */
    fun deMensagem(m: Message, s: AppState): DadosDePush? {
        val eu = s.eu ?: return null
        val sn = s.snapshot ?: return null
        if (m.kind != 0 || m.authorId == eu.id || eu.status == Status.Dnd) return null
        if (sn.blocked.any { it.id == m.authorId }) return null
        val dm = sn.dms.any { it.channelId == m.channelId }
        val servidor = if (dm) null else sn.servers.firstOrNull { sv -> sv.channels.any { it.id == m.channelId } } ?: return null
        val mencao = !dm && (
            m.content.contains("<@${eu.id}>") || m.content.contains("<@!${eu.id}>") || m.content.contains("@everyone") || m.content.contains("@here") ||
                (m.reply?.author == eu.name)
            )
        if (!dm && !mencao) return null
        val nomes = sn.servers.flatMap { it.members }.associate { it.id to it.name } + sn.dms.associate { it.user.id to it.user.name }
        val canalNome = servidor?.channels?.firstOrNull { it.id == m.channelId }?.name.orEmpty()
        return DadosDePush(
            if (dm) TipoDePush.Dm else TipoDePush.Mencao, m.channelId, m.id, servidor?.id ?: 0L,
            titulo(dm, m.author, canalNome, servidor?.name), corpo(m.content, { nomes[it] }, m.attachments.map { it.name }), m.authorId, m.ts,
        )
    }

    /** Pedidos de amizade que chegaram entre dois snapshots (`Incoming` que não estava no anterior). Sem snapshot anterior (primeira conexão) nada é novo. */
    fun pedidosNovos(antes: SnapshotDados?, depois: SnapshotDados): List<User> {
        if (antes == null) return emptyList()
        val tinha = antes.friends.filter { it.status == FriendStatus.Incoming }.map { it.user.id }.toSet()
        return depois.friends.filter { it.status == FriendStatus.Incoming && it.user.id !in tinha }.map(Friend::user)
    }

    fun dePedido(autor: User, agoraMs: Long) = DadosDePush(TipoDePush.PedidoDeAmizade, 0, 0, 0, "Pedido de amizade", "${autor.name} quer ser seu amigo", autor.id, agoraMs)
}

/** Anti-spam local (como a fila do servidor): 1 aviso sonoro por canal a cada 3 s (os seguintes dentro da janela só atualizam, em silêncio) e 20 sonoros por minuto. */
class AntiSpam(private val janelaMs: Long = 3_000, private val porMinuto: Int = 20, private val atualizacoesPorMinuto: Int = 40) {
    enum class Decisao { Mostrar, AtualizarEmSilencio, Descartar }

    private val ultimo = HashMap<Long, Long>()
    private val sonoros = ArrayDeque<Long>()
    private val silenciosos = ArrayDeque<Long>()

    @Synchronized fun avaliar(chave: Long, agora: Long): Decisao {
        while (sonoros.isNotEmpty() && agora - sonoros.first() >= 60_000) sonoros.removeFirst()
        while (silenciosos.isNotEmpty() && agora - silenciosos.first() >= 60_000) silenciosos.removeFirst()
        val u = ultimo[chave]
        if (u != null && agora - u < janelaMs) {
            if (silenciosos.size >= atualizacoesPorMinuto) return Decisao.Descartar
            silenciosos.addLast(agora); return Decisao.AtualizarEmSilencio
        }
        if (sonoros.size >= porMinuto) return Decisao.Descartar
        sonoros.addLast(agora); ultimo[chave] = agora
        if (ultimo.size > 500) ultimo.entries.removeAll { agora - it.value > 60_000 }
        return Decisao.Mostrar
    }
}

/**
 * Liga as regras locais ao [ReceptorDePush] (o mesmo do FCM): aplica a política "app na frente não notifica", o anti-spam e a
 * deduplicação por `message_id` (se o FCM entregar a mesma mensagem, só aparece uma notificação).
 */
class ReceptorLocal(
    private val receptor: ReceptorDePush,
    private val emPrimeiroPlano: () -> Boolean,
    private val relogio: () -> Long = System::currentTimeMillis,
    private val antiSpam: AntiSpam = AntiSpam(),
) {
    /** Mensagem ao vivo do WebSocket (nunca histórico nem snapshot). `s` é o estado já com a mensagem. */
    fun aoMensagemAoVivo(m: Message, s: AppState) {
        if (emPrimeiroPlano()) return
        val d = RegrasLocais.deMensagem(m, s) ?: return
        mostrar(d, d.canal)
    }

    fun aoPedidoDeAmizade(autor: User, s: AppState) {
        if (emPrimeiroPlano() || s.eu?.status == Status.Dnd) return
        mostrar(RegrasLocais.dePedido(autor, relogio()), -autor.id)
    }

    private fun mostrar(d: DadosDePush, chave: Long) {
        when (antiSpam.avaliar(chave, relogio())) {
            AntiSpam.Decisao.Descartar -> Unit
            AntiSpam.Decisao.Mostrar -> receptor.entregar(d, silenciosa = false)
            AntiSpam.Decisao.AtualizarEmSilencio -> receptor.entregar(d, silenciosa = true)
        }
    }
}
