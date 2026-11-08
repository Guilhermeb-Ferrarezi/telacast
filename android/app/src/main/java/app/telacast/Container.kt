package app.telacast

import app.telacast.armazenamento.CofreDeSegredos
import app.telacast.armazenamento.Preferencias
import androidx.compose.ui.graphics.ImageBitmap
import app.telacast.arquivos.ApiDeArquivos
import app.telacast.arquivos.Arquivos
import app.telacast.arquivos.CarregadorDeImagens
import app.telacast.arquivos.CarregadorDePrevias
import app.telacast.arquivos.EnviadorDeAnexos
import app.telacast.arquivos.OkHttpArquivos
import app.telacast.data.ControladorDoCompositor
import app.telacast.data.Repository
import app.telacast.push.ControladorDeFoco
import app.telacast.push.GerenciadorDePush
import app.telacast.push.Notificador
import app.telacast.push.NotificadorNulo
import app.telacast.push.ProvedorDeToken
import app.telacast.push.ReceptorDePush
import app.telacast.push.SemToken
import app.telacast.voz.ConcentusOpus
import app.telacast.voz.ControladorDeVoz
import app.telacast.voz.FabricaDeDispositivo
import app.telacast.voz.FabricaSemAudio
import app.telacast.voz.RotaDeAudio
import app.telacast.voz.ServicoDeVoz
import app.telacast.voz.ServicoDeVozNulo
import app.telacast.net.Gateway
import app.telacast.net.OkHttpTransporte
import app.telacast.net.Transporte
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn

/**
 * Contêiner de injeção de dependência manual (sem Hilt, KSP nem kapt): vive no `Application`.
 * Puro Kotlin (sem `Context`), então os testes montam um com armazenamento em memória e transporte de mentira.
 */
