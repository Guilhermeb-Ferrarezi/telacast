package app.telacast.ui

import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.AnnotatedString
import app.telacast.arquivos.BaixadorAndroid
import app.telacast.arquivos.FonteUri
import kotlinx.coroutines.delay
import app.telacast.AppViewModel
import app.telacast.Fase
import app.telacast.estado.Dialogo
import app.telacast.estado.MenuAberto
import app.telacast.net.EstadoConexao

/** Liga o [AppViewModel] às telas sem estado. Também é onde mora o `BackHandler`. */
@Composable
fun RaizDoApp(vm: AppViewModel) {
    val fase by vm.fase.collectAsState()
    val estado by vm.estado.collectAsState()
    when (fase) {
        Fase.Login -> {
            val form by vm.form.collectAsState()
            TelaDeLogin(form, AcoesLogin(vm::editarForm, vm::entrar, vm::alternarModo))
        }
        Fase.Retomando -> TelaDeCarregamento(estado.conexao is EstadoConexao.Reconectando, vm::usarOutraConta)
        Fase.Principal -> {
            val nav by vm.nav.collectAsState()
            val recolhidas by vm.recolhidas.collectAsState()
            val aviso by vm.aviso.collectAsState()
            val compositor by vm.compositor.collectAsState()
            val anexos by vm.anexos.collectAsState()
            val spoilers by vm.spoilers.collectAsState()
            val textos by vm.textosAbertos.collectAsState()
            val reveladas by vm.reveladas.collectAsState()
            val consulta by vm.consultaDaBusca.collectAsState()
            val buscaNoCanal by vm.buscaNoCanal.collectAsState()
            val tamanhoTexto by vm.tamanhoDoTexto.collectAsState()
            val senhaTrocada by vm.senhaTrocada.collectAsState()
            val contexto = LocalContext.current
            val area = LocalClipboardManager.current
            val links = LocalUriHandler.current
            // só há relógio correndo enquanto existe mensagem pendente (para "Enviando…" virar "Não foi enviada.")
            val agora by produceState(System.currentTimeMillis(), estado.pendentes.isNotEmpty()) {
                value = System.currentTimeMillis()
                while (estado.pendentes.isNotEmpty()) { delay(1_000); value = System.currentTimeMillis() }
            }
            val seletor = rememberLauncherForActivityResult(ActivityResultContracts.GetMultipleContents()) { uris ->
                vm.anexar(uris.map { FonteUri(contexto.contentResolver, it) })
            }
            val baixador = remember(contexto) { BaixadorAndroid(contexto) }
            val provedor = remember(vm) {
                ProvedorDeAnexos(
                    imagemEmCache = { vm.imagens.emCache(it.id) },
                    imagem = { vm.imagens.obter(it, vm.urlDoAnexo(it)) },
                    previaEmCache = { vm.previas.emCache(it) },
                    previa = { vm.previas.obter(it, vm.urlDoAnexo(it)) },
                    miniatura = { f -> (f as? FonteUri)?.miniatura() },
                )
            }
            val baixar: (app.telacast.proto.Attachment) -> Unit = { a -> runCatching { baixador.baixar(vm.urlDoAnexo(a), a.name) }.onFailure { vm.mostrarAviso("Não consegui baixar o arquivo.") } }
            // Voltar do sistema fecha diálogo, folha, menu, painel de membros e gaveta (nessa ordem) antes de sair do app
            BackHandler(enabled = nav.podeVoltar) { vm.voltar() }
            CompositionLocalProvider(LocalProvedorDeAnexos provides provedor, LocalEscalaDoTexto provides escalaDoTexto(tamanhoTexto)) {
                TelaPrincipal(
                    estado, nav, recolhidas, aviso,
                    acoesDe(vm, copiar = { area.setText(AnnotatedString(it)) }, abrirLink = { runCatching { links.openUri(it) } }, anexar = { seletor.launch("*/*") }, baixar = baixar),
                    conversa = DadosDaConversa(compositor, anexos, spoilers, textos, reveladas, agora, consultaDaBusca = consulta, buscaNoCanal = buscaNoCanal, tamanhoDoTexto = tamanhoTexto, senhaTrocada = senhaTrocada),
                )
            }
        }
    }
}

