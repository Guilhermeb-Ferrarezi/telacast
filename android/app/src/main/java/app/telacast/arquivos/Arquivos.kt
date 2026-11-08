package app.telacast.arquivos

import app.telacast.proto.Attachment
import java.io.InputStream
import java.net.URLEncoder

/** Anexos: endereços, limites e tipos. Puro Kotlin (espelha `app/src/files.rs` do desktop). */
object Arquivos {
    /** Limite do servidor (padrão 10 MiB); o app confere antes para não gastar rede. */
    const val MAX_UPLOAD: Long = 10L shl 20
    const val MAX_ANEXOS = 10
    const val MAX_SIMULTANEOS = 3
    const val PREVIA_BYTES = 50_000L
    const val MAX_IMAGEM_BYTES = 25L shl 20

    /** `ws://host` vira `http://host`, `wss://host` vira `https://host`, `host:porta` vira `http://host:porta`. */
    fun urlHttp(servidor: String): String {
        val s = servidor.trim().trimEnd('/').removeSuffix("/ws")
        return when {
            s.startsWith("wss://") -> "https://" + s.removePrefix("wss://")
            s.startsWith("ws://") -> "http://" + s.removePrefix("ws://")
            s.startsWith("http://") || s.startsWith("https://") -> s
            else -> (if (app.telacast.net.Endereco.publico(s)) "https://" else "http://") + s
        }
    }

    private fun codificar(s: String) = URLEncoder.encode(s, "UTF-8").replace("+", "%20")

    /** `GET /files/{id}/{nome}?token=` (o nome é decorativo). */
    fun url(base: String, a: Attachment, token: String) = "$base/files/${a.id}/${codificar(a.name)}?token=$token"

    /**
     * Miniatura de imagem (`&thumb=1`): o servidor devolve um JPEG/PNG de até 1024 px e, se não puder (imagem já pequena,
     * GIF, animado, arquivo inválido, servidor antigo), devolve o ORIGINAL. O cliente tem que aceitar os dois.
     */
    fun urlMiniatura(base: String, a: Attachment, token: String) = url(base, a, token) + "&thumb=1"

    /**
     * Dimensões para desenhar a imagem já decodificada. `Attachment.width/height` são as CRUAS do arquivo; numa foto com
     * orientação EXIF 5 a 8 a miniatura vem com largura e altura trocadas (o servidor aplica a orientação), e o bitmap
     * decodificado pode ter sido reduzido por `inSampleSize`. Por isso o que manda é a proporção do bitmap: se bate com a
     * do anexo, vale o tamanho do anexo; se bate com a trocada, vale o anexo trocado; senão, o próprio bitmap.
     */
    fun dimensoesDeExibicao(a: Attachment, bmpLargura: Int, bmpAltura: Int): Pair<Int, Int> {
        if (bmpLargura <= 0 || bmpAltura <= 0 || a.width <= 0 || a.height <= 0) return bmpLargura to bmpAltura
        fun parecido(w1: Int, h1: Int, w2: Int, h2: Int) = kotlin.math.abs(w1.toLong() * h2 - h1.toLong() * w2) * 50 <= maxOf(w1.toLong() * h2, h1.toLong() * w2)
        return when {
            parecido(bmpLargura, bmpAltura, a.width, a.height) -> a.width to a.height
            parecido(bmpLargura, bmpAltura, a.height, a.width) -> a.height to a.width
            else -> bmpLargura to bmpAltura
        }
    }

    fun urlDeUpload(base: String, token: String, canal: Long, nome: String) = "$base/upload?token=$token&channel=$canal&name=${codificar(nome)}"

    private val EXTENSOES_DE_TEXTO = setOf(
        "txt", "md", "markdown", "log", "json", "jsonc", "toml", "yaml", "yml", "ini", "cfg", "conf", "csv", "tsv", "xml", "html", "htm",
        "css", "js", "mjs", "ts", "tsx", "jsx", "rs", "py", "rb", "go", "java", "kt", "c", "h", "cpp", "hpp", "cs", "sh", "bash", "fish",
        "sql", "lua", "php", "swift", "diff", "patch", "env", "lock", "gradle", "properties",
    )

    /** Arquivo de texto/código que ganha prévia no chat (pelo tipo text (mime) ou pela extensão). */
    fun ehTexto(a: Attachment): Boolean {
        val ext = a.name.substringAfterLast('.', "").lowercase()
        return (a.mime.startsWith("text/") || ext in EXTENSOES_DE_TEXTO) && !a.mime.startsWith("image/")
    }

    fun ehImagem(a: Attachment) = a.mime.startsWith("image/") && a.width > 0 && a.height > 0

    /** Tamanho de exibição de uma imagem: no máximo 400 x 300 dp (e a largura disponível), nunca ampliada. */
    fun tamanhoDaImagem(largura: Int, altura: Int, larguraMax: Float = 400f): Pair<Float, Float> {
        val k = minOf(larguraMax / largura, 300f / altura, 1f)
        return maxOf(largura * k, 40f) to maxOf(altura * k, 40f)
    }
}

/** Arquivo escolhido para anexar (no Android vem de um `Uri` do seletor de arquivos). */
interface FonteDeArquivo {
    val nome: String
    val tamanho: Long
    fun abrir(): InputStream
    val ehImagem: Boolean get() = false
}

class FonteEmMemoria(override val nome: String, private val bytes: ByteArray, override val ehImagem: Boolean = false) : FonteDeArquivo {
    override val tamanho: Long get() = bytes.size.toLong()
    override fun abrir(): InputStream = bytes.inputStream()
}

/** Começo de um arquivo de texto: `bytesRestantes` = o que ficou de fora (0 = arquivo inteiro). */
data class PreviaDeTexto(val texto: String, val linhas: Int, val bytesRestantes: Long)

/** Rede dos anexos. A implementação de verdade usa OkHttp; os testes usam uma falsa. */
interface ApiDeArquivos {
    /** Baixa o arquivo inteiro (limitado a `maxBytes`); `null` se falhar. */
    suspend fun baixar(url: String, maxBytes: Long): ByteArray?

    /** Lê só o começo (cabeçalho `Range`) e monta a prévia; `null` se falhar. */
    suspend fun previa(url: String, total: Long): PreviaDeTexto?

    /** `POST /upload`; `progresso` recebe os bytes já enviados. Falha com [FalhaDeEnvio] (mensagem em português). */
    suspend fun enviar(url: String, fonte: FonteDeArquivo, progresso: (Long) -> Unit): Attachment
}

class FalhaDeEnvio(mensagem: String) : Exception(mensagem)
