package app.telacast.arquivos

import app.telacast.data.Formatacao
import app.telacast.proto.Attachment
import app.telacast.proto.Proto
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody
import okio.BufferedSink
import java.util.concurrent.TimeUnit

/** [ApiDeArquivos] sobre o OkHttp. */
class OkHttpArquivos(
    private val cliente: OkHttpClient = OkHttpClient.Builder().callTimeout(60, TimeUnit.SECONDS).build(),
) : ApiDeArquivos {

    override suspend fun baixar(url: String, maxBytes: Long): ByteArray? = withContext(Dispatchers.IO) {
        try {
            cliente.newCall(Request.Builder().url(url).build()).execute().use { r ->
                if (!r.isSuccessful) return@withContext null
                val corpo = r.body ?: return@withContext null
                if (corpo.contentLength() > maxBytes) return@withContext null
                val bytes = corpo.source().readByteArray()
                if (bytes.size > maxBytes) null else bytes
            }
        } catch (_: Exception) { null }
    }

    override suspend fun previa(url: String, total: Long): PreviaDeTexto? = withContext(Dispatchers.IO) {
        try {
            val req = Request.Builder().url(url).header("Range", "bytes=0-${Arquivos.PREVIA_BYTES}").build()
            cliente.newCall(req).execute().use { r ->
                if (!r.isSuccessful) return@withContext null
                val corpo = r.body ?: return@withContext null
                val fonte = corpo.source()
                fonte.request(Arquivos.PREVIA_BYTES + 1) // lê no máximo PREVIA_BYTES + 1 bytes
                val lidos = fonte.buffer.readByteArray(minOf(fonte.buffer.size, Arquivos.PREVIA_BYTES + 1))
                montarPrevia(lidos, total)
            }
        } catch (_: Exception) { null }
    }

    override suspend fun enviar(url: String, fonte: FonteDeArquivo, progresso: (Long) -> Unit): Attachment = withContext(Dispatchers.IO) {
        val corpo = object : RequestBody() {
            override fun contentType() = "application/octet-stream".toMediaType()
            override fun contentLength() = fonte.tamanho
            override fun writeTo(sink: BufferedSink) {
                fonte.abrir().use { entrada ->
                    val buf = ByteArray(16 * 1024)
                    var total = 0L
                    while (true) {
                        val n = entrada.read(buf)
                        if (n < 0) break
                        sink.write(buf, 0, n)
                        total += n
                        progresso(total)
                    }
                }
            }
        }
        try {
            cliente.newCall(Request.Builder().url(url).post(corpo).build()).execute().use { r ->
                when {
                    r.isSuccessful -> Proto.json.decodeFromString(Attachment.serializer(), r.body!!.string())
                    r.code == 401 -> throw FalhaDeEnvio("Sessão inválida. Entre de novo.")
                    r.code == 403 -> throw FalhaDeEnvio("Você não pode enviar arquivos neste canal.")
                    r.code == 413 -> throw FalhaDeEnvio("O arquivo é grande demais (limite de ${Formatacao.tamanho(Arquivos.MAX_UPLOAD)}).")
                    r.code == 429 -> throw FalhaDeEnvio("Muitos envios seguidos. Tente de novo em um minuto.")
                    else -> throw FalhaDeEnvio("O servidor recusou o arquivo (${r.code}).")
                }
            }
        } catch (e: FalhaDeEnvio) {
            throw e
        } catch (e: Exception) {
            throw FalhaDeEnvio("Não consegui enviar o arquivo. Confira a conexão.")
        }
    }

    companion object {
        /** Corta no limite da prévia, troca CRLF por LF e tira o caractere partido no fim. */
        fun montarPrevia(lidos: ByteArray, total: Long): PreviaDeTexto {
            val truncado = lidos.size > Arquivos.PREVIA_BYTES || total > lidos.size
            val bytes = if (lidos.size > Arquivos.PREVIA_BYTES) lidos.copyOf(Arquivos.PREVIA_BYTES.toInt()) else lidos
            var texto = String(bytes, Charsets.UTF_8).replace("\r\n", "\n")
            if (truncado) texto = texto.trimEnd('�')
            val restante = if (truncado) maxOf(total - bytes.size, 1L) else 0L
            return PreviaDeTexto(texto, texto.trimEnd('\n').split('\n').size, restante)
        }
    }
}
