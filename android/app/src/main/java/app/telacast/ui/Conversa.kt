package app.telacast.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.ime
import androidx.compose.foundation.layout.navigationBars
import androidx.compose.foundation.layout.union
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import app.telacast.arquivos.AnexoEmEnvio
import app.telacast.data.AppState
import app.telacast.data.Autocompletar
import app.telacast.data.EstadoCompositor
import app.telacast.data.ItensDaLista
import app.telacast.data.textoDigitando
import app.telacast.proto.User
import app.telacast.tema.Grafite
import java.time.ZoneId

/** O estado só de tela da conversa (revelações, textos abertos, compositor, bandeja). */
class DadosDaConversa(
    val compositor: EstadoCompositor = EstadoCompositor(),
    val anexos: List<AnexoEmEnvio> = emptyList(),
    val spoilers: Set<Pair<Long, Int>> = emptySet(),
    val textosAbertos: Set<Long> = emptySet(),
    val reveladas: Set<Long> = emptySet(),
    val agoraMs: Long = System.currentTimeMillis(),
    val zona: ZoneId = ZoneId.systemDefault(),
    val compositorComFoco: Boolean = false,
    val consultaDaBusca: String = "",
    val buscaNoCanal: Boolean = false,
    val tamanhoDoTexto: Int = 1,
    val senhaTrocada: Int = 0,
)

class AcoesDaConversa(
    val lista: AcoesDaLista = AcoesDaLista(),
    val compositor: AcoesDoCompositor = AcoesDoCompositor(),
    val aoAbrirBusca: () -> Unit = {},
    val aoAbrirFixadas: () -> Unit = {},
)

/** Todos os usuários que o app conhece pelo snapshot (para resolver `<@id>` com o nome atual). */
fun nomesDeUsuarios(estado: AppState): Map<Long, String> {
    val sn = estado.snapshot ?: return emptyMap()
    val m = HashMap<Long, String>()
    sn.dms.forEach { m[it.user.id] = it.user.name }
    sn.friends.forEach { m[it.user.id] = it.user.name }
    sn.servers.forEach { s -> s.members.forEach { m[it.id] = it.name } }
    m[sn.me.id] = sn.me.name
    return m
}

fun membrosDoCanal(estado: AppState, canal: Long): List<User> {
    val sn = estado.snapshot ?: return emptyList()
    return sn.servers.firstOrNull { s -> s.channels.any { it.id == canal } }?.members
        ?: sn.dms.firstOrNull { it.channelId == canal }?.let { listOf(it.user, sn.me) } ?: emptyList()
}

/**
 * Conversa de um canal ou DM: barra do topo, lista de mensagens (B4/B6) e compositor (B5).
 * `titulo` desenha o conteúdo da barra (nome do canal ou da pessoa).
 */
@Composable
fun TelaDeConversa(
    estado: AppState,
    canal: Long,
    dica: String,
    aoMenu: () -> Unit,
    dados: DadosDaConversa,
    acoes: AcoesDaConversa,
    modifier: Modifier = Modifier,
    acaoDireita: (@Composable () -> Unit)? = null,
    titulo: @Composable RowScope.() -> Unit,
) {
    val eu = estado.eu ?: return
    val nomes = remember(estado.snapshot) { nomesDeUsuarios(estado) }
    val canais = remember(estado.snapshot) { estado.snapshot?.servers?.flatMap { it.channels }?.associate { it.id to it.name } ?: emptyMap() }
    val bloqueados = remember(estado.snapshot) { estado.snapshot?.blocked?.mapTo(HashSet()) { it.id } ?: emptySet<Long>() }
    val reais = estado.mensagens[canal]
    val pendentes = estado.pendentes
    val marcador = estado.marcadores[canal]
    val minuto = dados.agoraMs / 60_000
    val itens = remember(reais, pendentes, eu, bloqueados, dados.reveladas, marcador, minuto) {
        ItensDaLista.montar(estado.lista(canal), eu.id, bloqueados, dados.reveladas, marcador, dados.agoraMs, dados.zona)
    }
    val ctx = ContextoDaLista(
        eu = eu, agoraMs = dados.agoraMs, conectado = estado.conexao == app.telacast.net.EstadoConexao.Conectado,
        nomeDoUsuario = { nomes[it] }, nomeDoCanal = { canais[it] }, spoilers = dados.spoilers, textosAbertos = dados.textosAbertos,
        pendentes = pendentes.associateBy { it.nonce }, destaque = estado.destaque?.takeIf { it.canal == canal }, zona = dados.zona,
    )
    val sugestoes = Autocompletar.sugestoes(dados.compositor.valor.text, dados.compositor.valor.selection.end, membrosDoCanal(estado, canal), eu.id)
    val digitando = textoDigitando(estado.digitando[canal].orEmpty().filter { it != eu.id }.sorted().map { nomes[it] ?: "alguém" })

    Column(modifier.fillMaxSize().background(Grafite.L2)) {
        BarraSuperior(aoMenu, temNovidade(estado), acao = acaoDireita, titulo = titulo)
        key(canal) {
            ListaDeMensagens(
                itens, temMais = estado.temMais[canal] == true, carregandoAntigas = canal in estado.buscandoAntigas,
                ctx = ctx, acoes = acoes.lista, modifier = Modifier.weight(1f), estado = rememberLazyListState(),
            )
        }
        Compositor(
            dados.compositor, dica, dados.anexos, sugestoes, digitando, acoes.compositor,
            Modifier.windowInsetsPadding(WindowInsets.navigationBars.union(WindowInsets.ime)), comFoco = dados.compositorComFoco,
        )
    }
}
