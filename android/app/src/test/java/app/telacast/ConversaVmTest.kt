package app.telacast

import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import app.telacast.armazenamento.CofreEmMemoria
import app.telacast.armazenamento.PreferenciasEmMemoria
import app.telacast.arquivos.ApiFalsa
import app.telacast.arquivos.EstadoDoEnvio
import app.telacast.arquivos.FonteEmMemoria
import app.telacast.estado.Dialogo
import app.telacast.net.FakeConexao
import app.telacast.net.FakeTransporte
import app.telacast.proto.Attachment
import app.telacast.proto.C2S
import app.telacast.proto.Fixtures
import app.telacast.proto.Message
import app.telacast.proto.Proto
import app.telacast.proto.Reaction
import app.telacast.proto.S2C
import app.telacast.proto.Snapshot
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** O AppViewModel da conversa: compositor, anexos, folha, apagar, reagir e saltar, sobre o transporte de mentira. */
@OptIn(ExperimentalCoroutinesApi::class)
class ConversaVmTest {
    private val snapshot: Snapshot = (Proto.lerS2C(Fixtures.texto("s2c", "Snapshot.json")) as S2C.Snapshot).snapshot
    private val eu = snapshot.me
    private val geral = 112623700000000012L
    private val marina = 112623612862660617L

    private inner class Amb(val t: TestScope) {
        val prefs = PreferenciasEmMemoria()
        val api = ApiFalsa()
        val transporte = FakeTransporte()
        val container = Container(prefs, CofreEmMemoria(), "h:1", transporte, t.backgroundScope, { t.currentTime }, api)
        val vm = AppViewModel(container)
        val c: FakeConexao get() = transporte.ultima

        init {
            vm.editarForm { it.copy(usuario = "g", senha = "s") }; vm.entrar(); t.runCurrent()
            c.abre(); c.envia(S2C.Auth("tok", eu)); c.envia(S2C.Snapshot(snapshot)); t.runCurrent()
            c.envia(S2C.History(geral, listOf(msg(10, marina, "oi"), msg(11, eu.id, "meu texto"))))
            t.runCurrent()
        }

        fun msg(id: Long, autor: Long, texto: String, reactions: List<Reaction> = emptyList()) =
            Message(id, geral, autor, if (autor == eu.id) eu.name else "marina.souza", texto, id, false, reactions = reactions)

        fun msg(id: Long, u: app.telacast.proto.User, texto: String) = msg(id, u.id, texto)
        fun rodar() = t.runCurrent()
        val enviados get() = c.c2s
    }

    private fun TestScope.amb() = Amb(this)

    @Test fun abrirOCanalPoeOCompositorNele() = runTest {
        val a = amb()
        assertEquals(geral, a.vm.compositor.value.canal)
    }

    @Test fun enviarMensagemLimpaOCampoEMandaSend() = runTest {
        val a = amb()
        a.vm.mudarCompositor(TextFieldValue("olá!", TextRange(4)))
        a.vm.enviarMensagem(); a.rodar()
        assertEquals("olá!", (a.enviados.last() as C2S.Send).content)
        assertEquals("", a.vm.compositor.value.valor.text)
        assertEquals(1, a.vm.estado.value.pendentes.size)
    }

    @Test fun anexoEnviadoJuntoESoDepoisDeTerminarOUpload() = runTest {
        val a = amb()
        a.vm.anexar(listOf(FonteEmMemoria("foto.png", ByteArray(100), ehImagem = true))); a.rodar()
        a.vm.mudarCompositor(TextFieldValue("olha", TextRange(4)))
        a.vm.enviarMensagem(); a.rodar()
        assertTrue("upload ainda em andamento: não envia", a.enviados.none { it is C2S.Send })
        assertEquals("olha", a.vm.compositor.value.valor.text)
        val pronto = Attachment(777, "foto.png", "image/png", 100, 10, 10)
        a.api.envios.single().second.complete(pronto); a.rodar()
        a.vm.enviarMensagem(); a.rodar()
        assertEquals(listOf(777L), (a.enviados.last() as C2S.Send).attachments)
        assertTrue("a bandeja esvazia", a.vm.anexos.value.isEmpty())
    }

    @Test fun limiteDeAnexosViraAviso() = runTest {
        val a = amb()
        a.vm.anexar((1..11).map { FonteEmMemoria("f$it", ByteArray(5)) }); a.rodar()
        assertEquals("Você só pode anexar 10 arquivos por mensagem.", a.vm.aviso.value)
        assertEquals(10, a.vm.anexos.value.size)
        a.vm.removerAnexo(a.vm.anexos.value.first().chave)
        assertEquals(9, a.vm.anexos.value.size)
    }

