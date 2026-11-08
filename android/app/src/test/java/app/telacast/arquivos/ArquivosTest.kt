package app.telacast.arquivos

import app.telacast.proto.Attachment
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.async
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** API de mentira: cada envio fica esperando o teste liberar. */
class ApiFalsa : ApiDeArquivos {
    val downloads = mutableListOf<String>()
    var bytesDoDownload: ByteArray? = ByteArray(10)
    /** Resposta específica por URL (ausente = `bytesDoDownload`). */
    val respostas = mutableMapOf<String, ByteArray?>()
    val envios = mutableListOf<Pair<String, CompletableDeferred<Attachment>>>()
    var simultaneosMax = 0
    private var simultaneos = 0
    val previas = mutableListOf<String>()
    var previa: PreviaDeTexto? = PreviaDeTexto("a\nb", 2, 0)

    override suspend fun baixar(url: String, maxBytes: Long): ByteArray? { downloads += url; kotlinx.coroutines.yield(); return if (url in respostas) respostas[url] else bytesDoDownload }
    override suspend fun previa(url: String, total: Long): PreviaDeTexto? { previas += url; return previa }
    override suspend fun enviar(url: String, fonte: FonteDeArquivo, progresso: (Long) -> Unit): Attachment {
        val d = CompletableDeferred<Attachment>()
        envios += url to d
        simultaneos++; simultaneosMax = maxOf(simultaneosMax, simultaneos)
        progresso(fonte.tamanho / 2)
        try { return d.await() } finally { simultaneos-- }
    }
}

@OptIn(ExperimentalCoroutinesApi::class)
class ArquivosTest {
    private fun anexo(id: Long, nome: String = "a.png", mime: String = "image/png", w: Int = 100, h: Int = 50) = Attachment(id, nome, mime, 10, w, h)

    @Test fun enderecos() {
        assertEquals("http://10.0.2.2:8080", Arquivos.urlHttp("10.0.2.2:8080"))
        assertEquals("http://h:1", Arquivos.urlHttp("ws://h:1/ws"))
        assertEquals("https://exemplo.com", Arquivos.urlHttp("wss://exemplo.com/"))
        assertEquals("https://telecast.guilhermebf.dev", Arquivos.urlHttp("telecast.guilhermebf.dev"))
        assertEquals("http://h", Arquivos.urlHttp("h"))
        assertEquals("http://h/files/7/foto%20da%20praia.png?token=T", Arquivos.url("http://h", anexo(7, "foto da praia.png"), "T"))
        assertEquals("http://h/upload?token=T&channel=5&name=relat%C3%B3rio.pdf", Arquivos.urlDeUpload("http://h", "T", 5, "relatório.pdf"))
    }

    @Test fun tiposDeAnexo() {
        assertTrue(Arquivos.ehTexto(anexo(1, "main.kt", "application/octet-stream", 0, 0)))
        assertTrue(Arquivos.ehTexto(anexo(1, "x", "text/plain", 0, 0)))
        assertFalse(Arquivos.ehTexto(anexo(1, "foto.png")))
        assertFalse(Arquivos.ehTexto(anexo(1, "relatório.pdf", "application/octet-stream", 0, 0)))
        assertTrue(Arquivos.ehImagem(anexo(1)))
        assertFalse(Arquivos.ehImagem(anexo(1, w = 0, h = 0)))
    }

    @Test fun tamanhoDaImagemAte400x300SemAmpliar() {
        assertEquals(100f to 50f, Arquivos.tamanhoDaImagem(100, 50))
        assertEquals(400f to 225f, Arquivos.tamanhoDaImagem(1280, 720))
        assertEquals(150f to 300f, Arquivos.tamanhoDaImagem(600, 1200))
        assertEquals("respeita a largura disponível", 280f to 157.5f, Arquivos.tamanhoDaImagem(1280, 720, 280f))
        assertEquals("mínimo de 40", 40f to 40f, Arquivos.tamanhoDaImagem(10, 10))
    }

    @Test fun previaDeTextoCortaECuidaDoFim() {
        val p = OkHttpArquivos.montarPrevia("linha1\r\nlinha2\n".toByteArray(), 14)
        assertEquals(PreviaDeTexto("linha1\nlinha2\n", 2, 0), p)
        val grande = ByteArray(60_000) { 'x'.code.toByte() }
        val q = OkHttpArquivos.montarPrevia(grande.copyOf(50_001), 60_000)
        assertEquals(50_000, q.texto.length)
        assertEquals(10_000L, q.bytesRestantes)
    }

