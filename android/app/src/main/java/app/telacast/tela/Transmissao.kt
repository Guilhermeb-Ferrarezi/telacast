package app.telacast.tela

import app.telacast.data.AppState
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

// ---------------------------------------------------------------------------------------------- transporte do vídeo

/** WebSocket de vídeo (`/publish` e `/watch`). Fica atrás de interface para a lógica rodar na JVM com um transporte de mentira. */
interface TransporteDeVideo {
    fun abrir(url: String, ouvinte: OuvinteDeVideo): ConexaoDeVideo
}

interface ConexaoDeVideo {
    /** Enfileira um quadro binário; `false` se a conexão já não aceita. Não bloqueia. */
    fun enviar(dados: ByteArray): Boolean
    /** Bytes enfileirados que ainda não saíram (a medida da lentidão da rede). */
    fun pendente(): Long
    fun fechar()
    fun cancelar()
}

/** Chamado da thread de rede: só repasse. `http` é o código da resposta quando o upgrade foi recusado (403, 429...). */
interface OuvinteDeVideo {
    fun aoAbrir()
    fun aoTexto(texto: String)
    fun aoBinario(dados: ByteArray)
    fun aoFim(motivo: String, http: Int? = null)
}

// ---------------------------------------------------------------------------------------------- captura e codificação

/** O que a captura (MediaProjection + VirtualDisplay + MediaCodec) entrega e avisa. Chamado da thread do codificador. */
interface OuvinteDaFonte {
    /** `ptsUs`: relógio da transmissão em microssegundos (monotônico, começa perto de 0), o mesmo que vai no cabeçalho do pacote. */
    fun aoQuadro(chave: Boolean, h264: ByteArray, ptsUs: Long)
    /** O vídeo mudou de tamanho (a tela girou): o codificador foi recriado e o primeiro quadro novo é um quadro-chave. */
    fun aoTamanho(largura: Int, altura: Int)
    /** A captura acabou (o sistema parou a projeção, o codificador quebrou). */
    fun aoFalhar(mensagem: String)
}

interface FonteDeTela {
    /** Frames por segundo que o codificador de hardware aceita (15 e 30 sempre; 60 só se o codec disser). */
    fun fpsSuportados(): List<Int>
    /**
     * Sobe o serviço em primeiro plano, a projeção e o codificador. `concessao` é o resultado da permissão da tela (no Android,
     * `Intent` + código). Bloqueia por pouco tempo: chamar fora da thread principal. `null` = deu certo; senão a mensagem.
     */
    fun iniciar(concessao: Any, opcoes: OpcoesDeTransmissao, ouvinte: OuvinteDaFonte): String?
    /** Muda qualidade/fps sem derrubar a projeção (nem pedir a permissão de novo). `null` = ok. */
    fun reconfigurar(opcoes: OpcoesDeTransmissao): String?
    /** Pede um quadro-chave já (espectador novo, fila de rede esvaziou). */
    fun pedirQuadroChave()
    fun parar()
}

fun interface FabricaDeFonte { fun criar(): FonteDeTela }

// ---------------------------------------------------------------------------------------------- estado

enum class FaseDaTransmissao { Inativa, PedindoPermissao, Conectando, Transmitindo, Reconectando, Erro }

data class EstadoDaTransmissao(
    val fase: FaseDaTransmissao = FaseDaTransmissao.Inativa,
    val canal: Long? = null,
    val opcoes: OpcoesDeTransmissao = OpcoesDeTransmissao(),
    val espectadores: Int = 0,
    /** Último erro ou aviso (permissão negada, canal já tem transmissão...). */
    val mensagem: String? = null,
    val desdeMs: Long? = null,
    val tamanho: Pair<Int, Int>? = null,
    /** Quadros por segundo realmente enviados no último segundo. */
    val fpsReal: Int = 0,
    val tentativa: Int = 0,
    val fpsDisponiveis: List<Int> = listOf(15, 30),
) {
    val ativa: Boolean get() = fase == FaseDaTransmissao.PedindoPermissao || fase == FaseDaTransmissao.Conectando ||
        fase == FaseDaTransmissao.Transmitindo || fase == FaseDaTransmissao.Reconectando
}

/**
 * Máquina de estados de quem TRANSMITE a tela: pedindo permissão -> conectando -> transmitindo (-> reconectando) / erro. A
 * captura e o socket entram por interface. Regras:
 * - só se transmite de um canal de voz em que a pessoa está ([canalDeVoz]); sair da voz para a transmissão;
 * - fila nunca cresce: [PoliticaDeEnvio] descarta e pede quadro-chave quando o socket está lento;
 * - queda do socket reconecta sozinha com espera crescente (a captura continua), e o primeiro quadro enviado depois é um quadro-chave;
 * - o servidor recusar (já há transmissão, 403) é erro definitivo, sem reconectar.
 */
