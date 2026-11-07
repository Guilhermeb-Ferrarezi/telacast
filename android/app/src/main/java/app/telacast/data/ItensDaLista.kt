package app.telacast.data

import app.telacast.proto.Message
import java.time.ZoneId

/** Um item da lista de mensagens, já decidido (agrupamento, divisores, bloqueadas...). Puro e testável. */
sealed interface ItemDaLista {
    /** Chave estável para a lista (id da mensagem; `dia-N`; `novas`). */
    val chave: String

    data class DivisorDeDia(val rotulo: String, val dia: Long) : ItemDaLista { override val chave get() = "dia-$dia" }
    data object NovasMensagens : ItemDaLista { override val chave get() = "novas" }

    /** `agrupada`: continua o bloco do mesmo autor (sem avatar nem nome). `pendente`: ainda não confirmada (id negativo). */
    data class Mensagem(val msg: Message, val agrupada: Boolean, val pendente: Boolean) : ItemDaLista { override val chave get() = "m${msg.id}" }

    /** Aviso de sistema: alguém entrou (kind 2) ou fixou uma mensagem (kind 3). Quebra o agrupamento. */
    data class Sistema(val msg: Message) : ItemDaLista { override val chave get() = "m${msg.id}" }

    /** Mensagem de usuário bloqueado, recolhida ("Mostrar mensagem"). */
    data class Bloqueada(val msg: Message) : ItemDaLista { override val chave get() = "m${msg.id}" }
}

object ItensDaLista {
    const val JANELA_DE_AGRUPAMENTO_MS = 5 * 60 * 1000L

    /**
     * Mesmas regras do desktop (`chat_view`): divisor de dia quando o dia muda; "novas mensagens" antes da primeira
     * mensagem de OUTRA pessoa mais nova que `marcador`; agrupa mesmo autor a menos de 5 min, sem resposta no meio e
     * sem quebra de dia, de "novas" ou de aviso de sistema.
     *
     * @param mensagens em ordem cronológica (as reais, depois as provisórias; ver `AppState.lista`).
     */
    fun montar(
        mensagens: List<Message>,
        eu: Long,
        bloqueados: Set<Long>,
        reveladas: Set<Long>,
        marcador: Long?,
        agora: Long,
        zona: ZoneId = ZoneId.systemDefault(),
    ): List<ItemDaLista> {
        val out = ArrayList<ItemDaLista>(mensagens.size + 8)
        for ((i, m) in mensagens.withIndex()) {
            val anterior = mensagens.getOrNull(i - 1)
            val novoDia = anterior == null || !Formatacao.mesmoDia(anterior.ts, m.ts, zona)
            if (novoDia) out += ItemDaLista.DivisorDeDia(Formatacao.dia(m.ts, agora, zona), diaDe(m.ts, zona))
            val primeiraNova = marcador != null && m.id > marcador && m.authorId != eu && (anterior == null || anterior.id <= marcador)
            if (primeiraNova) out += ItemDaLista.NovasMensagens
            if (m.authorId in bloqueados && m.kind == 0 && m.id !in reveladas) {
                out += ItemDaLista.Bloqueada(m)
                continue
            }
            if (m.kind == 2 || m.kind == 3) {
                out += ItemDaLista.Sistema(m)
                continue
            }
            val agrupada = !novoDia && !primeiraNova && anterior != null && anterior.kind == 0 &&
                anterior.authorId == m.authorId && m.ts - anterior.ts < JANELA_DE_AGRUPAMENTO_MS && m.reply == null
            out += ItemDaLista.Mensagem(m, agrupada, pendente = m.id < 0)
        }
        return out
    }

    private fun diaDe(ts: Long, zona: ZoneId): Long = java.time.Instant.ofEpochMilli(ts).atZone(zona).toLocalDate().toEpochDay()
}