    @Test fun cacheLruTiraOMenosUsadoQuandoPassaDoLimite() {
        val c = CacheLru<ByteArray>(100) { it.size.toLong() }
        c.guardar(1, ByteArray(40)); c.guardar(2, ByteArray(40))
        c.obter(1) // 1 fica mais recente
        c.guardar(3, ByteArray(40))
        assertNotNull(c.obter(1)); assertNull("o 2 era o menos usado", c.obter(2)); assertNotNull(c.obter(3))
        assertTrue(c.bytes <= 100)
    }

    @Test fun imagemBaixaUmaVezSoMesmoComVariosPedidos() = runTest {
        val api = ApiFalsa()
        val c = CarregadorDeImagens<String>(api, backgroundScope, { "bmp${it.size}" }, { 10 })
        val a = anexo(1)
        val r = listOf(async { c.obter(a, "u") }, async { c.obter(a, "u") })
        advanceUntilIdle()
        assertEquals(listOf("bmp10", "bmp10"), r.map { it.await() })
        assertEquals(1, c.downloads)
        assertEquals("do cache, sem nova rede", "bmp10", c.obter(a, "u"))
        assertEquals(1, api.downloads.size)
    }

    @Test fun imagemQueNaoBaixaOuNaoDecodificaVoltaNulo() = runTest {
        val api = ApiFalsa().also { it.bytesDoDownload = null }
        val c = CarregadorDeImagens<String>(api, backgroundScope, { "x" }, { 1 })
        assertNull(c.obter(anexo(1), "u"))
        api.bytesDoDownload = ByteArray(3)
        val d = CarregadorDeImagens<String>(api, backgroundScope, { null }, { 1 })
        assertNull(d.obter(anexo(2), "u"))
    }

    // ---------------------------------------------------------------- miniaturas (C3)

    @Test fun urlDaMiniaturaTemThumbEMantemATokenENome() {
        val a = anexo(7, nome = "férias 1.png")
        assertEquals("http://h/files/7/f%C3%A9rias%201.png?token=T&thumb=1", Arquivos.urlMiniatura("http://h", a, "T"))
        assertEquals("o original continua sem thumb", "http://h/files/7/f%C3%A9rias%201.png?token=T", Arquivos.url("http://h", a, "T"))
    }

    @Test fun miniaturaEOriginalTemCachesSeparadosEDownloadsSeparados() = runTest {
        val api = ApiFalsa().also { it.respostas["m"] = ByteArray(3); it.respostas["o"] = ByteArray(9) }
        val c = CarregadorDeImagens<String>(api, backgroundScope, { "bmp${it.size}" }, { 10 })
        val a = anexo(1)
        assertNull(c.miniaturaEmCache(1)); assertNull(c.emCache(1))
        assertEquals("bmp3", c.obterMiniatura(a, "m", "o"))
        assertEquals("a miniatura não enche o cache do original", null, c.emCache(1))
        assertEquals("bmp3", c.miniaturaEmCache(1))
        assertEquals("bmp9", c.obter(a, "o"))
        assertEquals("bmp9", c.emCache(1))
        assertEquals("com o original na memória a lista usa ele", "bmp9", c.obterMiniatura(a, "m", "o"))
        assertEquals(1, c.downloadsDeMiniatura)
        assertEquals(listOf("m", "o"), api.downloads)
    }

    @Test fun miniaturaBaixaUmaVezSoMesmoComVariosPedidos() = runTest {
        val api = ApiFalsa()
        val c = CarregadorDeImagens<String>(api, backgroundScope, { "t${it.size}" }, { 10 })
        val a = anexo(1)
        val r = listOf(async { c.obterMiniatura(a, "m", "o") }, async { c.obterMiniatura(a, "m", "o") }, async { c.obterMiniatura(a, "m", "o") })
        advanceUntilIdle()
        assertEquals(3, r.map { it.await() }.size)
        assertEquals(1, c.downloadsDeMiniatura)
        assertEquals(0, c.downloads)
    }

