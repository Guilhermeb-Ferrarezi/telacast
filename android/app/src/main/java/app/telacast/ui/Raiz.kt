package app.telacast.ui

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import androidx.activity.compose.BackHandler
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.core.content.ContextCompat
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
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
    val crash by vm.relatorioDeCrash.collectAsState()
    val ctx = LocalContext.current
    val area = LocalClipboardManager.current
    androidx.compose.foundation.layout.Box(androidx.compose.ui.Modifier.fillMaxSize()) {
        ConteudoDoApp(vm)
        crash?.let { rel ->
            JanelaDeCrash(
                rel, aoCopiar = { area.setText(AnnotatedString(rel)); vm.mostrarAviso("Relatório copiado") },
                aoCompartilhar = { runCatching { ctx.startActivity(android.content.Intent.createChooser(android.content.Intent(android.content.Intent.ACTION_SEND).setType("text/plain").putExtra(android.content.Intent.EXTRA_TEXT, rel), "Compartilhar relatório").addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)) } },
                aoFechar = vm::dispensarCrash,
            )
        }
    }
}

@Composable
private fun ConteudoDoApp(vm: AppViewModel) {
    val fase by vm.fase.collectAsState()
    val estado by vm.estado.collectAsState()
    when (fase) {
        Fase.Login -> ComAtualizacao(vm) {
            val form by vm.form.collectAsState()
            TelaDeLogin(form, AcoesLogin(vm::editarForm, vm::entrar, vm::alternarModo))
        }
        Fase.Retomando -> ComAtualizacao(vm) { TelaDeCarregamento(estado.conexao is EstadoConexao.Reconectando, vm::usarOutraConta) }
        Fase.Desatualizado -> ComAtualizacao(vm) {
            val atu by vm.atualizacao.collectAsState()
            val ctxD = LocalContext.current
            TelaDesatualizada(atu, vm.atualizacaoLigada, acoesDeAtualizacao(vm), aoBaixarPeloNavegador = { runCatching { ctxD.startActivity(android.content.Intent(android.content.Intent.ACTION_VIEW, android.net.Uri.parse(vm.urlDeDownload())).addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)) } }, aoTentarDeNovo = vm::tentarDeNovoAposVersao)
        }
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
            val vozEstado by vm.voz.collectAsState()
            val telaTransmissao by vm.transmissao.collectAsState()
            val atualizacaoEstado by vm.atualizacao.collectAsState()
            val chamadaEstado by vm.chamada.collectAsState()
            val pedidoDeEntrada by vm.pedidoDeEntrada.collectAsState()
            val manterConectado by vm.manterConectado.collectAsState()
            // volta das configurações do Android: reavalia se as notificações estão permitidas
            var retomadas by remember { mutableStateOf(0) }
            val dono = androidx.lifecycle.compose.LocalLifecycleOwner.current
            androidx.compose.runtime.DisposableEffect(dono) {
                val o = androidx.lifecycle.LifecycleEventObserver { _, e -> if (e == androidx.lifecycle.Lifecycle.Event.ON_RESUME) retomadas++ }
                dono.lifecycle.addObserver(o); onDispose { dono.lifecycle.removeObserver(o) }
            }
            val ctxNotif = LocalContext.current
            val permitidas = remember(retomadas, nav.dialogo) { androidx.core.app.NotificationManagerCompat.from(ctxNotif).areNotificationsEnabled() }
            val telaPlayer by vm.player.collectAsState()
            // permissão de captura da tela: o Android pede de novo a cada transmissão (diálogo próprio dele)
            val ctxTela = LocalContext.current
            val pedirProjecao = rememberLauncherForActivityResult(ActivityResultContracts.StartActivityForResult()) { r -> vm.permissaoDeTela(r.resultCode, r.data) }
            LaunchedEffect(telaTransmissao.fase) {
                if (telaTransmissao.fase == app.telacast.tela.FaseDaTransmissao.PedindoPermissao) {
                    val mpm = ctxTela.getSystemService(android.media.projection.MediaProjectionManager::class.java)
                    runCatching { pedirProjecao.launch(mpm.createScreenCaptureIntent()) }.onFailure { vm.permissaoDeTela(0, null) }
                }
            }
            // player: livre para girar; "tela cheia" força paisagem e esconde as barras do sistema
            var telaCheia by remember { mutableStateOf(false) }
            val assistindo = nav.dialogo is Dialogo.Assistir
            val janela = (ctxTela as? android.app.Activity)
            androidx.compose.runtime.DisposableEffect(assistindo, telaCheia) {
                janela?.requestedOrientation = when { assistindo && telaCheia -> android.content.pm.ActivityInfo.SCREEN_ORIENTATION_SENSOR_LANDSCAPE; assistindo -> android.content.pm.ActivityInfo.SCREEN_ORIENTATION_FULL_SENSOR; else -> android.content.pm.ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED }
                val ctrl = janela?.window?.let { androidx.core.view.WindowCompat.getInsetsController(it, it.decorView) }
                if (assistindo && telaCheia) ctrl?.hide(androidx.core.view.WindowInsetsCompat.Type.systemBars())
                onDispose { ctrl?.show(androidx.core.view.WindowInsetsCompat.Type.systemBars()); if (!assistindo) telaCheia = false }
            }
            val contexto = LocalContext.current
            // POST_NOTIFICATIONS (Android 13+): pedida uma vez ao entrar na conta; se negar, não insistimos
            val pedirNotificacoes = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { }
            LaunchedEffect(Unit) {
                if (Build.VERSION.SDK_INT >= 33 && vm.devePedirNotificacoes() &&
                    ContextCompat.checkSelfPermission(contexto, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
                ) {
                    vm.marcarPediuNotificacoes()
                    pedirNotificacoes.launch(Manifest.permission.POST_NOTIFICATIONS)
                }
            }
            // RECORD_AUDIO só é pedida quando a pessoa entra na voz (nunca ao abrir o app). POST_NOTIFICATIONS (Android 13+)
            // vai junto, para a notificação da chamada aparecer; negá-la não impede a voz.
            var canalPendente by remember { mutableStateOf<Long?>(null) }
            val pedirPermissoes = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { r ->
                val canal = canalPendente; canalPendente = null
                if (canal != null) {
                    if (r[Manifest.permission.RECORD_AUDIO] == true) vm.entrarNaVoz(canal) else vm.recusarMicrofone()
                }
            }
            val entrarNaVoz: (Long) -> Unit = { canal ->
                if (ContextCompat.checkSelfPermission(contexto, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) vm.entrarNaVoz(canal)
                else {
                    canalPendente = canal
                    vm.marcarPediuNotificacoes() // a mesma pergunta já vai junto: não repetir depois
                    val pedir = buildList {
                        add(Manifest.permission.RECORD_AUDIO)
                        if (Build.VERSION.SDK_INT >= 33) add(Manifest.permission.POST_NOTIFICATIONS)
                    }
                    pedirPermissoes.launch(pedir.toTypedArray())
                }
            }
            // "Ligar" na DM e "Atender" (faixa ou notificação): entra na chamada pedindo o microfone se faltar
            androidx.compose.runtime.LaunchedEffect(pedidoDeEntrada) {
                pedidoDeEntrada?.let { canal -> vm.consumirPedidoDeEntrada(); entrarNaVoz(canal) }
            }
            // tocar num canal de voz abre a tela dele e entra (como no desktop); se já está nele, só abre
            val abrirVoz: (Long, Long) -> Unit = { servidor, canal ->
                vm.abrirVoz(servidor, canal)
                if (!(vozEstado.naChamada && vozEstado.canal == canal)) entrarNaVoz(canal)
            }
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
                    miniaturaDoAnexoEmCache = { vm.imagens.miniaturaEmCache(it.id) },
                    miniaturaDoAnexo = { vm.imagens.obterMiniatura(it, vm.urlDaMiniatura(it), vm.urlDoAnexo(it)) },
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
                    acoesDe(vm, copiar = { area.setText(AnnotatedString(it)) }, abrirLink = { runCatching { links.openUri(it) } }, anexar = { seletor.launch("*/*") }, baixar = baixar, aoAbrirVoz = abrirVoz, aoEntrarNaVoz = entrarNaVoz, aoTelaCheia = { telaCheia = !telaCheia },
                        abrirConfig = { acao, comPacote -> runCatching { ctxTela.startActivity(android.content.Intent(acao).apply { if (comPacote) putExtra(android.provider.Settings.EXTRA_APP_PACKAGE, ctxTela.packageName) }.addFlags(android.content.Intent.FLAG_ACTIVITY_NEW_TASK)) } },
                    ),
                    conversa = DadosDaConversa(compositor, anexos, spoilers, textos, reveladas, agora, consultaDaBusca = consulta, buscaNoCanal = buscaNoCanal, tamanhoDoTexto = tamanhoTexto, senhaTrocada = senhaTrocada, notificacoes = DadosDeNotificacoes(manterConectado, permitidas, estado.trafegoDoMes)),
                    voz = DadosDeVoz(vozEstado),
                    atualizacao = atualizacaoEstado, chamada = chamadaEstado,
                    dadosDeTela = DadosDeTela(
                        telaTransmissao, telaPlayer, telaCheia = telaCheia,
                        superficie = { m ->
                            androidx.compose.ui.viewinterop.AndroidView(
                                factory = { ctx ->
                                    android.view.SurfaceView(ctx).apply {
                                        holder.addCallback(object : android.view.SurfaceHolder.Callback {
                                            override fun surfaceCreated(h: android.view.SurfaceHolder) = vm.definirSuperficieDoPlayer(h.surface)
                                            override fun surfaceChanged(h: android.view.SurfaceHolder, f: Int, w: Int, a: Int) = Unit
                                            override fun surfaceDestroyed(h: android.view.SurfaceHolder) = vm.definirSuperficieDoPlayer(null)
                                        })
                                    }
                                },
                                modifier = m,
                            )
                        },
                    ),
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
    aoAbrirVoz: (Long, Long) -> Unit,
    aoEntrarNaVoz: (Long) -> Unit,
    aoTelaCheia: () -> Unit,
    abrirConfig: (String, Boolean) -> Unit = { _, _ -> },
) = AcoesPrincipais(
    painel = AcoesPainel(
        aoAbrirCanal = vm::abrirCanal,
        aoAlternarCategoria = vm::alternarCategoria,
        aoCliqueEmVoz = aoAbrirVoz,
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
        aoLigar = vm::ligar,
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
        aoMudarVoz = vm::definirOpcoesDeVoz,
        aoManterConectado = vm::definirManterConectado,
        aoAbrirConfigDeNotificacoes = { abrirConfig(android.provider.Settings.ACTION_APP_NOTIFICATION_SETTINGS, true) },
        aoAbrirBateria = { abrirConfig(android.provider.Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS, false) },
        aoSair = { vm.abrirDialogo(Dialogo.ConfirmarSair) },
    ),
    aoAbrirAjustes = vm::abrirAjustes,
    aoAbrirPerfil = vm::abrirPerfil,
    chamada = AcoesDeChamada(aoAtender = vm::atenderChamada, aoRecusar = vm::recusarChamada),
    atualizacao = AcoesDeAtualizacao(
        aoAtualizar = vm::atualizar, aoInstalar = vm::instalarAtualizacao, aoDispensarFolha = vm::dispensarFolhaDeAtualizacao,
        aoAbrirPermissao = vm::abrirPermissaoDeInstalacao, aoCancelarPermissao = vm::cancelarPermissaoDeInstalacao,
    ),
    tela = AcoesDeTela(
        aoAbrirOpcoes = vm::abrirOpcoesDeTransmissao,
        aoFecharOpcoes = vm::fecharOpcoesDeTransmissao,
        aoTransmitir = vm::transmitir,
        aoPararTransmissao = vm::pararTransmissao,
        aoAssistir = vm::assistir,
        aoFecharPlayer = vm::fecharDialogo,
        aoTelaCheia = aoTelaCheia,
        aoAlternarSom = { vm.definirMudoDoPlayer(!vm.player.value.mudo) },
    ),
    voz = AcoesDeVoz(
        aoEntrar = aoEntrarNaVoz,
        aoSair = vm::sairDaVoz,
        aoAlternarMudo = vm::alternarMudo,
        aoAlternarSurdo = vm::alternarSurdo,
        aoAlternarRota = vm::alternarRotaDoAudio,
        aoAbrirTela = vm::abrirTelaDaVoz,
        aoEscolherVolume = vm::abrirVolume,
        aoMudarVolume = vm::definirVolumeDe,
        aoFecharVolume = vm::fecharVolume,
    ),
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

private fun acoesDeAtualizacao(vm: AppViewModel) = AcoesDeAtualizacao(
    aoAtualizar = vm::atualizar, aoInstalar = vm::instalarAtualizacao, aoDispensarFolha = vm::dispensarFolhaDeAtualizacao,
    aoAbrirPermissao = vm::abrirPermissaoDeInstalacao, aoCancelarPermissao = vm::cancelarPermissaoDeInstalacao,
)

/**
 * Telas sem login (entrar, retomando, app desatualizado) com a faixa "Nova versão X disponível · Atualizar" e as folhas de atualização por cima:
 * a consulta é HTTP e não depende de estar logado, então dá para atualizar sem entrar (e sem conseguir conectar).
 */
@Composable
private fun ComAtualizacao(vm: AppViewModel, conteudo: @Composable () -> Unit) {
    val atu by vm.atualizacao.collectAsState()
    val acoes = acoesDeAtualizacao(vm)
    androidx.compose.foundation.layout.Box(androidx.compose.ui.Modifier.fillMaxSize()) {
        androidx.compose.foundation.layout.Column(androidx.compose.ui.Modifier.fillMaxSize()) {
            if (atu.temNovidade) androidx.compose.foundation.layout.Box(androidx.compose.ui.Modifier.statusBarsPadding()) { FaixaDeAtualizacao(atu, acoes) }
            androidx.compose.foundation.layout.Box(androidx.compose.ui.Modifier.weight(1f)) { conteudo() }
        }
        if (atu.fase == app.telacast.atualizacao.FaseDaAtualizacao.PrecisaDePermissao) FolhaDeFonteDesconhecida(acoes)
        else if (atu.folhaPendente && (atu.fase == app.telacast.atualizacao.FaseDaAtualizacao.Disponivel || atu.fase == app.telacast.atualizacao.FaseDaAtualizacao.Pronta)) FolhaDeAtualizacao(atu, acoes)
    }
}
