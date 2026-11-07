package app.telacast.armazenamento

import android.content.Context
import android.content.SharedPreferences
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.security.KeyStore
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey

/**
 * Token da sessão cifrado com uma chave AES-GCM do Android Keystore (a chave não é exportável) e guardado em
 * SharedPreferences privadas. Se a decifragem falhar (chave perdida), o token é descartado: o usuário entra de novo.
 */
class CofreKeystore(contexto: Context) : CofreDeSegredos {
    private val prefs: SharedPreferences = contexto.getSharedPreferences("telacast_sessao", Context.MODE_PRIVATE)
    private val cifra = CifraAesGcm(::chave)

    override fun guardar(texto: String) {
        prefs.edit().putString(CAMPO, cifra.cifrar(texto)).apply()
    }

    override fun ler(): String? {
        val guardado = prefs.getString(CAMPO, null) ?: return null
        return cifra.decifrar(guardado) ?: run { apagar(); null }
    }

    override fun apagar() {
        prefs.edit().remove(CAMPO).apply()
    }

    private fun chave(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getKey(ALIAS, null) as? SecretKey)?.let { return it }
        val gerador = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gerador.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return gerador.generateKey()
    }

    private companion object {
        const val ALIAS = "telacast_token"
        const val CAMPO = "token"
    }
}