    @Test fun seOOriginalJaEstaNaMemoriaAListaUsaEleENaoBaixaMiniatura() = runTest {
        val api = ApiFalsa().also { it.respostas["o"] = ByteArray(9) }
        val c = CarregadorDeImagens<String>(api, backgroundScope, { "bmp${it.size}" }, { 10 })
        c.obter(anexo(1), "o")
        assertEquals("bmp9", c.miniaturaEmCache(1))
        assertEquals("bmp9", c.obterMiniatura(anexo(1), "m", "o"))
        assertEquals(listOf("o"), api.downloads)
    }

    @Test fun servidorAntigoSemThumbDevolveOOriginalEServeIgual() = runTest {
        // o servidor ignora `thumb` e manda o arquivo inteiro: o cliente só decodifica o que veio
        val api = ApiFalsa().also { it.respostas["m"] = ByteArray(8_000) }
        val c = CarregadorDeImagens<String>(api, backgroundScope, { "bmp${it.size}" }, { 10 })
        assertEquals("bmp8000", c.obterMiniatura(anexo(1), "m", "o"))
        assertEquals(listOf("m"), api.downloads)
    }

    @Test fun miniaturaQueFalhaCaiParaOOriginal() = runTest {
        val api = ApiFalsa().also { it.respostas["m"] = null; it.respostas["o"] = ByteArray(9) }
        val c = CarregadorDeImagens<String>(api, backgroundScope, { "bmp${it.size}" }, { 10 })
        assertEquals("bmp9", c.obterMiniatura(anexo(1), "m", "o"))
        assertEquals(listOf("m", "o"), api.downloads)
    }

    @Test fun semMiniaturaNemOriginalVoltaNulo() = runTest {
        val api = ApiFalsa().also { it.bytesDoDownload = null }
        val c = CarregadorDeImagens<String>(api, backgroundScope, { "x" }, { 1 })
        assertNull(c.obterMiniatura(anexo(1), "m", "o"))
    }

    @Test fun dimensoesDeExibicaoFicamComAsDoAnexoQuandoAProporcaoBate() {
        val a = anexo(1, w = 4000, h = 3000)
        assertEquals(4000 to 3000, Arquivos.dimensoesDeExibicao(a, 1024, 768))
        assertEquals("bitmap reduzido por inSampleSize", 4000 to 3000, Arquivos.dimensoesDeExibicao(a, 500, 375))
    }

    @Test fun dimensoesDeExibicaoTrocamQuandoAMiniaturaVemComEixosTrocados() {
        // foto de celular em pé: o anexo diz 4000 x 3000 (cru, EXIF 6), a miniatura já vem em pé: 768 x 1024
        val a = anexo(1, w = 4000, h = 3000)
        assertEquals(3000 to 4000, Arquivos.dimensoesDeExibicao(a, 768, 1024))
    }

    @Test fun dimensoesDeExibicaoUsamOBitmapQuandoNadaBate() {
        assertEquals(640 to 480, Arquivos.dimensoesDeExibicao(anexo(1, w = 100, h = 100), 640, 480))
        assertEquals(10 to 20, Arquivos.dimensoesDeExibicao(anexo(1, w = 0, h = 0), 10, 20))
    }

    @Test fun previaTemCache() = runTest {
        val api = ApiFalsa()
        val c = CarregadorDePrevias(api)
        c.obter(anexo(1), "u"); c.obter(anexo(1), "u")
        assertEquals(1, api.previas.size)
    }

    // ---------------------------------------------------------------- envio de anexos

    private fun TestScopeEnv(t: kotlinx.coroutines.test.TestScope, api: ApiFalsa, token: String? = "T") =
        EnviadorDeAnexos(api, t.backgroundScope, { "http://h" }, { token })

    private fun fonte(nome: String, n: Int = 100) = FonteEmMemoria(nome, ByteArray(n))
    private fun pronto(id: Long) = Attachment(id, "f$id", "application/octet-stream", 100, 0, 0)