class ControladorDeTransmissao(
    private val escopo: CoroutineScope,
    private val estadoDoApp: StateFlow<AppState>,
    private val canalDeVoz: StateFlow<Long?>,
    private val transporte: TransporteDeVideo,
    private val fabrica: FabricaDeFonte,
    /** `/publish/{canal}?token=` completo, ou `null` se não há sessão. */
    private val urlDePublicacao: (Long) -> String?,
    private val relogio: () -> Long = System::currentTimeMillis,
    private val despachante: CoroutineDispatcher = Dispatchers.Default,
    private val esperasDeReconexaoMs: List<Long> = listOf(1_000, 2_000, 4_000, 8_000, 8_000, 8_000, 8_000, 8_000),
    private val intervaloDePedidoDeChaveMs: Long = 500,
) {
    private val lock = Any()
    private val _estado = MutableStateFlow(EstadoDaTransmissao())
    val estado: StateFlow<EstadoDaTransmissao> = _estado.asStateFlow()

    private val politica = PoliticaDeEnvio()
    private var fonte: FonteDeTela? = null
    @Volatile private var conexao: ConexaoDeVideo? = null
    private var jobReconexao: Job? = null
    private var jobIniciar: Job? = null
    private var ultimoPedidoDeChave: Long? = null
    private var geracao = 0
    private val enviadosNoSegundo = java.util.concurrent.atomic.AtomicInteger(0)

    init {
        // quem transmite precisa continuar no canal de voz
        escopo.launch { canalDeVoz.collect { c -> synchronized(lock) { val atual = _estado.value; if (atual.ativa && c != atual.canal) pararInterno(null) } } }
        // contagem de espectadores (`S2C::Viewers`)
        escopo.launch {
            estadoDoApp.map { s -> _estado.value.canal?.let { s.espectadores[it]?.size } ?: 0 }.distinctUntilChanged().collect { n ->
                synchronized(lock) { if (_estado.value.ativa) atualizar { it.copy(espectadores = n) } }
            }
        }
        escopo.launch { while (true) { delay(1_000); synchronized(lock) { if (_estado.value.fase == FaseDaTransmissao.Transmitindo) atualizar { it.copy(fpsReal = enviadosNoSegundo.getAndSet(0)) } } } }
    }

    private inline fun atualizar(f: (EstadoDaTransmissao) -> EstadoDaTransmissao) { _estado.value = f(_estado.value) }

    // ------------------------------------------------------------------ comandos

    /** Começa: confere a voz e vai para [FaseDaTransmissao.PedindoPermissao]; a interface abre o diálogo do Android e responde com [permissaoConcedida] ou [permissaoNegada]. */
    fun pedir(canal: Long, opcoes: OpcoesDeTransmissao) = synchronized(lock) {
        if (_estado.value.ativa) return
        if (canalDeVoz.value != canal) { atualizar { it.copy(fase = FaseDaTransmissao.Erro, canal = canal, mensagem = "Entre no canal de voz para transmitir a tela.") }; return }
        atualizar { EstadoDaTransmissao(FaseDaTransmissao.PedindoPermissao, canal, opcoes, fpsDisponiveis = it.fpsDisponiveis) }
    }

    fun permissaoNegada() = synchronized(lock) {
        if (_estado.value.fase != FaseDaTransmissao.PedindoPermissao) return
        atualizar { it.copy(fase = FaseDaTransmissao.Inativa, canal = null, mensagem = "Sem a permissão de captura de tela, não dá para transmitir.") }
    }

    fun permissaoConcedida(concessao: Any) {
        val (canal, opcoes, minha) = synchronized(lock) {
            if (_estado.value.fase != FaseDaTransmissao.PedindoPermissao) return
            val c = _estado.value.canal ?: return
            atualizar { it.copy(fase = FaseDaTransmissao.Conectando) }
            Triple(c, _estado.value.opcoes, ++geracao)
        }
        jobIniciar = escopo.launch(despachante) {
            val f = fabrica.criar()
            val erro = f.iniciar(concessao, opcoes, OuvinteDaFonteImpl(minha)) // bloqueia um pouco: serviço em primeiro plano e projeção
            val aceita = synchronized(lock) {
                if (minha != geracao || _estado.value.fase != FaseDaTransmissao.Conectando) false
                else if (erro != null) { atualizar { it.copy(fase = FaseDaTransmissao.Erro, mensagem = erro) }; false }
                else {
                    fonte = f
                    atualizar { it.copy(fpsDisponiveis = f.fpsSuportados()) }
                    abrirConexao(canal)
                    true
                }
            }
            if (!aceita) runCatching { f.parar() } // fora do lock: parar espera a thread do codificador, que pode estar chamando de volta
        }
    }

    /** Muda qualidade/fps com a transmissão no ar (e guarda a escolha para a próxima). */
    fun aplicarOpcoes(opcoes: OpcoesDeTransmissao) = synchronized(lock) {
        val s = _estado.value
        if (!s.ativa || s.fase == FaseDaTransmissao.PedindoPermissao) { atualizar { it.copy(opcoes = opcoes) }; return }
        val erro = fonte?.reconfigurar(opcoes)
        if (erro != null) { atualizar { it.copy(mensagem = erro) }; return }
        politica.exigirChave()
        atualizar { it.copy(opcoes = opcoes) }
    }

    fun parar() = synchronized(lock) { pararInterno(null) }

    // ------------------------------------------------------------------ conexão

    private fun abrirConexao(canal: Long) {
        val url = urlDePublicacao(canal) ?: run { pararInterno("Sem sessão: entre de novo.", erro = true); return }
        val minha = geracao
        politica.exigirChave()
        conexao = transporte.abrir(url, object : OuvinteDeVideo {
            override fun aoAbrir() { synchronized(lock) {
                if (minha != geracao || !_estado.value.ativa) return@synchronized
                atualizar { it.copy(fase = FaseDaTransmissao.Transmitindo, tentativa = 0, mensagem = null, desdeMs = it.desdeMs ?: relogio()) }
                fonte?.pedirQuadroChave() // quem entra agora só decodifica a partir de um quadro-chave
            } }
            override fun aoTexto(texto: String) { synchronized(lock) { if (minha == geracao && _estado.value.ativa) pararInterno(texto.take(120), erro = true) } }
            override fun aoBinario(dados: ByteArray) = Unit
            override fun aoFim(motivo: String, http: Int?) { synchronized(lock) {
                if (minha != geracao || !_estado.value.ativa) return@synchronized
                conexao = null
                when (http) {
                    403 -> pararInterno("Você precisa estar neste canal de voz para transmitir.", erro = true)
                    429 -> pararInterno("Muitas conexões. Tente de novo em instantes.", erro = true)
                    else -> reconectar(canal)
                }
            } }
        })
    }

    private fun reconectar(canal: Long) {
        val tentativa = _estado.value.tentativa + 1
        if (tentativa > esperasDeReconexaoMs.size) { pararInterno("Não consegui reconectar a transmissão.", erro = true); return }
        atualizar { it.copy(fase = FaseDaTransmissao.Reconectando, tentativa = tentativa) }
        val minha = geracao
        jobReconexao?.cancel()
        jobReconexao = escopo.launch {
            delay(esperasDeReconexaoMs[tentativa - 1])
            synchronized(lock) { if (minha == geracao && _estado.value.fase == FaseDaTransmissao.Reconectando) abrirConexao(canal) }
        }
    }

    private fun pararInterno(mensagem: String?, erro: Boolean = false) {
        geracao++
        jobReconexao?.cancel(); jobReconexao = null
        jobIniciar?.cancel(); jobIniciar = null
        // `parar` da captura espera a thread do codificador, que chama de volta (aoQuadro, aoFalhar): faz fora do lock
        val antiga = fonte; fonte = null
        if (antiga != null) escopo.launch(despachante) { runCatching { antiga.parar() } }
        runCatching { conexao?.fechar() }; conexao = null
        atualizar { EstadoDaTransmissao(if (erro) FaseDaTransmissao.Erro else FaseDaTransmissao.Inativa, if (erro) it.canal else null, it.opcoes, mensagem = mensagem, fpsDisponiveis = it.fpsDisponiveis) }
    }

    // ------------------------------------------------------------------ quadros da captura

    private inner class OuvinteDaFonteImpl(private val minha: Int) : OuvinteDaFonte {
        override fun aoQuadro(chave: Boolean, h264: ByteArray, ptsUs: Long) {
            if (minha != geracao) return
            val c = conexao
            if (c == null || _estado.value.fase != FaseDaTransmissao.Transmitindo) { politica.exigirChave(); return }
            when (val d = politica.decidir(chave, c.pendente())) {
                PoliticaDeEnvio.Decisao.Enviar -> {
                    // pacote inválido derrubaria a transmissão no servidor: nunca manda; vazio/enorme descarta e pede quadro-chave
                    val pacote = PacoteDeMidia.codificar(if (chave) TipoDeMidia.H264Chave else TipoDeMidia.H264Delta, ptsUs, h264)
                    if (pacote != null && c.enviar(pacote)) enviadosNoSegundo.incrementAndGet() else { politica.exigirChave(); pedirChave() }
                }
                is PoliticaDeEnvio.Decisao.Descartar -> if (d.pedirChave) pedirChave()
            }
        }

        override fun aoTamanho(largura: Int, altura: Int) { synchronized(lock) { if (minha == geracao) atualizar { it.copy(tamanho = largura to altura) } } }

        override fun aoFalhar(mensagem: String) { synchronized(lock) { if (minha == geracao && _estado.value.ativa) pararInterno(mensagem, erro = true) } }
    }

    /** No máximo um pedido a cada [intervaloDePedidoDeChaveMs]: o codificador não precisa de uma rajada. */
    private fun pedirChave() {
        val agora = relogio()
        synchronized(lock) { val u = ultimoPedidoDeChave; if (u != null && agora - u < intervaloDePedidoDeChaveMs) return; ultimoPedidoDeChave = agora }
        fonte?.pedirQuadroChave()
    }
}
