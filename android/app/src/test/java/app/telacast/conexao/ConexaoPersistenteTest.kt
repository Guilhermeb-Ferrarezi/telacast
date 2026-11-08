package app.telacast.conexao

import app.telacast.data.AppState
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class ConexaoPersistenteTest {
    private class Servico : ServicoPersistente { val eventos = ArrayList<String>(); override fun iniciar() { eventos += "iniciar" }; override fun parar() { eventos += "parar" } }

    @Test fun sobeNoLoginComAOpcaoLigadaEParaAoSair() = runTest {
        val estado = MutableStateFlow(AppState()); val opcao = MutableStateFlow(true); val s = Servico()
        GerenciadorDaConexaoPersistente(backgroundScope, estado, opcao, s); runCurrent()
        assertEquals("deslogado: nada a manter", listOf("parar"), s.eventos)
        estado.value = estado.value.copy(autenticado = true); runCurrent()
        assertEquals(listOf("parar", "iniciar"), s.eventos)
        estado.value = estado.value.copy(autenticado = false); runCurrent()
        assertEquals("sair da conta encerra o serviço", listOf("parar", "iniciar", "parar"), s.eventos)
    }

    @Test fun desligarAOpcaoParaEReligarSobe() = runTest {
        val estado = MutableStateFlow(AppState(autenticado = true)); val opcao = MutableStateFlow(true); val s = Servico()
        GerenciadorDaConexaoPersistente(backgroundScope, estado, opcao, s); runCurrent()
        opcao.value = false; runCurrent(); opcao.value = true; runCurrent()
        assertEquals(listOf("iniciar", "parar", "iniciar"), s.eventos)
    }

    @Test fun mudancasDoEstadoQueNaoAlteramALoginNaoRepetem() = runTest {
        val estado = MutableStateFlow(AppState(autenticado = true)); val opcao = MutableStateFlow(true); val s = Servico()
        GerenciadorDaConexaoPersistente(backgroundScope, estado, opcao, s); runCurrent()
        estado.value = estado.value.copy(naoLidos = setOf(1)); runCurrent(); estado.value = estado.value.copy(naoLidos = setOf(1, 2)); runCurrent()
        assertEquals(listOf("iniciar"), s.eventos)
    }
}
