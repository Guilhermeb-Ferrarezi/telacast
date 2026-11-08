package app.telacast.integracao

import app.telacast.data.Aviso
import app.telacast.data.Repository
import app.telacast.net.Credencial
import app.telacast.net.EstadoConexao
import app.telacast.net.EventoGateway
import app.telacast.net.Gateway
import app.telacast.net.OkHttpTransporte
import app.telacast.proto.Channel
import app.telacast.proto.ErrorCode
import app.telacast.proto.Message
import app.telacast.proto.Reaction
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.AfterClass
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.BeforeClass
import org.junit.Test
import java.util.concurrent.CopyOnWriteArrayList

/**
 * Integração contra o servidor REAL (`android/scripts/it.sh` sobe um num banco descartável e passa
 * `TELACAST_IT_URL` e `TELACAST_IT_INVITE`). Sem essas variáveis, os testes são ignorados.
 * Usa o OkHttp de verdade, o Gateway e o Repository: o mesmo caminho que o app vai usar.
 *
 * Cuidados com os limites do servidor: cadastro/login são 10 por 5 min por IP (aqui: 6 cadastros, 1 login errado e 1 convite inválido);
 * `Send` é 12 por 10 s (o teste de paginação faz pausas). Cada teste cria o próprio canal, então não se atrapalham.
 */
class IntegracaoTest {
    companion object {
        private val url: String? = System.getenv("TELACAST_IT_URL")
        private val convite: String? = System.getenv("TELACAST_IT_INVITE")
        private val escopo = CoroutineScope(SupervisorJob() + Dispatchers.Default)
        private val sufixo = System.nanoTime().toString().takeLast(6)

        private lateinit var ana: Cliente // dona do servidor
        private lateinit var bob: Cliente // membro
        private var servidor: Long = 0
        private var codigoDoServidor: String = ""
        private var contadorDeCanais = 0

        @BeforeClass @JvmStatic fun preparar() {
            assumeTrue("sem TELACAST_IT_URL: rode android/scripts/it.sh", url != null && convite != null)
            ana = Cliente("ana_$sufixo").also { it.cadastrar() }
            bob = Cliente("bob_$sufixo").also { it.cadastrar() }
            ana.repo.criarServidor("Turma IT")
            val aviso = esperar("ServerJoined") { ana.avisos.filterIsInstance<Aviso.ServidorEntrou>().firstOrNull() }
            servidor = aviso.servidor
            val info = esperar("servidor no snapshot") { ana.estado.snapshot?.servers?.firstOrNull { it.id == servidor } }
            codigoDoServidor = info.invite
            bob.repo.entrarNoServidor(codigoDoServidor)
            esperar("bob no servidor") { bob.estado.snapshot?.servers?.firstOrNull { it.id == servidor } }
        }

        @AfterClass @JvmStatic fun encerrar() {
            if (url != null) escopo.cancel()
        }

        /** Espera uma condição (polling) até 10 s; devolve o valor não nulo. */
        fun <T : Any> esperar(motivo: String, ms: Long = 10_000, cond: () -> T?): T {
            val fim = System.currentTimeMillis() + ms
            while (System.currentTimeMillis() < fim) {
                cond()?.let { return it }
                Thread.sleep(20)
            }
            throw AssertionError("tempo esgotado esperando: $motivo")
        }

        /** Cria um canal de texto só deste teste e espera os dois lados o verem. */
        private fun novoCanal(): Long {
            val nome = "it-${++contadorDeCanais}-${System.nanoTime().toString().takeLast(5)}"
            ana.repo.criarCanal(servidor, nome)
            val id = esperar("canal $nome (ana)") { ana.canalPorNome(nome)?.id }
            esperar("canal $nome (bob)") { bob.canalPorNome(nome) }
            return id
        }
    }

    /** Um usuário: transporte espiado (para simular queda de rede), Gateway, Repository e os avisos coletados. */
    class Cliente(val nome: String, token: String? = null) {
        val transporte = Espiao(OkHttpTransporte())
        val gateway = Gateway(escopo, transporte, esperasMs = listOf(200L, 400L))
        val repo = Repository(escopo, gateway, duracaoDigitandoMs = 1_500, intervaloDigitandoMs = 100)
        val avisos = CopyOnWriteArrayList<Aviso>()
        val eventos = CopyOnWriteArrayList<EventoGateway>()
        val estado get() = repo.estado.value
        var tokenAtual: String? = token

        init {
            escopo.launch { repo.avisos.collect { avisos += it } }
        }

        fun cadastrar() {
            repo.entrar(url!!, Credencial.Registro(nome, "senha-$nome", convite!!))
            esperar("$nome autenticado e com snapshot") { if (estado.autenticado && estado.snapshot != null) true else null }
            tokenAtual = gateway.token
        }

