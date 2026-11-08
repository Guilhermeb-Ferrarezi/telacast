package app.telacast.tela

import app.telacast.data.AppState
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

/** Decodificador H.264 por hardware (no Android: `MediaCodec` desenhando numa `Surface`). Fica atrás de interface para os testes. */
interface DecodificadorDeVideo {
    /** `null` = pronto. `superficie` é a `Surface` do `SurfaceView` (opaca aqui). */
    fun configurar(superficie: Any, largura: Int, altura: Int): String?
    /** Entrega um quadro. `false` = o decodificador não aceitou (sem buffer livre ou quebrou): quem chama espera o próximo quadro-chave. */
    fun decodificar(h264: ByteArray, instanteUs: Long): Boolean
    fun fechar()
}

fun interface FabricaDeDecodificador { fun criar(): DecodificadorDeVideo }

enum class FaseDoPlayer { Inativo, Carregando, AoVivo, Encerrou, Lotada, Erro }

data class EstadoDoPlayer(
    val fase: FaseDoPlayer = FaseDoPlayer.Inativo,
    val canal: Long? = null,
    val espectadores: Int = 0,
    val mensagem: String? = null,
    /** Tamanho do vídeo (do SPS), para a tela reservar a proporção certa. */
    val tamanho: Pair<Int, Int>? = null,
)

/**
 * Quem ASSISTE: `/watch/{canal}` -> descarta tudo até o primeiro quadro-chave -> decodificador. Baixa latência: se o decodificador
 * não aceita um quadro, o resto até o próximo quadro-chave é descartado em vez de acumular. Muda o tamanho do vídeo (a tela de
 * quem transmite girou): o SPS novo chega num quadro-chave e o decodificador é recriado.
 */
