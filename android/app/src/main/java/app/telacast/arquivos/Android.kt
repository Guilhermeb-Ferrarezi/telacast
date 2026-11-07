package app.telacast.arquivos

import android.app.DownloadManager
import android.content.ContentResolver
import android.content.Context
import android.graphics.BitmapFactory
import android.net.Uri
import android.os.Environment
import android.provider.OpenableColumns
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import java.io.InputStream

/** Decodifica uma imagem reduzindo (`inSampleSize`) para caber em `maxLado` px: poupa RAM com fotos de câmera. */
fun decodificarBitmap(bytes: ByteArray, maxLado: Int = 2048): ImageBitmap? = try {
    val limites = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, limites)
    var amostra = 1
    while (maxOf(limites.outWidth, limites.outHeight) / amostra > maxLado) amostra *= 2
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply { inSampleSize = amostra })?.asImageBitmap()
} catch (_: Exception) { null }

/** Arquivo escolhido no seletor de arquivos do sistema (Storage Access Framework). */
class FonteUri(private val resolver: ContentResolver, private val uri: Uri) : FonteDeArquivo {
    private val colunas by lazy {
        resolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { c ->
            if (c.moveToFirst()) {
                val n = c.getColumnIndex(OpenableColumns.DISPLAY_NAME).takeIf { it >= 0 }?.let { c.getString(it) }
                val t = c.getColumnIndex(OpenableColumns.SIZE).takeIf { it >= 0 && !c.isNull(it) }?.let { c.getLong(it) }
                n to t
            } else null
        } ?: (null to null)
    }
    override val nome: String get() = colunas.first ?: uri.lastPathSegment ?: "arquivo"
    override val tamanho: Long get() = colunas.second ?: -1L
    override fun abrir(): InputStream = resolver.openInputStream(uri) ?: throw FalhaDeEnvio("Não consegui abrir o arquivo.")
    override val ehImagem: Boolean get() = resolver.getType(uri)?.startsWith("image/") == true

    /** Miniatura para a bandeja (lado maior de ~240 px). */
    fun miniatura(): ImageBitmap? = try {
        val limites = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        abrir().use { BitmapFactory.decodeStream(it, null, limites) }
        var amostra = 1
        while (maxOf(limites.outWidth, limites.outHeight) / amostra > 480) amostra *= 2
        abrir().use { BitmapFactory.decodeStream(it, null, BitmapFactory.Options().apply { inSampleSize = amostra }) }?.asImageBitmap()
    } catch (_: Exception) { null }
}

/** Baixa pelo `DownloadManager` do sistema (pasta Downloads, com notificação); não precisa de permissão no Android 10+. */
class BaixadorAndroid(private val contexto: Context) {
    fun baixar(url: String, nome: String) {
        val gerente = contexto.getSystemService(Context.DOWNLOAD_SERVICE) as DownloadManager
        gerente.enqueue(
            DownloadManager.Request(Uri.parse(url)).setTitle(nome)
                .setNotificationVisibility(DownloadManager.Request.VISIBILITY_VISIBLE_NOTIFY_COMPLETED)
                .setDestinationInExternalPublicDir(Environment.DIRECTORY_DOWNLOADS, nome),
        )
    }
}