    @Test fun anexoSoComArquivoPronto() = runTest {
        val a = amb()
        a.vm.anexar(listOf(FonteEmMemoria("a.pdf", ByteArray(5)))); a.rodar()
        a.api.envios.single().second.complete(Attachment(5, "a.pdf", "application/pdf", 5, 0, 0)); a.rodar()
        assertTrue(a.vm.anexos.value.single().estado is EstadoDoEnvio.Pronto)
        a.vm.enviarMensagem(); a.rodar() // sem texto, mas com anexo pronto
        assertEquals(listOf(5L), (a.enviados.last() as C2S.Send).attachments)
    }

    @Test fun editarPelaFolhaEnviaEditEVoltaAoRascunho() = runTest {
        val a = amb()
        a.vm.mudarCompositor(TextFieldValue("rascunho", TextRange(8)))
        a.vm.abrirFolha(a.msg(11, eu, "meu texto")); assertEquals(11L, a.vm.nav.value.folha?.mensagem)
        a.vm.editarMensagem(11)
        assertNull(a.vm.nav.value.folha)
        assertEquals("meu texto", a.vm.compositor.value.valor.text)
        a.vm.mudarCompositor(TextFieldValue("texto novo", TextRange(10)))
        a.vm.enviarMensagem()
        assertEquals(C2S.Edit(11, "texto novo"), a.enviados.last())
        assertEquals("rascunho", a.vm.compositor.value.valor.text)
    }

    @Test fun responderPelaFolhaGuardaAMensagemOriginal() = runTest {
        val a = amb()
        a.vm.responderA(10)
        assertEquals(10L, a.vm.compositor.value.respondendo?.id)
        a.vm.mudarCompositor(TextFieldValue("certo", TextRange(5))); a.vm.enviarMensagem()
        assertEquals(10L, (a.enviados.last() as C2S.Send).replyTo)
    }

    @Test fun apagarPedeConfirmacaoEDepoisManda() = runTest {
        val a = amb()
        a.vm.pedirApagar(11)
        assertEquals(Dialogo.ConfirmarApagar(11, "meu texto"), a.vm.nav.value.dialogo)
        assertTrue("ainda não apagou", a.enviados.none { it is C2S.Delete })
        a.vm.apagarMensagem(11)
        assertEquals(C2S.Delete(11), a.enviados.last())
        assertNull(a.vm.nav.value.dialogo)
    }

    @Test fun reagirDaFolhaAlternaConformeEuJaReagi() = runTest {
        val a = amb()
        a.c.envia(S2C.History(geral, listOf(a.msg(10, marina, "oi", listOf(Reaction("👍", 1, true))), a.msg(11, eu.id, "x"))))
        a.rodar()
        a.vm.reagirComEmoji(10, "👍")
        assertEquals(C2S.React(10, "👍", false), a.enviados.last())
        a.vm.reagirComEmoji(11, "🎉")
        assertEquals(C2S.React(11, "🎉", true), a.enviados.last())
    }

    @Test fun voltarFechaAFolhaAntesDaGaveta() = runTest {
        val a = amb()
        a.vm.abrirGaveta(); a.vm.abrirFolha(a.msg(10, marina, "oi"))
        a.vm.voltar(); assertNull(a.vm.nav.value.folha); assertTrue(a.vm.nav.value.gavetaAberta)
    }

    @Test fun spoilersTextosEBloqueadasSaoLocais() = runTest {
        val a = amb()
        a.vm.alternarSpoiler(10, 0); assertEquals(setOf(10L to 0), a.vm.spoilers.value)
        a.vm.alternarSpoiler(10, 0); assertTrue(a.vm.spoilers.value.isEmpty())
        a.vm.alternarTexto(5); assertEquals(setOf(5L), a.vm.textosAbertos.value)
        a.vm.mostrarBloqueada(10); assertEquals(setOf(10L), a.vm.reveladas.value)
    }

    @Test fun saltarPedePaginasAntigasDoCanalAberto() = runTest {
        val a = amb()
        a.c.envia(S2C.History(geral, (1000L..1049L).map { a.msg(it, marina, "m") })); a.rodar()
        a.vm.saltarPara(5)
        assertEquals(C2S.History(geral, 1000), a.enviados.last())
    }

    @Test fun urlDoAnexoUsaOServidorEOToken() = runTest {
        val a = amb()
        assertEquals("http://h:1/files/9/a%20b.png?token=tok", a.vm.urlDoAnexo(Attachment(9, "a b.png", "image/png", 1, 1, 1)))
    }

    // ---------------------------------------------------------------- B7/B8

    @Test fun perfilAbreFechaEVoltaFechaPrimeiro() = runTest {
        val a = amb()
        a.vm.abrirGaveta(); a.vm.abrirPerfil(marina, "marina.souza")
        assertEquals(marina, a.vm.nav.value.perfil?.usuario)
        a.vm.voltar()
        assertNull(a.vm.nav.value.perfil); assertTrue(a.vm.nav.value.gavetaAberta)
        a.vm.abrirPerfilDe(marina); assertEquals("marina.souza", a.vm.nav.value.perfil?.nome)
    }

