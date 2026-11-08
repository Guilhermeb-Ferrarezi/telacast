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

class PreAgcTest {
    private fun seno(db: Float): ShortArray {
        val a = Math.pow(10.0, db / 20.0) * 1.4142 * 32768
        return ShortArray(QUADRO) { i -> (a * kotlin.math.sin(2 * Math.PI * 300 * i / 48000)).toInt().toShort() }
    }

    private fun ruido(db: Float, r: java.util.Random): ShortArray {
        val a = Math.pow(10.0, db / 20.0) * 32768
        return ShortArray(QUADRO) { (r.nextGaussian() * a).toInt().coerceIn(-32768, 32767).toShort() }
    }

    @Test
    fun fala_baixa_sobe_para_perto_do_alvo_depois_do_fundo() {
        val p = PreAgc(); val r = java.util.Random(1)
        repeat(300) { p.processar(ruido(-62f, r)) } // fundo constante
        var q = seno(-55f)
        repeat(60) { q = seno(-55f); p.processar(q) }
        assertTrue("ganho ${p.ganhoDb}", p.ganhoDb > 18f)
        assertEquals(PreAgc.ALVO_DB, rmsDb(q), 4f)
    }

    @Test
    fun ruido_constante_nao_vira_fala() {
        val p = PreAgc(); val r = java.util.Random(2)
        repeat(800) { p.processar(ruido(-50f, r)) }
        assertTrue("ganho ${p.ganhoDb}", p.ganhoDb < 6f)
    }

    @Test
    fun ganho_cai_devagar_no_silencio() {
        val p = PreAgc(); val r = java.util.Random(3)
        repeat(300) { p.processar(ruido(-62f, r)) }
        repeat(60) { p.processar(seno(-50f)) }
        val g = p.ganhoDb
        repeat(25) { p.processar(ruido(-62f, r)) } // 0,5 s
        assertTrue(p.ganhoDb < g && p.ganhoDb > g - 4f)
    }

    @Test
    fun fala_normal_nao_e_amplificada() {
        val p = PreAgc()
        repeat(80) { p.processar(seno(-28f)) }
        assertTrue(p.ganhoDb < 3f)
    }
}
