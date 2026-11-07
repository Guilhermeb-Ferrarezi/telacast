package app.telacast.data

import app.telacast.proto.Channel
import app.telacast.proto.Snapshot
import app.telacast.proto.User

/** Interpreta a consulta da busca como o desktop (`parse_search`): `de:nome` (ou `de:eu`) e `em:#canal`; o resto é texto. */
object Busca {
    class Interpretada(val texto: String, val de: Long?, val em: Long?)

    fun interpretar(consulta: String, snap: Snapshot, canalAberto: Long?, eu: Long): Interpretada {
        val servidor = snap.servers.firstOrNull { sv -> sv.channels.any { it.id == canalAberto } }
        val membros: List<User> = servidor?.members ?: snap.dms.filter { it.channelId == canalAberto }.map { it.user }
        val canais: List<Channel> = servidor?.channels?.filter { it.kind == Channel.TEXTO }.orEmpty()
        var de: Long? = null
        var em: Long? = null
        val texto = ArrayList<String>()
        for (tok in consulta.split(Regex("\\s+")).filter { it.isNotEmpty() }) {
            val baixo = tok.lowercase()
            if (baixo.startsWith("de:")) {
                val v = baixo.removePrefix("de:")
                val quem = if (v == "eu") eu else membros.firstOrNull { it.name.lowercase() == v }?.id
                if (quem != null) { de = quem; continue }
            }
            if (baixo.startsWith("em:")) {
                val v = baixo.removePrefix("em:").trimStart('#')
                val achado = canais.firstOrNull { it.name.lowercase() == v }
                if (achado != null) { em = achado.id; continue }
            }
            texto += tok
        }
        return Interpretada(texto.joinToString(" "), de, em)
    }

    /**
     * Monta o pedido. `escopoCanal`: buscar só neste canal (em vez do servidor inteiro). `null` se não há o que buscar
     * (texto e `de:` vazios) ou se não há conversa aberta.
     */
    fun pedido(consulta: String, snap: Snapshot, canalAberto: Long?, eu: Long, escopoCanal: Boolean): PedidoDeBusca? {
        if (canalAberto == null) return null
        val i = interpretar(consulta, snap, canalAberto, eu)
        if (i.texto.isEmpty() && i.de == null) return null
        val servidor = snap.servers.firstOrNull { sv -> sv.channels.any { it.id == canalAberto } }
        val (canal, srv) = when {
            servidor != null -> when {
                i.em != null -> i.em to null
                escopoCanal -> canalAberto to null
                else -> null to servidor.id
            }
            else -> canalAberto to null // DM: sempre a própria conversa
        }
        return PedidoDeBusca(consulta, i.texto, canal, srv, i.de)
    }
}
