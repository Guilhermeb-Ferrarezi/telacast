package app.telacast.diagnostico

import java.io.File

/**
 * Relatório do último crash: um arquivo de texto privado (só o app lê) com versão, aparelho e a pilha, SEM dados sensíveis (nada de
 * mensagens, usuários, tokens ou senhas: mensagens de exceção passam por [limpar]). Na próxima abertura o app mostra "O app fechou
 * inesperadamente" com "Copiar relatório" e "Compartilhar". Puro Kotlin (o Android só entra em quem instala o handler).
 */
class RelatorioDeCrash(private val arquivo: File) {
    /** Instala o handler de exceções não tratadas (encadeia com o anterior, que ainda derruba o processo como sempre). */
    fun instalar(versao: String, aparelho: String, agora: () -> Long = System::currentTimeMillis) {
        val anterior = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { t, e ->
            runCatching { arquivo.writeText(formatar(versao, aparelho, agora(), t.name, e)) }
            anterior?.uncaughtException(t, e)
        }
    }

    fun pendente(): String? = runCatching { arquivo.takeIf { it.exists() }?.readText()?.takeIf { it.isNotBlank() } }.getOrNull()

    fun descartar() { runCatching { arquivo.delete() } }

    companion object {
        /** Tira o que parece segredo de um texto de exceção: `token=...`, `Bearer ...`, `password=...`, URLs com credenciais. */
        fun limpar(texto: String?): String {
            var t = texto ?: return ""
            t = t.replace(Regex("""(?i)(token|senha|password|passwd|secret|authorization|invite|convite)=[^&\s"']+"""), "$1=***")
            t = t.replace(Regex("""(?i)bearer\s+[A-Za-z0-9._\-]+"""), "Bearer ***")
            t = t.replace(Regex("""(?i)(https?|wss?)://[^/\s:@]+:[^/\s@]+@"""), "$1://***@")
            return t.replace(Regex("\\s+"), " ").take(200)
        }

        fun formatar(versao: String, aparelho: String, quandoMs: Long, thread: String, e: Throwable): String = buildString {
            appendLine("Telacast $versao · $aparelho")
            appendLine("Quando: ${java.time.Instant.ofEpochMilli(quandoMs)} · thread: $thread")
            var causa: Throwable? = e
            var n = 0
            while (causa != null && n < 4) {
                appendLine(if (n == 0) "Erro: ${causa.javaClass.name}: ${limpar(causa.message)}" else "Causado por: ${causa.javaClass.name}: ${limpar(causa.message)}")
                for (f in causa.stackTrace.take(25)) appendLine("    em $f")
                causa = causa.cause?.takeIf { it !== causa }; n++
            }
        }
    }
}