class ControladorDoPlayer(
    private val escopo: CoroutineScope,
    private val estadoDoApp: StateFlow<AppState>,
    private val transporte: TransporteDeVideo,
    private val fabrica: FabricaDeDecodificador,
    private val urlDeAssistir: (Long) -> String?,
    private val relogio: () -> Long = System::nanoTime,
) {
    private val lock = Any()
    private val _estado = MutableStateFlow(EstadoDoPlayer())
    val estado: StateFlow<EstadoDoPlayer> = _estado.asStateFlow()

    private var conexao: ConexaoDeVideo? = null
    private var decodificador: DecodificadorDeVideo? = null
    private var superficie: Any? = null
    private var tamanhoDoDecodificador: Pair<Int, Int>? = null
    private val politica = PoliticaDeDecodificacao()
    private var geracao = 0

    init {
        escopo.launch {
            estadoDoApp.map { s -> _estado.value.canal?.let { s.espectadores[it]?.size } ?: 0 }.distinctUntilChanged().collect { n ->
                synchronized(lock) { if (_estado.value.fase != FaseDoPlayer.Inativo) atualizar { it.copy(espectadores = n) } }
            }
        }
    }

    private inline fun atualizar(f: (EstadoDoPlayer) -> EstadoDoPlayer) { _estado.value = f(_estado.value) }

    fun abrir(canal: Long) = synchronized(lock) {
        if (_estado.value.canal == canal && _estado.value.fase != FaseDoPlayer.Inativo) return
        encerrarInterno()
        val url = urlDeAssistir(canal)
        if (url == null) { atualizar { EstadoDoPlayer(FaseDoPlayer.Erro, canal, mensagem = "Sem sessão: entre de novo.") }; return }
        val minha = ++geracao
        atualizar { EstadoDoPlayer(FaseDoPlayer.Carregando, canal) }
        politica.exigirChave()
        conexao = transporte.abrir(url, object : OuvinteDeVideo {
            override fun aoAbrir() = Unit
            override fun aoTexto(texto: String) { synchronized(lock) { if (minha == geracao) aoTextoDoServidor(texto) } }
            override fun aoBinario(dados: ByteArray) = aoQuadro(minha, dados)
            override fun aoFim(motivo: String, http: Int?) { synchronized(lock) {
                if (minha != geracao) return@synchronized
                conexao = null
                val f = _estado.value.fase
                if (f == FaseDoPlayer.Lotada || f == FaseDoPlayer.Erro || f == FaseDoPlayer.Encerrou) return@synchronized
                when {
                    http == 403 -> atualizar { it.copy(fase = FaseDoPlayer.Erro, mensagem = "Você não tem acesso a esta transmissão.") }
                    http == 429 -> atualizar { it.copy(fase = FaseDoPlayer.Erro, mensagem = "Muitas conexões. Tente de novo em instantes.") }
                    f == FaseDoPlayer.AoVivo || f == FaseDoPlayer.Carregando && http == null -> atualizar { it.copy(fase = FaseDoPlayer.Encerrou, mensagem = null) }
                    else -> atualizar { it.copy(fase = FaseDoPlayer.Erro, mensagem = "A transmissão não iniciou: $motivo".take(120)) }
                }
            } }
        })
    }

    /** Textos do servidor (`PROTOCOL.md` 1.2): sem transmissão, lotada, ou a que já está no ar acabou. */
    private fun aoTextoDoServidor(texto: String) {
        decodificador?.fechar(); decodificador = null; tamanhoDoDecodificador = null
        when {
            texto.contains("máximo de espectadores") -> atualizar { it.copy(fase = FaseDoPlayer.Lotada, mensagem = "Esta transmissão está lotada.") }
            texto.contains("ninguém está transmitindo") -> atualizar { it.copy(fase = FaseDoPlayer.Encerrou, mensagem = "Ninguém está transmitindo neste canal.") }
            else -> atualizar { it.copy(fase = FaseDoPlayer.Erro, mensagem = texto.take(120)) }
        }
    }

    /** A `Surface` do vídeo nasceu (ou morreu: `null`). Sem superfície o decodificador fecha e o player espera o próximo quadro-chave. */
    fun definirSuperficie(s: Any?) = synchronized(lock) {
        superficie = s
        decodificador?.fechar(); decodificador = null; tamanhoDoDecodificador = null
        politica.exigirChave()
    }

    private fun aoQuadro(minha: Int, bytes: ByteArray) {
        val q = QuadroDeVideo.decodificar(bytes) ?: return
        synchronized(lock) {
            if (minha != geracao) return
            val fase = _estado.value.fase
            if (fase != FaseDoPlayer.Carregando && fase != FaseDoPlayer.AoVivo) return
            val surf = superficie ?: return // sem onde desenhar: ignora (o próximo quadro-chave recomeça)
            if (!politica.aceitar(q.chave)) return
            if (q.chave) {
                val dim = H264.dimensoes(q.h264)
                if (decodificador == null || (dim != null && dim != tamanhoDoDecodificador)) {
                    decodificador?.fechar()
                    val (l, a) = dim ?: (1280 to 720)
                    val d = fabrica.criar()
                    val erro = d.configurar(surf, l, a)
                    if (erro != null) { d.fechar(); decodificador = null; politica.exigirChave(); atualizar { it.copy(fase = FaseDoPlayer.Erro, mensagem = erro) }; return }
                    decodificador = d; tamanhoDoDecodificador = dim ?: (l to a)
                    atualizar { it.copy(tamanho = tamanhoDoDecodificador) }
                }
            }
            val d = decodificador ?: return
            if (!d.decodificar(q.h264, relogio() / 1000)) { politica.exigirChave(); return }
            if (fase != FaseDoPlayer.AoVivo) atualizar { it.copy(fase = FaseDoPlayer.AoVivo, mensagem = null) }
        }
    }

    fun parar() = synchronized(lock) { encerrarInterno(); atualizar { EstadoDoPlayer() } }

    private fun encerrarInterno() {
        geracao++
        runCatching { conexao?.cancelar() }; conexao = null
        decodificador?.fechar(); decodificador = null; tamanhoDoDecodificador = null
        politica.exigirChave()
    }
}
