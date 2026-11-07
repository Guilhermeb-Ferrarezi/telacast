package app.telacast.data

import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import app.telacast.armazenamento.PreferenciasEmMemoria
import app.telacast.net.Credencial
import app.telacast.net.FakeTransporte
import app.telacast.net.Gateway
import app.telacast.proto.Attachment
import app.telacast.proto.C2S
import app.telacast.proto.Message
import app.telacast.proto.S2C
import app.telacast.proto.Status
import app.telacast.proto.User
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class CompositorTest {
    private val eu = User(1, "guilherme", true, Status.Online)
    private val marina = User(2, "marina.souza", true, Status.Idle)
    private val marcos = User(3, "marcos", false, Status.Offline)
    private val ana = User(4, "ana", true, Status.Online)
    private val membros = listOf(eu, marina, marcos, ana)

    // ---------------------------------------------------------------- autocompletar

    @Test fun achaOTokenColadoAoCursor() {
        val s = Autocompletar.sugestoes("oi @ma", 6, membros, eu.id)!!
        assertEquals(3, s.inicio)
        assertEquals("ma", s.consulta)
        assertEquals(listOf("marina.souza", "marcos"), s.itens.map { it.nome }) // online antes
    }

    @Test fun arrobaSozinhoListaTodosMenosEuEMaisEveryoneHere() {
        val s = Autocompletar.sugestoes("@", 1, membros, eu.id)!!
        assertEquals(listOf("ana", "marina.souza", "marcos", "everyone", "here"), s.itens.map { it.nome })
        assertTrue(s.itens.any { it.usuario == null && it.nome == "everyone" })
        assertFalse(s.itens.any { it.nome == "guilherme" })
    }

    @Test fun naoSugereEmEmailNemSemArroba() {
        assertNull(Autocompletar.sugestoes("fulano@ma", 9, membros, eu.id))
        assertNull(Autocompletar.sugestoes("sem arroba", 10, membros, eu.id))
        assertNull(Autocompletar.sugestoes("@zzz", 4, membros, eu.id))
        assertNull(Autocompletar.sugestoes("@ana oi", 7, membros, eu.id))
    }

    @Test fun usaOCursorNoMeioDoTexto() {
        val s = Autocompletar.sugestoes("@ma e mais texto", 3, membros, eu.id)!!
        assertEquals("ma", s.consulta)
        val (t, c) = Autocompletar.aplicar("@ma e mais texto", 3, s, "marina.souza")
        assertEquals("@marina.souza  e mais texto", t)
        assertEquals(14, c)
    }

    @Test fun everyoneCasaPeloPrefixo() {
        val s = Autocompletar.sugestoes("@eve", 4, membros, eu.id)!!
        assertEquals(listOf("everyone"), s.itens.map { it.nome })
    }

    // ---------------------------------------------------------------- controlador

    private inner class Amb(val t: TestScope) {
        val prefs = PreferenciasEmMemoria()
        val transporte = FakeTransporte()
        val gateway = Gateway(t.backgroundScope, transporte, relogio = { t.currentTime })
        val repo = Repository(t.backgroundScope, gateway, relogio = { t.currentTime })
        val comp = ControladorDoCompositor(repo, prefs)
        val enviados get() = transporte.ultima.c2s

        init {
            repo.entrar("h", Credencial.Login("guilherme", "x")); t.runCurrent()
            transporte.ultima.abre(); transporte.ultima.envia(S2C.Auth("tok", eu)); t.runCurrent()
        }
    }

    private fun msg(id: Long, autor: User = eu, texto: String = "t$id") = Message(id, 10, autor.id, autor.name, texto, 0, false)
    private fun TestScope.amb() = Amb(this)

    @Test fun rascunhoPorCanalFicaGuardadoEVoltaAoTrocar() = runTest {
        val a = amb()
        a.comp.abrirCanal(10); a.comp.mudar(TextFieldValue("rascunho do dez"))
        a.comp.abrirCanal(20)
        assertEquals("", a.comp.estado.value.valor.text)
        a.comp.mudar(TextFieldValue("outro"))
        a.comp.abrirCanal(10)
        assertEquals("rascunho do dez", a.comp.estado.value.valor.text)
        assertEquals("outro", a.prefs.rascunho(20))
        assertEquals("o cursor vai para o fim", 15, a.comp.estado.value.valor.selection.end)
    }

    @Test fun enviarLimpaCampoERascunhoELevaAResposta() = runTest {
        val a = amb(); a.comp.abrirCanal(10)
        a.comp.responder(msg(99, marina, "x".repeat(300)))
        assertEquals(120, a.comp.estado.value.respondendo!!.content.length)
        a.comp.mudar(TextFieldValue("concordo"))
        assertTrue(a.comp.enviar())
        val send = a.enviados.last() as C2S.Send
        assertEquals("concordo", send.content)
        assertEquals(99L, send.replyTo)
        assertEquals("", a.comp.estado.value.valor.text)
        assertNull(a.comp.estado.value.respondendo)
        assertEquals("", a.prefs.rascunho(10))
    }

    @Test fun naoEnviaVazioMasEnviaSoComAnexo() = runTest {
        val a = amb(); a.comp.abrirCanal(10)
        a.comp.mudar(TextFieldValue("   "))
        assertFalse(a.comp.enviar())
        val anexo = Attachment(5, "f.png", "image/png", 1, 1, 1)
        assertTrue(a.comp.enviar(listOf(anexo)))
        assertEquals(listOf(5L), (a.enviados.last() as C2S.Send).attachments)
    }

    @Test fun editarPoeOTextoNoCampoESalvaEnviaEditEVoltaAoRascunho() = runTest {
        val a = amb(); a.comp.abrirCanal(10)
        a.comp.mudar(TextFieldValue("rascunho"))
        a.comp.editar(msg(7, texto = "texto original"))
        assertEquals(7L, a.comp.estado.value.editando)
        assertEquals("texto original", a.comp.estado.value.valor.text)
        a.comp.mudar(TextFieldValue("texto corrigido"))
        assertEquals("editar não vira rascunho", "rascunho", a.prefs.rascunho(10))
        assertTrue(a.comp.enviar())
        assertEquals(C2S.Edit(7, "texto corrigido"), a.enviados.last())
        assertNull(a.comp.estado.value.editando)
        assertEquals("rascunho", a.comp.estado.value.valor.text)
    }

    @Test fun cancelarEdicaoRestauraORascunhoESemEdicaoNaoFazNada() = runTest {
        val a = amb(); a.comp.abrirCanal(10)
        a.comp.mudar(TextFieldValue("meu rascunho"))
        a.comp.editar(msg(7))
        a.comp.cancelarEdicao()
        assertEquals("meu rascunho", a.comp.estado.value.valor.text)
        a.comp.cancelarEdicao()
        assertEquals("meu rascunho", a.comp.estado.value.valor.text)
    }

    @Test fun editarVazioNaoEnvia() = runTest {
        val a = amb(); a.comp.abrirCanal(10)
        a.comp.editar(msg(7)); a.comp.mudar(TextFieldValue("  "))
        assertFalse(a.comp.enviar())
        assertEquals(7L, a.comp.estado.value.editando)
    }

    @Test fun typingSaiNoMaximoACadaQuatroSegundos() = runTest {
        val a = amb(); a.comp.abrirCanal(10)
        a.comp.mudar(TextFieldValue("o")); a.comp.mudar(TextFieldValue("oi")); a.comp.mudar(TextFieldValue("oi!"))
        assertEquals(1, a.enviados.count { it is C2S.Typing })
        a.t.advanceTimeBy(4_000); a.comp.mudar(TextFieldValue("oi!!"))
        assertEquals(2, a.enviados.count { it is C2S.Typing })
        a.comp.mudar(TextFieldValue("oi!!", TextRange(1))) // só mexeu o cursor: não conta
        assertEquals(2, a.enviados.count { it is C2S.Typing })
        a.comp.mudar(TextFieldValue(""))
        assertEquals("apagar tudo não avisa", 2, a.enviados.count { it is C2S.Typing })
    }

    @Test fun completarMencaoTrocaOTokenNoCursor() = runTest {
        val a = amb(); a.comp.abrirCanal(10)
        a.comp.mudar(TextFieldValue("oi @ma", TextRange(6)))
        val s = Autocompletar.sugestoes("oi @ma", 6, membros, eu.id)!!
        a.comp.completarMencao(s, "marina.souza")
        assertEquals("oi @marina.souza ", a.comp.estado.value.valor.text)
        assertEquals(17, a.comp.estado.value.valor.selection.end)
    }
}
