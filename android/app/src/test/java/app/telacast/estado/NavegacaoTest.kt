package app.telacast.estado

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class NavegacaoTest {
    @Test fun voltarFechaUmaCoisaPorVezNaOrdemCerta() {
        var n = Navegacao(gavetaAberta = true, menu = MenuAberto.Status, dialogo = Dialogo.ConfirmarSair)
        assertTrue(n.podeVoltar)
        n = n.voltar(); assertEquals(null, n.dialogo); assertEquals(MenuAberto.Status, n.menu); assertTrue(n.gavetaAberta)
        n = n.voltar(); assertEquals(null, n.menu); assertTrue(n.gavetaAberta)
        n = n.voltar(); assertFalse(n.gavetaAberta)
        assertFalse("sem nada aberto, Voltar sai do app", n.podeVoltar)
        assertEquals(n, n.voltar())
    }

    @Test fun membrosFechamAntesDaGaveta() {
        val n = Navegacao(membrosAbertos = true).voltar()
        assertFalse(n.membrosAbertos)
    }

    @Test fun gavetaEMembrosNaoFicamAbertosJuntos() {
        assertFalse(Navegacao(gavetaAberta = true).abrirMembros().gavetaAberta)
        assertFalse(Navegacao(membrosAbertos = true).abrirGaveta().membrosAbertos)
    }

    @Test fun irParaCanalFechaAGavetaEMostraOServidorDele() {
        val n = Navegacao(gavetaAberta = true, menu = MenuAberto.Servidor, servidorNoPainel = 9).irPara(Destino.Canal(7, 70))
        assertEquals(Destino.Canal(7, 70), n.destino)
        assertEquals(7L, n.servidorNoPainel)
        assertFalse(n.gavetaAberta)
        assertEquals(null, n.menu)
    }

    @Test fun selecionarServidorNaoMudaOCentroNemFechaAGaveta() {
        val n = Navegacao(destino = Destino.Canal(1, 10), servidorNoPainel = 1, gavetaAberta = true).selecionarServidor(2)
        assertEquals(Destino.Canal(1, 10), n.destino)
        assertEquals(2L, n.servidorNoPainel)
        assertTrue(n.gavetaAberta)
        assertEquals(null, n.selecionarServidor(null).servidorNoPainel)
    }

    @Test fun abrirMenuDuasVezesFecha() {
        val n = Navegacao().abrirMenu(MenuAberto.Servidor)
        assertEquals(MenuAberto.Servidor, n.menu)
        assertEquals(null, n.abrirMenu(MenuAberto.Servidor).menu)
        assertEquals(MenuAberto.Status, n.abrirMenu(MenuAberto.Status).menu)
    }
}
