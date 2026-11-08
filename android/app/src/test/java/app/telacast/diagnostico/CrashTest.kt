package app.telacast.diagnostico

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class CrashTest {
    @Test fun relatorioTemVersaoAparelhoEPilhaMasNaoSegredos() {
        val e = IllegalStateException("falhou em https://u:senha123@host/ws?token=abc123XYZ&x=1 com Bearer eyJ.hhh.sss")
        val r = RelatorioDeCrash.formatar("0.1.0 (1)", "Xiaomi Redmi · Android 17 (API 37)", 1_794_000_000_000, "main", e)
        assertTrue(r.startsWith("Telacast 0.1.0 (1) · Xiaomi Redmi"))
        assertTrue(r.contains("IllegalStateException")); assertTrue(r.contains("em app.telacast") || r.contains("em org.junit"))
        for (segredo in listOf("senha123", "abc123XYZ", "eyJ.hhh.sss")) assertFalse("vazou $segredo", r.contains(segredo))
        assertTrue(r.contains("token=***"))
    }

    @Test fun causasEncadeadasEntramELimitam() {
        val r = RelatorioDeCrash.formatar("1", "x", 0, "t", RuntimeException("a", IllegalArgumentException("b", java.io.IOException("c"))))
        assertTrue(r.contains("Causado por: java.lang.IllegalArgumentException: b")); assertTrue(r.contains("java.io.IOException: c"))
    }

    @Test fun limparCortaTextoLongoETiraQuebras() {
        assertEquals(200, RelatorioDeCrash.limpar("a".repeat(500)).length)
        assertEquals("a b c", RelatorioDeCrash.limpar("a\n b\t\tc"))
        assertEquals("", RelatorioDeCrash.limpar(null))
        assertEquals("password=*** e senha=***", RelatorioDeCrash.limpar("password=x1 e senha=y2"))
    }

    @Test fun handlerGravaOArquivoEEncadeiaComOAnterior() {
        val arq = java.io.File.createTempFile("crash", ".txt").also { it.delete() }
        val original = Thread.getDefaultUncaughtExceptionHandler()
        var chamado: Throwable? = null
        Thread.setDefaultUncaughtExceptionHandler { _, e -> chamado = e }
        try {
            val r = RelatorioDeCrash(arq)
            assertNull(r.pendente())
            r.instalar("0.1.0 (1)", "aparelho") { 5L }
            val erro = RuntimeException("boom")
            Thread.getDefaultUncaughtExceptionHandler()!!.uncaughtException(Thread.currentThread(), erro)
            assertEquals("o handler anterior ainda roda (o processo morre como sempre)", erro, chamado)
            assertNotNull(r.pendente()); assertTrue(r.pendente()!!.contains("RuntimeException: boom"))
            r.descartar(); assertNull(r.pendente())
        } finally { Thread.setDefaultUncaughtExceptionHandler(original) }
    }
}