        fun canalPorNome(nome: String): Channel? =
            estado.snapshot?.servers?.flatMap { it.channels }?.firstOrNull { it.name == nome }

        fun texto(canal: Long) = estado.lista(canal).map { it.content }
    }

    /** Guarda a última conexão para o teste poder derrubá-la sem Logout (rede que some). */
    class Espiao(private val real: app.telacast.net.Transporte) : app.telacast.net.Transporte {
        @Volatile var ultima: app.telacast.net.ConexaoTransporte? = null
        override fun abrir(url: String, ouvinte: app.telacast.net.OuvinteTransporte) = real.abrir(url, ouvinte).also { ultima = it }
    }

    // ------------------------------------------------------------------ cadastro, servidor, mensagens

    @Test fun cadastroEServidor() {
        assertEquals("ana_$sufixo", ana.estado.eu?.name)
        val sv = ana.estado.snapshot!!.servers.first { it.id == servidor }
        assertEquals("Turma IT", sv.name)
        assertTrue("o servidor nasce com um canal de texto 'geral'", sv.channels.any { it.name == "geral" && it.kind == Channel.TEXTO })
        assertEquals(setOf("ana_$sufixo", "bob_$sufixo"), esperar("bob nos membros") {
            ana.estado.snapshot?.servers?.first { it.id == servidor }?.members?.map { it.name }?.toSet()?.takeIf { it.size == 2 }
        })
    }

    @Test fun enviarReceberERespostaComEcoOtimista() {
        val canal = novoCanal()
        ana.repo.abrirCanal(canal); bob.repo.abrirCanal(canal)
        esperar("histórico ana") { if (canal in ana.estado.carregados && ana.estado.mensagens.containsKey(canal)) true else null }
        esperar("histórico bob") { if (bob.estado.mensagens.containsKey(canal)) true else null }

        val nonce = ana.repo.enviarMensagem(canal, "olá, bob!")!!
        // na hora, provisória
        assertTrue(ana.estado.lista(canal).any { it.id == -nonce && it.content == "olá, bob!" })
        // o eco troca pela definitiva e a pendente some
        val real = esperar("eco da ana") { ana.estado.mensagens[canal]?.firstOrNull { it.content == "olá, bob!" } }
        assertTrue(real.id > 0)
        assertTrue(ana.estado.pendentes.isEmpty())
        assertEquals(1, ana.estado.lista(canal).count { it.content == "olá, bob!" })
        // bob recebe (sem nonce, que é só do autor)
        val recebida = esperar("bob recebe") { bob.estado.mensagens[canal]?.firstOrNull { it.id == real.id } }
        // (o servidor manda o mesmo `nonce` a todos os participantes; só o autor o usa para casar o eco)
        assertEquals("ana_$sufixo", recebida.author)

        // responder
        bob.repo.enviarMensagem(canal, "oi, ana", responderA = app.telacast.proto.Reply(real.id, real.author, real.content))
        val resposta = esperar("resposta chega na ana") { ana.estado.mensagens[canal]?.firstOrNull { it.content == "oi, ana" } }
        assertEquals(real.id, resposta.reply?.id)
        assertEquals("olá, bob!", resposta.reply?.content)
        assertEquals(listOf(real.id, resposta.id), ana.estado.lista(canal).map { it.id })
    }

    @Test fun editarApagarReagirEFixar() {
        val canal = novoCanal()
        ana.repo.abrirCanal(canal); bob.repo.abrirCanal(canal)
        esperar("histórico") { if (ana.estado.mensagens.containsKey(canal) && bob.estado.mensagens.containsKey(canal)) true else null }
        ana.repo.enviarMensagem(canal, "versão 1")
        val m = esperar("mensagem") { bob.estado.mensagens[canal]?.firstOrNull { it.content == "versão 1" } }

        ana.repo.editar(m.id, "versão 2")
        val editada = esperar("edição") { bob.estado.mensagens[canal]?.firstOrNull { it.id == m.id && it.content == "versão 2" } }
        assertTrue(editada.edited)
        esperar("edição na ana") { ana.estado.mensagens[canal]?.firstOrNull { it.id == m.id && it.edited } }

        bob.repo.reagir(m.id, "👍", true)
        val comReacao = esperar("reação") { ana.estado.mensagens[canal]?.firstOrNull { it.id == m.id && it.reactions.isNotEmpty() } }
        assertEquals(listOf(Reaction("👍", 1, false)), comReacao.reactions)
        esperar("reação na própria") { bob.estado.mensagens[canal]?.firstOrNull { it.id == m.id && it.reactions.firstOrNull()?.me == true } }
        bob.repo.reagir(m.id, "👍", false)
        esperar("reação removida") { if (ana.estado.mensagens[canal]!!.first { it.id == m.id }.reactions.isEmpty()) true else null }

        ana.repo.fixar(m.id, true)
        esperar("fixada") { bob.estado.mensagens[canal]?.firstOrNull { it.id == m.id && it.pinned } }
        ana.repo.pedirFixadas(canal)
        esperar("lista de fixadas") { ana.estado.fixadas[canal]?.firstOrNull { it.id == m.id } }

        ana.repo.apagar(m.id)
        esperar("exclusão") { if (bob.estado.mensagens[canal]!!.none { it.id == m.id }) true else null }
        assertTrue(ana.estado.mensagens[canal]!!.none { it.id == m.id })
    }

