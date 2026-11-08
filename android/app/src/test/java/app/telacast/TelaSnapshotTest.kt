package app.telacast

import androidx.compose.runtime.CompositionLocalProvider
import app.cash.paparazzi.DeviceConfig
import app.cash.paparazzi.Paparazzi
import app.telacast.AmostrasDeConversa.agora
import app.telacast.data.AppState
import app.telacast.estado.Destino
import app.telacast.estado.Dialogo
import app.telacast.estado.Navegacao
import app.telacast.proto.Live
import app.telacast.proto.VoiceState
import app.telacast.tela.EstadoDaTransmissao
import app.telacast.tela.EstadoDoPlayer
import app.telacast.tela.FaseDaTransmissao
import app.telacast.tela.FaseDoPlayer
import app.telacast.tela.OpcoesDeTransmissao
import app.telacast.tema.GrafiteTheme
import app.telacast.ui.AcoesPrincipais
import app.telacast.ui.DadosDaConversa
import app.telacast.ui.DadosDeTela
import app.telacast.ui.DadosDeVoz
import app.telacast.ui.LocalProvedorDeAnexos
import app.telacast.ui.TelaPrincipal
import app.telacast.voz.EstadoDeVoz
import app.telacast.voz.FaseDaVoz
import com.android.resources.Density
import org.junit.Rule
import org.junit.Test

/** Imagens da tela compartilhada (C5): transmitir, assistir, faixa, cartão da gaveta, folha de opções e os estados do player. */
class TelaSnapshotTest {
    @get:Rule
    val paparazzi = Paparazzi(
        deviceConfig = DeviceConfig.PIXEL_5.copy(screenWidth = 720, screenHeight = 1560, density = Density.XHIGH, xdpi = 320, ydpi = 320),
        theme = "Theme.Telacast",
        useDeviceResolution = true,
    )

    private val eu = Amostras.eu.id
    private val naTela = Navegacao(destino = Destino.Voz(Amostras.SV_TURMA, Amostras.SALA), servidorNoPainel = Amostras.SV_TURMA)
    private val emTexto = Navegacao(destino = Destino.Canal(Amostras.SV_TURMA, Amostras.AVISOS), servidorNoPainel = Amostras.SV_TURMA)
    private val conectada = EstadoDeVoz(fase = FaseDaVoz.Conectada, canal = Amostras.SALA, desdeMs = agora - 83_000, euFalando = false)

    private fun estado(vararg ao_vivo: Long): AppState {
        val na = listOf(VoiceState(eu, Amostras.SALA, false, false), VoiceState(Amostras.marina.id, Amostras.SALA, false, false), VoiceState(Amostras.joao.id, Amostras.SALA, true, false))
        val base = Amostras.estado
        return base.copy(voz = na, snapshot = base.snapshot!!.copy(voice = na, live = ao_vivo.map { Live(Amostras.SALA, it) }))
    }

    private fun tela(nome: String, estado: AppState, nav: Navegacao, t: DadosDeTela = DadosDeTela(), voz: EstadoDeVoz = conectada, dp: Int = 1560) {
        if (dp != 1560) paparazzi.unsafeUpdateConfig(deviceConfig = DeviceConfig.PIXEL_5.copy(screenWidth = 720, screenHeight = dp, density = Density.XHIGH, xdpi = 320, ydpi = 320))
        paparazzi.snapshot(nome) {
            GrafiteTheme {
                CompositionLocalProvider(LocalProvedorDeAnexos provides AmostrasDeConversa.provedor()) {
                    TelaPrincipal(estado, nav, emptySet(), null, AcoesPrincipais(), conversa = DadosDaConversa(agoraMs = agora, zona = AmostrasDeConversa.zona), voz = DadosDeVoz(voz, agora), dadosDeTela = t)
                }
            }
        }
    }

    private fun transmitindo(espectadores: Int = 3) = EstadoDaTransmissao(FaseDaTransmissao.Transmitindo, Amostras.SALA, OpcoesDeTransmissao(), espectadores, desdeMs = agora - 60_000, tamanho = 720 to 1552, fpsReal = 29)

    @Test fun tela_voz_botao_transmitir() = tela("tela_voz_botao_transmitir", estado(), naTela)
    @Test fun tela_voz_alguem_ao_vivo() = tela("tela_voz_alguem_ao_vivo", estado(Amostras.marina.id), naTela)
    @Test fun tela_voz_eu_transmitindo() = tela("tela_voz_eu_transmitindo", estado(eu), naTela, DadosDeTela(transmitindo()))
    @Test fun tela_voz_transmissao_reconectando() = tela("tela_voz_transmissao_reconectando", estado(eu), naTela, DadosDeTela(transmitindo().copy(fase = FaseDaTransmissao.Reconectando, tentativa = 2)))
    @Test fun tela_voz_transmissao_erro() = tela(
        "tela_voz_transmissao_erro", estado(), naTela,
        DadosDeTela(EstadoDaTransmissao(FaseDaTransmissao.Erro, Amostras.SALA, mensagem = "já há uma transmissão neste canal")),
    )
    @Test fun tela_folha_de_opcoes() = tela("tela_folha_de_opcoes", estado(), naTela.abrirTransmitir(), DadosDeTela(EstadoDaTransmissao(fpsDisponiveis = listOf(15, 30))))
    @Test fun tela_folha_de_opcoes_no_ar() = tela("tela_folha_de_opcoes_no_ar", estado(eu), naTela.abrirTransmitir(), DadosDeTela(transmitindo().copy(opcoes = OpcoesDeTransmissao(fps = 60, kbps = 4000), fpsDisponiveis = listOf(15, 30, 60))))
    @Test fun tela_faixa_no_texto() = tela("tela_faixa_no_texto", estado(eu), emTexto, DadosDeTela(transmitindo(1)))
    @Test fun tela_gaveta_com_cartao() = tela("tela_gaveta_com_cartao", estado(eu), emTexto.abrirGaveta(), DadosDeTela(transmitindo()))

    private val assistir = naTela.abrirDialogo(Dialogo.Assistir(Amostras.SALA, Amostras.marina.id))
    @Test fun tela_player_ao_vivo() = tela("tela_player_ao_vivo", estado(Amostras.marina.id), assistir, DadosDeTela(player = EstadoDoPlayer(FaseDoPlayer.AoVivo, Amostras.SALA, 4, tamanho = 720 to 1552)))
    @Test fun tela_player_carregando() = tela("tela_player_carregando", estado(Amostras.marina.id), assistir, DadosDeTela(player = EstadoDoPlayer(FaseDoPlayer.Carregando, Amostras.SALA)))
    @Test fun tela_player_encerrou() = tela("tela_player_encerrou", estado(), assistir, DadosDeTela(player = EstadoDoPlayer(FaseDoPlayer.Encerrou, Amostras.SALA)))
    @Test fun tela_player_lotada() = tela("tela_player_lotada", estado(Amostras.marina.id), assistir, DadosDeTela(player = EstadoDoPlayer(FaseDoPlayer.Lotada, Amostras.SALA)))
    @Test fun tela_player_erro() = tela("tela_player_erro", estado(Amostras.marina.id), assistir, DadosDeTela(player = EstadoDoPlayer(FaseDoPlayer.Erro, Amostras.SALA, mensagem = "Você não tem acesso a esta transmissão.")))
}
