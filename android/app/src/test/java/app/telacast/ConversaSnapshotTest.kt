package app.telacast

import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import app.cash.paparazzi.DeviceConfig
import app.cash.paparazzi.Paparazzi
import app.telacast.AmostrasDeConversa.agora
import app.telacast.AmostrasDeConversa.estado
import app.telacast.AmostrasDeConversa.m
import app.telacast.AmostrasDeConversa.ms
import app.telacast.arquivos.AnexoEmEnvio
import app.telacast.arquivos.EstadoDoEnvio
import app.telacast.arquivos.FonteEmMemoria
import app.telacast.data.Destaque
import app.telacast.data.EstadoCompositor
import app.telacast.data.Pendente
import app.telacast.estado.Destino
import app.telacast.estado.Dialogo
import app.telacast.estado.Navegacao
import app.telacast.net.EstadoConexao
import app.telacast.proto.Reply
import app.telacast.tema.GrafiteTheme
import app.telacast.ui.AcoesPrincipais
import app.telacast.ui.DadosDaConversa
import app.telacast.ui.LocalProvedorDeAnexos
import app.telacast.ui.TelaPrincipal
import com.android.resources.Density
import org.junit.Rule
import org.junit.Test

/**
 * Imagens da conversa (B4 a B6): lista de mensagens, markdown, anexos, estados de envio, compositor e folhas.
 * `AppState` falso, bitmaps gerados em código, sem rede. Os PNGs vão para `docs/telas/` por `scripts/telas.sh`.
 */
class ConversaSnapshotTest {
    @get:Rule
    val paparazzi = Paparazzi(
        deviceConfig = DeviceConfig.PIXEL_5.copy(screenWidth = 720, screenHeight = 1560, density = Density.XHIGH, xdpi = 320, ydpi = 320),
        theme = "Theme.Telacast",
        useDeviceResolution = true,
    )

    private val emCanal = Navegacao(destino = Destino.Canal(Amostras.SV_TURMA, Amostras.AVISOS), servidorNoPainel = Amostras.SV_TURMA)

    private fun dados(compositor: EstadoCompositor = EstadoCompositor(), anexos: List<AnexoEmEnvio> = emptyList(),
                      spoilers: Set<Pair<Long, Int>> = emptySet(), textosAbertos: Set<Long> = emptySet(), reveladas: Set<Long> = emptySet(), foco: Boolean = false) =
        DadosDaConversa(compositor, anexos, spoilers, textosAbertos, reveladas, agora, AmostrasDeConversa.zona, foco)

    private fun tela(nome: String, estado: app.telacast.data.AppState, nav: Navegacao = emCanal, dados: DadosDaConversa = dados(), aviso: String? = null, provedor: app.telacast.ui.ProvedorDeAnexos = AmostrasDeConversa.provedor()) {
        paparazzi.snapshot(nome) {
            GrafiteTheme {
                CompositionLocalProvider(LocalProvedorDeAnexos provides provedor) {
                    TelaPrincipal(estado, nav, emptySet(), aviso, AcoesPrincipais(), conversa = dados)
                }
            }
        }
    }

    private val eu = Amostras.eu
    private val marina = Amostras.marina
    private val joao = Amostras.joao
    private fun comp(texto: String = "", cursor: Int = texto.length, respondendo: Reply? = null, editando: Long? = null) =
        EstadoCompositor(Amostras.AVISOS, TextFieldValue(texto, TextRange(cursor)), respondendo, editando)

    // ---------------------------------------------------------------- B4: lista de mensagens
    @Test fun msgs_conversa() = tela("msgs_conversa", estado(AmostrasDeConversa.conversaBasica))
    @Test fun msgs_markdown() = tela("msgs_markdown", estado(AmostrasDeConversa.conversaMarkdown), dados = dados(spoilers = setOf(2003L to 1)))
    @Test fun msgs_anexo_imagem_arquivo() = tela("msgs_anexo_imagem_arquivo", estado(AmostrasDeConversa.conversaAnexos.take(1)))
    @Test fun msgs_anexos() = tela("msgs_anexos", estado(AmostrasDeConversa.conversaAnexos))
    @Test fun msgs_anexo_texto_expandido() = tela("msgs_anexo_texto_expandido", estado(AmostrasDeConversa.conversaAnexos.takeLast(1)), dados = dados(textosAbertos = setOf(AmostrasDeConversa.ID_TEXTO)))
    @Test fun msgs_vazio() = tela("msgs_vazio", estado(emptyList()))
    @Test fun msgs_destaque() = tela("msgs_destaque", estado(AmostrasDeConversa.conversaBasica, destaque = Destaque(Amostras.AVISOS, 1001, 0)))
    @Test fun msgs_carregando_antigas() = tela("msgs_carregando_antigas", estado(AmostrasDeConversa.conversaBasica.take(3), temMais = true, buscando = true))

    @Test fun msgs_pendentes() {
        val base = AmostrasDeConversa.conversaBasica.takeLast(2)
        val pend = listOf(
            Pendente(Amostras.AVISOS, 777, "Estou chegando! (enviando agora)", null, emptyList(), agora - 2_000, 1007),
            Pendente(Amostras.AVISOS, 778, "Essa não saiu: o servidor não respondeu", null, emptyList(), agora - 40_000, 1007),
        )
        tela("msgs_pendentes", estado(base, pendentes = pend))
    }

    @Test fun msgs_sem_conexao() {
        val pend = listOf(Pendente(Amostras.AVISOS, 779, "Escrevi isto sem rede, sai ao reconectar", null, emptyList(), agora - 5_000, 1007))
        tela("msgs_sem_conexao", estado(AmostrasDeConversa.conversaBasica.takeLast(2), pendentes = pend, conexao = EstadoConexao.Reconectando(1, agora + 1_000)))
    }