    // ------------------------------------------------------------------ não lido, Ack, digitando

    @Test fun naoLidoEAckViajamPeloServidor() {
        val canal = novoCanal()
        ana.repo.abrirCanal(canal) // bob NÃO abre
        esperar("histórico ana") { if (ana.estado.mensagens.containsKey(canal)) true else null }
        ana.repo.enviarMensagem(canal, "ninguém leu")
        val id = esperar("mensagem") { ana.estado.mensagens[canal]?.firstOrNull { it.content == "ninguém leu" } }.id
        esperar("bob vê não lido") { if (canal in bob.estado.naoLidos) true else null }

        bob.repo.abrirCanal(canal) // zera local e manda Ack
        assertFalse(canal in bob.estado.naoLidos)
        esperar("histórico bob") { bob.estado.mensagens[canal]?.firstOrNull { it.id == id } }
        // prova no servidor: uma sessão nova do bob (Resume) recebe o Snapshot com last_read_id == última mensagem
        val bob2 = Cliente("bob_$sufixo", bob.tokenAtual)
        bob2.repo.entrar(url!!, Credencial.Retomar(bob.tokenAtual!!))
        val canalVisto = esperar("snapshot do bob2") { bob2.canalPorNome(canalNome(canal)) }
        assertEquals(id, canalVisto.lastMessageId)
        assertEquals("o servidor guardou o Ack", id, canalVisto.lastReadId)
        assertFalse(canal in bob2.estado.naoLidos)
        bob2.gateway.desconectar()
    }

    private fun canalNome(id: Long) = ana.estado.snapshot!!.servers.flatMap { it.channels }.first { it.id == id }.name

    @Test fun digitandoApareceEExpira() {
        val canal = novoCanal()
        bob.repo.avisarDigitando(canal)
        val bobId = bob.estado.eu!!.id
        esperar("ana vê bob digitando") { ana.estado.digitando[canal]?.takeIf { bobId in it } }
        esperar("expira", ms = 4_000) { if (ana.estado.digitando[canal].isNullOrEmpty()) true else null }
    }

    // ------------------------------------------------------------------ paginação

    @Test fun historicoComPaginacao() {
        val canal = novoCanal()
        // conta própria (paula): os 55 envios gastariam o limite de 12 por 10 s da ana e atrapalhariam os outros testes
        val paula = Cliente("paula_$sufixo").also { it.cadastrar() }
        paula.repo.entrarNoServidor(codigoDoServidor)
        esperar("paula vê o canal") { paula.canalPorNome(canalNome(canal)) }
        paula.repo.abrirCanal(canal)
        esperar("histórico") { if (paula.estado.mensagens.containsKey(canal)) true else null }
        val total = 55
        // limite do servidor: 12 envios por 10 s. Lotes de 11, com pausa de 10,5 s entre eles.
        for (i in 1..total) {
            paula.repo.enviarMensagem(canal, "msg %03d".format(i))!!
            if (i % 11 == 0 && i < total) Thread.sleep(10_500)
        }
        esperar("todas confirmadas", ms = 20_000) { if (paula.estado.pendentes.isEmpty() && (paula.estado.mensagens[canal]?.size ?: 0) == total) true else null }
        paula.gateway.desconectar()

        // sessão nova do bob: pagina do zero
        bob.repo.abrirCanal(canal)
        val primeira = esperar("página 1") { bob.estado.mensagens[canal]?.takeIf { it.isNotEmpty() } }
        assertEquals("a primeira página vem cheia", 50, primeira.size)
        assertEquals(true, bob.estado.temMais[canal])
        assertEquals("msg 006", primeira.first().content)
        assertEquals("msg 055", primeira.last().content)

        // salto até uma mensagem fora da primeira página: busca a página antiga e realça (B6)
        val alvo = paula.estado.mensagens[canal]!!.first { it.content == "msg 003" }.id
        bob.repo.saltarPara(canal, alvo)
        esperar("destaque da mensagem antiga") { bob.estado.destaque?.takeIf { it.id == alvo } }
        val tudo = esperar("página 2") { bob.estado.mensagens[canal]?.takeIf { it.size == total } }
        assertEquals((1..total).map { "msg %03d".format(it) }, tudo.map { it.content })
        assertEquals("ordem por id", tudo.map { it.id }.sorted(), tudo.map { it.id })
        assertEquals(false, bob.estado.temMais[canal])
    }

