package app.telacast.armazenamento

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Test
import java.util.Base64
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey

class CifraAesGcmTest {
    private fun chaveNova(): SecretKey = KeyGenerator.getInstance("AES").apply { init(256) }.generateKey()

    @Test fun cifraEDecifraDeVolta() {
        val c = CifraAesGcm(chaveNova().let { k -> { k } })
        val token = "q3Vb9Zt1mXw0L5nY8rJc2HdKpEoAaSuIvGf7TzBN4kM"
        val guardado = c.cifrar(token)
        assertNotEquals(token, guardado)
        assertEquals(false, guardado.contains(token))
        assertEquals(token, c.decifrar(guardado))
    }

    @Test fun cadaCifragemUsaUmIvNovo() {
        val c = CifraAesGcm(chaveNova().let { k -> { k } })
        assertNotEquals(c.cifrar("igual"), c.cifrar("igual"))
    }

    @Test fun chaveDiferenteOuTextoAdulteradoNaoDecifra() {
        val c = CifraAesGcm(chaveNova().let { k -> { k } })
        val guardado = c.cifrar("segredo")
        assertNull(CifraAesGcm(chaveNova().let { k -> { k } }).decifrar(guardado))
        val bytes = Base64.getDecoder().decode(guardado)
        bytes[bytes.size - 1] = (bytes[bytes.size - 1].toInt() xor 1).toByte()
        assertNull("a etiqueta GCM pega adulteração", c.decifrar(Base64.getEncoder().encodeToString(bytes)))
        assertNull(c.decifrar("isto não é base64 válido!!"))
        assertNull(c.decifrar(""))
    }

    @Test fun acentosEEmojisSobrevivem() {
        val c = CifraAesGcm(chaveNova().let { k -> { k } })
        assertEquals("ação 🎉", c.decifrar(c.cifrar("ação 🎉")))
    }
}