    @Test fun msgs_bloqueada_sistema_novas() {
        val msgs = listOf(
            m(4001, eu, ms(7, 9, 0), "Bom dia! Última coisa que eu li."),
            m(4002, Amostras.bruno, ms(7, 9, 5), "bruno entrou", kind = 2),
            m(4003, Amostras.carla, ms(7, 9, 6), "Mensagem de alguém que você bloqueou."),
            m(4004, marina, ms(7, 9, 10), "Fixei o cronograma pra gente.", pinned = false),
            m(4005, marina, ms(7, 9, 11), "4004", kind = 3),
            m(4006, joao, ms(7, 9, 20), "Valeu! Vou olhar agora."),
            m(4007, joao, ms(7, 9, 21), "Alguma dúvida me chama."),
        )
        tela("msgs_bloqueada_sistema_novas", estado(msgs, bloqueados = listOf(Amostras.carla), marcador = 4001))
    }

    // ---------------------------------------------------------------- B5: compositor
    private val poucas get() = AmostrasDeConversa.conversaBasica.takeLast(3)

    @Test fun compositor_vazio() = tela("compositor_vazio", estado(poucas))
    @Test fun compositor_texto_digitando() = tela(
        "compositor_texto_digitando", estado(poucas, digitando = setOf(marina.id, joao.id)),
        dados = dados(comp("Já estou terminando aqui, dá uma olhada depois"), foco = true),
    )
    @Test fun compositor_respondendo() = tela(
        "compositor_respondendo", estado(poucas),
        dados = dados(comp("Combinado!", respondendo = Reply(1007, "joao_pedro", "Alguém na reunião das 18h?")), foco = true),
    )
    @Test fun compositor_editando() = tela(
        "compositor_editando", estado(poucas + m(1008, eu, ms(7, 17, 55), "Eu vou, mas chego atrasado")),
        dados = dados(comp("Eu vou, mas chego um pouco atrasado", editando = 1008), foco = true),
    )
    @Test fun compositor_autocomplete() = tela(
        "compositor_autocomplete", estado(poucas), dados = dados(comp("Oi @"), foco = true),
    )
    @Test fun compositor_autocomplete_filtrado() = tela(
        "compositor_autocomplete_filtrado", estado(poucas), dados = dados(comp("Oi @ma e depois", cursor = 6), foco = true),
    )

    @Test fun compositor_bandeja() {
        val foto = AnexoEmEnvio(1, Amostras.AVISOS, FonteEmMemoria("praia.png", ByteArray(480_000), ehImagem = true), EstadoDoEnvio.Pronto(AmostrasDeConversa.anexoImagem))
        val pdf = AnexoEmEnvio(2, Amostras.AVISOS, FonteEmMemoria("relatório-final.pdf", ByteArray(1_048_576)), EstadoDoEnvio.Enviando(500_000))
        val grande = AnexoEmEnvio(3, Amostras.AVISOS, FonteEmMemoria("filme.mkv", ByteArray(10)), EstadoDoEnvio.Falhou("O arquivo tem 20.0 MB e o limite é 10.0 MB."))
        tela("compositor_bandeja", estado(poucas), dados = dados(comp("Segue o material"), anexos = listOf(foto, pdf, grande), foco = true))
    }

    // ---------------------------------------------------------------- B5: folha de ações, diálogos e visualizadores
    @Test fun folha_acoes_minha() = tela("folha_acoes_minha", estado(AmostrasDeConversa.conversaBasica), nav = emCanal.abrirFolha(1004))
    @Test fun folha_acoes_outro() = tela("folha_acoes_outro", estado(AmostrasDeConversa.conversaBasica), nav = emCanal.abrirFolha(1006))
    @Test fun folha_emojis() = tela("folha_emojis", estado(AmostrasDeConversa.conversaBasica), nav = emCanal.abrirFolha(1006, emojis = true))
    @Test fun dialogo_apagar_mensagem() = tela(
        "dialogo_apagar_mensagem", estado(AmostrasDeConversa.conversaBasica), nav = emCanal.abrirDialogo(Dialogo.ConfirmarApagar(1004, "Bom dia, pessoal! Fechei a revisão e subi as correções.")),
    )
    @Test fun imagem_tela_cheia() = tela("imagem_tela_cheia", estado(AmostrasDeConversa.conversaAnexos), nav = emCanal.abrirDialogo(Dialogo.Imagem(AmostrasDeConversa.anexoImagem)))
    @Test fun texto_tela_cheia() = tela("texto_tela_cheia", estado(AmostrasDeConversa.conversaAnexos), nav = emCanal.abrirDialogo(Dialogo.Texto(AmostrasDeConversa.anexoTexto)))

    // ---------------------------------------------------------------- C3: miniaturas
    @Test fun imagem_tela_cheia_so_miniatura() = tela(
        "imagem_tela_cheia_so_miniatura", estado(AmostrasDeConversa.conversaAnexos), nav = emCanal.abrirDialogo(Dialogo.Imagem(AmostrasDeConversa.anexoImagem)),
        provedor = AmostrasDeConversa.provedorSoMiniaturas(),
    )
    @Test fun foto_com_eixos_trocados_pelo_exif() = tela("foto_com_eixos_trocados_pelo_exif", estado(AmostrasDeConversa.conversaFotoExif))
    @Test fun lista_so_com_miniaturas() = tela("lista_so_com_miniaturas", estado(AmostrasDeConversa.conversaAnexos), provedor = AmostrasDeConversa.provedorSoMiniaturas())
}