    // ------------------------------------------------------------------ anexos por HTTP

    @Test fun anexoSobeBaixaEViajaNaMensagem() {
        val canal = novoCanal()
        ana.repo.abrirCanal(canal); bob.repo.abrirCanal(canal)
        esperar("histórico") { if (ana.estado.mensagens.containsKey(canal) && bob.estado.mensagens.containsKey(canal)) true else null }
        // PNG de 1x1 pixel
        val png = java.util.Base64.getDecoder().decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==")
        val api = app.telacast.arquivos.OkHttpArquivos()
        val enviador = app.telacast.arquivos.EnviadorDeAnexos(api, escopo, { app.telacast.arquivos.Arquivos.urlHttp(url!!) }, { ana.gateway.token })
        assertEquals(null, enviador.adicionar(canal, listOf(app.telacast.arquivos.FonteEmMemoria("pixel.png", png, ehImagem = true))))
        val anexo = esperar("upload pronto") { enviador.prontos(canal).firstOrNull() }
        assertEquals("image/png", anexo.mime)
        assertEquals(1, anexo.width)
        assertEquals("pixel.png", anexo.name)

        ana.repo.enviarMensagem(canal, "olha o anexo", anexos = listOf(anexo))
        val recebida = esperar("bob recebe a mensagem com anexo") { bob.estado.mensagens[canal]?.firstOrNull { it.content == "olha o anexo" } }
        assertEquals(listOf(anexo.id), recebida.attachments.map { it.id })

        // bob baixa pelo mesmo endereço que o app usa
        val baixado = runBlocking { api.baixar(app.telacast.arquivos.Arquivos.url(app.telacast.arquivos.Arquivos.urlHttp(url!!), anexo, bob.gateway.token!!), 1L shl 20) }
        assertEquals(png.toList(), baixado?.toList())
        // sem token o servidor recusa
        assertEquals(null, runBlocking { api.baixar(app.telacast.arquivos.Arquivos.url(app.telacast.arquivos.Arquivos.urlHttp(url!!), anexo, "invalido"), 1L shl 20) })
    }

    // ------------------------------------------------------------------ miniaturas (C3)

    /** JPEGs de `src/test/resources/imagens` (gerados uma vez com Pillow; o `exif6` leva Orientation = 6). */
    private fun recurso(nome: String): ByteArray = IntegracaoTest::class.java.getResourceAsStream("/imagens/$nome")!!.use { it.readBytes() }

    /** Largura e altura de um JPEG lendo o marcador SOF (sem AWT: o classpath do Android não tem). */
    private fun dimensoes(b: ByteArray): Pair<Int, Int> {
        var i = 2
        while (i + 9 < b.size) {
            if (b[i] != 0xFF.toByte()) { i++; continue }
            val m = b[i + 1].toInt() and 0xFF
            if (m in 0xC0..0xCF && m != 0xC4 && m != 0xC8 && m != 0xCC) {
                val h = ((b[i + 5].toInt() and 0xFF) shl 8) or (b[i + 6].toInt() and 0xFF)
                val w = ((b[i + 7].toInt() and 0xFF) shl 8) or (b[i + 8].toInt() and 0xFF)
                return w to h
            }
            i += 2 + (((b[i + 2].toInt() and 0xFF) shl 8) or (b[i + 3].toInt() and 0xFF))
        }
        throw AssertionError("JPEG sem SOF")
    }

