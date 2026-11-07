package app.telacast.tema

import org.junit.Test
import org.junit.Assert.assertEquals

class GrafiteTest {
    /** Ids do servidor são snowflakes de 64 bits: o índice nunca pode sair da lista. */
    @Test
    fun tomDoUsuarioAceitaQualquerId() {
        val ids = listOf(0L, 1L, 4L, 5L, 7_000_000_000_000_000_000L, 1_234_567_890_123_456_789L, -1L, -3L, Long.MAX_VALUE, Long.MIN_VALUE)
        for (id in ids) Grafite.tomDoUsuario(id) // não pode lançar
    }

    /** Mesma regra do desktop: `id.unsigned_abs() % 5`. */
    @Test
    fun tomDoUsuarioIgualAoDesktop() {
        assertEquals(Grafite.TonsDeAvatar[3], Grafite.tomDoUsuario(1_234_567_890_123_456_788L)) // ...788 % 5 = 3
        assertEquals(Grafite.TonsDeAvatar[3], Grafite.tomDoUsuario(-1_234_567_890_123_456_788L))
        assertEquals(Grafite.TonsDeAvatar[2], Grafite.tomDoUsuario(7))
        assertEquals(Grafite.TonsDeAvatar[3], Grafite.tomDoUsuario(Long.MIN_VALUE)) // 2^63 % 5 = 3
    }
}
