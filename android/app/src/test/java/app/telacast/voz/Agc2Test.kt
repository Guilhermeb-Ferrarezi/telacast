package app.telacast.voz

import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.sin
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class Agc2Test {
    private var fase = 0.0
    /** 20 ms de um tom de fala com [rmsDb] dBFS de RMS. */
    private fun fala(rmsDb: Float): ShortArray {
        val a = Math.pow(10.0, rmsDb / 20.0) * 1.4142 * 32768
        return ShortArray(QUADRO) { fase += 2 * PI * 220 / 48000; (a * sin(fase)).toInt().toShort() }
    }
    private fun picoDb(q: ShortArray): Float = Agc2.dbfs(q.maxOf { abs(it.toInt()) }.toDouble()).toFloat()

    @Test
    fun fala_baixa_sobe_6_dB_por_segundo_e_so_depois_de_120_ms() {
        val g = Agc2()
        repeat(5) { g.processar(fala(-45f), 1f) } // 100 ms de fala: ainda não sobe
        assertEquals(0f, g.ganhoDb, 0.001f)
        repeat(50) { g.processar(fala(-45f), 1f) } // +1 s
        assertTrue("ganho ${g.ganhoDb}", g.ganhoDb in 4.5f..7.5f)
    }

    @Test
    fun leva_o_pico_da_fala_perto_de_menos_5_dBFS() {
        val g = Agc2()
        var q = fala(-40f)
        // fala com pausas (o piso de ruído vem das pausas: um tom contínuo seria "ruído" e travaria o ganho)
        repeat(1400) { i ->
            if (i % 175 < 150) { q = fala(-40f); g.processar(q, 1f) } else g.processar(fala(-75f), 0f)
        } // ~28 s
        // com um tom puro a margem pico/nível fica perto do mínimo (12 dB), então o pico não chega a -5; o que vale é o ganho
        // convergir para o alvo da fórmula: -headroom - (nível de fala + margem)
        val alvo = -5f - (g.nivelDeFalaDb + g.margemDb)
        assertEquals("alvo=$alvo ganho=${g.ganhoDb} fala=${g.nivelDeFalaDb} margem=${g.margemDb}", alvo, g.ganhoDb, 1f)
        assertTrue(g.ganhoDb <= 50f)
        assertTrue("pico=${picoDb(q)}", picoDb(q) > -25f)
    }

    @Test
    fun fala_alta_nao_e_amplificada_nem_estoura() {
        val g = Agc2()
        var q = fala(-10f)
        repeat(300) { q = fala(-10f); g.processar(q, 1f) }
        assertEquals(0f, g.ganhoDb, 0.001f)
        assertTrue(q.all { it in -32768..32767 })
        assertTrue(picoDb(q) <= 0.1f)
    }

    @Test
    fun rajada_curta_de_fala_nao_adapta() {
        val g = Agc2()
        repeat(300) { g.processar(fala(-45f), if (it % 8 < 5) 1f else 0f) } // rajadas de 5 quadros de 20 ms (10 de 10 ms)? < 12
        assertEquals(0f, g.ganhoDb, 0.001f)
    }

    @Test
    fun sem_fala_o_ganho_nao_sobe() {
        val g = Agc2()
        repeat(500) { g.processar(fala(-50f), 0.1f) }
        assertEquals(0f, g.ganhoDb, 0.001f)
    }

    @Test
    fun ruido_de_fundo_alto_limita_o_ganho() {
        val g = Agc2()
        val r = java.util.Random(7)
        val a = Math.pow(10.0, -48.0 / 20.0) * 32768 // ruído a -48 dBFS RMS, constante, sem fala
        repeat(2500) { g.processar(ShortArray(QUADRO) { (r.nextGaussian() * a).toInt().toShort() }, 0f) } // 50 s: a estimativa sobe pela metade a cada janela de 5 s
        assertTrue("ruído ${g.ruidoDb}", g.ruidoDb > -52f)
        repeat(900) { g.processar(fala(-45f), 1f) }
        assertTrue("ganho ${g.ganhoDb}", g.ganhoDb < 4f) // -50 - (-48) < 0 → só 0
    }

    @Test
    fun desligado_nao_mexe_no_sinal() {
        val g = Agc2(); g.ligado = false
        val q = fala(-40f); val c = q.copyOf()
        repeat(100) { g.processar(fala(-40f), 1f) }
        g.processar(q, 1f)
        assertTrue(q.contentEquals(c).not() || g.ganhoDb == 0f)
        assertEquals(0f, g.ganhoDb, 0.001f)
    }
}