    /** Contrato do servidor para `&thumb=1` e o jeito como o app usa (CarregadorDeImagens + OkHttpArquivos). */
    @Test fun miniaturaDeImagemVemReduzidaEOOriginalContinuaIntacto() {
        val canal = novoCanal()
        ana.repo.abrirCanal(canal); bob.repo.abrirCanal(canal)
        esperar("histórico") { if (ana.estado.mensagens.containsKey(canal) && bob.estado.mensagens.containsKey(canal)) true else null }
        val grande = recurso("foto_2000x1500.jpg")
        val pequena = recurso("foto_800x600.jpg")
        val emPe = recurso("foto_exif6_2000x1500.jpg") // cru 2000 x 1500, mas "em pé" pelo EXIF
        val api = app.telacast.arquivos.OkHttpArquivos()
        val base = app.telacast.arquivos.Arquivos.urlHttp(url!!)
        val enviador = app.telacast.arquivos.EnviadorDeAnexos(api, escopo, { base }, { ana.gateway.token })
        assertEquals(null, enviador.adicionar(canal, listOf(
            app.telacast.arquivos.FonteEmMemoria("grande.jpg", grande, ehImagem = true),
            app.telacast.arquivos.FonteEmMemoria("pequena.jpg", pequena, ehImagem = true),
            app.telacast.arquivos.FonteEmMemoria("em-pe.jpg", emPe, ehImagem = true),
        )))
        val prontos = esperar("3 uploads") { enviador.prontos(canal).takeIf { it.size == 3 } }
        ana.repo.enviarMensagem(canal, "fotos", anexos = prontos)
        esperar("bob recebe") { bob.estado.mensagens[canal]?.firstOrNull { it.content == "fotos" } }
        val aGrande = prontos.first { it.name == "grande.jpg" }
        val aPequena = prontos.first { it.name == "pequena.jpg" }
        val aEmPe = prontos.first { it.name == "em-pe.jpg" }
        assertEquals(2000 to 1500, aGrande.width to aGrande.height)
        assertEquals("o anexo guarda as dimensões CRUAS", 2000 to 1500, aEmPe.width to aEmPe.height)

        val http = okhttp3.OkHttpClient()
        fun pegar(u: String): Triple<Int, String?, ByteArray> = http.newCall(okhttp3.Request.Builder().url(u).build()).execute().use { r -> Triple(r.code, r.header("X-Telacast-Thumb"), r.body!!.bytes()) }
        val token = bob.gateway.token!!

        val (c1, h1, miniGrande) = pegar(app.telacast.arquivos.Arquivos.urlMiniatura(base, aGrande, token))
        assertEquals(200, c1); assertEquals("1", h1)
        assertEquals("lado maior em 1024, proporção mantida", 1024 to 768, dimensoes(miniGrande))
        assertTrue("a miniatura é bem menor que o original (${miniGrande.size} x ${grande.size})", miniGrande.size < grande.size)

        val (c2, h2, original) = pegar(app.telacast.arquivos.Arquivos.url(base, aGrande, token))
        assertEquals(200, c2); assertEquals("o original não leva o cabeçalho", null, h2)
        assertEquals(grande.toList(), original.toList())

        val (c3, h3, pedida) = pegar(app.telacast.arquivos.Arquivos.urlMiniatura(base, aPequena, token))
        assertEquals(200, c3); assertEquals("imagem que já cabe volta como está, sem o cabeçalho", null, h3)
        assertEquals(pequena.toList(), pedida.toList())

        val (c4, h4, miniEmPe) = pegar(app.telacast.arquivos.Arquivos.urlMiniatura(base, aEmPe, token))
        assertEquals(200, c4); assertEquals("1", h4)
        assertEquals("o EXIF 6 vira a miniatura em pé: largura e altura trocadas em relação ao anexo", 768 to 1024, dimensoes(miniEmPe))
        assertEquals("e o cliente dimensiona pelo bitmap", 1500 to 2000, app.telacast.arquivos.Arquivos.dimensoesDeExibicao(aEmPe, 768, 1024))

        // sem token ou com token errado, a miniatura segue a mesma regra do download
        assertEquals(401, pegar(app.telacast.arquivos.Arquivos.urlMiniatura(base, aGrande, "invalido")).first)

        // o caminho do app: o carregador baixa a miniatura e, separado, o original
        val carregador = app.telacast.arquivos.CarregadorDeImagens<String>(api, escopo, { b -> dimensoes(b).let { "${it.first}x${it.second}" } }, { 1 })
        val m = runBlocking { carregador.obterMiniatura(aGrande, app.telacast.arquivos.Arquivos.urlMiniatura(base, aGrande, token), app.telacast.arquivos.Arquivos.url(base, aGrande, token)) }
        assertEquals("1024x768", m)
        assertEquals(null, carregador.emCache(aGrande.id))
        val o = runBlocking { carregador.obter(aGrande, app.telacast.arquivos.Arquivos.url(base, aGrande, token)) }
        assertEquals("2000x1500", o)
        val p = runBlocking { carregador.obterMiniatura(aPequena, app.telacast.arquivos.Arquivos.urlMiniatura(base, aPequena, token), app.telacast.arquivos.Arquivos.url(base, aPequena, token)) }
        assertEquals("a pequena veio inteira e serve igual", "800x600", p)
    }

    // ------------------------------------------------------------------ push (D1)