private fun acoesDe(
    vm: AppViewModel,
    copiar: (String) -> Unit,
    abrirLink: (String) -> Unit,
    anexar: () -> Unit,
    baixar: (app.telacast.proto.Attachment) -> Unit,
) = AcoesPrincipais(
    painel = AcoesPainel(
        aoAbrirCanal = vm::abrirCanal,
        aoAlternarCategoria = vm::alternarCategoria,
        aoCliqueEmVoz = vm::cliqueEmVoz,
        aoAbrirMenuServidor = { vm.abrirMenu(MenuAberto.Servidor) },
        aoAbrirMenuStatus = { vm.abrirMenu(MenuAberto.Status) },
        aoCriarCanal = { sv, categoria, pai -> vm.abrirDialogo(Dialogo.CriarCanal(sv, categoria, pai)) },
        aoAbrirAmigos = vm::abrirAmigos,
        aoAbrirDm = vm::abrirDm,
    ),
    dialogo = AcoesDialogo(
        aoFechar = vm::fecharDialogo,
        aoCriarServidor = vm::criarServidor,
        aoEntrarNoServidor = vm::entrarNoServidor,
        aoTrocarModo = { criar -> vm.abrirDialogo(Dialogo.AdicionarServidor(criar)) },
        aoCriarCanal = vm::criarCanal,
        aoConfirmarSaida = vm::sair,
        aoApagarMensagem = vm::apagarMensagem,
        aoBaixarAnexo = baixar,
        aoBloquear = vm::bloquear,
        aoRemoverAmigo = vm::desfazerAmizade,
    ),
    amigos = AcoesAmigos(
        aoEscolherAba = vm::escolherAba,
        aoPedirAmizade = { vm.pedirAmizade(it); vm.mostrarAviso("Pedido de amizade enviado") },
        aoAceitar = vm::aceitarAmigo,
        aoRecusar = vm::removerAmigo,
        aoConversar = vm::abrirDmCom,
        aoAbrirPerfil = vm::abrirPerfil,
        aoDesbloquear = vm::desbloquear,
    ),
    aoAbrirGaveta = vm::abrirGaveta,
    aoFecharGaveta = vm::fecharGaveta,
    aoAbrirMembros = vm::abrirMembros,
    aoFecharMembros = vm::fecharMembros,
    aoFecharMenu = vm::fecharMenu,
    aoAbrirInicio = { vm.selecionarServidor(null) },
    aoSelecionarServidor = vm::selecionarServidor,
    aoAdicionarServidor = { vm.abrirDialogo(Dialogo.AdicionarServidor(true)) },
    aoConvidar = { vm.abrirDialogo(Dialogo.Convidar(it)) },
    aoEscolherStatus = vm::definirStatus,
    aoPedirSaida = { vm.abrirDialogo(Dialogo.ConfirmarSair) },
    conversa = AcoesDaConversa(
        lista = AcoesDaLista(
            aoPressionar = vm::abrirFolha,
            aoSaltar = vm::saltarPara,
            aoReagir = vm::reagir,
            aoMaisReacoes = vm::maisEmojis,
            aoReenviar = vm::reenviar,
            aoDescartar = vm::descartar,
            aoMostrarBloqueada = vm::mostrarBloqueada,
            aoSpoiler = vm::alternarSpoiler,
            aoLink = abrirLink,
            aoUsuario = vm::abrirPerfilDe,
            aoAbrirPerfil = vm::abrirPerfil,
            aoVerFixadas = vm::abrirFixadas,
            anexos = AcoesDeAnexo(aoAbrirImagem = vm::abrirImagem, aoBaixar = baixar, aoAbrirTexto = vm::abrirTexto, aoAlternarTexto = vm::alternarTexto),
            aoCarregarAnteriores = { vm.estado.value.canalAberto?.let { vm.carregarAnteriores(it) } },
            aoDestaqueTerminou = vm::limparDestaque,
        ),
        compositor = AcoesDoCompositor(
            aoMudar = vm::mudarCompositor, aoEnviar = vm::enviarMensagem, aoAnexar = anexar,
            aoCancelarResposta = vm::cancelarResposta, aoCancelarEdicao = vm::cancelarEdicao,
            aoRemoverAnexo = vm::removerAnexo, aoTentarAnexo = vm::tentarAnexo, aoEscolherSugestao = vm::escolherSugestao,
        ),
        aoAbrirBusca = vm::abrirBusca,
        aoAbrirFixadas = vm::abrirFixadas,
    ),
    perfil = AcoesDoPerfil(
        aoFechar = vm::fecharPerfil,
        aoConversar = vm::conversarCom,
        aoAdicionar = vm::adicionarAmigo,
        aoAceitar = { vm.aceitarAmigo(it); vm.fecharPerfil() },
        aoDesfazerAmizade = vm::pedirDesfazerAmizade,
        aoCancelarPedido = { vm.removerAmigo(it); vm.fecharPerfil() },
        aoBloquear = vm::pedirBloquear,
        aoDesbloquear = vm::desbloquear,
        aoCopiarId = { copiar(it.toString()); vm.fecharPerfil(); vm.mostrarAviso("ID copiado") },
        aoMinhaConta = vm::abrirAjustes,
    ),
    busca = AcoesDaBusca(
        aoFechar = vm::fecharTelaCheia,
        aoMudarConsulta = vm::mudarConsultaDaBusca,
        aoMudarEscopo = vm::mudarEscopoDaBusca,
        aoBuscar = { vm.buscar() },
        aoMostrarMais = { vm.buscar(mais = true) },
        aoAbrirResultado = vm::irParaMensagem,
    ),
    ajustes = AcoesDeAjustes(
        aoFechar = vm::fecharTelaCheia,
        aoTrocarSenha = vm::trocarSenha,
        aoEncerrarSessao = vm::encerrarSessao,
        aoEncerrarOutras = vm::encerrarOutrasSessoes,
        aoEscolherTamanho = vm::definirTamanhoDoTexto,
        aoSair = { vm.abrirDialogo(Dialogo.ConfirmarSair) },
    ),
    aoAbrirAjustes = vm::abrirAjustes,
    aoAbrirPerfil = vm::abrirPerfil,
    folha = AcoesDaFolha(
        aoFechar = vm::fecharFolha,
        aoResponder = { vm.nav.value.folha?.let { vm.responderA(it.mensagem) } },
        aoReagir = { e -> vm.nav.value.folha?.let { vm.reagirComEmoji(it.mensagem, e) } },
        aoMaisEmojis = { vm.nav.value.folha?.let { vm.maisEmojis(it.mensagem) } },
        aoEditar = { vm.nav.value.folha?.let { vm.editarMensagem(it.mensagem) } },
        aoCopiar = { vm.nav.value.folha?.let { f -> vm.textoDaMensagem(f.mensagem)?.let(copiar); vm.fecharFolha() } },
        aoFixar = { vm.nav.value.folha?.let { vm.fixarMensagem(it.mensagem) } },
        aoApagar = { vm.nav.value.folha?.let { vm.pedirApagar(it.mensagem) } },
    ),
)
