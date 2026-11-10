package app.telacast.chamada

import app.telacast.Amostras
import app.telacast.data.AppState
import app.telacast.proto.Status
import app.telacast.proto.VoiceState
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ChamadasTest {
    private val eu = Amostras.eu.id            // 1
    private val marina = Amostras.marina.id    // 2
    private val dm = Amostras.DM_MARINA        // canal 300 com marina

    private class ToqueFalso : ToqueDeChamada {
        val eventos = ArrayList<String>()
        override fun tocarRecebendo() { eventos += "recebendo" }
        override fun tocarChamando() { eventos += "chamando" }
        override fun parar() { eventos += "parar" }
        override fun fim() { eventos += "fim" }
    }
    private class NotificadorFalso : NotificadorDeChamada {
        val eventos = ArrayList<String>()
        override fun chamadaRecebida(canal: Long, nome: String) { eventos += "recebida:$nome" }
        override fun chamadaPerdida(canal: Long, nome: String) { eventos += "perdida:$nome" }
        override fun limpar(canal: Long) { eventos += "limpar" }
    }

    private inner class Amb(val t: TestScope, var frente: Boolean = false) {
        val app = MutableStateFlow(Amostras.estado.copy(autenticado = true, voz = emptyList()))
        val toque = ToqueFalso(); val notif = NotificadorFalso(); val avisos = ArrayList<String>(); var saiu = 0
        val c = ControladorDeChamadasEmDm(t.backgroundScope, app, { saiu++ }, { avisos += it }, toque, notif, { frente }, { t.currentTime })
        init { t.runCurrent() }
        fun voz(vararg us: Long) { app.value = app.value.copy(voz = us.map { VoiceState(it, dm, false, false) }); t.runCurrent() }
    }

    private fun TestScope.amb(frente: Boolean = false) = Amb(this, frente)

    @Test fun oOutroLigaEuRecebo() = runTest {
        val a = amb(); a.voz(marina)
        val e = a.c.estado.value as EstadoDaChamada.Recebendo
        assertEquals(dm, e.canal); assertEquals("marina.souza", e.nome); assertEquals(marina, e.usuario)
        assertEquals(listOf("recebendo"), a.toque.eventos); assertEquals(listOf("recebida:marina.souza"), a.notif.eventos)
    }

    @Test fun comOAppNaFrenteTocaMasNaoNotifica() = runTest {
        val a = amb(frente = true); a.voz(marina)
        assertEquals(listOf("recebendo"), a.toque.eventos); assertTrue(a.notif.eventos.isEmpty())
    }

    @Test fun atenderParaOToqueELimpaANotificacao() = runTest {
        val a = amb(); a.voz(marina); a.voz(marina, eu)
        assertEquals(EstadoDaChamada.Nenhuma, a.c.estado.value)
        assertTrue(a.toque.eventos.contains("parar")); assertTrue(a.notif.eventos.contains("limpar"))
    }

    @Test fun oOutroDesligaAntesDeEuAtenderViraChamadaPerdida() = runTest {
        val a = amb(); a.voz(marina); a.voz()
        assertEquals(EstadoDaChamada.Nenhuma, a.c.estado.value)
        assertEquals(listOf("recebida:marina.souza", "limpar", "perdida:marina.souza"), a.notif.eventos)
        assertEquals(listOf("Chamada perdida de marina.souza."), a.avisos)
    }

    @Test fun sessentaSegundosSemAtenderTambemEPerdida() = runTest {
        val a = amb(); a.voz(marina)
        a.t.advanceTimeBy(59_000); a.t.runCurrent(); assertTrue(a.c.estado.value is EstadoDaChamada.Recebendo)
        a.t.advanceTimeBy(2_000); a.t.runCurrent()
        assertEquals(EstadoDaChamada.Nenhuma, a.c.estado.value); assertEquals(1, a.avisos.size); assertTrue(a.toque.eventos.contains("parar"))
    }

    @Test fun recusarParaOToqueMasNaoVoltaATocarNaMesmaChamada() = runTest {
        val a = amb(); a.voz(marina)
        a.c.recusar(dm)
        assertEquals(EstadoDaChamada.Nenhuma, a.c.estado.value); assertTrue(a.toque.eventos.last() == "parar")
        a.voz(marina, 3) // mais alguém, mesma chamada: não volta a tocar
        assertEquals(EstadoDaChamada.Nenhuma, a.c.estado.value)
        a.voz(); a.voz(marina) // sala esvaziou e ela ligou de novo: toca
        assertTrue(a.c.estado.value is EstadoDaChamada.Recebendo)
    }

    @Test fun euLigoEEsperoEQuandoAtendemParaDeChamar() = runTest {
        val a = amb(); a.voz(eu)
        assertTrue(a.c.estado.value is EstadoDaChamada.Chamando); assertEquals(listOf("chamando"), a.toque.eventos)
        a.voz(eu, marina)
        assertEquals(EstadoDaChamada.Nenhuma, a.c.estado.value); assertEquals(0, a.saiu)
    }

    @Test fun ligarEMinutoSemAtenderSaiDaChamadaSozinho() = runTest {
        val a = amb(); a.voz(eu)
        a.t.advanceTimeBy(61_000); a.t.runCurrent()
        assertEquals(1, a.saiu); assertEquals(EstadoDaChamada.Nenhuma, a.c.estado.value)
        assertEquals(listOf("A ligação expirou: ninguém atendeu em 1 minuto."), a.avisos); assertTrue(a.toque.eventos.contains("fim"))
    }

    @Test fun desistirDeLigarLimpaOEstado() = runTest {
        val a = amb(); a.voz(eu); a.voz()
        assertEquals(EstadoDaChamada.Nenhuma, a.c.estado.value)
    }

    @Test fun naoTocaEmNaoPerturbe() = runTest {
        val a = amb(); a.app.value = a.app.value.copy(eu = Amostras.eu.copy(status = Status.Dnd)); a.t.runCurrent()
        a.voz(marina)
        assertTrue("a faixa aparece", a.c.estado.value is EstadoDaChamada.Recebendo)
        assertTrue("mas sem toque nem notificação", a.toque.eventos.isEmpty() && a.notif.eventos.isEmpty())
    }

    @Test fun quemJaEstavaNaSalaNoPrimeiroEstadoNaoEUmaChamadaNova() = runTest {
        val app = MutableStateFlow(Amostras.estado.copy(autenticado = true, voz = listOf(VoiceState(marina, dm, false, false))))
        val toque = ToqueFalso()
        ControladorDeChamadasEmDm(backgroundScope, app, {}, {}, toque, NotificadorDeChamadaNulo, { false }, { currentTime }); runCurrent()
        assertTrue(toque.eventos.isEmpty())
    }

    @Test fun salaDeCanalDeServidorNaoToca() = runTest {
        val a = amb()
        a.app.value = a.app.value.copy(voz = listOf(VoiceState(marina, Amostras.SALA, false, false))); a.t.runCurrent()
        assertEquals(EstadoDaChamada.Nenhuma, a.c.estado.value)
    }

    @Test fun sairDaContaLimpaTudo() = runTest {
        val a = amb(); a.voz(marina)
        a.app.value = a.app.value.copy(autenticado = false); a.t.runCurrent()
        assertEquals(EstadoDaChamada.Nenhuma, a.c.estado.value); assertTrue(a.toque.eventos.contains("parar"))
    }
}
