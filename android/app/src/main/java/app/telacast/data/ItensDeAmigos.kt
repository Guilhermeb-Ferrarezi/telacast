package app.telacast.data

import app.telacast.estado.AbaInicio
import app.telacast.proto.Friend
import app.telacast.proto.FriendStatus
import app.telacast.proto.User

sealed interface ItemDeAmigos {
    val chave: String
    data class Cabecalho(val rotulo: String) : ItemDeAmigos { override val chave get() = "c-$rotulo" }
    data class Pessoa(val amigo: Friend) : ItemDeAmigos { override val chave get() = "p-${amigo.user.id}" }
    data class Bloqueado(val usuario: User) : ItemDeAmigos { override val chave get() = "b-${usuario.id}" }
}

/** Texto do estado vazio de cada aba (do `docs/ui-spec/07`), sem ilustração. */
fun textoVazioDeAmigos(aba: AbaInicio, comFiltro: Boolean): String = when {
    comFiltro -> "Nenhum resultado encontrado."
    aba == AbaInicio.Disponivel -> "Não há amigos disponíveis agora. Volte mais tarde!"
    aba == AbaInicio.Todos -> "Não há amigos no momento. Toque em “Adicionar amigo” para enviar pedidos de amizade."
    aba == AbaInicio.Pendente -> "Não há pedidos de amizade pendentes. Toque em “Adicionar amigo” para enviar pedidos de amizade."
    aba == AbaInicio.Bloqueados -> "Não há ninguém bloqueado no momento."
    else -> ""
}

object ItensDeAmigos {
    /** Seções da lista: "ONLINE — N", "TODOS OS AMIGOS — N", "RECEBIDOS — N", "ENVIADOS — N", "BLOQUEADOS — N". Ordem por nome. */
    fun montar(amigos: List<Friend>, bloqueados: List<User>, aba: AbaInicio, filtro: String): List<ItemDeAmigos> {
        fun casa(u: User) = filtro.isBlank() || u.name.contains(filtro.trim(), ignoreCase = true)
        val porNome = compareBy<Friend> { it.user.name.lowercase() }
        val out = ArrayList<ItemDeAmigos>()
        fun secao(rotulo: String, lista: List<Friend>) {
            if (lista.isEmpty()) return
            out += ItemDeAmigos.Cabecalho("$rotulo — ${lista.size}")
            lista.sortedWith(porNome).forEach { out += ItemDeAmigos.Pessoa(it) }
        }
        when (aba) {
            AbaInicio.Disponivel -> secao("ONLINE", amigos.filter { it.status == FriendStatus.Friend && it.user.online && casa(it.user) })
            AbaInicio.Todos -> secao("TODOS OS AMIGOS", amigos.filter { it.status == FriendStatus.Friend && casa(it.user) })
            AbaInicio.Pendente -> {
                secao("RECEBIDOS", amigos.filter { it.status == FriendStatus.Incoming && casa(it.user) })
                secao("ENVIADOS", amigos.filter { it.status == FriendStatus.Outgoing && casa(it.user) })
            }
            AbaInicio.Bloqueados -> bloqueados.filter(::casa).sortedBy { it.name.lowercase() }.let { l ->
                if (l.isNotEmpty()) { out += ItemDeAmigos.Cabecalho("BLOQUEADOS — ${l.size}"); l.forEach { out += ItemDeAmigos.Bloqueado(it) } }
            }
            AbaInicio.Adicionar -> Unit
        }
        return out
    }
}