    /** O servidor aceita os três comandos em silêncio (sem credencial do Firebase o push fica desligado) e recusa o inválido. */
    @Test fun comandosDePushSaoAceitosEInvalidosRecusados() {
        val antes = bob.avisos.size
        val token = "tok-it-${sufixo}"
        assertTrue(bob.repo.registrarPush(token))
        assertTrue(bob.repo.registrarPush(token)) // idempotente
        assertTrue(bob.repo.definirFoco(true))
        assertTrue(bob.repo.definirFoco(false))
        assertTrue(bob.repo.cancelarPush(token))
        // as mensagens de uma conexão são tratadas em ordem: quando a resposta de um comando com resposta chega, os de push já passaram
        bob.gateway.enviar(app.telacast.proto.C2S.ListSessions)
        esperar("sessões de volta") { if (bob.estado.sessoes.isNotEmpty()) true else null }
        assertEquals("nenhum erro para comandos válidos", emptyList<Aviso>(), bob.avisos.drop(antes).filterIsInstance<Aviso.Erro>())

        bob.gateway.enviar(app.telacast.proto.C2S.PushRegister("com espaço", "fcm"))
        esperar("erro para token inválido") { bob.avisos.drop(antes).filterIsInstance<Aviso.Erro>().firstOrNull() }
        val n = bob.avisos.count { it is Aviso.Erro }
        bob.gateway.enviar(app.telacast.proto.C2S.PushRegister("tok-ok", "apns"))
        esperar("erro para plataforma desconhecida") { if (bob.avisos.count { it is Aviso.Erro } > n) true else null }
    }

    // ------------------------------------------------------------------ amizade, bloqueio, fixadas, busca e conta (B7/B8)

    @Test fun amizadeDmEBloqueio() {
        val bobNome = bob.estado.eu!!.name
        ana.repo.pedirAmizade(bobNome)
        esperar("bob vê o pedido recebido") { bob.estado.snapshot?.friends?.firstOrNull { it.user.name == ana.estado.eu!!.name && it.status == app.telacast.proto.FriendStatus.Incoming } }
        esperar("ana vê o pedido enviado") { ana.estado.snapshot?.friends?.firstOrNull { it.user.name == bobNome && it.status == app.telacast.proto.FriendStatus.Outgoing } }
        bob.repo.aceitarAmigo(ana.estado.eu!!.id)
        esperar("são amigos") { ana.estado.snapshot?.friends?.firstOrNull { it.user.name == bobNome && it.status == app.telacast.proto.FriendStatus.Friend } }

        val antes = ana.avisos.size
        ana.repo.abrirDm(bob.estado.eu!!.id)
        val dm = esperar("DM aberta") { ana.avisos.drop(antes).filterIsInstance<Aviso.DmAberta>().firstOrNull() }
        assertTrue(ana.estado.snapshot!!.dms.any { it.channelId == dm.canal })

        ana.repo.bloquear(bob.estado.eu!!.id)
        esperar("bob na lista de bloqueados") { ana.estado.snapshot?.blocked?.firstOrNull { it.name == bobNome } }
        assertTrue("bloquear desfaz a amizade", ana.estado.snapshot!!.friends.none { it.user.name == bobNome })
        ana.repo.desbloquear(bob.estado.eu!!.id)
        esperar("desbloqueado") { if (ana.estado.snapshot?.blocked?.isEmpty() == true) true else null }
    }

    @Test fun fixadasEBusca() {
        val canal = novoCanal()
        ana.repo.abrirCanal(canal)
        esperar("histórico") { if (ana.estado.mensagens.containsKey(canal)) true else null }
        ana.repo.enviarMensagem(canal, "tesouro unico qwzx7788 enterrado aqui")
        val m = esperar("mensagem") { ana.estado.mensagens[canal]?.firstOrNull { it.content.contains("qwzx7788") } }

        ana.repo.fixar(m.id, true)
        ana.repo.pedirFixadas(canal)
        esperar("lista de fixadas") { ana.estado.fixadas[canal]?.firstOrNull { it.id == m.id } }
        ana.repo.fixar(m.id, false) // o Repository pede a lista de novo sozinho
        esperar("fixadas atualizada sem a mensagem") { if (ana.estado.fixadas[canal]?.none { it.id == m.id } == true) true else null }

        val snap = ana.estado.snapshot!!
        val pedido = app.telacast.data.Busca.pedido("qwzx7788 de:eu", snap, canal, ana.estado.eu!!.id, escopoCanal = true)!!
        ana.repo.buscar(pedido)
        val achou = esperar("resultado da busca") { ana.estado.busca.hits.firstOrNull { it.message.id == m.id } }
        assertTrue(achou.channelName.isNotEmpty())
        assertTrue(ana.estado.busca.buscou)
        ana.repo.buscar(app.telacast.data.PedidoDeBusca("zzzinexistentezzz", "zzzinexistentezzz", canal, null, null))
        esperar("busca sem resultados") { if (ana.estado.busca.buscou && ana.estado.busca.hits.isEmpty() && !ana.estado.busca.carregando) true else null }
    }