class Container(
    val preferencias: Preferencias,
    val cofre: CofreDeSegredos,
    /** Endereço sugerido quando o usuário ainda não digitou nenhum (vem do `BuildConfig`). */
    val servidorPadrao: String = "",
    transporte: Transporte = OkHttpTransporte(),
    val escopo: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Default),
    relogio: () -> Long = System::currentTimeMillis,
    val api: ApiDeArquivos = OkHttpArquivos(),
    /** Decodifica os bytes de uma imagem (no app: `BitmapFactory`); nos testes, qualquer coisa. */
    decodificarImagem: (ByteArray) -> ImageBitmap? = { null },
    /** Microfone e alto-falante (no app: `DispositivoAndroid`); nos testes, um falso. */
    fabricaDeDispositivo: FabricaDeDispositivo = FabricaSemAudio,
    servicoDeVoz: ServicoDeVoz = ServicoDeVozNulo,
    rotaInicialDaVoz: RotaDeAudio = RotaDeAudio.AltoFalante,
    /** Desenha as notificações de push (no app: `NotificadorAndroid`); nos testes, nenhum. */
    val notificador: Notificador = NotificadorNulo,
    /** Token do FCM (no app: `ProvedorFirebase`; sem credenciais ou nos testes: nenhum). */
    provedorDeToken: ProvedorDeToken = SemToken,
    /** RNNoise da voz (no app: `SupressorRnnoise`); `null` nos testes. */
    fabricaDeSupressor: app.telacast.voz.FabricaDeSupressor? = null,
    /** Captura de tela e decodificador de vídeo (no app: `FonteAndroid`, `DecodificadorAndroid`); nos testes, os sem tela. */
    fabricaDeFonte: app.telacast.tela.FabricaDeFonte = app.telacast.tela.FabricaDeFonte { app.telacast.tela.FonteSemCaptura },
    fabricaDeDecodificador: app.telacast.tela.FabricaDeDecodificador = app.telacast.tela.FabricaDeDecodificador { app.telacast.tela.DecodificadorSemVideo },
    transporteDeVideo: app.telacast.tela.TransporteDeVideo = app.telacast.tela.OkHttpTransporteDeVideo(),
    /** Atualização pelo app: chave pública (hex) do servidor; vazia = atualização desligada. */
    chaveDeAtualizacao: String = "",
    versaoDoApp: String = "0.0.0",
    instaladorDeApk: app.telacast.atualizacao.InstaladorDeApk = app.telacast.atualizacao.SemInstalador,
    pastaDeAtualizacao: java.io.File = java.io.File(System.getProperty("java.io.tmpdir") ?: ".", "telacast-atualizacao"),
    apiDeAtualizacao: app.telacast.atualizacao.ApiDeAtualizacao? = null,
    /** Relatório do último crash (no app, o arquivo privado). */
    val relatorioDeCrash: app.telacast.diagnostico.RelatorioDeCrash? = null,
    /** Serviço em primeiro plano "Telacast conectado" (no app: `FachadaDoServicoDeConexao`). */
    servicoDeConexao: app.telacast.conexao.ServicoPersistente = app.telacast.conexao.ServicoPersistenteNulo,
) {
    val gateway = Gateway(escopo, transporte, relogio)
    val repositorio = Repository(escopo, gateway, relogio)

    /** Endereço HTTP do servidor (anexos, `/upload`): o mesmo da conexão. */
    fun baseHttp(): String = Arquivos.urlHttp(preferencias.servidor.ifBlank { servidorPadrao })

    val enviador = EnviadorDeAnexos(api, escopo, ::baseHttp, token = { repositorio.token })
    val imagens = CarregadorDeImagens(api, escopo, decodificarImagem, { it.width.toLong() * it.height * 4 })
    val previas = CarregadorDePrevias(api)
    val compositor = ControladorDoCompositor(repositorio, preferencias)

    /** Chat de voz: máquina de estados, motor de áudio e serviço em primeiro plano. */
    val voz = ControladorDeVoz(
        repositorio, escopo, ConcentusOpus(), fabricaDeDispositivo, servicoDeVoz, relogio,
        opcoesIniciais = preferencias.opcoesDeVoz, rotaInicial = rotaInicialDaVoz, supressores = fabricaDeSupressor,
    )

    /** Push (D1): registro do token, foco (`SetFocus`) e a decisão de mostrar a notificação. */
    val push = GerenciadorDePush(repositorio, escopo, provedorDeToken, preferencias)
    val foco = ControladorDeFoco(repositorio, escopo)
    val receptorDePush = ReceptorDePush(notificador, { foco.emPrimeiroPlano }, relogio)

    /** Tela compartilhada (C5): transmitir a do celular (`/publish`) e assistir à dos outros (`/watch`). Só vale com a pessoa num canal de voz. */
    private val canalDaVoz = voz.estado.map { if (it.naChamada) it.canal else null }.stateIn(escopo, SharingStarted.Eagerly, null)
    val transmissao = app.telacast.tela.ControladorDeTransmissao(
        escopo, repositorio.estado, canalDaVoz, transporteDeVideo, fabricaDeFonte,
        { id -> repositorio.token?.let { app.telacast.tela.urlDeVideo(baseHttp(), "publish", id, it) } }, relogio,
    )
    val player = app.telacast.tela.ControladorDoPlayer(
        escopo, repositorio.estado, transporteDeVideo, fabricaDeDecodificador,
        { id -> repositorio.token?.let { app.telacast.tela.urlDeVideo(baseHttp(), "watch", id, it) } },
    )

    /** Atualização pelo próprio app (APK assinado pelo servidor, verificado antes de instalar). */
    val atualizacao = app.telacast.atualizacao.ControladorDeAtualizacao(
        escopo, apiDeAtualizacao ?: app.telacast.atualizacao.OkHttpAtualizacao({ baseHttp() }),
        app.telacast.atualizacao.VerificadorDeAtualizacao(app.telacast.atualizacao.hexParaBytes(chaveDeAtualizacao)),
        instaladorDeApk, versaoDoApp, pastaDeAtualizacao, { preferencias.versaoVista }, { preferencias.versaoVista = it }, relogio,
    )

    // ---- notificações locais e "Manter conectado" (C8)

    /** Opção "Manter conectado" (Ajustes > Notificações). */
    private val _manterConectado = kotlinx.coroutines.flow.MutableStateFlow(preferencias.manterConectado)
    val manterConectado: kotlinx.coroutines.flow.StateFlow<Boolean> = _manterConectado
    fun definirManterConectado(ligado: Boolean) { preferencias.manterConectado = ligado; _manterConectado.value = ligado }

    val conexaoPersistente = app.telacast.conexao.GerenciadorDaConexaoPersistente(escopo, repositorio.estado, manterConectado, servicoDeConexao)

    /** As mesmas regras do push do servidor, decididas aqui a partir do WebSocket vivo (sem Firebase); duplicatas com o FCM caem em `message_id`. */
    val receptorLocal = app.telacast.notificacao.ReceptorLocal(receptorDePush, { foco.emPrimeiroPlano }, relogio).also { r ->
        repositorio.aoMensagemAoVivo = r::aoMensagemAoVivo
        repositorio.aoPedidoDeAmizade = r::aoPedidoDeAmizade
    }

    /** Retoma a sessão salva sem a interface (o serviço reiniciado pelo sistema, ou depois de ligar o aparelho). `true` se começou a conectar. */
    fun retomarSessaoSalva(): Boolean {
        val estado = repositorio.estado.value
        if (estado.autenticado || estado.conexao !is app.telacast.net.EstadoConexao.Desconectado) return false
        val token = cofre.ler() ?: return false
        val servidor = preferencias.servidor.ifBlank { servidorPadrao }
        if (servidor.isBlank()) return false
        repositorio.entrar(servidor, app.telacast.net.Credencial.Retomar(token))
        return true
    }
}
