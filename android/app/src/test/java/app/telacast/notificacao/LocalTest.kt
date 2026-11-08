package app.telacast.notificacao

import app.telacast.Amostras
import app.telacast.data.AppState
import app.telacast.proto.Attachment
import app.telacast.proto.Friend
import app.telacast.proto.FriendStatus
import app.telacast.proto.Message
import app.telacast.proto.Reply
import app.telacast.proto.Status
import app.telacast.proto.User
import app.telacast.push.CanalDeNotificacao
import app.telacast.push.ModeloDeNotificacao
import app.telacast.push.Notificador
import app.telacast.push.ReceptorDePush
import app.telacast.push.ResultadoDoPush
import app.telacast.push.TipoDePush
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class LocalTest {
    private val eu = Amostras.eu            // id 1 "guilherme"
    private val marina = Amostras.marina    // id 2
    private val estado = Amostras.estado
    private val canalTexto = Amostras.AVISOS
    private val dm = Amostras.DM_MARINA

    private fun msg(canal: Long, texto: String, autor: User = marina, id: Long = 9000, kind: Int = 0, reply: Reply? = null, anexos: List<Attachment> = emptyList()) =
        Message(id, canal, autor.id, autor.name, texto, 1_794_000_000_000, false, reply = reply, kind = kind, attachments = anexos)

    // ---------------------------------------------------------------- texto (push_body)
    private fun corpo(t: String, anexos: List<String> = emptyList()) = RegrasLocais.corpo(t, { mapOf(1L to "guilherme", 2L to "marina.souza")[it] }, anexos)

    @Test fun corpoTiraMarcasDeMarkdown() {
        assertEquals("oi tudo bem? não ok", corpo("oi **tudo** *bem*? ~~não~~ ok"))
        assertEquals("spoiler oculto aqui", corpo("||segredo|| aqui"))
        assertEquals("código e citação lista um dois", corpo("`código` e\n> citação\n- lista\n# um\n-# dois"))
    }

    @Test fun corpoMencaoViraNomeEJuntaLinhas() {
        assertEquals("@guilherme olha isso", corpo("<@1>   olha\n\n  isso"))
        assertEquals("@usuário olha", corpo("<@999> olha"))
        assertEquals("veja #canal horário", corpo("veja <#5> <t:1700000000:R>"))
        assertEquals("@everyone atenção", corpo("@everyone atenção"))
    }

    @Test fun corpoComAnexos() {
        assertEquals("foto 📎 praia.png", corpo("foto", listOf("praia.png")))
        assertEquals("📎 primeiro.png (+2)", corpo("", listOf("primeiro.png", "b.png", "c.png")))
    }

    @Test fun corpoCortaEm180CaracteresNaoEmBytes() {
        val c = corpo("á".repeat(300))
        assertEquals(180, c.codePointCount(0, c.length)); assertTrue(c.endsWith("…"))
        assertEquals(180, corpo("😀".repeat(500)).let { it.codePointCount(0, it.length) })
        assertEquals("curto", corpo("curto"))
    }

    @Test fun titulos() {
        assertEquals("marina", RegrasLocais.titulo(true, "marina", "x", null))
        assertEquals("marina em #geral (Turma)", RegrasLocais.titulo(false, "marina", "geral", "Turma"))
    }

    // ---------------------------------------------------------------- quem recebe notificação (decide)
    @Test fun dmSempreNotifica() {
        val d = RegrasLocais.deMensagem(msg(dm, "oi **você**"), estado)!!
        assertEquals(TipoDePush.Dm, d.tipo); assertEquals("marina.souza", d.titulo); assertEquals("oi você", d.texto); assertEquals(dm, d.canal); assertEquals(0L, d.servidor)
    }

    @Test fun canalSemMencaoNaoNotifica() = assertNull(RegrasLocais.deMensagem(msg(canalTexto, "bom dia"), estado))

    @Test fun mencaoDiretaNotificaComTituloDeCanal() {
        val d = RegrasLocais.deMensagem(msg(canalTexto, "<@1> olha isso"), estado)!!
        assertEquals(TipoDePush.Mencao, d.tipo)
        assertEquals("marina.souza em #avisos (Turma do Telacast)", d.titulo); assertEquals("@guilherme olha isso", d.texto); assertEquals(Amostras.SV_TURMA, d.servidor)
        assertNotNull(RegrasLocais.deMensagem(msg(canalTexto, "<@!1> oi"), estado))
    }

    @Test fun everyoneEHereNotificam() {
        assertNotNull(RegrasLocais.deMensagem(msg(canalTexto, "@everyone reunião"), estado))
        assertNotNull(RegrasLocais.deMensagem(msg(canalTexto, "@here reunião"), estado))
    }

    @Test fun respostaAMensagemMinhaNotifica() {
        assertNotNull(RegrasLocais.deMensagem(msg(canalTexto, "concordo", reply = Reply(5, "guilherme", "bom dia")), estado))
        assertNull("resposta a outra pessoa não", RegrasLocais.deMensagem(msg(canalTexto, "concordo", reply = Reply(5, "joao_pedro", "bom dia")), estado))
    }

    @Test fun mencaoAOutraPessoaNaoNotifica() = assertNull(RegrasLocais.deMensagem(msg(canalTexto, "<@3> oi"), estado))

    @Test fun nuncaMinhaNemDeBloqueadoNemEmNaoPerturbe() {
        assertNull("minha", RegrasLocais.deMensagem(msg(dm, "oi", autor = eu), estado))
        val bloq = estado.copy(snapshot = estado.snapshot!!.copy(blocked = listOf(marina)))
        assertNull("bloqueado (DM)", RegrasLocais.deMensagem(msg(dm, "oi"), bloq)); assertNull("bloqueado (menção)", RegrasLocais.deMensagem(msg(canalTexto, "<@1>"), bloq))
        val dnd = estado.copy(eu = eu.copy(status = Status.Dnd))
        assertNull("Não perturbe (DM)", RegrasLocais.deMensagem(msg(dm, "oi"), dnd)); assertNull("Não perturbe (menção)", RegrasLocais.deMensagem(msg(canalTexto, "<@1>"), dnd))
    }

    @Test fun mensagemDeSistemaECanalDesconhecidoNaoNotificam() {
        assertNull(RegrasLocais.deMensagem(msg(dm, "entrou", kind = 2), estado))
        assertNull(RegrasLocais.deMensagem(msg(777777, "<@1> oi"), estado))
        assertNull(RegrasLocais.deMensagem(msg(dm, "oi"), AppState()))
    }

    @Test fun anexoViraClipeNoCorpo() {
        val d = RegrasLocais.deMensagem(msg(dm, "olha", anexos = listOf(Attachment(1, "praia.png", "image/png", 10, 10, 10), Attachment(2, "b.pdf", "x", 1, 0, 0))), estado)!!
        assertEquals("olha 📎 praia.png (+1)", d.texto)
    }

    // ---------------------------------------------------------------- pedidos de amizade
    @Test fun pedidoNovoSoEntreDoisSnapshots() {
        val antes = estado.snapshot!!
        assertTrue("primeira conexão: nada é novo", RegrasLocais.pedidosNovos(null, antes).isEmpty())
        val ana = User(50, "ana.nova", true, Status.Online)
        val depois = antes.copy(friends = antes.friends + Friend(ana, FriendStatus.Incoming))
        assertEquals(listOf(ana), RegrasLocais.pedidosNovos(antes, depois))
        assertTrue("o que já estava não se repete", RegrasLocais.pedidosNovos(depois, depois).isEmpty())
        assertTrue("pedido que eu mandei não conta", RegrasLocais.pedidosNovos(antes, antes.copy(friends = antes.friends + Friend(ana, FriendStatus.Outgoing))).isEmpty())
        val d = RegrasLocais.dePedido(ana, 5)
        assertEquals(TipoDePush.PedidoDeAmizade, d.tipo); assertEquals("ana.nova quer ser seu amigo", d.texto); assertEquals(50L, d.autor)
    }

    // ---------------------------------------------------------------- anti-spam
    @Test fun antiSpamUmSonoroPorCanalACada3s() {
        val a = AntiSpam()
        assertEquals(AntiSpam.Decisao.Mostrar, a.avaliar(1, 0))
        assertEquals(AntiSpam.Decisao.AtualizarEmSilencio, a.avaliar(1, 1_000))
        assertEquals(AntiSpam.Decisao.AtualizarEmSilencio, a.avaliar(1, 2_900))
        assertEquals("outro canal tem a sua janela", AntiSpam.Decisao.Mostrar, a.avaliar(2, 1_500))
        assertEquals("depois de 3 s volta a tocar", AntiSpam.Decisao.Mostrar, a.avaliar(1, 3_100))
    }

    @Test fun antiSpamVinteSonorosPorMinuto() {
        val a = AntiSpam()
        for (c in 1L..20L) assertEquals(AntiSpam.Decisao.Mostrar, a.avaliar(c, c * 100))
        assertEquals(AntiSpam.Decisao.Descartar, a.avaliar(21, 2_500))
        assertEquals("passado um minuto libera", AntiSpam.Decisao.Mostrar, a.avaliar(22, 62_000))
    }

    // ---------------------------------------------------------------- receptor: política, deduplicação, silêncio
    private class Falso : Notificador {
        val mostradas = ArrayList<ModeloDeNotificacao>()
        override fun mostrar(modelo: ModeloDeNotificacao) { mostradas += modelo }
        override fun limparCanal(canal: Long) = Unit
        override fun limparPedidos() = Unit
    }

    private class Amb(var frente: Boolean = false, var t: Long = 1_794_000_000_000) {
        val n = Falso()
        val push = ReceptorDePush(n, { frente }, { t })
        val local = ReceptorLocal(push, { frente }, { t })
    }

    @Test fun appNaFrenteNaoNotifica() {
        val a = Amb(frente = true)
        a.local.aoMensagemAoVivo(msg(dm, "oi"), estado)
        assertTrue(a.n.mostradas.isEmpty())
    }

    @Test fun emSegundoPlanoNotificaComOCanalCertoEAAgrupa() {
        val a = Amb()
        a.local.aoMensagemAoVivo(msg(dm, "oi"), estado)
        val m = a.n.mostradas.single()
        assertEquals(CanalDeNotificacao.Dms, m.canal); assertEquals("c:$dm", m.tag); assertEquals("marina.souza", m.titulo); assertFalse(m.silenciosa)
        a.t += 5_000; a.local.aoMensagemAoVivo(msg(dm, "tudo bem?", id = 9001), estado)
        assertEquals("a segunda do mesmo canal usa a mesma tag (empilha)", a.n.mostradas[0].tag, a.n.mostradas[1].tag)
        assertFalse(a.n.mostradas[1].silenciosa)
    }

    @Test fun dentroDaJanelaDe3sAtualizaSemSom() {
        val a = Amb()
        a.local.aoMensagemAoVivo(msg(dm, "oi"), estado)
        a.t += 1_000; a.local.aoMensagemAoVivo(msg(dm, "mais uma", id = 9001), estado)
        assertEquals(2, a.n.mostradas.size); assertTrue(a.n.mostradas[1].silenciosa)
    }

    @Test fun omesmoMessageIdNaoDuplicaVindoDoFcmDepoisOuAntes() {
        val a = Amb()
        a.local.aoMensagemAoVivo(msg(dm, "oi", id = 9500), estado)
        val r = a.push.processar(mapOf("kind" to "dm", "channel_id" to "$dm", "message_id" to "9500", "title" to "marina.souza", "body" to "oi", "author_id" to "2", "ts" to "1794000000000"))
        assertEquals(ResultadoDoPush.Duplicada, r); assertEquals(1, a.n.mostradas.size)
        // e ao contrário: o FCM chegou primeiro
        val b = Amb()
        b.push.processar(mapOf("kind" to "dm", "channel_id" to "$dm", "message_id" to "9600", "title" to "marina.souza", "body" to "oi", "author_id" to "2", "ts" to "1"))
        b.local.aoMensagemAoVivo(msg(dm, "oi", id = 9600), estado)
        assertEquals(1, b.n.mostradas.size)
    }

    @Test fun pedidoDeAmizadeNotificaUmaVezEDndBloqueia() {
        val ana = User(50, "ana.nova", true, Status.Online)
        val a = Amb()
        a.local.aoPedidoDeAmizade(ana, estado); a.t += 10_000; a.local.aoPedidoDeAmizade(ana, estado)
        assertEquals("o mesmo pedido não repete", 1, a.n.mostradas.size)
        assertEquals(CanalDeNotificacao.Amizades, a.n.mostradas[0].canal)
        val b = Amb(); b.local.aoPedidoDeAmizade(ana, estado.copy(eu = eu.copy(status = Status.Dnd)))
        assertTrue(b.n.mostradas.isEmpty())
    }
}