    @Test fun senhaESessoes() {
        val eva = Cliente("eva_$sufixo").also { it.cadastrar() }
        eva.repo.pedirSessoes()
        val sessao = esperar("sessões") { eva.estado.sessoes.firstOrNull { it.current } }
        assertEquals(1, eva.estado.sessoes.size)
        assertTrue(sessao.id != 0L)
        eva.repo.trocarSenha("senha-errada-xx", "outra-senha-123")
        val erro = esperar("erro de senha atual") { eva.avisos.filterIsInstance<Aviso.Erro>().firstOrNull() }
        assertEquals(ErrorCode.WrongPassword, erro.codigo)
        eva.repo.trocarSenha("senha-eva_$sufixo", "outra-senha-123")
        esperar("senha trocada") { eva.avisos.filterIsInstance<Aviso.SenhaTrocada>().firstOrNull() }
        eva.repo.encerrarOutrasSessoes()
        eva.gateway.desconectar()
    }

    // ------------------------------------------------------------------ rede caindo e Resume

    @Test fun quedaDeRedeReconectaComResumeERecuperaMensagens() {
        val canal = novoCanal()
        val carlos = Cliente("carlos_$sufixo").also { it.cadastrar() } // conta própria: queda não afeta os outros testes
        carlos.repo.entrarNoServidor(codigoDoServidor)
        esperar("carlos no servidor") { carlos.estado.snapshot?.servers?.firstOrNull { it.id == servidor } }
        esperar("carlos vê o canal") { carlos.canalPorNome(canalNome(canal)) }
        carlos.repo.abrirCanal(canal)
        esperar("histórico") { if (carlos.estado.mensagens.containsKey(canal)) true else null }
        val token = carlos.gateway.token!!

        // a rede some (sem Logout)
        carlos.transporte.ultima!!.cancelar()
        // enquanto está fora: ele escreve (fica pendente) e a ana também escreve
        val nonce = carlos.repo.enviarMensagem(canal, "escrita offline")!!
        ana.repo.abrirCanal(canal)
        ana.repo.enviarMensagem(canal, "enquanto você estava fora")

        // volta sozinho, com Resume, e acerta tudo
        esperar("reconectou", ms = 15_000) { if (carlos.estado.conexao == EstadoConexao.Conectado && carlos.estado.pendentes.isEmpty()) true else null }
        assertEquals("mesmo token", token, carlos.gateway.token)
        esperar("recuperou a mensagem da ana") { carlos.estado.mensagens[canal]?.firstOrNull { it.content == "enquanto você estava fora" } }
        val minha = esperar("a minha saiu da fila com id real") { carlos.estado.mensagens[canal]?.firstOrNull { it.content == "escrita offline" } }
        assertNotEquals(-nonce, minha.id)
        assertTrue(minha.id > 0)
        val textos = carlos.texto(canal)
        assertEquals("sem duplicata", textos.distinct(), textos)
        // a ana também recebeu a dele exatamente uma vez
        esperar("ana recebeu") { ana.estado.mensagens[canal]?.firstOrNull { it.content == "escrita offline" } }
        assertEquals(1, ana.texto(canal).count { it == "escrita offline" })
        carlos.gateway.desconectar()
    }

    // ------------------------------------------------------------------ erros de entrada

    @Test fun senhaErradaEhFalhaDefinitiva() = runBlocking<Unit> {
        val c = Cliente("ana_$sufixo")
        c.repo.entrar(url!!, Credencial.Login("ana_$sufixo", "senha-errada-123"))
        val e = esperar("erro de entrada") { c.estado.erroDeEntrada }
        assertEquals(ErrorCode.InvalidCredentials, e.codigo)
        assertFalse(e.sessaoInvalida)
        assertEquals(EstadoConexao.Desconectado, withTimeout(5_000) { c.gateway.estado.first { it == EstadoConexao.Desconectado } })
    }

    @Test fun logoutRevogaOTokenESessaoExpiradaVaiParaOLogin() {
        val dani = Cliente("dani_$sufixo").also { it.cadastrar() }
        val token = dani.gateway.token!!
        dani.repo.sair()
        esperar("desconectou") { if (dani.estado.conexao == EstadoConexao.Desconectado) true else null }
        val outro = Cliente("dani_$sufixo")
        outro.repo.entrar(url!!, Credencial.Retomar(token))
        val e = esperar("sessão expirada") { outro.estado.erroDeEntrada }
        assertEquals(ErrorCode.SessionExpired, e.codigo)
        assertTrue(e.sessaoInvalida)
    }

    @Test fun cadastroComConviteInvalidoEhRecusado() {
        // conta como 1 tentativa de cadastro (o arquivo todo usa 9 das 10 permitidas por 5 min)
        val c = Cliente("fulano_$sufixo")
        c.repo.entrar(url!!, Credencial.Registro("fulano_$sufixo", "senha-fulano", "XXXX-0000"))
        val e = esperar("convite recusado") { c.estado.erroDeEntrada }
        assertEquals(ErrorCode.InvalidInvite, e.codigo)
    }

    // ------------------------------------------------------------------ voz

