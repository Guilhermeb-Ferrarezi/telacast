package app.telacast

import androidx.compose.runtime.CompositionLocalProvider
import app.cash.paparazzi.DeviceConfig
import app.cash.paparazzi.Paparazzi
import app.telacast.AmostrasDeConversa.agora
import app.telacast.AmostrasDeConversa.estado
import app.telacast.AmostrasDeConversa.m
import app.telacast.AmostrasDeConversa.ms
import app.telacast.data.AppState
import app.telacast.data.EstadoBusca
import app.telacast.estado.AbaInicio
import app.telacast.estado.Destino
import app.telacast.estado.Dialogo
import app.telacast.estado.Navegacao
import app.telacast.proto.Friend
import app.telacast.proto.FriendStatus
import app.telacast.proto.SearchHit
import app.telacast.proto.SessionInfo
import app.telacast.proto.Status
import app.telacast.proto.User
import app.telacast.tema.GrafiteTheme
import app.telacast.ui.AcoesPrincipais
import app.telacast.ui.DadosDaConversa
import app.telacast.ui.LocalProvedorDeAnexos
import app.telacast.ui.TelaPrincipal
import com.android.resources.Density
import org.junit.Rule
import org.junit.Test

/** Imagens de B7 (Início completo) e B8 (perfil, fixadas, busca, ajustes). `AppState` falso, sem rede. */
class PerfilBuscaSnapshotTest {
    @get:Rule
    val paparazzi = Paparazzi(
        deviceConfig = DeviceConfig.PIXEL_5.copy(screenWidth = 720, screenHeight = 1560, density = Density.XHIGH, xdpi = 320, ydpi = 320),
        theme = "Theme.Telacast",
        useDeviceResolution = true,
    )

    private val emCanal = Navegacao(destino = Destino.Canal(Amostras.SV_TURMA, Amostras.AVISOS), servidorNoPainel = Amostras.SV_TURMA)
    private val bloqueado = User(8, "spam.bot", false, Status.Offline)
    private val amigosExtras = Amostras.amigos + Friend(User(9, "Bia.Souza", false, Status.Offline), FriendStatus.Friend)

    private fun dados(consulta: String = "", noCanal: Boolean = false, tamanho: Int = 1, foco: Boolean = false) =
        DadosDaConversa(agoraMs = agora, zona = AmostrasDeConversa.zona, consultaDaBusca = consulta, buscaNoCanal = noCanal, tamanhoDoTexto = tamanho, compositorComFoco = foco)

    private fun tela(nome: String, estado: AppState, nav: Navegacao = emCanal, dados: DadosDaConversa = dados()) {
        paparazzi.snapshot(nome) {
            GrafiteTheme {
                CompositionLocalProvider(LocalProvedorDeAnexos provides AmostrasDeConversa.provedor()) {
                    TelaPrincipal(estado, nav, emptySet(), null, AcoesPrincipais(), conversa = dados)
                }
            }
        }
    }

    private fun comAmigos(bloqueados: List<User> = emptyList(), amigos: List<Friend> = amigosExtras, base: AppState = Amostras.estado): AppState =
        base.copy(snapshot = base.snapshot!!.copy(friends = amigos, blocked = bloqueados))

    private val inicio = Navegacao(destino = Destino.Amigos)

    // ---------------------------------------------------------------- B7: Início
    @Test fun amigos_disponiveis() = tela("amigos_disponiveis", comAmigos(), inicio.escolherAba(AbaInicio.Disponivel))
    @Test fun amigos_todos() = tela("amigos_todos", comAmigos(), inicio.escolherAba(AbaInicio.Todos))
    @Test fun amigos_pendentes() = tela("amigos_pendentes", comAmigos(), inicio.escolherAba(AbaInicio.Pendente))
    @Test fun amigos_bloqueados() = tela("amigos_bloqueados", comAmigos(listOf(bloqueado, User(10, "troll.123", false, Status.Offline))), inicio.escolherAba(AbaInicio.Bloqueados))
    @Test fun amigos_bloqueados_vazio() = tela("amigos_bloqueados_vazio", comAmigos(), inicio.escolherAba(AbaInicio.Bloqueados))
    @Test fun amigos_todos_vazio() = tela("amigos_todos_vazio", comAmigos(amigos = emptyList()), inicio.escolherAba(AbaInicio.Todos))
    @Test fun amigos_adicionar() = tela("amigos_adicionar", comAmigos(), inicio.escolherAba(AbaInicio.Adicionar))
    @Test fun dialogo_remover_amigo() = tela("dialogo_remover_amigo", comAmigos(), inicio.abrirDialogo(Dialogo.ConfirmarRemoverAmigo(2, "marina.souza")))
    @Test fun dialogo_bloquear() = tela("dialogo_bloquear", comAmigos(), inicio.abrirDialogo(Dialogo.ConfirmarBloquear(2, "marina.souza")))

