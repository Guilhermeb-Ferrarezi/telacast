package app.telacast

import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.foundation.layout.Box
import app.cash.paparazzi.DeviceConfig
import app.cash.paparazzi.Paparazzi
import app.telacast.AmostrasDeConversa.agora
import app.telacast.atualizacao.EstadoDaAtualizacao
import app.telacast.atualizacao.FaseDaAtualizacao
import app.telacast.atualizacao.ManifestoDeAtualizacao
import app.telacast.estado.Destino
import app.telacast.estado.Navegacao
import app.telacast.tema.GrafiteTheme
import app.telacast.ui.AcoesPrincipais
import app.telacast.ui.DadosDaConversa
import app.telacast.ui.JanelaDeCrash
import app.telacast.ui.LocalProvedorDeAnexos
import app.telacast.ui.TelaPrincipal
import com.android.resources.Density
import org.junit.Rule
import org.junit.Test

/** Imagens de C7: faixa e folhas de atualização, permissão de instalar apps desconhecidos e o relatório de crash. */
class AtualizacaoSnapshotTest {
    @get:Rule
    val paparazzi = Paparazzi(
        deviceConfig = DeviceConfig.PIXEL_5.copy(screenWidth = 720, screenHeight = 1560, density = Density.XHIGH, xdpi = 320, ydpi = 320),
        theme = "Theme.Telacast",
        useDeviceResolution = true,
    )

    private val emCanal = Navegacao(destino = Destino.Canal(Amostras.SV_TURMA, Amostras.AVISOS), servidorNoPainel = Amostras.SV_TURMA)
    private val m = ManifestoDeAtualizacao("0.2.0", "android-arm64", "ab".repeat(32), 14_425_834, "Voz com menos cortes e supressor de ruído.\nTransmissão de tela do celular.\nCorreções na lista de mensagens.")

    private fun tela(nome: String, e: EstadoDaAtualizacao, extra: @androidx.compose.runtime.Composable () -> Unit = {}) {
        paparazzi.snapshot(nome) {
            GrafiteTheme {
                CompositionLocalProvider(LocalProvedorDeAnexos provides AmostrasDeConversa.provedor()) {
                    Box {
                        TelaPrincipal(AmostrasDeConversa.estado(AmostrasDeConversa.conversaBasica), emCanal, emptySet(), null, AcoesPrincipais(), conversa = DadosDaConversa(agoraMs = agora, zona = AmostrasDeConversa.zona), atualizacao = e)
                        extra()
                    }
                }
            }
        }
    }

    @Test fun atualizacao_faixa_disponivel() = tela("atualizacao_faixa_disponivel", EstadoDaAtualizacao(FaseDaAtualizacao.Disponivel, m))
    @Test fun atualizacao_faixa_baixando() = tela("atualizacao_faixa_baixando", EstadoDaAtualizacao(FaseDaAtualizacao.Baixando, m, progresso = 42))
    @Test fun atualizacao_faixa_pronta() = tela("atualizacao_faixa_pronta", EstadoDaAtualizacao(FaseDaAtualizacao.Pronta, m))
    @Test fun atualizacao_faixa_erro() = tela("atualizacao_faixa_erro", EstadoDaAtualizacao(FaseDaAtualizacao.Erro, m, mensagem = "O arquivo baixado não bate com o hash do manifesto."))
    @Test fun atualizacao_folha_o_que_mudou() = tela("atualizacao_folha_o_que_mudou", EstadoDaAtualizacao(FaseDaAtualizacao.Disponivel, m, folhaPendente = true))
    @Test fun atualizacao_permissao_fonte_desconhecida() = tela("atualizacao_permissao_fonte_desconhecida", EstadoDaAtualizacao(FaseDaAtualizacao.PrecisaDePermissao, m))

    @Test fun crash_relatorio() = tela("crash_relatorio", EstadoDaAtualizacao()) {
        JanelaDeCrash(
            "Telacast 0.1.0 (1) · Xiaomi Redmi Note · Android 17 (API 37)\nQuando: 2026-11-07T21:49:30Z · thread: main\nErro: java.lang.IllegalStateException: Fragmento sem ligação\n    em app.telacast.ui.RaizKt.ConteudoDoApp(Raiz.kt:120)\n    em androidx.compose.runtime.Recomposer.recompose(Recomposer.kt:300)",
            {}, {}, {},
        )
    }
}
