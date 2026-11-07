package app.telacast

import app.cash.paparazzi.DeviceConfig
import app.cash.paparazzi.Paparazzi
import app.telacast.Amostras.AVISOS
import app.telacast.Amostras.CAT_PROJETOS
import app.telacast.Amostras.DM_MARINA
import app.telacast.Amostras.GERAL
import app.telacast.Amostras.SV_TURMA
import app.telacast.estado.AbaInicio
import app.telacast.estado.Destino
import app.telacast.estado.Dialogo
import app.telacast.estado.FormLogin
import app.telacast.estado.MenuAberto
import app.telacast.estado.Navegacao
import app.telacast.net.EstadoConexao
import app.telacast.tema.GrafiteTheme
import app.telacast.ui.AcoesLogin
import app.telacast.ui.AcoesPrincipais
import app.telacast.ui.TelaDeCarregamento
import app.telacast.ui.TelaDeLogin
import app.telacast.ui.TelaPrincipal
import com.android.resources.Density
import org.junit.Rule
import org.junit.Test

/**
 * Imagens de cada tela e estado importante, sem rede (AppState falso). Os PNGs vão para `docs/telas/` pelo
 * `scripts/telas.sh`. Aparelho de 360 x 780 dp (720 x 1560 px), para os PNGs ficarem pequenos.
 */
class TelasSnapshotTest {
    @get:Rule
    val paparazzi = Paparazzi(
        deviceConfig = DeviceConfig.PIXEL_5.copy(screenWidth = 720, screenHeight = 1560, density = Density.XHIGH, xdpi = 320, ydpi = 320),
        theme = "Theme.Telacast",
        useDeviceResolution = true,
    )

    private fun tela(nome: String, conteudo: @androidx.compose.runtime.Composable () -> Unit) =
        paparazzi.snapshot(nome) { GrafiteTheme { conteudo() } }

    private val form = FormLogin(servidor = "10.0.2.2:8080", usuario = "guilherme")

    private fun principal(
        nome: String, nav: Navegacao, estado: app.telacast.data.AppState = Amostras.estado,
        recolhidas: Set<Long> = emptySet(), aviso: String? = null,
    ) = tela(nome) { TelaPrincipal(estado, nav, recolhidas, aviso, AcoesPrincipais()) }

    private val emCanal = Navegacao(destino = Destino.Canal(SV_TURMA, AVISOS), servidorNoPainel = SV_TURMA)

    // ---------------------------------------------------------------- B1 login
    @Test fun login_vazio() = tela("login_vazio") { TelaDeLogin(FormLogin(servidor = "10.0.2.2:8080"), AcoesLogin()) }
    @Test fun login_preenchido() = tela("login_preenchido") { TelaDeLogin(form.copy(senha = "senha1234"), AcoesLogin()) }
    @Test fun login_erro() = tela("login_erro") { TelaDeLogin(form.copy(senha = "errada123", erro = "Usuário ou senha incorretos."), AcoesLogin()) }
    @Test fun login_entrando() = tela("login_entrando") { TelaDeLogin(form.copy(senha = "senha1234", entrando = true), AcoesLogin()) }
    @Test fun login_registrar() = tela("login_registrar") {
        TelaDeLogin(form.copy(registrar = true, senha = "12345678", convite = "B4NZ-7TQK", erro = "Código de convite inválido, expirado ou já usado."), AcoesLogin())
    }
    @Test fun carregando() = tela("carregando") { TelaDeCarregamento(false, {}) }

    // ---------------------------------------------------------------- B2 e B3 gaveta, canais, menus
    @Test fun gaveta_servidor() = principal("gaveta_servidor", emCanal.copy(gavetaAberta = true))
    @Test fun gaveta_categoria_recolhida() = principal("gaveta_categoria_recolhida", emCanal.copy(gavetaAberta = true), recolhidas = setOf(CAT_PROJETOS, Amostras.CAT_TEXTO))
    @Test fun gaveta_menu_servidor() = principal("gaveta_menu_servidor", emCanal.copy(gavetaAberta = true, menu = MenuAberto.Servidor))
    @Test fun gaveta_menu_status() = principal("gaveta_menu_status", emCanal.copy(gavetaAberta = true, menu = MenuAberto.Status))
    @Test fun gaveta_inicio() = principal("gaveta_inicio", Navegacao(destino = Destino.Amigos, servidorNoPainel = null, gavetaAberta = true))
    @Test fun gaveta_inicio_dm() = principal("gaveta_inicio_dm", Navegacao(destino = Destino.Dm(DM_MARINA), servidorNoPainel = null, gavetaAberta = true))

    // ---------------------------------------------------------------- centro
    @Test fun centro_canal() = principal("centro_canal", emCanal.copy(destino = Destino.Canal(SV_TURMA, GERAL)))
    @Test fun centro_canal_reconectando() = principal(
        "centro_canal_reconectando", emCanal, estado = Amostras.estado.copy(conexao = EstadoConexao.Reconectando(2, 0)),
    )
    @Test fun centro_membros() = principal("centro_membros", emCanal.copy(membrosAbertos = true))
    @Test fun centro_dm() = principal("centro_dm", Navegacao(destino = Destino.Dm(DM_MARINA)))
    @Test fun centro_amigos() = principal("centro_amigos", Navegacao(destino = Destino.Amigos))
    @Test fun centro_amigos_pendentes() = principal("centro_amigos_pendentes", Navegacao(destino = Destino.Amigos, abaInicio = AbaInicio.Pendente))
    @Test fun centro_amigos_adicionar() = principal("centro_amigos_adicionar", Navegacao(destino = Destino.Amigos, abaInicio = AbaInicio.Adicionar), aviso = "Pedido de amizade enviado")

    // ---------------------------------------------------------------- diálogos
    @Test fun dialogo_criar_servidor() = principal("dialogo_criar_servidor", emCanal.copy(gavetaAberta = true, dialogo = Dialogo.AdicionarServidor(true)))
    @Test fun dialogo_entrar_servidor() = principal("dialogo_entrar_servidor", emCanal.copy(gavetaAberta = true, dialogo = Dialogo.AdicionarServidor(false)))
    @Test fun dialogo_convidar() = principal("dialogo_convidar", emCanal.copy(gavetaAberta = true, dialogo = Dialogo.Convidar("B4NZ-7TQK")))
    @Test fun dialogo_criar_canal() = principal("dialogo_criar_canal", emCanal.copy(gavetaAberta = true, dialogo = Dialogo.CriarCanal(SV_TURMA, false, Amostras.CAT_TEXTO)))
    @Test fun dialogo_sair() = principal("dialogo_sair", emCanal.copy(gavetaAberta = true, dialogo = Dialogo.ConfirmarSair))
}
