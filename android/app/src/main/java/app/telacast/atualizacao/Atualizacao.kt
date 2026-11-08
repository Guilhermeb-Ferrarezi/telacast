package app.telacast.atualizacao

import kotlinx.serialization.Serializable
import net.i2p.crypto.eddsa.EdDSAEngine
import net.i2p.crypto.eddsa.EdDSAPublicKey
import net.i2p.crypto.eddsa.spec.EdDSANamedCurveTable
import net.i2p.crypto.eddsa.spec.EdDSAPublicKeySpec
import java.security.MessageDigest

/** Plataforma deste app no servidor (`GET /update/manifest?platform=`). */
const val PLATAFORMA_ANDROID = "android-arm64"

/** `proto::UpdateManifest` do servidor (docs/DEPLOY.md, "Atualização do app"). */
@Serializable
data class ManifestoDeAtualizacao(
    val version: String,
    val platform: String,
    val sha256: String,
    val size: Long,
    val notes: String = "",
    val signature: String = "",
)

/** Os mesmos bytes que o servidor assina (`update_signing_message` em proto/src/lib.rs). */
fun mensagemAssinada(version: String, platform: String, sha256: String, size: Long): ByteArray =
    "telacast-update-v1\n$version\n$platform\n$sha256\n$size\n".toByteArray(Charsets.UTF_8)

/** `candidato` é mais novo que `atual`? `maior.menor.correção` numérico, sem o sufixo depois de `-` ou `+`; o que não for número vale 0 (igual ao `version_is_newer` do servidor). */
fun versaoMaisNova(candidato: String, atual: String): Boolean {
    fun partes(v: String): List<Long> {
        val nucleo = v.trim().trimStart('v').split('-', '+').first()
        val p = nucleo.split('.').take(3).map { it.toLongOrNull() ?: 0L }
        return p + List(3 - p.size) { 0L }
    }
    val a = partes(candidato); val b = partes(atual)
    for (i in 0 until 3) if (a[i] != b[i]) return a[i] > b[i]
    return false
}

fun hexParaBytes(h: String): ByteArray? {
    val s = h.trim()
    if (s.isEmpty() || s.length % 2 != 0 || !s.all { it in '0'..'9' || it in 'a'..'f' || it in 'A'..'F' }) return null
    return ByteArray(s.length / 2) { s.substring(it * 2, it * 2 + 2).toInt(16).toByte() }
}

fun bytesParaHex(b: ByteArray): String = b.joinToString("") { "%02x".format(it) }

/**
 * Confere o que o servidor mandou, antes de baixar e antes de instalar. Sem chave pública ([chavePublica] `null`) a atualização fica
 * DESLIGADA (`ligada == false`): sem a assinatura, só o hash protegeria de corrupção, não de um servidor adulterado.
 */
class VerificadorDeAtualizacao(private val chavePublica: ByteArray?, private val limiteDeTamanho: Long = 300L shl 20) {
    val ligada: Boolean get() = chavePublica != null && chavePublica.size == 32

    sealed interface Resultado {
        data object Ok : Resultado
        /** Nada a fazer (já está na última versão ou a atualização está desligada). */
        data object SemNovidade : Resultado
        data class Recusado(val motivo: String) : Resultado
    }

    /** O manifesto vale como "há versão nova": plataforma certa, versão maior, hash e tamanho plausíveis e assinatura Ed25519 válida. */
    fun conferirManifesto(m: ManifestoDeAtualizacao, versaoAtual: String, plataforma: String = PLATAFORMA_ANDROID): Resultado {
        if (!ligada) return Resultado.SemNovidade
        if (m.platform != plataforma) return Resultado.Recusado("O manifesto é de outra plataforma (${m.platform}).")
        if (!versaoMaisNova(m.version, versaoAtual)) return Resultado.SemNovidade
        if (m.sha256.length != 64 || hexParaBytes(m.sha256) == null) return Resultado.Recusado("O manifesto tem um hash inválido.")
        if (m.size <= 0 || m.size > limiteDeTamanho) return Resultado.Recusado("O manifesto tem um tamanho inválido.")
        if (!assinaturaValida(m)) return Resultado.Recusado("A assinatura da atualização não confere. Nada foi baixado.")
        return Resultado.Ok
    }

    /** O arquivo baixado tem o tamanho e o SHA-256 do manifesto? */
    fun conferirArquivo(m: ManifestoDeAtualizacao, tamanho: Long, sha256: String): Resultado = when {
        tamanho != m.size -> Resultado.Recusado("O arquivo baixado tem o tamanho errado ($tamanho de ${m.size} bytes).")
        !sha256.equals(m.sha256, ignoreCase = true) -> Resultado.Recusado("O arquivo baixado não bate com o hash do manifesto.")
        else -> Resultado.Ok
    }

    fun assinaturaValida(m: ManifestoDeAtualizacao): Boolean {
        val pub = chavePublica?.takeIf { it.size == 32 } ?: return false
        val sig = hexParaBytes(m.signature)?.takeIf { it.size == 64 } ?: return false
        return try {
            val spec = EdDSAPublicKeySpec(pub, EdDSANamedCurveTable.getByName(EdDSANamedCurveTable.ED_25519))
            val motor = EdDSAEngine(MessageDigest.getInstance("SHA-512"))
            motor.initVerify(EdDSAPublicKey(spec))
            motor.update(mensagemAssinada(m.version, m.platform, m.sha256, m.size))
            motor.verify(sig)
        } catch (_: Exception) { false }
    }
}
