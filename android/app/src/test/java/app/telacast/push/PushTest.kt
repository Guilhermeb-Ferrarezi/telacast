package app.telacast.push

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PushTest {
    private fun dm(extra: Map<String, String> = emptyMap()) = mapOf(
        "kind" to "dm", "channel_id" to "300", "message_id" to "9001", "server_id" to "0",
        "title" to "marina.souza", "body" to "oi, tudo bem?", "author_id" to "2", "ts" to "1794096000000",
    ) + extra

    // ---------------------------------------------------------------- leitura do FCM
    @Test fun leTodasAsChaves() {
        val d = DadosDePush.de(dm())!!
        assertEquals(DadosDePush(TipoDePush.Dm, 300, 9001, 0, "marina.souza", "oi, tudo bem?", 2, 1794096000000), d)
    }

    @Test fun reconheceOsQuatroTipos() {
        assertEquals(TipoDePush.Mencao, DadosDePush.de(dm(mapOf("kind" to "mention", "server_id" to "100")))!!.tipo)
        assertEquals(TipoDePush.Mensagem, DadosDePush.de(dm(mapOf("kind" to "message", "server_id" to "100")))!!.tipo)
        assertEquals(TipoDePush.PedidoDeAmizade, DadosDePush.de(mapOf("kind" to "friend_request", "author_id" to "5", "title" to "bruno"))!!.tipo)
    }

    @Test fun tipoNovoVaiComoMensagemComoManda11_2() {
        assertEquals(TipoDePush.Mensagem, DadosDePush.de(dm(mapOf("kind" to "sticker", "server_id" to "100")))!!.tipo)
        assertEquals(TipoDePush.Mensagem, DadosDePush.de(dm(mapOf("kind" to "", "server_id" to "100")))!!.tipo)
    }

    @Test fun semIdsNecessariosEhInvalido() {
        assertNull(DadosDePush.de(mapOf("title" to "x")))
        assertNull("dm sem canal", DadosDePush.de(dm(mapOf("channel_id" to ""))))
        assertNull("canal não numérico", DadosDePush.de(dm(mapOf("channel_id" to "abc"))))
        assertNull("autor não numérico", DadosDePush.de(dm(mapOf("author_id" to "x1"))))
        assertNull("pedido de amizade sem autor", DadosDePush.de(mapOf("kind" to "friend_request", "title" to "bruno")))
    }

    @Test fun tsEmSegundosViraMilissegundosEAusenteFicaZero() {
        assertEquals(1794096000000, DadosDePush.de(dm(mapOf("ts" to "1794096000")))!!.ts)
        assertEquals(0L, DadosDePush.de(dm(mapOf("ts" to "")))!!.ts)
        assertEquals(0L, DadosDePush.de(dm() - "ts")!!.ts)
    }

    @Test fun textoLongoEControlesSaoLimpos() {
        val d = DadosDePush.de(dm(mapOf("body" to "a".repeat(900), "title" to "  nome\u0000\u0007 ")))!!
        assertEquals(500, d.texto.length)
        assertTrue(d.texto.endsWith("…"))
        assertEquals("nome", d.titulo)
        assertEquals("a\n\nb", DadosDePush.de(dm(mapOf("body" to "a\n\n\n\n\nb")))!!.texto)
    }

    // ---------------------------------------------------------------- destino do toque
    @Test fun destinoPorTipo() {
        assertEquals(DestinoDoToque.Dm(300), DestinoDoToque.de(DadosDePush.de(dm())!!))
        assertEquals(DestinoDoToque.Canal(100, 300), DestinoDoToque.de(DadosDePush.de(dm(mapOf("kind" to "mention", "server_id" to "100")))!!))
        assertEquals("canal sem servidor cai na DM", DestinoDoToque.Dm(300), DestinoDoToque.de(DadosDePush.de(dm(mapOf("kind" to "message")))!!))
        assertEquals(DestinoDoToque.Pendentes, DestinoDoToque.de(DadosDePush.de(mapOf("kind" to "friend_request", "author_id" to "5"))!!))
    }

    @Test fun destinoVaiEVoltaPelosExtras() {
        for (d in listOf(DestinoDoToque.Dm(7), DestinoDoToque.Canal(1, 2), DestinoDoToque.Pendentes)) assertEquals(d, DestinoDoToque.deMapa(d.paraMapa()))
        assertNull(DestinoDoToque.deMapa(emptyMap()))
        assertNull(DestinoDoToque.deMapa(mapOf("destino" to "canal", "canal" to "2")))
        assertNull(DestinoDoToque.deMapa(mapOf("destino" to "dm", "canal" to "zero")))
        assertNull(DestinoDoToque.deMapa(mapOf("destino" to "dm", "canal" to "-3")))
        assertNull(DestinoDoToque.deMapa(mapOf("destino" to "?")))
    }

    // ---------------------------------------------------------------- modelo da notificação
    @Test fun canalDeNotificacaoEImportanciaPorTipo() {
        val m = { kind: String -> ModeloDeNotificacao.de(DadosDePush.de(dm(mapOf("kind" to kind, "server_id" to "1", "author_id" to "2")))!!, 5) }
        assertEquals(CanalDeNotificacao.Dms, m("dm").canal)
        assertEquals(CanalDeNotificacao.Mencoes, m("mention").canal)
        assertEquals(CanalDeNotificacao.Mensagens, m("message").canal)
        assertEquals(CanalDeNotificacao.Amizades, m("friend_request").canal)
        assertEquals(Importancia.Alta, CanalDeNotificacao.Dms.importancia)
        assertEquals(Importancia.Alta, CanalDeNotificacao.Mencoes.importancia)
        assertEquals(Importancia.Baixa, CanalDeNotificacao.Mensagens.importancia)
        assertEquals("ids únicos", 4, CanalDeNotificacao.entries.map { it.id }.toSet().size)
    }

    @Test fun colapsaPorCanalETagDoPedidoEPorAutor() {
        assertEquals("c:300", ModeloDeNotificacao.de(DadosDePush.de(dm())!!, 5).tag)
        assertEquals("duas mensagens do mesmo canal = mesma notificação", ModeloDeNotificacao.de(DadosDePush.de(dm(mapOf("message_id" to "9002")))!!, 5).tag, ModeloDeNotificacao.de(DadosDePush.de(dm())!!, 5).tag)
        val pedido = ModeloDeNotificacao.de(DadosDePush.de(mapOf("kind" to "friend_request", "author_id" to "5", "title" to "bruno"))!!, 5)
        assertEquals("f:5", pedido.tag)
        assertFalse("pedido não é conversa", pedido.conversa)
    }

    @Test fun textosPadraoQuandoOServidorNaoMandaTituloNemCorpo() {
        val m = ModeloDeNotificacao.de(DadosDePush.de(dm(mapOf("title" to "", "body" to "")))!!, 5)
        assertEquals("Nova mensagem direta", m.titulo)
        assertEquals("Toque para abrir a conversa.", m.texto)
        assertEquals("Pedido de amizade", ModeloDeNotificacao.de(DadosDePush.de(mapOf("kind" to "friend_request", "author_id" to "5"))!!, 5).titulo)
    }

    @Test fun semTsUsaAgora() {
        assertEquals(777L, ModeloDeNotificacao.de(DadosDePush.de(dm() - "ts")!!, 777)!!.ts)
        assertEquals(1794096000000, ModeloDeNotificacao.de(DadosDePush.de(dm())!!, 777).ts)
    }

    // ---------------------------------------------------------------- linhas empilhadas
    private fun l(id: Long, ts: Long, texto: String = "t$id") = LinhaDeMensagem("marina", texto, ts, id)

    @Test fun mantemAsCincoMaisNovasEmOrdemDeHora() {
        var h = emptyList<LinhaDeMensagem>()
        for (i in 1L..8L) h = acumularLinhas(h, l(i, i * 10))
        assertEquals(listOf(4L, 5L, 6L, 7L, 8L), h.map { it.mensagem })
    }

    @Test fun mensagemRepetidaNaoDuplicaEAtualiza() {
        val h = acumularLinhas(listOf(l(1, 10), l(2, 20)), l(2, 20, "editada"))
        assertEquals(listOf("t1", "editada"), h.map { it.texto })
    }

    @Test fun mensagemForaDeOrdemEntraNoLugarCerto() {
        val h = acumularLinhas(listOf(l(1, 10), l(3, 30)), l(2, 20))
        assertEquals(listOf(1L, 2L, 3L), h.map { it.mensagem })
    }

    @Test fun linhasSemIdNaoSaoDeduplicadas() {
        val h = acumularLinhas(listOf(LinhaDeMensagem("a", "x", 1)), LinhaDeMensagem("a", "x", 2))
        assertEquals(2, h.size)
    }

    // ---------------------------------------------------------------- receptor
    private class NotificadorFalso : Notificador {
        val mostradas = ArrayList<ModeloDeNotificacao>(); val limpas = ArrayList<Long>()
        override fun mostrar(modelo: ModeloDeNotificacao) { mostradas += modelo }
        override fun limparCanal(canal: Long) { limpas += canal }
        override fun limparPedidos() = Unit
    }

    @Test fun appNaFrenteNaoMostra() {
        val n = NotificadorFalso()
        assertEquals(ResultadoDoPush.AppEmPrimeiroPlano, ReceptorDePush(n, { true }, { 1 }).processar(dm()))
        assertTrue(n.mostradas.isEmpty())
    }

    @Test fun appEmSegundoPlanoMostra() {
        val n = NotificadorFalso()
        assertEquals(ResultadoDoPush.Mostrada, ReceptorDePush(n, { false }, { 1 }).processar(dm()))
        assertEquals("c:300", n.mostradas.single().tag)
    }

    @Test fun forcarMostraComAppNaFrenteEInvalidaNaoMostraNada() {
        val n = NotificadorFalso()
        val r = ReceptorDePush(n, { true }, { 1 })
        assertEquals(ResultadoDoPush.Mostrada, r.processar(dm(), forcar = true))
        assertEquals(ResultadoDoPush.Invalida, r.processar(mapOf("kind" to "dm"), forcar = true))
        assertEquals(1, n.mostradas.size)
    }

    // ---------------------------------------------------------------- título "autor em #canal (servidor)"
    @Test fun mencaoSeparaAutorEConversa() {
        val d = DadosDePush.de(dm(mapOf("kind" to "mention", "server_id" to "1", "title" to "marina.souza em #geral (Turma do Telacast)")))!!
        val m = ModeloDeNotificacao.de(d, 5)
        assertEquals("marina.souza", m.linha.autor)
        assertEquals("#geral (Turma do Telacast)", m.tituloDaConversa)
        assertEquals("o título completo segue como título da notificação", "marina.souza em #geral (Turma do Telacast)", m.titulo)
    }

    @Test fun dmEPedidoNaoTemTituloDeConversa() {
        assertEquals(null, ModeloDeNotificacao.de(DadosDePush.de(dm())!!, 5).tituloDaConversa)
        assertEquals("marina.souza", ModeloDeNotificacao.de(DadosDePush.de(dm())!!, 5).linha.autor)
        assertEquals("Maria em Paris" to null, ModeloDeNotificacao.partirTitulo("Maria em Paris"))
    }

    // ---------------------------------------------------------------- as fixtures do servidor (proto/fixtures/push)
    private fun fixture(nome: String): Map<String, String> =
        (kotlinx.serialization.json.Json.parseToJsonElement(app.telacast.proto.Fixtures.texto("push", nome)) as kotlinx.serialization.json.JsonObject)
            .mapValues { (it.value as kotlinx.serialization.json.JsonPrimitive).content }

    @Test fun fixtureDeDm() {
        val m = ModeloDeNotificacao.de(DadosDePush.de(fixture("PushData.dm.json"))!!, 1)
        assertEquals(CanalDeNotificacao.Dms, m.canal)
        assertEquals(DestinoDoToque.Dm(112623800000000001L), m.destino)
        assertEquals("c:112623800000000001", m.tag)
        assertEquals(1762512435678L, m.ts)
        assertEquals("marina.souza", m.titulo)
    }

    @Test fun fixtureDeMencao() {
        val m = ModeloDeNotificacao.de(DadosDePush.de(fixture("PushData.mention.json"))!!, 1)
        assertEquals(CanalDeNotificacao.Mencoes, m.canal)
        assertEquals(DestinoDoToque.Canal(112623700000000001L, 112623700000000012L), m.destino)
        assertEquals("#geral (Turma do Telacast)", m.tituloDaConversa)
        assertEquals("marina.souza", m.linha.autor)
        assertEquals(112624001234567905L, m.linha.mensagem)
    }

    @Test fun fixtureDePedidoDeAmizade() {
        val m = ModeloDeNotificacao.de(DadosDePush.de(fixture("PushData.friend_request.json"))!!, 1)
        assertEquals(CanalDeNotificacao.Amizades, m.canal)
        assertEquals(DestinoDoToque.Pendentes, m.destino)
        assertEquals("f:112623612862660641", m.tag)
        assertEquals("ana-clara quer ser seu amigo", m.texto)
    }
}
