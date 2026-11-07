package app.telacast.armazenamento

import java.util.Base64
import javax.crypto.Cipher
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * AES-256-GCM em poucas linhas. A chave vem de fora (no app, do Android Keystore: ela nunca sai do chip/TEE).
 * Formato guardado: Base64( IV de 12 bytes + texto cifrado com a etiqueta de autenticação ).
 * Puro `javax.crypto`: roda na JVM, então dá para testar sem aparelho.
 *
 * Em vez de `androidx.security-crypto` (alfa e descontinuada).
 */
class CifraAesGcm(private val chave: () -> SecretKey) {
    fun cifrar(texto: String): String {
        val c = Cipher.getInstance(TRANSFORMACAO)
        c.init(Cipher.ENCRYPT_MODE, chave()) // o IV é gerado pelo provedor (Keystore exige isso)
        val cifrado = c.doFinal(texto.toByteArray(Charsets.UTF_8))
        return Base64.getEncoder().encodeToString(c.iv + cifrado)
    }

    /** `null` se o texto estiver corrompido ou a chave mudou (ex.: o app foi restaurado em outro aparelho). */
    fun decifrar(guardado: String): String? = try {
        val bytes = Base64.getDecoder().decode(guardado)
        require(bytes.size > TAMANHO_IV)
        val c = Cipher.getInstance(TRANSFORMACAO)
        c.init(Cipher.DECRYPT_MODE, chave(), GCMParameterSpec(ETIQUETA_BITS, bytes, 0, TAMANHO_IV))
        String(c.doFinal(bytes, TAMANHO_IV, bytes.size - TAMANHO_IV), Charsets.UTF_8)
    } catch (_: Exception) {
        null
    }

    companion object {
        const val TRANSFORMACAO = "AES/GCM/NoPadding"
        const val TAMANHO_IV = 12
        const val ETIQUETA_BITS = 128
    }
}