    @Test fun nuncaMaisDeTresEnviosAoMesmoTempoEOsOutrosEsperamNaFila() = runTest {
        val api = ApiFalsa(); val e = TestScopeEnv(this, api)
        e.adicionar(5, (1..5).map { fonte("f$it") }); runCurrent()
        assertEquals(3, api.envios.size)
        assertEquals(3, e.anexos.value.count { it.estado is EstadoDoEnvio.Enviando })
        assertEquals(2, e.anexos.value.count { it.estado is EstadoDoEnvio.NaFila })
        assertEquals("metade enviada", 0.5f, e.anexos.value.first().progresso, 0.001f)
        api.envios[0].second.complete(pronto(1)); runCurrent()
        assertEquals("ao terminar um, entra o próximo", 4, api.envios.size)
        api.envios.drop(1).forEachIndexed { i, (_, d) -> d.complete(pronto(i + 2L)) }; runCurrent()
        api.envios.drop(4).forEach { it.second.complete(pronto(9)) }; runCurrent()
        assertEquals(3, api.simultaneosMax)
        assertTrue(e.anexos.value.all { it.estado is EstadoDoEnvio.Pronto })
        assertTrue(api.envios.all { it.first.startsWith("http://h/upload?token=T&channel=5&name=f") })
        assertEquals(5, e.prontos(5).size)
    }

    @Test fun limiteDeDezAnexosPorMensagem() = runTest {
        val api = ApiFalsa(); val e = TestScopeEnv(this, api)
        assertNull(e.adicionar(1, (1..10).map { fonte("f$it") }))
        assertEquals("Você só pode anexar 10 arquivos por mensagem.", e.adicionar(1, listOf(fonte("extra"))))
        assertEquals(10, e.doCanal(1).size)
        assertNull("outro canal tem a sua própria bandeja", e.adicionar(2, listOf(fonte("x"))))
    }

    @Test fun arquivoGrandeDemaisFalhaNaHoraSemRede() = runTest {
        val api = ApiFalsa(); val e = TestScopeEnv(this, api)
        e.adicionar(1, listOf(object : FonteDeArquivo { override val nome = "filme.mkv"; override val tamanho = 20L shl 20; override fun abrir() = ByteArray(0).inputStream() }))
        runCurrent()
        val f = e.anexos.value.single().estado as EstadoDoEnvio.Falhou
        assertEquals("O arquivo tem 20.0 MB e o limite é 10.0 MB.", f.motivo)
        assertTrue(api.envios.isEmpty())
        e.tentarDeNovo(e.anexos.value.single().chave); runCurrent()
        assertTrue("não adianta tentar de novo", api.envios.isEmpty())
    }

    @Test fun falhaPodeTentarDeNovoEOutrosPodemSerRemovidos() = runTest {
        val api = ApiFalsa(); val e = TestScopeEnv(this, api)
        e.adicionar(1, listOf(fonte("a"), fonte("b"))); runCurrent()
        api.envios[0].second.completeExceptionally(FalhaDeEnvio("Muitos envios seguidos. Tente de novo em um minuto.")); runCurrent()
        val a = e.anexos.value[0]
        assertEquals(EstadoDoEnvio.Falhou("Muitos envios seguidos. Tente de novo em um minuto."), a.estado)
        e.tentarDeNovo(a.chave); runCurrent()
        assertEquals(3, api.envios.size)
        api.envios[2].second.complete(pronto(1)); runCurrent()
        assertTrue(e.anexos.value[0].estado is EstadoDoEnvio.Pronto)
        e.remover(e.anexos.value[1].chave); runCurrent()
        assertEquals(1, e.anexos.value.size)
    }

    @Test fun concluirTiraSoOsProntosEFalhasFicam() = runTest {
        val api = ApiFalsa(); val e = TestScopeEnv(this, api)
        e.adicionar(1, listOf(fonte("a"), fonte("b"))); runCurrent()
        api.envios[0].second.complete(pronto(1)); api.envios[1].second.completeExceptionally(FalhaDeEnvio("x")); runCurrent()
        e.concluir(1)
        assertEquals(1, e.anexos.value.size)
        assertTrue(e.anexos.value.single().estado is EstadoDoEnvio.Falhou)
        assertFalse(e.emAndamento(1))
    }

    @Test fun semTokenFalha() = runTest {
        val api = ApiFalsa(); val e = TestScopeEnv(this, api, token = null)
        e.adicionar(1, listOf(fonte("a"))); runCurrent()
        assertTrue(e.anexos.value.single().estado is EstadoDoEnvio.Falhou)
    }
}
