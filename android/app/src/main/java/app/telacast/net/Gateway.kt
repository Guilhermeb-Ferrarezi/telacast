package app.telacast.net

import app.telacast.proto.C2S
import app.telacast.proto.ErrorCode
import app.telacast.proto.PROTO_VERSION
import app.telacast.proto.Proto
import app.telacast.proto.S2C
import app.telacast.proto.User
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch

sealed interface EstadoConexao {
    data object Desconectado : EstadoConexao
    /** Abrindo a primeira conexão (ou autenticando). */
    data object Conectando : EstadoConexao
    /** Autenticado: já dá para mandar mensagens. */
    data object Conectado : EstadoConexao
    /** Caiu; `quandoMs` é o relógio (ms) em que a próxima tentativa começa. */
    data class Reconectando(val tentativa: Int, val quandoMs: Long) : EstadoConexao
}

sealed interface Credencial {
    data class Login(val usuario: String, val senha: String) : Credencial
    data class Registro(val usuario: String, val senha: String, val convite: String) : Credencial
    data class Retomar(val token: String) : Credencial
}

sealed interface EventoGateway {
    /** `Auth` recebido. `reconexao` = já havia autenticado antes nesta sessão do app (conexão nova após queda). */
    data class Autenticado(val token: String, val eu: User, val reconexao: Boolean) : EventoGateway

    /** Qualquer outra mensagem do servidor (já decodificada; variantes desconhecidas nunca chegam aqui). */
    data class Recebido(val msg: S2C) : EventoGateway

    /** Quadro binário (áudio de voz): entregue como veio; por ora ninguém usa. */
    class Binario(val bytes: ByteArray) : EventoGateway

    /**
     * A conexão acabou de vez, sem nova tentativa. `sessaoInvalida`: o token não vale mais (ir para o login).
     * `code` é nulo quando foi falha de rede.
     */
    data class Falha(val code: ErrorCode?, val msg: String, val sessaoInvalida: Boolean) : EventoGateway
}

/**
 * Conexão com `<servidor>/ws`: autentica (`Register`/`Login`/`Resume`), mantém o estado de conexão e reconecta
 * sozinho com recuo de 1, 2, 4, 8 e depois 15 s, retomando com `Resume(token)` (docs/PROTOCOL.md, seção 4).
 *
 * Puro Kotlin. Tudo o que muda de estado roda numa coroutine só (a do [scope]); [enviar] pode ser chamado de
 * qualquer thread. Os eventos saem por [eventos] (um consumidor só, nada se perde).
 */
