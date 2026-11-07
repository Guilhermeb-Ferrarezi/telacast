package app.telacast.data

import app.telacast.estado.AbaInicio
import app.telacast.proto.Channel
import app.telacast.proto.Dm
import app.telacast.proto.Friend
import app.telacast.proto.FriendStatus
import app.telacast.proto.ServerInfo
import app.telacast.proto.Snapshot
import app.telacast.proto.Status
import app.telacast.proto.User
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class BuscaTest {
    private val eu = User(1, "guilherme", true, Status.Online)
    private val ana = User(2, "ana", true, Status.Online)
    private val bia = User(3, "Bia.Souza", false, Status.Offline)
    private val geral = Channel(10, "geral", 0, null, 0, 0, 0)
    private val memes = Channel(11, "memes-e-risadas", 0, null, 1, 0, 0)
    private val voz = Channel(12, "sala", 1, null, 2, 0, 0)
    private val sv = ServerInfo(100, "Turma", 1, "X", listOf(geral, memes, voz), listOf(eu, ana, bia))
    private val snap = Snapshot(eu, listOf(sv), emptyList(), listOf(Dm(300, ana, 0, 0)), emptyList())

    @Test fun deEmEOResto() {
        val i = Busca.interpretar("relatório de:ana em:#memes-e-risadas final", snap, 10, 1)
        assertEquals("relatório final", i.texto)
        assertEquals(2L, i.de)
        assertEquals(11L, i.em)
    }

    @Test fun deEuEMaiusculas() {
        val i = Busca.interpretar("DE:eu oi", snap, 10, 1)
        assertEquals(1L, i.de); assertEquals("oi", i.texto)
        assertEquals(3L, Busca.interpretar("de:bia.souza", snap, 10, 1).de)
    }

    @Test fun filtroQueNaoCasaViraTexto() {
        val i = Busca.interpretar("de:fulano em:#nada em:#sala", snap, 10, 1)
        assertEquals("de:fulano em:#nada em:#sala", i.texto)
        assertNull(i.de); assertNull(i.em)
    }

    @Test fun pedidoNoServidorNoCanalOuEmOutroCanal() {
        assertEquals(PedidoDeBusca("oi", "oi", null, 100, null), Busca.pedido("oi", snap, 10, 1, escopoCanal = false))
        assertEquals(PedidoDeBusca("oi", "oi", 10, null, null), Busca.pedido("oi", snap, 10, 1, escopoCanal = true))
        assertEquals("em: vence o escopo", 11L, Busca.pedido("oi em:memes-e-risadas", snap, 10, 1, false)!!.canal)
    }

    @Test fun emDmSempreBuscaNaConversa() {
        val p = Busca.pedido("oi", snap, 300, 1, escopoCanal = false)!!
        assertEquals(300L, p.canal); assertNull(p.servidor)
    }

    @Test fun somenteDeListaMensagensDaPessoaEVazioNaoBusca() {
        assertEquals(2L, Busca.pedido("de:ana", snap, 10, 1, false)!!.de)
        assertNull(Busca.pedido("   ", snap, 10, 1, false))
        assertNull(Busca.pedido("oi", snap, null, 1, false))
    }

    // ---------------------------------------------------------------- lista de amigos (B7)
    private fun f(u: User, s: FriendStatus) = Friend(u, s)
    private val amigos = listOf(f(bia, FriendStatus.Friend), f(ana, FriendStatus.Friend), f(User(4, "carlos", true, Status.Idle), FriendStatus.Incoming), f(User(5, "duda", false, Status.Offline), FriendStatus.Outgoing))

    private fun rotulos(l: List<ItemDeAmigos>) = l.map { when (it) { is ItemDeAmigos.Cabecalho -> "#" + it.rotulo; is ItemDeAmigos.Pessoa -> it.amigo.user.name; is ItemDeAmigos.Bloqueado -> "x" + it.usuario.name } }

    @Test fun secoesPorAba() {
        assertEquals(listOf("#ONLINE — 1", "ana"), rotulos(ItensDeAmigos.montar(amigos, emptyList(), AbaInicio.Disponivel, "")))
        assertEquals(listOf("#TODOS OS AMIGOS — 2", "ana", "Bia.Souza"), rotulos(ItensDeAmigos.montar(amigos, emptyList(), AbaInicio.Todos, "")))
        assertEquals(listOf("#RECEBIDOS — 1", "carlos", "#ENVIADOS — 1", "duda"), rotulos(ItensDeAmigos.montar(amigos, emptyList(), AbaInicio.Pendente, "")))
        assertEquals(listOf("#BLOQUEADOS — 1", "xfulano"), rotulos(ItensDeAmigos.montar(amigos, listOf(User(9, "fulano", false, Status.Offline)), AbaInicio.Bloqueados, "")))
        assertEquals(emptyList<String>(), rotulos(ItensDeAmigos.montar(amigos, emptyList(), AbaInicio.Adicionar, "")))
    }

    @Test fun filtroPorNomeEEstadosVazios() {
        assertEquals(listOf("#TODOS OS AMIGOS — 1", "Bia.Souza"), rotulos(ItensDeAmigos.montar(amigos, emptyList(), AbaInicio.Todos, "souza")))
        assertEquals(emptyList<String>(), rotulos(ItensDeAmigos.montar(amigos, emptyList(), AbaInicio.Todos, "zzz")))
        assertEquals("Nenhum resultado encontrado.", textoVazioDeAmigos(AbaInicio.Todos, true))
        assertEquals("Não há ninguém bloqueado no momento.", textoVazioDeAmigos(AbaInicio.Bloqueados, false))
        assertEquals("Não há amigos disponíveis agora. Volte mais tarde!", textoVazioDeAmigos(AbaInicio.Disponivel, false))
    }

    @Test fun datasDaContaEHa() {
        assertEquals(1_767_225_600_000L, contaCriadaEm(0))
        assertEquals("agora", Formatacao.ha(1000, 900))
        assertEquals("há 5 min", Formatacao.ha(5 * 60_000L, 0))
        assertEquals("há 3 h", Formatacao.ha(3 * 3_600_000L, 0))
        assertEquals("há 2 dias", Formatacao.ha(2 * 86_400_000L + 5, 0))
    }
}
