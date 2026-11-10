package app.telacast

import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.CompositionLocalProvider
import app.cash.paparazzi.DeviceConfig
import app.cash.paparazzi.Paparazzi
import app.telacast.AmostrasDeConversa.agora
import app.telacast.atualizacao.EstadoDaAtualizacao
import app.telacast.atualizacao.FaseDaAtualizacao
import app.telacast.atualizacao.ManifestoDeAtualizacao
import app.telacast.chamada.EstadoDaChamada
import app.telacast.estado.Destino
import app.telacast.estado.FormLogin
import app.telacast.estado.Navegacao
import app.telacast.proto.VoiceState
import app.telacast.tema.GrafiteTheme
import app.telacast.ui.AcoesDeAtualizacao
import app.telacast.ui.AcoesLogin
import app.telacast.ui.AcoesPrincipais
import app.telacast.ui.DadosDaConversa
import app.telacast.ui.DadosDeVoz
import app.telacast.ui.FaixaDeAtualizacao
import app.telacast.ui.LocalProvedorDeAnexos
import app.telacast.ui.TelaDesatualizada
import app.telacast.ui.TelaDeLogin
import app.telacast.ui.TelaPrincipal
import app.telacast.voz.EstadoDeVoz
import app.telacast.voz.FaseDaVoz
import com.android.resources.Density
import org.junit.Rule
import org.junit.Test

/** C9: chamadas em DM (toque, tela da chamada), app desatualizado e atualização na tela de login. */
class ChamadaSnapshotTest {
    @get:Rule
    val paparazzi = Paparazzi(
        deviceConfig = DeviceConfig.PIXEL_5.copy(screenWidth = 720, screenHeight = 1560, density = Density.XHIGH, xdpi = 320, ydpi = 320),
        theme = "Theme.Telacast", useDeviceResolution = true,
    )

    private val dm = Amostras.DM_MARINA
    private val m = ManifestoDeAtualizacao("0.1.1", "android-arm64", "ab".repeat(32), 14_000_000, "Chamadas em DM, áudio da transmissão e protocolo 2.")
    private val emDm = Navegacao(destino = Destino.Dm(dm))
    private val naChamada = Navegacao(destino = Destino.Voz(0L, dm))

    private fun tela(nome: String, nav: Navegacao, chamada: EstadoDaChamada = EstadoDaChamada.Nenhuma, voz: EstadoDeVoz = EstadoDeVoz(), nasala: List<VoiceState> = emptyList()) {
        val base = Amostras.estado
        val estado = base.copy(voz = nasala, snapshot = base.snapshot!!.copy(voice = nasala))
        paparazzi.snapshot(nome) {
            GrafiteTheme {
                CompositionLocalProvider(LocalProvedorDeAnexos provides AmostrasDeConversa.provedor()) {
                    TelaPrincipal(estado, nav, emptySet(), null, AcoesPrincipais(), conversa = DadosDaConversa(agoraMs = agora, zona = AmostrasDeConversa.zona), voz = DadosDeVoz(voz, agora), chamada = chamada)
                }
            }
        }
    }

    @Test fun chamada_recebendo() = tela("chamada_recebendo", emDm, EstadoDaChamada.Recebendo(dm, Amostras.marina.id, "marina.souza", agora))

    @Test fun chamada_dm_conectada() = tela(
        "chamada_dm_conectada", naChamada, voz = EstadoDeVoz(fase = FaseDaVoz.Conectada, canal = dm, desdeMs = agora - 42_000, falando = setOf(Amostras.marina.id)),
        nasala = listOf(VoiceState(Amostras.eu.id, dm, false, false), VoiceState(Amostras.marina.id, dm, false, false)),
    )

    @Test fun chamada_dm_chamando() = tela(
        "chamada_dm_chamando", naChamada, EstadoDaChamada.Chamando(dm, "marina.souza", agora), EstadoDeVoz(fase = FaseDaVoz.Conectada, canal = dm, desdeMs = agora - 5_000),
        listOf(VoiceState(Amostras.eu.id, dm, false, false)),
    )

    @Test fun app_desatualizado() = paparazzi.snapshot("app_desatualizado") {
        GrafiteTheme { TelaDesatualizada(EstadoDaAtualizacao(FaseDaAtualizacao.Disponivel, m), true, AcoesDeAtualizacao(), {}, {}) }
    }

    @Test fun app_desatualizado_sem_atualizacao_pelo_app() = paparazzi.snapshot("app_desatualizado_sem_atualizacao_pelo_app") {
        GrafiteTheme { TelaDesatualizada(EstadoDaAtualizacao(), false, AcoesDeAtualizacao(), {}, {}) }
    }

    @Test fun login_com_nova_versao() = paparazzi.snapshot("login_com_nova_versao") {
        GrafiteTheme {
            Column {
                FaixaDeAtualizacao(EstadoDaAtualizacao(FaseDaAtualizacao.Disponivel, m), AcoesDeAtualizacao())
                TelaDeLogin(FormLogin(servidor = "telecast.guilhermebf.dev", usuario = "", senha = ""), AcoesLogin())
            }
        }
    }
}