    // ---------------------------------------------------------------- B8: perfil
    private fun conversa() = estado(AmostrasDeConversa.conversaBasica).let { comAmigos(listOf(bloqueado), base = it) }
    @Test fun perfil_amigo() = tela("perfil_amigo", conversa(), emCanal.abrirPerfil(Amostras.marina.id, "marina.souza"))
    @Test fun perfil_desconhecido() = tela("perfil_desconhecido", conversa(), emCanal.abrirPerfil(77, "visitante"))
    @Test fun perfil_pedido_recebido() = tela("perfil_pedido_recebido", conversa(), emCanal.abrirPerfil(Amostras.bruno.id, "bruno"))
    @Test fun perfil_bloqueado() = tela("perfil_bloqueado", conversa(), emCanal.abrirPerfil(bloqueado.id, "spam.bot"))
    @Test fun perfil_eu_dono() = tela("perfil_eu_dono", conversa(), emCanal.abrirPerfil(Amostras.eu.id, "guilherme"))

    // ---------------------------------------------------------------- B8: fixadas
    @Test fun fixadas() {
        val c = AmostrasDeConversa.conversaBasica
        tela("fixadas", estado(c).copy(fixadas = mapOf(Amostras.AVISOS to listOf(c[3], c[0]))), emCanal.abrirDialogo(Dialogo.Fixadas))
    }
    @Test fun fixadas_vazio() = tela("fixadas_vazio", estado(AmostrasDeConversa.conversaBasica).copy(fixadas = mapOf(Amostras.AVISOS to emptyList())), emCanal.abrirDialogo(Dialogo.Fixadas))

    // ---------------------------------------------------------------- B8: busca
    private val hits = listOf(
        SearchHit(m(9001, Amostras.marina, ms(7, 9, 30), "Fechamos o **relatório** hoje cedo, <@1>."), "avisos"),
        SearchHit(m(9002, Amostras.joao, ms(6, 21, 44), "Vi o relatório de ontem, ficou ótimo."), "geral"),
        SearchHit(m(9003, Amostras.eu, ms(2, 15, 2), "Subi a versão final do relatório no drive."), "memes-e-risadas"),
    )
    private val abrirBusca = emCanal.abrirDialogo(Dialogo.Busca)
    @Test fun busca_inicial() = tela("busca_inicial", estado(AmostrasDeConversa.conversaBasica), abrirBusca)
    @Test fun busca_resultados() = tela(
        "busca_resultados", estado(AmostrasDeConversa.conversaBasica).copy(busca = EstadoBusca("relatório", 1, hits, mais = true, buscou = true)),
        abrirBusca, dados("relatório"),
    )
    @Test fun busca_sem_resultados() = tela(
        "busca_sem_resultados", estado(AmostrasDeConversa.conversaBasica).copy(busca = EstadoBusca("zzz", 1, emptyList(), buscou = true)),
        abrirBusca, dados("zzz de:ana", noCanal = true),
    )
    @Test fun busca_buscando() = tela(
        "busca_buscando", estado(AmostrasDeConversa.conversaBasica).copy(busca = EstadoBusca("relatório", 1, carregando = true)), abrirBusca, dados("relatório"),
    )

    // ---------------------------------------------------------------- B8: ajustes
    private val sessoes = listOf(
        SessionInfo(1, "192.168.0.12", agora - 3 * 3_600_000, agora - 60_000, current = true),
        SessionInfo(2, "177.20.1.55", agora - 5 * 86_400_000L, agora - 2 * 3_600_000, current = false),
    )

    @Test fun ajustes() {
        paparazzi.unsafeUpdateConfig(deviceConfig = DeviceConfig.PIXEL_5.copy(screenWidth = 720, screenHeight = 2300, density = Density.XHIGH, xdpi = 320, ydpi = 320))
        tela("ajustes", estado(AmostrasDeConversa.conversaBasica).copy(sessoes = sessoes), emCanal.abrirDialogo(Dialogo.Ajustes), dados(tamanho = 2))
    }
}