    private class DispositivoDeTeste : app.telacast.voz.DispositivoDeAudio {
        override fun iniciar(): String? = null
        override fun parar() = Unit
        override fun definirRota(rota: app.telacast.voz.RotaDeAudio) = Unit
        override fun aplicarOpcoes(opcoes: app.telacast.voz.OpcoesDeVoz) = Unit
    }

    /** Dois clientes reais no mesmo canal de voz: o áudio de um (seno gerado, codificado em Opus) chega ao outro, decodificado. */
    @Test fun vozDeUmClienteChegaAoOutro() {
        val nome = "voz-${System.nanoTime().toString().takeLast(5)}"
        ana.repo.criarCanal(servidor, nome, Channel.VOZ)
        val canal = esperar("canal de voz (ana)") { ana.canalPorNome(nome)?.id }
        esperar("canal de voz (bob)") { bob.canalPorNome(nome) }
        val anaId = ana.estado.eu!!.id
        val bobId = bob.estado.eu!!.id

        val motores = java.util.concurrent.ConcurrentHashMap<String, app.telacast.voz.MotorDeVoz>()
        fun controlador(c: Cliente) = app.telacast.voz.ControladorDeVoz(
            c.repo, escopo, app.telacast.voz.ConcentusOpus(),
            { motor, _, _, _ -> motores[c.nome] = motor; DispositivoDeTeste() },
            intervaloDoMedidorMs = 50,
        )
        val vozAna = controlador(ana)
        val vozBob = controlador(bob)
        try {
            vozAna.entrar(canal); vozBob.entrar(canal)
            esperar("ana conectada") { if (vozAna.estado.value.fase == app.telacast.voz.FaseDaVoz.Conectada) true else null }
            esperar("bob conectado") { if (vozBob.estado.value.fase == app.telacast.voz.FaseDaVoz.Conectada) true else null }
            esperar("o servidor mostra os dois na sala") { ana.estado.voz.count { it.channelId == canal }.takeIf { it == 2 } }
            val motorAna = motores.getValue(ana.nome)
            val motorBob = motores.getValue(bob.nome)

            // ana "fala" (seno de 440 Hz, 20 ms por quadro); bob "toca" no mesmo ritmo, como o AudioTrack faria
            val saida = ShortArray(app.telacast.voz.QUADRO)
            var maiorRmsDeBob = 0.0
            var maiorRmsDeAna = 0.0
            var bobViuAnaFalando = false
            for (q in 0 until 150) {
                val quadro = ShortArray(app.telacast.voz.QUADRO) { i ->
                    (0.3 * 32767 * kotlin.math.sin(2 * Math.PI * 440.0 * (q * app.telacast.voz.QUADRO + i) / app.telacast.voz.TAXA_DE_AMOSTRAGEM)).toInt().toShort()
                }
                motorAna.capturar(quadro)
                motorBob.reproduzir(saida)
                maiorRmsDeBob = maxOf(maiorRmsDeBob, Math.sqrt(saida.sumOf { it.toDouble() * it } / saida.size))
                motorAna.reproduzir(saida)
                maiorRmsDeAna = maxOf(maiorRmsDeAna, Math.sqrt(saida.sumOf { it.toDouble() * it } / saida.size))
                if (anaId in vozBob.estado.value.falando) bobViuAnaFalando = true
                Thread.sleep(20)
            }
            assertTrue("ana enviou pacotes (${motorAna.pacotesEnviados})", motorAna.pacotesEnviados > 50)
            assertEquals("nenhum pacote do servidor foi rejeitado", 0, motorBob.quadrosIgnorados)
            assertTrue("bob decodificou áudio com energia (rms $maiorRmsDeBob)", maiorRmsDeBob > 1_000)
            assertTrue("bob viu ana falando", bobViuAnaFalando)
            assertTrue("ana não ouve a si mesma (rms $maiorRmsDeAna)", maiorRmsDeAna < 1)

            // mudo de ana chega ao bob pelo servidor
            vozAna.alternarMudo()
            esperar("bob vê ana muda") { bob.estado.voz.firstOrNull { it.userId == anaId }?.muted?.takeIf { it } }
            vozAna.alternarSurdo()
            esperar("bob vê ana surda") { bob.estado.voz.firstOrNull { it.userId == anaId }?.deafened?.takeIf { it } }

            // sair: o servidor tira ana da sala e o estado local volta a Fora
            vozAna.sair()
            esperar("bob vê ana sair") { if (bob.estado.voz.none { it.userId == anaId && it.channelId == canal }) true else null }
            assertEquals(app.telacast.voz.FaseDaVoz.Fora, vozAna.estado.value.fase)
            assertTrue(bob.estado.voz.any { it.userId == bobId })
        } finally {
            vozAna.sair(); vozBob.sair()
        }
    }
}
