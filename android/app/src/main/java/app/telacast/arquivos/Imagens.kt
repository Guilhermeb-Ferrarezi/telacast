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
 * Baixa e decodifica imagens de anexos (sem Coil). Duas camadas de cache em memória, separadas: a das MINIATURAS (a
 * que a lista do chat usa) e a dos ORIGINAIS (tela cheia). Um só download por anexo e por camada, mesmo com vários
 * pedidos ao mesmo tempo. `T` é o bitmap da plataforma (`ImageBitmap` no app; qualquer coisa nos testes).
 */
class CarregadorDeImagens<T : Any>(
    private val api: ApiDeArquivos,
    private val escopo: CoroutineScope,
    private val decodificar: (ByteArray) -> T?,
    tamanho: (T) -> Long,
    maxBytesDeCache: Long = 48L shl 20,
    maxBytesDeMiniaturas: Long = 32L shl 20,
) {
    private class Camada<T : Any>(maxBytes: Long, tamanho: (T) -> Long) {
        val cache = CacheLru(maxBytes, tamanho)
        val emAndamento = HashMap<Long, Deferred<T?>>()
    }

    private val originais = Camada(maxBytesDeCache, tamanho)
    private val miniaturas = Camada(maxBytesDeMiniaturas, tamanho)

    /** Downloads do original e da miniatura (para os testes conferirem o "um só download"). */
    @Volatile var downloads = 0
        private set
    @Volatile var downloadsDeMiniatura = 0
        private set

    private suspend fun carregar(camada: Camada<T>, a: Attachment, url: String, contar: () -> Unit): T? {
        camada.cache.obter(a.id)?.let { return it }
        val d = synchronized(camada.emAndamento) {
            camada.emAndamento.getOrPut(a.id) {
                escopo.async {
                    contar()
                    val bytes = api.baixar(url, Arquivos.MAX_IMAGEM_BYTES)
                    val bmp = bytes?.let(decodificar)
                    if (bmp != null) camada.cache.guardar(a.id, bmp)
                    synchronized(camada.emAndamento) { camada.emAndamento.remove(a.id) }
                    bmp
                }
            }
        }
        return d.await()
    }

    /** O ORIGINAL. `null` se não baixou ou não decodificou (a tela mostra o cartão de arquivo, ou fica com a miniatura). */
    suspend fun obter(a: Attachment, url: String): T? = carregar(originais, a, url) { downloads++ }

    /**
     * A MINIATURA (`urlMiniatura`). O servidor pode devolver o original no lugar (imagem pequena, GIF...): serve igual.
     * Se o pedido falhar (servidor sem suporte a `thumb`, erro de rede), cai para o original em `urlOriginal`.
     */
    suspend fun obterMiniatura(a: Attachment, urlMiniatura: String, urlOriginal: String): T? {
        originais.cache.obter(a.id)?.let { return it } // se o original já está na memória, nem baixa a miniatura
        return carregar(miniaturas, a, urlMiniatura) { downloadsDeMiniatura++ } ?: obter(a, urlOriginal)
    }

    fun emCache(id: Long): T? = originais.cache.obter(id)

    /** A melhor imagem que já está na memória para a lista: a miniatura, ou o original se for o que há. */
    fun miniaturaEmCache(id: Long): T? = miniaturas.cache.obter(id) ?: originais.cache.obter(id)

    val bytesEmCache: Long get() = originais.cache.bytes
    val bytesEmMiniaturas: Long get() = miniaturas.cache.bytes
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
