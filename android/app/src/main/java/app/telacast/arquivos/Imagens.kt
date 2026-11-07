package app.telacast.arquivos

import app.telacast.proto.Attachment
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.async
import kotlinx.coroutines.Deferred

/** Cache LRU em memória, limitado por bytes (a ordem de acesso decide quem sai). */
class CacheLru<V : Any>(private val maxBytes: Long, private val tamanho: (V) -> Long) {
    private val mapa = object : LinkedHashMap<Long, V>(16, 0.75f, true) {}
    private var total = 0L

    @Synchronized fun obter(chave: Long): V? = mapa[chave]

    @Synchronized fun guardar(chave: Long, v: V) {
        mapa.remove(chave)?.let { total -= tamanho(it) }
        mapa[chave] = v
        total += tamanho(v)
        val it = mapa.entries.iterator()
        while (total > maxBytes && it.hasNext()) {
            val e = it.next()
            if (e.key == chave) continue
            total -= tamanho(e.value)
            it.remove()
        }
    }

    @get:Synchronized val bytes: Long get() = total
    @get:Synchronized val quantidade: Int get() = mapa.size
}

/**
 * Baixa e decodifica imagens de anexos (sem Coil): cache LRU em memória e um só download por anexo mesmo com vários
 * pedidos ao mesmo tempo. `T` é o bitmap da plataforma (`ImageBitmap` no app; qualquer coisa nos testes).
 */
class CarregadorDeImagens<T : Any>(
    private val api: ApiDeArquivos,
    private val escopo: CoroutineScope,
    private val decodificar: (ByteArray) -> T?,
    tamanho: (T) -> Long,
    maxBytesDeCache: Long = 48L shl 20,
) {
    private val cache = CacheLru(maxBytesDeCache, tamanho)
    private val emAndamento = HashMap<Long, Deferred<T?>>()
    @Volatile var downloads = 0
        private set

    /** `null` se não baixou ou não decodificou (a tela mostra o cartão de arquivo). */
    suspend fun obter(a: Attachment, url: String): T? {
        cache.obter(a.id)?.let { return it }
        val d = synchronized(emAndamento) {
            emAndamento.getOrPut(a.id) {
                escopo.async {
                    downloads++
                    val bytes = api.baixar(url, Arquivos.MAX_IMAGEM_BYTES)
                    val bmp = bytes?.let(decodificar)
                    if (bmp != null) cache.guardar(a.id, bmp)
                    synchronized(emAndamento) { emAndamento.remove(a.id) }
                    bmp
                }
            }
        }
        return d.await()
    }

    fun emCache(id: Long): T? = cache.obter(id)
    val bytesEmCache: Long get() = cache.bytes
}

/** Prévias de texto (só o começo do arquivo), com cache simples. */
class CarregadorDePrevias(private val api: ApiDeArquivos) {
    private val cache = HashMap<Long, PreviaDeTexto>()

    fun emCache(a: Attachment): PreviaDeTexto? = synchronized(cache) { cache[a.id] }

    suspend fun obter(a: Attachment, url: String): PreviaDeTexto? {
        synchronized(cache) { cache[a.id] }?.let { return it }
        val p = api.previa(url, a.size) ?: return null
        synchronized(cache) { cache[a.id] = p }
        return p
    }
}