    @Test fun conversarFechaOPerfilEPedeADm() = runTest {
        val a = amb()
        a.vm.abrirPerfil(marina, "m"); a.vm.conversarCom(marina)
        assertNull(a.vm.nav.value.perfil)
        assertEquals(C2S.OpenDm(marina), a.enviados.last())
    }

    @Test fun bloquearPedeConfirmacaoDesbloquearVaiDireto() = runTest {
        val a = amb()
        a.vm.abrirPerfil(marina, "marina.souza"); a.vm.pedirBloquear(marina, "marina.souza")
        assertNull("o perfil dá lugar ao diálogo", a.vm.nav.value.perfil)
        assertEquals(Dialogo.ConfirmarBloquear(marina, "marina.souza"), a.vm.nav.value.dialogo)
        assertTrue(a.enviados.none { it is C2S.Block })
        a.vm.bloquear(marina)
        assertEquals(C2S.Block(marina), a.enviados.last()); assertNull(a.vm.nav.value.dialogo)
        a.vm.desbloquear(marina); assertEquals(C2S.Unblock(marina), a.enviados.last())
    }

    @Test fun desfazerAmizadePedeConfirmacao() = runTest {
        val a = amb()
        a.vm.pedirDesfazerAmizade(marina, "marina.souza")
        assertEquals(Dialogo.ConfirmarRemoverAmigo(marina, "marina.souza"), a.vm.nav.value.dialogo)
        a.vm.desfazerAmizade(marina)
        assertEquals(C2S.FriendRemove(marina), a.enviados.last())
    }

    @Test fun adicionarAmigoAvisa() = runTest {
        val a = amb()
        a.vm.adicionarAmigo("ana"); a.rodar()
        assertEquals(C2S.FriendRequest("ana"), a.enviados.last())
        assertEquals("Pedido de amizade enviado", a.vm.aviso.value)
    }

    @Test fun fixadasPedeAListaEAbreATela() = runTest {
        val a = amb()
        a.vm.abrirFixadas()
        assertEquals(C2S.Pins(geral), a.enviados.last())
        assertEquals(Dialogo.Fixadas, a.vm.nav.value.dialogo)
    }

    @Test fun buscaUsaOEscopoEAsFiltros() = runTest {
        val a = amb()
        a.vm.abrirBusca()
        a.vm.mudarConsultaDaBusca("relatório de:eu")
        a.vm.buscar()
        val s1 = a.enviados.last() as C2S.Search
        assertEquals(eu.id, s1.from); assertEquals("relatório", s1.text); assertEquals(null, s1.channelId); assertEquals(112623700000000001L, s1.serverId)
        a.vm.mudarEscopoDaBusca(true); a.vm.buscar()
        assertEquals(geral, (a.enviados.last() as C2S.Search).channelId)
        a.vm.mudarConsultaDaBusca("   "); val n = a.enviados.size; a.vm.buscar(); assertEquals("vazio não envia", n, a.enviados.size)
        a.vm.fecharTelaCheia(); assertNull(a.vm.nav.value.dialogo); assertFalse(a.vm.estado.value.busca.buscou)
    }

    @Test fun irParaMensagemDeOutroCanalAbreOCanal() = runTest {
        val a = amb()
        val memes = 112623700000000013L
        a.vm.abrirBusca()
        a.vm.irParaMensagem(memes, 4242)
        assertNull(a.vm.nav.value.dialogo)
        assertEquals(memes, a.vm.estado.value.canalAberto)
        assertEquals(memes to 4242L, a.vm.estado.value.saltoAgendado)
    }

    @Test fun ajustesPedemSessoesESenhaTrocadaAvisa() = runTest {
        val a = amb()
        a.vm.abrirAjustes()
        assertEquals(C2S.ListSessions, a.enviados.last()); assertEquals(Dialogo.Ajustes, a.vm.nav.value.dialogo)
        a.vm.trocarSenha("velha123", "nova12345"); assertEquals(C2S.ChangePassword("velha123", "nova12345"), a.enviados.last())
        a.vm.encerrarSessao(7); assertEquals(C2S.RevokeSession(7), a.enviados.last())
        a.vm.encerrarOutrasSessoes(); assertEquals(C2S.RevokeOtherSessions, a.enviados.last())
        val antes = a.vm.senhaTrocada.value
        a.c.envia(S2C.PasswordChanged); a.rodar()
        assertEquals(antes + 1, a.vm.senhaTrocada.value)
    }

    @Test fun tamanhoDoTextoFicaSalvo() = runTest {
        val a = amb()
        assertEquals(1, a.vm.tamanhoDoTexto.value)
        a.vm.definirTamanhoDoTexto(2); assertEquals(2, a.prefs.tamanhoDoTexto)
        a.vm.definirTamanhoDoTexto(9); assertEquals(2, a.vm.tamanhoDoTexto.value)
        assertEquals(0.9f, app.telacast.ui.escalaDoTexto(0), 0.001f)
    }
}
