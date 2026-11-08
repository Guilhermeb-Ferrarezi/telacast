package app.telacast

import androidx.compose.runtime.CompositionLocalProvider
import app.cash.paparazzi.DeviceConfig
import app.cash.paparazzi.Paparazzi
import app.telacast.AmostrasDeConversa.agora
import app.telacast.data.AppState
import app.telacast.estado.Destino
import app.telacast.estado.Dialogo
import app.telacast.estado.Navegacao
import app.telacast.proto.VoiceState
import app.telacast.tema.GrafiteTheme
import app.telacast.ui.AcoesPrincipais
import app.telacast.ui.DadosDaConversa
import app.telacast.ui.DadosDeVoz
import app.telacast.ui.LocalProvedorDeAnexos
import app.telacast.ui.TelaPrincipal
import app.telacast.voz.EstadoDeVoz
import app.telacast.voz.FaseDaVoz
import app.telacast.voz.OpcoesDeVoz
import app.telacast.voz.RotaDeAudio
import com.android.resources.Density
import org.junit.Rule
import org.junit.Test

/** Imagens do chat de voz (C2): tela do canal, estados da chamada, cartão da gaveta, faixa, volume e opções. */
class VozSnapshotTest {
    @get:Rule
    val paparazzi = Paparazzi(
        deviceConfig = DeviceConfig.PIXEL_5.copy(screenWidth = 720, screenHeight = 1560, density = Density.XHIGH, xdpi = 320, ydpi = 320),
        theme = "Theme.Telacast",
        useDeviceResolution = true,
    )

    private val eu = Amostras.eu.id
    private val naTela = Navegacao(destino = Destino.Voz(Amostras.SV_TURMA, Amostras.SALA), servidorNoPainel = Amostras.SV_TURMA)
    private val emTexto = Navegacao(destino = Destino.Canal(Amostras.SV_TURMA, Amostras.AVISOS), servidorNoPainel = Amostras.SV_TURMA)

    private fun v(u: Long, mudo: Boolean = false, surdo: Boolean = false) = VoiceState(u, Amostras.SALA, mudo, surdo)

    /** Quem está na sala segundo o servidor. */
    private fun estado(vararg na: VoiceState): AppState =
        Amostras.estado.copy(voz = na.toList(), snapshot = Amostras.estado.snapshot!!.copy(voice = na.toList()))

    private val sala = arrayOf(v(eu), v(Amostras.marina.id), v(Amostras.joao.id, mudo = true), v(Amostras.bruno.id, mudo = true, surdo = true))

    private fun conectada() = EstadoDeVoz(
        fase = FaseDaVoz.Conectada, canal = Amostras.SALA, falando = setOf(Amostras.marina.id), euFalando = true, nivel = 0.45f,
        desdeMs = agora - 83_000, volumes = mapOf(Amostras.marina.id to 1.4f),
    )

    private fun tela(nome: String, estado: AppState, nav: Navegacao, voz: EstadoDeVoz, estatisticas: Boolean = false) {
        paparazzi.snapshot(nome) {
            GrafiteTheme {
                CompositionLocalProvider(LocalProvedorDeAnexos provides AmostrasDeConversa.provedor()) {
                    TelaPrincipal(
                        estado, nav, emptySet(), null, AcoesPrincipais(),
                        conversa = DadosDaConversa(agoraMs = agora, zona = AmostrasDeConversa.zona), voz = DadosDeVoz(voz, agora, estatisticas),
                    )
                }
            }
        }
    }

    @Test fun voz_conectada() = tela("voz_conectada", estado(*sala), naTela, conectada())
    @Test fun voz_estatisticas() = tela(
        "voz_estatisticas", estado(*sala), naTela,
        conectada().copy(estatisticas = app.telacast.voz.EstatisticasDeVoz(perdaPct = 1.8f, jitterMs = 7f, alvoMs = 100, bufferMs = 118f, cortesPorMin = 2, subfluxos = 1, nivelMic = 0.45f, ganhoEntradaDb = 0f, agcDb = 11.5f, probDeVoz = 0.87f, esticados = 3, comprimidos = 9, pacotesEnviados = 1840, usuarios = 2)),
        estatisticas = true,
    )
    @Test fun voz_eu_mudo() = tela("voz_eu_mudo", estado(*sala), naTela, conectada().copy(mudo = true, euFalando = false, nivel = 0f))
    @Test fun voz_eu_surdo() = tela("voz_eu_surdo", estado(*sala), naTela, conectada().copy(mudo = true, surdo = true, euFalando = false, nivel = 0f, rota = RotaDeAudio.Fone))
    @Test fun voz_conectando() = tela("voz_conectando", estado(v(Amostras.marina.id), v(Amostras.joao.id, mudo = true)), naTela, EstadoDeVoz(fase = FaseDaVoz.Entrando, canal = Amostras.SALA))
    @Test fun voz_reconectando() = tela("voz_reconectando", estado(*sala), naTela, conectada().copy(fase = FaseDaVoz.Reconectando, falando = emptySet(), euFalando = false, nivel = 0f))
    @Test fun voz_erro_de_permissao() = tela(
        "voz_erro_de_permissao", estado(v(Amostras.marina.id)), naTela,
        EstadoDeVoz(erro = "Sem a permissão do microfone não dá para entrar na voz. Libere em Ajustes do Android > Apps > Telacast."),
    )
    @Test fun voz_fora_do_canal() = tela("voz_fora_do_canal", estado(v(Amostras.marina.id), v(Amostras.joao.id, mudo = true)), naTela, EstadoDeVoz())
    @Test fun voz_canal_vazio() = tela("voz_canal_vazio", estado(), naTela, EstadoDeVoz())

    @Test fun voz_gaveta_com_cartao() = tela("voz_gaveta_com_cartao", estado(*sala), emTexto.abrirGaveta(), conectada())
    @Test fun voz_gaveta_cartao_mudo() = tela("voz_gaveta_cartao_mudo", estado(*sala), emTexto.abrirGaveta(), conectada().copy(mudo = true, euFalando = false))
    @Test fun voz_faixa_no_texto() = tela("voz_faixa_no_texto", estado(*sala), emTexto, conectada())
    @Test fun voz_faixa_reconectando() = tela("voz_faixa_reconectando", estado(*sala), emTexto, conectada().copy(fase = FaseDaVoz.Reconectando))
    @Test fun voz_folha_de_volume() = tela("voz_folha_de_volume", estado(*sala), naTela.abrirVolume(Amostras.marina.id), conectada())

    @Test fun ajustes_voz() {
        paparazzi.unsafeUpdateConfig(deviceConfig = DeviceConfig.PIXEL_5.copy(screenWidth = 720, screenHeight = 2350, density = Density.XHIGH, xdpi = 320, ydpi = 320))
        tela(
            "ajustes_voz", estado(*sala), emTexto.abrirDialogo(Dialogo.Ajustes),
            EstadoDeVoz(opcoes = OpcoesDeVoz(supressaoDeRuido = true, cancelamentoDeEco = true, ganhoAutomatico = false, sensibilidade = 0.7f, ganhoDeEntrada = 1.5f)),
        )
    }
}
