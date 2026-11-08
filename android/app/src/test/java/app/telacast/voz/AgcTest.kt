package app.telacast.voz

import kotlin.math.PI
import kotlin.math.sin
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class AgcTest {
    private fun seno(amplitudeDb: Float): ShortArray {
        val a = Math.pow(10.0, amplitudeDb / 20.0) * 1.4142 * 32768
        return ShortArray(QUADRO) { i -> (a * sin(2 * PI * 300 * i / 48000)).toInt().toShort() }
    }

    @Test
    fun voz_distante_sobe_ate_perto_do_alvo() {
        val agc = Agc()
        var q = seno(-45f)
        repeat(400) { q = seno(-45f); agc.processar(q, rmsDb(q), true) }
        assertTrue("ganho ${agc.ganhoDb}", agc.ganhoDb > 20f)
        assertEquals(Agc.ALVO_DB, rmsDb(q), 3f)
    }

    @Test
    fun silencio_congela_o_ganho_e_nao_sobe_o_ruido() {
        val agc = Agc()
        repeat(100) { val q = seno(-40f); agc.processar(q, rmsDb(q), true) }
        val g = agc.ganhoDb
        repeat(200) { val q = seno(-70f); agc.processar(q, rmsDb(q), false) }
        assertEquals(g, agc.ganhoDb, 0.001f)
    }

    @Test
    fun voz_alta_nunca_e_atenuada_nem_estoura() {
        val agc = Agc()
        repeat(50) { val q = seno(-6f); agc.processar(q, rmsDb(q), true) }
        assertEquals(0f, agc.ganhoDb, 0.001f)
        val q = seno(-1f); agc.processar(q, rmsDb(q), true)
        assertTrue(q.all { it in -32768..32767 })
    }

    @Test
    fun desligado_volta_a_zero() {
        val agc = Agc()
        repeat(100) { val q = seno(-45f); agc.processar(q, rmsDb(q), true) }
        agc.ligado = false
        repeat(60) { val q = seno(-45f); agc.processar(q, rmsDb(q), true) }
        assertTrue(agc.ganhoDb < 0.5f)
    }
}
