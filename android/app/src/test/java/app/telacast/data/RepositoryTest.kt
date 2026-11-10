package app.telacast.data

import app.telacast.proto.PROTO_VERSION

import app.telacast.net.Credencial
import app.telacast.net.FakeConexao
import app.telacast.net.FakeTransporte
import app.telacast.net.Gateway
import app.telacast.proto.C2S
import app.telacast.proto.ErrorCode
import app.telacast.proto.Fixtures
import app.telacast.proto.Message
import app.telacast.proto.Proto
import app.telacast.proto.Reaction
import app.telacast.proto.S2C
import app.telacast.proto.Snapshot
import app.telacast.proto.User
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class RepositoryTest {
    // Dados da fixture de Snapshot (a mesma do contrato): eu = guilherme (id 112623612862660608)
    private val snapshot: Snapshot = (Proto.lerS2C(Fixtures.texto("s2c", "Snapshot.json")) as S2C.Snapshot).snapshot
    private val eu: User = snapshot.me
    private val geral = 112623700000000012L
    private val memes = 112623700000000013L
    private val marina = 112623612862660617L

    /** Ids na escala do servidor (maiores que o `last_message_id` do snapshot da fixture). */
    private val base = 112624001234567905L

    private inner class Amb(val t: TestScope) {
        val transporte = FakeTransporte()
        val gateway = Gateway(t.backgroundScope, transporte, relogio = { t.currentTime })
        val repo = Repository(t.backgroundScope, gateway, relogio = { t.currentTime })
        val estado get() = repo.estado.value
        val c: FakeConexao get() = transporte.ultima

        /** Faz login e recebe o Snapshot. */
        fun entrar() {
            repo.entrar("h", Credencial.Login("guilherme", "senha1234"))
            t.runCurrent(); c.abre(); c.envia(S2C.Auth("tok", eu)); c.envia(S2C.Snapshot(snapshot)); t.runCurrent()
        }

        fun servidor(vararg m: S2C) { m.forEach { c.envia(it) }; t.runCurrent() }
        fun enviados() = c.c2s
        /** Tira da lista o que já foi visto, devolvendo só o que veio depois. */
        private var visto = 0
        fun novos(): List<C2S> = c.c2s.drop(visto).also { visto = c.c2s.size }
    }

    private fun TestScope.amb() = Amb(this)

    private fun msg(id: Long, canal: Long = geral, autor: Long = marina, texto: String = "m$id", nonce: Long = 0, kind: Int = 0) =
        Message(id, canal, autor, if (autor == eu.id) eu.name else "marina.souza", texto, id / 1000, false, nonce = nonce, kind = kind)

    // ------------------------------------------------------------------ não lidos

    @Test fun naoLidosEMencoesVemDoSnapshot() = runTest {
        val a = amb(); a.entrar()
        val s = a.estado
        assertTrue(s.autenticado)
        assertEquals(eu, s.eu)
        // geral: última > lida; memes: iguais; DM com marina: última > lida; categoria e voz não contam
        assertEquals(setOf(geral, 112623800000000001L), s.naoLidos)
        assertEquals(mapOf(geral to 2, 112623800000000001L to 1), s.mencoes)
        assertEquals(2, s.voz.size)
    }

    @Test fun abrirCanalMarcaComoLidoEPedeHistorico() = runTest {
        val a = amb(); a.entrar(); a.novos()
        a.repo.abrirCanal(geral); a.t.runCurrent()
        assertEquals(listOf<C2S>(C2S.Ack(geral, 112624001234567905), C2S.History(geral, null)), a.novos())
        assertFalse(geral in a.estado.naoLidos)
        assertFalse(a.estado.mencoes.containsKey(geral))
        assertEquals(geral, a.estado.canalAberto)
        // reabrir não pede o histórico de novo
        a.repo.abrirCanal(geral); a.t.runCurrent()
        assertEquals(listOf<C2S>(C2S.Ack(geral, 112624001234567905)), a.novos())
    }

    @Test fun historicoDoCanalAbertoMarcaComoLido() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral); a.novos()
        // mensagens que entraram depois do Snapshot: o `last_message_id` dele está velho, então o Ack sai com o id do histórico
        a.servidor(S2C.History(geral, listOf(msg(base + 5), msg(base + 9))))
        assertEquals(listOf<C2S>(C2S.Ack(geral, base + 9)), a.novos())
        // canal que não está aberto não confirma nada
        a.servidor(S2C.History(memes, listOf(msg(base + 3, canal = memes))))
        assertTrue(a.novos().isEmpty())
    }

    @Test fun snapshotNaoMarcaNaoLidoNoCanalAberto() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral); a.t.runCurrent()
        a.servidor(S2C.Snapshot(snapshot))
        assertFalse(geral in a.estado.naoLidos)
        assertFalse(a.estado.mencoes.containsKey(geral))
    }

    @Test fun mensagemDeOutroNoCanalAbertoFazAck_NoutroCanalViraNaoLida() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral); a.servidor(S2C.History(geral, listOf(msg(10))))
        a.novos()
        a.servidor(S2C.Message(msg(11)))
        assertEquals(listOf<C2S>(C2S.Ack(geral, 11)), a.novos())
        assertEquals(listOf(10L, 11L), a.estado.lista(geral).map { it.id })

        assertFalse(memes in a.estado.naoLidos)
        a.servidor(S2C.Message(msg(12, canal = memes, texto = "oi <@${eu.id}> e @everyone")))
        assertTrue(memes in a.estado.naoLidos)
        assertEquals(1, a.estado.mencoes[memes])
        a.servidor(S2C.Message(msg(13, canal = memes, texto = "sem menção")))
        assertEquals(1, a.estado.mencoes[memes])
        assertTrue("canal sem histórico carregado não guarda a mensagem", a.estado.mensagens[memes] == null)
    }

    // ------------------------------------------------------------------ envio otimista

    @Test fun envioOtimistaEEcoDoServidor() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral)
        a.servidor(S2C.History(geral, listOf(msg(10), msg(11))))
        a.novos()

        val nonce = a.repo.enviarMensagem(geral, "  olá, mundo  ")!!
        a.t.runCurrent()
        // mostra na hora, como provisória (id = -nonce), depois das reais
        val lista = a.estado.lista(geral)
        assertEquals(listOf(10L, 11L, -nonce), lista.map { it.id })
        assertEquals("olá, mundo", lista.last().content)
        assertEquals(eu.id, lista.last().authorId)
        assertEquals(listOf<C2S>(C2S.Send(geral, "olá, mundo", null, nonce, emptyList())), a.novos())
        assertTrue(nonce > 0 && nonce < (1L shl 62))

        // o eco (mesmo nonce, autor = eu) troca a provisória pela definitiva
        a.servidor(S2C.Message(msg(12, autor = eu.id, texto = "olá, mundo", nonce = nonce)))
        assertEquals(listOf(10L, 11L, 12L), a.estado.lista(geral).map { it.id })
        assertTrue(a.estado.pendentes.isEmpty())
        assertTrue("o próprio eco não gera Ack", a.novos().none { it is C2S.Ack && it.messageId == 12L })
    }

    @Test fun ecoComMesmoNonceDeOutroAutorNaoTiraAPendente() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral); a.servidor(S2C.History(geral, emptyList()))
        val nonce = a.repo.enviarMensagem(geral, "oi")!!
        a.servidor(S2C.Message(msg(20, autor = marina, texto = "oi", nonce = nonce)))
        assertEquals(1, a.estado.pendentes.size)
    }

    @Test fun nonceSempreDiferenteEPositivoMenorQue2a62() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral)
        val nonces = (1..2000).map { a.repo.enviarMensagem(geral, "m$it")!! }
        assertEquals(2000, nonces.toSet().size)
        assertTrue(nonces.all { it > 0 && it < (1L shl 62) })
        assertEquals("crescente", nonces.sorted(), nonces)
    }

    @Test fun textoInvalidoNaoEnviaNada() = runTest {
        val a = amb(); a.entrar(); a.novos()
        assertEquals(null, a.repo.enviarMensagem(geral, "   "))
        assertEquals(null, a.repo.enviarMensagem(geral, "x".repeat(4001)))
        assertTrue(a.estado.pendentes.isEmpty())
        assertTrue(a.novos().isEmpty())
        assertNotNull(a.repo.enviarMensagem(geral, "x".repeat(4000)))
    }

    @Test fun arrobaNomeViraTokenDeMencao() = runTest {
        val a = amb(); a.entrar()
        a.repo.enviarMensagem(geral, "oi @Marina.Souza, tudo bem? email a@b e @ninguem")
        val send = a.c.c2s.last() as C2S.Send
        assertEquals("oi <@$marina>, tudo bem? email a@b e @ninguem", send.content)
    }

    @Test fun respostaLevaOIdDaOriginal() = runTest {
        val a = amb(); a.entrar()
        a.repo.enviarMensagem(geral, "concordo", responderA = app.telacast.proto.Reply(99, "marina.souza", "algo"))
        assertEquals(99L, (a.c.c2s.last() as C2S.Send).replyTo)
        assertEquals(99L, a.estado.lista(geral).last().reply?.id)
    }

    // ------------------------------------------------------------------ reconexão e pendentes

    @Test fun pendenteFeitaOffline_SaiDepoisDeReconectar() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral); a.servidor(S2C.History(geral, listOf(msg(10))))
        a.c.cai(); a.t.runCurrent()
        val nonce = a.repo.enviarMensagem(geral, "offline")!!
        a.t.runCurrent()
        assertEquals("nada foi enviado ainda", 0, a.c.c2s.count { it is C2S.Send })
        assertEquals(listOf(10L, -nonce), a.estado.lista(geral).map { it.id })

        a.t.advanceTimeBy(1_000); a.t.runCurrent()
        a.c.abre(); a.servidor(S2C.Auth("tok", eu), S2C.Snapshot(snapshot))
        // conexão nova: Resume, recarrega o histórico do canal aberto e reenvia a pendente com o MESMO nonce
        val enviados = a.c.c2s
        assertEquals(C2S.Resume("tok", PROTO_VERSION), enviados[0])
        assertEquals(C2S.History(geral, null), enviados[1])
        assertEquals(C2S.Send(geral, "offline", null, nonce, emptyList()), enviados[2])
        // o servidor responde: histórico (sem a nova) e depois o eco
        a.servidor(S2C.History(geral, listOf(msg(10))))
        assertEquals("segue pendente até o eco", 1, a.estado.pendentes.size)
        a.servidor(S2C.Message(msg(11, autor = eu.id, texto = "offline", nonce = nonce)))
        assertEquals(listOf(10L, 11L), a.estado.lista(geral).map { it.id })
    }

    @Test fun historicoConciliaPendenteCujoEcoSePerdeu() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral); a.servidor(S2C.History(geral, listOf(msg(base + 1))))
        val nonce = a.repo.enviarMensagem(geral, "cheguei")!!
        // a primeira cópia chegou ao servidor, mas a conexão caiu antes do eco
        a.c.cai(); a.t.runCurrent()
        a.t.advanceTimeBy(1_000); a.t.runCurrent()
        a.c.abre(); a.servidor(S2C.Auth("tok", eu))
        // o servidor ignora o reenvio (nonce repetido) e o histórico já traz a mensagem, com nonce 0
        a.servidor(S2C.History(geral, listOf(msg(base + 1), msg(base + 2, autor = eu.id, texto = "cheguei", nonce = 0))))
        assertTrue("a pendente casou com a mensagem do histórico", a.estado.pendentes.isEmpty())
        assertEquals(listOf(base + 1, base + 2), a.estado.lista(geral).map { it.id })
        assertEquals(nonce, a.c.c2s.filterIsInstance<C2S.Send>().single().nonce)
    }

    @Test fun mensagemIgualMasAntigaNaoConciliaPendente() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral)
        a.servidor(S2C.History(geral, listOf(msg(base + 1, autor = eu.id, texto = "oi"))))
        a.repo.enviarMensagem(geral, "oi")
        a.c.cai(); a.t.runCurrent(); a.t.advanceTimeBy(1_000); a.t.runCurrent(); a.c.abre(); a.servidor(S2C.Auth("tok", eu))
        a.servidor(S2C.History(geral, listOf(msg(base + 1, autor = eu.id, texto = "oi"))))
        assertEquals("o `oi` antigo é anterior ao envio: a pendente continua", 1, a.estado.pendentes.size)
    }

    @Test fun descartarEReenviarPendente() = runTest {
        val a = amb(); a.entrar(); a.novos()
        val n = a.repo.enviarMensagem(geral, "x")!!
        a.novos()
        a.repo.reenviar(n)
        assertEquals(listOf<C2S>(C2S.Send(geral, "x", null, n, emptyList())), a.novos())
        a.repo.descartarPendente(n)
        assertTrue(a.estado.pendentes.isEmpty())
    }

    // ------------------------------------------------------------------ digitando

    @Test fun digitandoExpiraEm8Segundos() = runTest {
        val a = amb(); a.entrar()
        a.servidor(S2C.Typing(geral, marina))
        assertEquals(mapOf(geral to setOf(marina)), a.estado.digitando)
        a.t.advanceTimeBy(7_999); a.t.runCurrent()
        assertEquals(setOf(marina), a.estado.digitando[geral])
        // novo aviso renova o prazo
        a.servidor(S2C.Typing(geral, marina))
        a.t.advanceTimeBy(7_999); a.t.runCurrent()
        assertEquals(setOf(marina), a.estado.digitando[geral])
        a.t.advanceTimeBy(1); a.t.runCurrent()
        assertTrue(a.estado.digitando.isEmpty())
    }

    @Test fun digitandoSomeQuandoAPessoaEnviaMensagem() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral); a.servidor(S2C.History(geral, emptyList()))
        a.servidor(S2C.Typing(geral, marina), S2C.Message(msg(30)))
        assertTrue(a.estado.digitando.isEmpty())
    }

    @Test fun meuProprioTypingNaoApareceEAvisoTemIntervalo() = runTest {
        val a = amb(); a.entrar(); a.novos()
        a.servidor(S2C.Typing(geral, eu.id))
        assertTrue(a.estado.digitando.isEmpty())
        a.repo.avisarDigitando(geral); a.repo.avisarDigitando(geral)
        assertEquals(listOf<C2S>(C2S.Typing(geral)), a.novos())
        a.t.advanceTimeBy(4_000); a.repo.avisarDigitando(geral)
        assertEquals(listOf<C2S>(C2S.Typing(geral)), a.novos())
    }

    // ------------------------------------------------------------------ histórico e paginação

    @Test fun paginacaoDeHistorico() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral)
        val recentes = (101L..150L).map { msg(it) } // 50 = página cheia
        a.servidor(S2C.History(geral, recentes))
        assertEquals(true, a.estado.temMais[geral])
        a.novos()
        a.repo.carregarAnteriores(geral); a.repo.carregarAnteriores(geral) // o segundo é ignorado: já há pedido
        assertEquals(listOf<C2S>(C2S.History(geral, 101)), a.novos())

        val antigas = (51L..100L).map { msg(it) }
        a.servidor(S2C.History(geral, antigas))
        assertEquals((51L..150L).toList(), a.estado.lista(geral).map { it.id })
        assertEquals(true, a.estado.temMais[geral])
        assertTrue(geral !in a.estado.buscandoAntigas)

        a.repo.carregarAnteriores(geral)
        assertEquals(listOf<C2S>(C2S.History(geral, 51)), a.novos())
        a.servidor(S2C.History(geral, (40L..50L).map { msg(it) } + msg(51))) // repetida é ignorada; < 50 = chegou ao começo
        assertEquals((40L..150L).toList(), a.estado.lista(geral).map { it.id })
        assertEquals(false, a.estado.temMais[geral])
        a.repo.carregarAnteriores(geral)
        assertTrue("sem mais páginas não pede", a.novos().isEmpty())
    }

    @Test fun mensagemRepetidaPorEcoEHistoricoNaoDuplica() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral); a.servidor(S2C.History(geral, listOf(msg(10))))
        a.servidor(S2C.Message(msg(11)), S2C.Message(msg(11)), S2C.Message(msg(9)))
        assertEquals(listOf(9L, 10L, 11L), a.estado.lista(geral).map { it.id })
    }

    // ------------------------------------------------------------------ editar, apagar, reagir, fixar

    @Test fun editarApagarReagirFixar() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral)
        a.servidor(S2C.History(geral, listOf(msg(10), msg(11))))
        a.novos()
        a.repo.editar(10, "novo"); a.repo.apagar(11); a.repo.reagir(10, "👍", true); a.repo.fixar(10, true)
        assertEquals(
            listOf<C2S>(C2S.Edit(10, "novo"), C2S.Delete(11), C2S.React(10, "👍", true), C2S.Pin(10, true)),
            a.novos(),
        )

        a.servidor(S2C.MessageEdited(geral, 10, "novo"))
        assertEquals("novo", a.estado.lista(geral)[0].content)
        assertTrue(a.estado.lista(geral)[0].edited)

        a.servidor(S2C.Reaction(geral, 10, "👍", eu.id, true), S2C.Reaction(geral, 10, "👍", marina, true), S2C.Reaction(geral, 10, "🎉", marina, true))
        assertEquals(listOf(Reaction("👍", 2, true), Reaction("🎉", 1, false)), a.estado.lista(geral)[0].reactions)
        a.servidor(S2C.Reaction(geral, 10, "👍", eu.id, false), S2C.Reaction(geral, 10, "🎉", marina, false))
        assertEquals(listOf(Reaction("👍", 1, false)), a.estado.lista(geral)[0].reactions)

        a.servidor(S2C.Pinned(geral, 10, true))
        assertTrue(a.estado.lista(geral)[0].pinned)

        a.servidor(S2C.MessageDeleted(geral, 11))
        assertEquals(listOf(10L), a.estado.lista(geral).map { it.id })
    }

    // ------------------------------------------------------------------ erros, sessão, avisos

    @Test fun sessaoExpiradaVaiParaOErroDeEntrada() = runTest {
        val a = amb()
        a.repo.entrar("h", Credencial.Retomar("velho"))
        a.t.runCurrent(); a.c.abre(); a.c.envia(S2C.Error(ErrorCode.SessionExpired, "expirou")); a.t.runCurrent()
        val e = a.estado.erroDeEntrada!!
        assertTrue(e.sessaoInvalida)
        assertEquals(ErrorCode.SessionExpired, e.codigo)
        assertFalse(a.estado.autenticado)
    }

    @Test fun erroDoServidorViraAviso() = runTest {
        val a = amb(); a.entrar()
        val avisos = mutableListOf<Aviso>()
        a.t.backgroundScope.launch { a.repo.avisos.collect { avisos += it } }
        a.t.runCurrent()
        a.servidor(S2C.Error(ErrorCode.RateLimited, "devagar"), S2C.DmOpened(5), S2C.ServerJoined(6), S2C.PasswordChanged)
        assertEquals(
            listOf(Aviso.Erro(ErrorCode.RateLimited, "devagar"), Aviso.DmAberta(5), Aviso.ServidorEntrou(6), Aviso.SenhaTrocada),
            avisos,
        )
    }

    @Test fun sairLimpaOEstado() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral)
        a.repo.sair(); a.t.runCurrent()
        assertEquals(C2S.Logout, a.c.c2s.last())
        assertFalse(a.estado.autenticado)
        assertEquals(null, a.estado.snapshot)
        assertTrue(a.estado.naoLidos.isEmpty())
    }

    @Test fun quemTemOEstadoDeOutraContaComecaDoZero() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral)
        a.repo.sair(); a.t.runCurrent()
        a.repo.entrar("h", Credencial.Login("outra", "senha1234")); a.t.runCurrent()
        a.c.abre(); a.c.envia(S2C.Auth("t2", User(77, "outra", true))); a.t.runCurrent()
        assertEquals(77L, a.estado.eu?.id)
        assertEquals(null, a.estado.canalAberto)
    }

    // ------------------------------------------------------------------ marcador "novas mensagens" e salto (B4/B6)

    @Test fun abrirCanalComNaoLidasGuardaOMarcadorNoUltimoLido() = runTest {
        val a = amb(); a.entrar()
        a.repo.abrirCanal(geral)
        // snapshot da fixture: geral tem last_message_id 112624001234567905 e last_read_id 112624001234567890
        assertEquals(112624001234567890L, a.estado.marcadores[geral])
        a.repo.dispensarMarcador(geral)
        assertTrue(a.estado.marcadores.isEmpty())
        a.repo.abrirCanal(memes) // memes já estava lido
        assertTrue(memes !in a.estado.marcadores)
    }

    private fun paginaCheia(de: Long) = (de until de + 50).map { msg(it) }

    @Test fun saltoAcheiAMensagemJaCarregada() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral); a.servidor(S2C.History(geral, listOf(msg(10), msg(11))))
        a.repo.saltarPara(geral, 10)
        assertEquals(10L, a.estado.destaque?.id)
        a.repo.limparDestaque()
        assertNull(a.estado.destaque)
    }

    @Test fun saltoBuscaPaginasAntigasAteAchar() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral); a.servidor(S2C.History(geral, paginaCheia(1000)))
        a.novos()
        a.repo.saltarPara(geral, 920)
        assertEquals(listOf<C2S>(C2S.History(geral, 1000)), a.novos())
        a.servidor(S2C.History(geral, paginaCheia(950))) // 950..999: não tem a 920
        assertNull(a.estado.destaque)
        assertEquals(listOf<C2S>(C2S.History(geral, 950)), a.novos())
        a.servidor(S2C.History(geral, paginaCheia(900))) // 900..949: tem
        assertEquals(920L, a.estado.destaque?.id)
        assertNull(a.estado.salto)
        assertTrue(a.novos().isEmpty())
    }

    @Test fun saltoDesisteDepoisDe10PaginasOuQuandoOHistoricoAcaba() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral); a.servidor(S2C.History(geral, paginaCheia(10_000)))
        val avisos = mutableListOf<Aviso>()
        a.t.backgroundScope.launch { a.repo.avisos.collect { avisos += it } }
        a.repo.saltarPara(geral, 1)
        var base = 10_000L
        repeat(10) { a.t.runCurrent(); base -= 50; a.servidor(S2C.History(geral, paginaCheia(base))) }
        assertNull(a.estado.salto)
        assertNull(a.estado.destaque)
        assertEquals("exatamente 10 páginas pedidas", 10, a.c.c2s.count { it is C2S.History && it.before != null })
        assertTrue(avisos.any { it is Aviso.Texto && it.msg.contains("muito antiga") })

        // começo do histórico (página incompleta) também encerra
        val b = amb(); b.entrar(); b.repo.abrirCanal(geral); b.servidor(S2C.History(geral, paginaCheia(1000)))
        b.repo.saltarPara(geral, 5)
        b.servidor(S2C.History(geral, (990L..999L).map { msg(it) }))
        assertNull(b.estado.salto)
    }

    @Test fun saltoSemMaisPaginasAvisa() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral); a.servidor(S2C.History(geral, listOf(msg(10))))
        val avisos = mutableListOf<Aviso>()
        a.t.backgroundScope.launch { a.repo.avisos.collect { avisos += it } }
        a.t.runCurrent()
        a.repo.saltarPara(geral, 3); a.t.runCurrent()
        assertEquals(listOf<Aviso>(Aviso.Texto("A mensagem não está mais disponível")), avisos)
    }

    // ------------------------------------------------------------------ busca, fixadas e salto agendado (B8)

    private fun hit(id: Long, canal: Long = geral, texto: String = "t$id") =
        app.telacast.proto.SearchHit(msg(id, canal = canal, texto = texto), "geral")

    @Test fun buscaPedeEGuardaResultadosEPaginaComMostrarMais() = runTest {
        val a = amb(); a.entrar(); a.novos()
        val pedido = PedidoDeBusca("relatório", "relatório", geral, null, null)
        a.repo.buscar(pedido)
        val envio = a.novos().single() as C2S.Search
        assertEquals(C2S.Search(1, geral, null, null, "relatório", null), envio)
        assertTrue(a.estado.busca.carregando)
        a.servidor(S2C.SearchResults(1, listOf(hit(30), hit(29)), more = true))
        assertEquals(listOf(30L, 29L), a.estado.busca.hits.map { it.message.id })
        assertTrue(a.estado.busca.mais && a.estado.busca.buscou && !a.estado.busca.carregando)

        a.repo.buscar(pedido, mais = true)
        assertEquals("a próxima página começa depois da última listada", 29L, (a.novos().single() as C2S.Search).before)
        a.servidor(S2C.SearchResults(2, listOf(hit(28)), more = false))
        assertEquals(listOf(30L, 29L, 28L), a.estado.busca.hits.map { it.message.id })
        assertFalse(a.estado.busca.mais)
    }

    @Test fun respostaDeBuscaAntigaEIgnorada() = runTest {
        val a = amb(); a.entrar()
        a.repo.buscar(PedidoDeBusca("a", "a", geral, null, null))
        a.repo.buscar(PedidoDeBusca("b", "b", geral, null, null)) // nonce 2
        a.servidor(S2C.SearchResults(1, listOf(hit(1)), more = false))
        assertTrue("o nonce 1 já não vale", a.estado.busca.hits.isEmpty())
        a.servidor(S2C.SearchResults(2, emptyList(), more = false))
        assertTrue(a.estado.busca.buscou && a.estado.busca.hits.isEmpty())
        a.repo.limparBusca()
        assertFalse(a.estado.busca.buscou)
    }

    @Test fun fixarOuDesafixarAtualizaAListaDeFixadasJaPedida() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral); a.servidor(S2C.History(geral, listOf(msg(10))))
        a.repo.pedirFixadas(geral); a.servidor(S2C.Pins(geral, listOf(msg(10))))
        a.novos()
        a.servidor(S2C.Pinned(geral, 10, false))
        assertEquals(listOf<C2S>(C2S.Pins(geral)), a.novos())
        a.servidor(S2C.Pinned(memes, 5, true)) // canal sem lista pedida: não pede
        assertTrue(a.novos().isEmpty())
    }

    @Test fun saltoAgendadoParaOutroCanalSaiQuandoOHistoricoChega() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral); a.servidor(S2C.History(geral, listOf(msg(10))))
        a.repo.irParaMensagem(memes, 777)
        assertEquals(memes to 777L, a.estado.saltoAgendado)
        a.repo.abrirCanal(memes)
        a.servidor(S2C.History(memes, listOf(msg(776, canal = memes), msg(777, canal = memes))))
        assertEquals(777L, a.estado.destaque?.id)
        assertNull(a.estado.saltoAgendado)
    }

    @Test fun irParaMensagemNoCanalAbertoSaltaNaHora() = runTest {
        val a = amb(); a.entrar(); a.repo.abrirCanal(geral); a.servidor(S2C.History(geral, listOf(msg(10), msg(11))))
        a.repo.irParaMensagem(geral, 10)
        assertEquals(10L, a.estado.destaque?.id)
    }
}