class Gateway(
    private val scope: CoroutineScope,
    private val transporte: Transporte,
    private val relogio: () -> Long = System::currentTimeMillis,
    private val esperasMs: List<Long> = ESPERAS_PADRAO_MS,
) {
    private val _estado = MutableStateFlow<EstadoConexao>(EstadoConexao.Desconectado)
    val estado: StateFlow<EstadoConexao> = _estado.asStateFlow()

    private val _eventos = Channel<EventoGateway>(Channel.UNLIMITED)
    val eventos: Flow<EventoGateway> = _eventos.receiveAsFlow()

    /** Token da sessão (vale para o gateway e para o HTTP); `null` antes do primeiro `Auth` e depois de [sair]. */
    @Volatile var token: String? = null
        private set

    /** Quadros de texto que não decodificaram (lixo ou variante nova do servidor) e foram descartados. */
    @Volatile var descartados: Int = 0
        private set

    @Volatile private var conexao: ConexaoTransporte? = null
    @Volatile private var encerrando = false
    private var job: Job? = null
    private var jaAutenticou = false

    /** Começa (ou recomeça) a conexão. Uma chamada anterior ainda ativa é encerrada antes. */
    fun conectar(servidor: String, credencial: Credencial) {
        val antigo = job
        encerrando = false
        job = scope.launch {
            antigo?.cancelAndJoin()
            jaAutenticou = false
            token = (credencial as? Credencial.Retomar)?.token
            rodar(urlDoGateway(servidor), credencial)
        }
    }

    /** Manda uma mensagem. `false` se não está autenticado (quem chama guarda e reenvia depois). */
    fun enviar(msg: C2S): Boolean {
        val c = conexao ?: return false
        if (_estado.value != EstadoConexao.Conectado) return false
        return c.enviar(Proto.escreverC2S(msg))
    }

    /** `Logout` (revoga o token no servidor) e desconecta. */
    fun sair() {
        val c = conexao
        if (c != null && _estado.value == EstadoConexao.Conectado) c.enviar(Proto.escreverC2S(C2S.Logout))
        encerrando = true
        token = null
        if (c == null) {
            job?.cancel()
        } else {
            c.fechar()
            // se o outro lado nunca responder ao fechamento, não fico pendurado
            scope.launch { delay(3_000); if (encerrando) job?.cancel() }
        }
    }

    /** Fecha a conexão sem `Logout` (o token continua válido para um `Resume` futuro). */
    fun desconectar() {
        encerrando = true
        job?.cancel()
    }

    private sealed interface Fim {
        data class Caiu(val motivo: String, val autenticou: Boolean) : Fim
        data class Fatal(val falha: EventoGateway.Falha) : Fim
    }

    private suspend fun rodar(url: String, inicial: Credencial) {
        var tentativa = 0
        try {
            _estado.value = EstadoConexao.Conectando
            while (true) {
                val cred = token?.let { Credencial.Retomar(it) } ?: inicial
                val fim = sessao(url, cred)
                if (encerrando) break
                when (fim) {
                    is Fim.Fatal -> { _eventos.trySend(fim.falha); break }
                    is Fim.Caiu -> {
                        if (fim.autenticou) tentativa = 0
                        // Login/Registro que nem chegou a autenticar: avisa o usuário em vez de insistir com a senha
                        if (token == null && inicial !is Credencial.Retomar) {
                            _eventos.trySend(EventoGateway.Falha(null, "Sem conexão com o servidor (${fim.motivo})", false))
                            break
                        }
                        tentativa++
                        val espera = esperasMs[minOf(tentativa, esperasMs.size) - 1]
                        _estado.value = EstadoConexao.Reconectando(tentativa, relogio() + espera)
                        delay(espera)
                    }
                }
            }
        } finally {
            conexao?.cancelar()
            conexao = null
            _estado.value = EstadoConexao.Desconectado
        }
    }

    private sealed interface Ev {
        data object Abriu : Ev
        data class Texto(val s: String) : Ev
        class Binario(val b: ByteArray) : Ev
        data class Terminou(val motivo: String) : Ev
    }

    /** Uma conexão, do `abrir` até cair (ou até um erro que encerra tudo). */
    private suspend fun sessao(url: String, cred: Credencial): Fim {
        val fila = Channel<Ev>(Channel.UNLIMITED)
        val c = transporte.abrir(url, object : OuvinteTransporte {
            override fun aoAbrir() { fila.trySend(Ev.Abriu) }
            override fun aoTexto(texto: String) { fila.trySend(Ev.Texto(texto)) }
            override fun aoBinario(dados: ByteArray) { fila.trySend(Ev.Binario(dados)) }
            override fun aoFim(motivo: String) { fila.trySend(Ev.Terminou(motivo)) }
        })
        conexao = c
        var autenticou = false
        try {
            for (ev in fila) {
                when (ev) {
                    Ev.Abriu -> c.enviar(Proto.escreverC2S(mensagemDeEntrada(cred)))
                    is Ev.Binario -> _eventos.trySend(EventoGateway.Binario(ev.b))
                    is Ev.Terminou -> return Fim.Caiu(ev.motivo, autenticou)
                    is Ev.Texto -> {
                        val msg = Proto.lerS2COuNulo(ev.s)
                        when {
                            msg == null || msg is S2C.Desconhecida -> descartados++
                            msg is S2C.Auth -> {
                                autenticou = true
                                token = msg.token
                                _estado.value = EstadoConexao.Conectado
                                val reconexao = jaAutenticou
                                jaAutenticou = true
                                _eventos.trySend(EventoGateway.Autenticado(msg.token, msg.me, reconexao))
                            }
                            msg is S2C.Error && !autenticou -> return erroAntesDeAutenticar(msg, cred)
                            msg is S2C.Error && msg.code in SESSAO_INVALIDA ->
                                return Fim.Fatal(EventoGateway.Falha(msg.code, msg.msg, true))
                            autenticou -> _eventos.trySend(EventoGateway.Recebido(msg))
                            else -> descartados++ // nada vale antes do Auth
                        }
                    }
                }
            }
            return Fim.Caiu("conexão encerrada", autenticou)
        } finally {
            if (conexao === c) conexao = null
            c.cancelar()
        }
    }

    /**
     * Erro antes do `Auth`. Login/Registro: qualquer erro é definitivo (o usuário precisa ver). `Resume`: só os
     * códigos que invalidam a sessão voltam ao login; limite e falha interna são passageiros (tenta de novo).
     */
    private fun erroAntesDeAutenticar(e: S2C.Error, cred: Credencial): Fim {
        val retomando = cred is Credencial.Retomar
        return if (!retomando || e.code in SESSAO_INVALIDA) {
            Fim.Fatal(EventoGateway.Falha(e.code, e.msg, sessaoInvalida = retomando))
        } else {
            Fim.Caiu("erro passageiro: ${e.msg}", false)
        }
    }

    private fun mensagemDeEntrada(cred: Credencial): C2S = when (cred) {
        is Credencial.Login -> C2S.Login(cred.usuario, cred.senha, PROTO_VERSION)
        is Credencial.Registro -> C2S.Register(cred.usuario, cred.senha, cred.convite, PROTO_VERSION)
        is Credencial.Retomar -> C2S.Resume(cred.token, PROTO_VERSION)
    }

    companion object {
        /** 1, 2, 4, 8 e depois 15 s entre tentativas (igual ao desktop). */
        val ESPERAS_PADRAO_MS = listOf(1_000L, 2_000L, 4_000L, 8_000L, 15_000L)

        /** Códigos que dizem "esta sessão não vale mais" (docs/PROTOCOL.md, seção 4). */
        val SESSAO_INVALIDA = setOf(ErrorCode.SessionExpired, ErrorCode.NotAuthenticated, ErrorCode.VersionMismatch, ErrorCode.InvalidCredentials)

        /** `host:porta`, `http(s)://...` ou `ws(s)://...` viram `ws(s)://.../ws`. Sem esquema vale `ws://`. */
        fun urlDoGateway(servidor: String): String {
            var s = servidor.trim().trimEnd('/')
            s = when {
                s.startsWith("http://") -> "ws://" + s.removePrefix("http://")
                s.startsWith("https://") -> "wss://" + s.removePrefix("https://")
                s.startsWith("ws://") || s.startsWith("wss://") -> s
                else -> "ws://$s"
            }
            return if (s.endsWith("/ws")) s else "$s/ws"
        }
    }
}
