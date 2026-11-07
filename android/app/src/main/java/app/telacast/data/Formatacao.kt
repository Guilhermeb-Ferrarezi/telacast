package app.telacast.data

import java.time.Instant
import java.time.LocalDate
import java.time.ZoneId
import java.time.ZonedDateTime

/** Datas e tamanhos em português do Brasil, como `theme.rs`/`md_view.rs`/`files.rs` do desktop. Puro Kotlin (java.time). */
object Formatacao {
    private fun z(ts: Long, zona: ZoneId): ZonedDateTime = Instant.ofEpochMilli(ts).atZone(zona)

    /** Hora do dia, "14:05". */
    fun hora(ts: Long, zona: ZoneId = ZoneId.systemDefault()): String = z(ts, zona).let { "%02d:%02d".format(it.hour, it.minute) }

    fun data(ts: Long, zona: ZoneId = ZoneId.systemDefault()): String = z(ts, zona).let { "%02d/%02d/%d".format(it.dayOfMonth, it.monthValue, it.year) }

    fun mesmoDia(a: Long, b: Long, zona: ZoneId = ZoneId.systemDefault()): Boolean = z(a, zona).toLocalDate() == z(b, zona).toLocalDate()

    /** Rótulo do dia para o divisor do chat: "hoje", "ontem" ou "07/11/2026". */
    fun dia(ts: Long, agora: Long, zona: ZoneId = ZoneId.systemDefault()): String {
        val d = z(ts, zona).toLocalDate()
        val hoje: LocalDate = z(agora, zona).toLocalDate()
        return when (d) {
            hoje -> "hoje"
            hoje.minusDays(1) -> "ontem"
            else -> data(ts, zona)
        }
    }

    fun tamanho(n: Long): String = when {
        n < 1024 -> "$n B"
        n < 1 shl 20 -> "%.1f KB".format(java.util.Locale.US, n / 1024.0)
        else -> "%.1f MB".format(java.util.Locale.US, n / (1 shl 20).toDouble())
    }

    private val MESES = listOf("janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro", "novembro", "dezembro")
    private val DIAS = listOf("domingo", "segunda-feira", "terça-feira", "quarta-feira", "quinta-feira", "sexta-feira", "sábado")

    /** Carimbo `<t:unix:estilo>` em português do Brasil (`unix` em segundos). */
    fun carimbo(unix: Long, estilo: Char, agoraSeg: Long, zona: ZoneId = ZoneId.systemDefault()): String {
        val t = Instant.ofEpochSecond(unix).atZone(zona)
        val longa = "${t.dayOfMonth} de ${MESES[t.monthValue - 1]} de ${t.year}"
        val hm = "%02d:%02d".format(t.hour, t.minute)
        val dm = "%02d/%02d/%d".format(t.dayOfMonth, t.monthValue, t.year)
        return when (estilo) {
            't' -> hm
            'T' -> "$hm:%02d".format(t.second)
            'd' -> dm
            'D' -> longa
            'F' -> "${DIAS[t.dayOfWeek.value % 7]}, $longa $hm"
            's' -> "$dm $hm"
            'S' -> "$dm $hm:%02d".format(t.second)
            'R' -> relativo(unix - agoraSeg)
            else -> "$longa $hm"
        }
    }

    fun relativo(diff: Long): String {
        val futuro = diff > 0
        val s = Math.abs(diff)
        if (s < 45) return if (futuro) "em alguns segundos" else "há alguns segundos"
        val (n, un) = when {
            s < 90 -> 1L to "minuto"
            s < 45 * 60 -> ((s + 30) / 60) to "minuto"
            s < 90 * 60 -> 1L to "hora"
            s < 22 * 3600 -> ((s + 1800) / 3600) to "hora"
            s < 36 * 3600 -> 1L to "dia"
            s < 26 * 86400 -> ((s + 43200) / 86400) to "dia"
            s < 45 * 86400 -> 1L to "mês"
            s < 320 * 86400 -> ((s + 15 * 86400) / (30 * 86400)) to "mês"
            else -> maxOf(1L, (s + 182 * 86400) / (365 * 86400)) to "ano"
        }
        val unidade = when {
            un == "mês" && n > 1 -> "meses"
            un == "ano" && n > 1 -> "anos"
            n > 1 -> "${un}s"
            else -> un
        }
        val num = if (n == 1L && unidade == "hora") "uma" else if (n == 1L && unidade == "minuto") "um" else n.toString()
        return if (futuro) "em $num $unidade" else "há $num $unidade"
    }
}
