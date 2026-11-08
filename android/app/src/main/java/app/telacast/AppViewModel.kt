package app.telacast

import androidx.lifecycle.ViewModel
import app.telacast.arquivos.Arquivos
import app.telacast.arquivos.FonteDeArquivo
import app.telacast.data.Aviso
import app.telacast.data.Busca
import androidx.compose.ui.text.input.TextFieldValue
import app.telacast.data.SugestoesDeMencao
import app.telacast.proto.Attachment
import app.telacast.proto.Message
import app.telacast.data.AppState
import app.telacast.estado.AbaInicio
import app.telacast.estado.Destino
import app.telacast.estado.Dialogo
import app.telacast.estado.FormLogin
import app.telacast.estado.MenuAberto
import app.telacast.estado.Navegacao
import app.telacast.estado.TextosDeErro
import app.telacast.net.Credencial
import app.telacast.proto.Channel
import app.telacast.push.DestinoDoToque
import app.telacast.proto.Status
import app.telacast.voz.EstadoDeVoz
import app.telacast.voz.OpcoesDeVoz
import app.telacast.voz.RotaDeAudio
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.filterNotNull
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** Qual das três telas raiz aparece. */
enum class Fase { Retomando, Login, Principal }

/**
 * ViewModel fino: junta o [app.telacast.data.Repository] com a navegação, o formulário de entrada e o
 * armazenamento. A lógica de cada pedaço está em classes puras (`Navegacao`, `FormLogin`), então aqui só há cola.
 * Nada daqui conhece Compose: as telas leem os `StateFlow` e chamam estes métodos.
 */
class AppViewModel(private val c: Container) : ViewModel() {
    private val repo = c.repositorio
    private val escopo = c.escopo

    val estado: StateFlow<AppState> = repo.estado

    private val _form = MutableStateFlow(
        FormLogin(servidor = c.preferencias.servidor.ifBlank { c.servidorPadrao }, usuario = c.preferencias.usuario),
    )
    val form: StateFlow<FormLogin> = _form.asStateFlow()

    private val _nav = MutableStateFlow(Navegacao())
    val nav: StateFlow<Navegacao> = _nav.asStateFlow()

    private val _recolhidas = MutableStateFlow(c.preferencias.categoriasRecolhidas)
    val recolhidas: StateFlow<Set<Long>> = _recolhidas.asStateFlow()

    private val _aviso = MutableStateFlow<String?>(null)
    /** Aviso curto (erro do servidor ou da voz); some sozinho. */
    val aviso: StateFlow<String?> = _aviso.asStateFlow()
    private var jobAviso: Job? = null

    /** Há token salvo e ainda não sabemos se vale: mostra "Entrando…" em vez do formulário. */
    private val retomando = MutableStateFlow(false)
    private var navInicializada = false

    val fase: StateFlow<Fase> = combine(repo.estado, retomando) { s, r ->
        when {
            s.autenticado -> Fase.Principal
            r && s.erroDeEntrada == null -> Fase.Retomando
            else -> Fase.Login
        }
    }.stateIn(escopo, SharingStarted.Eagerly, Fase.Login)

    init {
        escopo.launch { repo.estado.collect { aoMudarEstado(it) } }
        escopo.launch { repo.avisos.collect { aoReceberAviso(it) } }
        // o player abre e fecha junto com o diálogo "Assistir" (inclusive pelo botão Voltar)
        escopo.launch { _nav.map { (it.dialogo as? Dialogo.Assistir)?.canal }.distinctUntilChanged().collect { canal -> if (canal != null) c.player.abrir(canal) else c.player.parar() } }
        // erro ou fim da transmissão viram aviso curto em qualquer tela
        escopo.launch { c.transmissao.estado.map { if (it.fase == app.telacast.tela.FaseDaTransmissao.Erro || it.fase == app.telacast.tela.FaseDaTransmissao.Inativa) it.mensagem else null }.distinctUntilChanged().collect { m -> if (m != null) mostrarAviso(m) } }
        // erros da voz (permissão negada, microfone ocupado, queda) também viram aviso curto, mesmo fora da tela da voz
        escopo.launch { c.voz.estado.map { it.erro }.distinctUntilChanged().filterNotNull().collect { mostrarAviso(it) } }
        retomarSessaoSalva()
        c.atualizacao.verificar(forcar = true) // a cada abertura do app; falha de rede é silenciosa
    }

    // ------------------------------------------------------------------ entrada

    private fun retomarSessaoSalva() {
        val token = c.cofre.ler() ?: return
        val servidor = _form.value.servidor
        if (servidor.isBlank()) return
        retomando.value = true
        repo.entrar(servidor, Credencial.Retomar(token))
    }

    fun editarForm(f: (FormLogin) -> FormLogin) = _form.update { f(it).copy(erro = null) }

    fun alternarModo() = _form.update { it.copy(registrar = !it.registrar, erro = null) }

    fun entrar() {
        val f = _form.value
        if (f.entrando) return
        f.validar()?.let { erro -> _form.update { it.copy(erro = erro) }; return }
        c.preferencias.servidor = f.servidor.trim()
        c.preferencias.usuario = f.usuario.trim()
        _form.update { it.copy(entrando = true, erro = null) }
        repo.entrar(f.servidor, f.credencial())
    }

    /** `Logout` (revoga o token) e volta ao formulário. */
    fun sair() {
        c.voz.sair()
        c.push.aoSair() // PushUnregister antes do Logout; apaga o token daqui e do FCM
        toquePendente = null
        repo.sair()
        c.cofre.apagar()
        retomando.value = false
        navInicializada = false
        _nav.value = Navegacao()
        _form.update { it.copy(senha = "", convite = "", entrando = false, erro = null) }
    }

    // ------------------------------------------------------------------ push

    /** Toque numa notificação: se o app ainda está entrando, guarda e aplica quando o snapshot chegar. */
    private var toquePendente: DestinoDoToque? = null

    fun abrirPorNotificacao(d: DestinoDoToque) {
        toquePendente = d
        aplicarToque(repo.estado.value)
    }

    private fun aplicarToque(s: AppState) {
        val d = toquePendente ?: return
        val sn = s.snapshot
        if (!s.autenticado || sn == null || !navInicializada) return
        toquePendente = null
        when (d) {
            is DestinoDoToque.Dm -> abrirDm(d.canal)
            is DestinoDoToque.Canal -> {
                val sv = sn.servers.firstOrNull { it.id == d.servidor }
                if (sv != null && sv.channels.any { it.id == d.canal }) abrirCanal(d.servidor, d.canal) else abrirAmigos()
            }
            DestinoDoToque.Pendentes -> { abrirAmigos(); escolherAba(AbaInicio.Pendente) }
        }
    }

    /** Permissão de notificações: pedimos uma vez só (ao entrar na conta ou ao entrar na voz), sem insistir. */
    fun devePedirNotificacoes(): Boolean = !c.preferencias.pediuNotificacoes
    fun marcarPediuNotificacoes() { c.preferencias.pediuNotificacoes = true }

    /** Na tela "Entrando…" quando a retomada demora: desiste e mostra o formulário. */
    fun usarOutraConta() = sair()

    private fun aoMudarEstado(s: AppState) {
        if (s.autenticado) {
            repo.token?.let { c.cofre.guardar(it) }
            s.eu?.let { c.preferencias.usuario = it.name }
            retomando.value = false
            if (_form.value.entrando || _form.value.senha.isNotEmpty()) _form.update { it.copy(entrando = false, senha = "", convite = "") }
            if (!navInicializada && s.snapshot != null) { navInicializada = true; escolherDestinoInicial(s) }
            corrigirDestino(s)
            aplicarToque(s)
        }
        s.erroDeEntrada?.let { e ->
            retomando.value = false
            if (e.sessaoInvalida) { c.cofre.apagar(); navInicializada = false; _nav.value = Navegacao() }
            _form.update { it.copy(entrando = false, erro = TextosDeErro.paraEntrada(e.codigo, e.msg)) }
            repo.limparErroDeEntrada()
        }
    }

    // ------------------------------------------------------------------ navegação

    /** Reabre onde parou; senão o primeiro canal de texto do primeiro servidor; senão o Início. */
    private fun escolherDestinoInicial(s: AppState) {
        val servidores = s.snapshot?.servers.orEmpty()
        val ultimo = servidores.firstOrNull { it.id == c.preferencias.ultimoServidor }
            ?.takeIf { sv -> sv.channels.any { it.id == c.preferencias.ultimoCanal && it.kind == Channel.TEXTO } }
        val (servidor, canal) = if (ultimo != null) ultimo.id to c.preferencias.ultimoCanal else {
            val sv = servidores.firstOrNull { sv -> sv.channels.any { it.kind == Channel.TEXTO } }
            if (sv == null) 0L to 0L else sv.id to sv.channels.filter { it.kind == Channel.TEXTO }.sortedBy { it.position }.first().id
        }
        val dm = s.snapshot?.dms?.firstOrNull { it.channelId == c.preferencias.ultimaDm }
        if (dm != null) abrirDm(dm.channelId) else if (canal != 0L) abrirCanal(servidor, canal) else _nav.update { it.selecionarServidor(null) }
    }

    /** Se o canal aberto sumiu do snapshot (apagado, saí do servidor), volta ao Início. */
    private fun corrigirDestino(s: AppState) {
        val sn = s.snapshot ?: return
        val d = _nav.value.destino
        val existe = when (d) {
            Destino.Amigos -> true
            is Destino.Dm -> sn.dms.any { it.channelId == d.canal }
            is Destino.Canal -> sn.servers.any { sv -> sv.id == d.servidor && sv.channels.any { it.id == d.canal } }
            is Destino.Voz -> sn.servers.any { sv -> sv.id == d.servidor && sv.channels.any { it.id == d.canal && it.kind == Channel.VOZ } }
        }
        if (!existe) { _nav.update { it.irPara(Destino.Amigos) }; repo.fecharCanal() }
        _nav.value.servidorNoPainel?.let { id -> if (sn.servers.none { it.id == id }) _nav.update { it.selecionarServidor(null) } }
    }

    fun abrirGaveta() = _nav.update { it.abrirGaveta() }
    fun fecharGaveta() = _nav.update { it.fecharGaveta() }
    fun abrirMembros() = _nav.update { it.abrirMembros() }
    fun fecharMembros() = _nav.update { it.fecharMembros() }
    fun voltar() = _nav.update { it.voltar() }
    fun abrirMenu(m: MenuAberto) = _nav.update { it.abrirMenu(m) }
    fun fecharMenu() = _nav.update { it.fecharMenu() }
    fun abrirDialogo(d: Dialogo) = _nav.update { it.abrirDialogo(d) }
    fun fecharDialogo() = _nav.update { it.fecharDialogo() }
    fun escolherAba(a: AbaInicio) {
        _nav.update { it.escolherAba(a) }
        if (a == AbaInicio.Pendente) c.notificador.limparPedidos()
    }

    fun selecionarServidor(id: Long?) = _nav.update { it.selecionarServidor(id) }

    fun abrirCanal(servidor: Long, canal: Long) {
        _nav.update { it.irPara(Destino.Canal(servidor, canal)) }
        repo.abrirCanal(canal)
        c.notificador.limparCanal(canal)
        c.compositor.abrirCanal(canal)
        c.preferencias.ultimoServidor = servidor
        c.preferencias.ultimoCanal = canal
        c.preferencias.ultimaDm = 0
    }

    fun abrirDm(canal: Long) {
        _nav.update { it.irPara(Destino.Dm(canal)).copy(servidorNoPainel = null) }
        repo.abrirCanal(canal)
        c.notificador.limparCanal(canal)
        c.compositor.abrirCanal(canal)
        c.preferencias.ultimaDm = canal
    }

    fun abrirAmigos() {
        _nav.update { it.irPara(Destino.Amigos) }
        repo.fecharCanal()
    }

    fun alternarCategoria(id: Long) {
        val novo = _recolhidas.value.let { if (id in it) it - id else it + id }
        _recolhidas.value = novo
        c.preferencias.categoriasRecolhidas = novo
    }

    // ------------------------------------------------------------------ conversa: mensagens, compositor e anexos

    val compositor = c.compositor.estado
    val anexos = c.enviador.anexos
    val imagens get() = c.imagens
    val previas get() = c.previas

    private val _spoilers = MutableStateFlow<Set<Pair<Long, Int>>>(emptySet())
    val spoilers: StateFlow<Set<Pair<Long, Int>>> = _spoilers.asStateFlow()
    private val _textosAbertos = MutableStateFlow<Set<Long>>(emptySet())
    val textosAbertos: StateFlow<Set<Long>> = _textosAbertos.asStateFlow()
    private val _reveladas = MutableStateFlow<Set<Long>>(emptySet())
    /** Mensagens de usuários bloqueados que a pessoa mandou mostrar. */
    val reveladas: StateFlow<Set<Long>> = _reveladas.asStateFlow()

    fun urlDoAnexo(a: Attachment): String = Arquivos.url(c.baseHttp(), a, repo.token.orEmpty())
    fun urlDaMiniatura(a: Attachment): String = Arquivos.urlMiniatura(c.baseHttp(), a, repo.token.orEmpty())

    fun alternarSpoiler(mensagem: Long, id: Int) = _spoilers.update { val k = mensagem to id; if (k in it) it - k else it + k }
    fun alternarTexto(id: Long) = _textosAbertos.update { if (id in it) it - id else it + id }
    fun mostrarBloqueada(id: Long) = _reveladas.update { it + id }

    private fun mensagem(id: Long): Message? = repo.estado.value.canalAberto?.let { repo.estado.value.lista(it) }?.firstOrNull { it.id == id }

    fun textoDaMensagem(id: Long): String? = mensagem(id)?.content
    fun carregarAnteriores(canal: Long) = repo.carregarAnteriores(canal)

    fun mudarCompositor(v: TextFieldValue) = c.compositor.mudar(v)
    fun escolherSugestao(s: SugestoesDeMencao, nome: String) = c.compositor.completarMencao(s, nome)
    fun cancelarResposta() = c.compositor.cancelarResposta()
    fun cancelarEdicao() = c.compositor.cancelarEdicao()

    /** Envia a mensagem (ou salva a edição). Com anexos ainda subindo, não envia (o botão fica apagado). */
    fun enviarMensagem() {
        val estado = c.compositor.estado.value
        val canal = estado.canal
        if (c.enviador.emAndamento(canal)) return
        val editando = estado.editando != null
        if (c.compositor.enviar(c.enviador.prontos(canal)) && !editando) c.enviador.concluir(canal)
    }

    fun anexar(fontes: List<FonteDeArquivo>) {
        val canal = c.compositor.estado.value.canal
        if (canal == 0L || fontes.isEmpty()) return
        c.enviador.adicionar(canal, fontes)?.let(::mostrarAviso)
    }

    fun removerAnexo(chave: Long) = c.enviador.remover(chave)
    fun tentarAnexo(chave: Long) = c.enviador.tentarDeNovo(chave)

    fun abrirFolha(m: Message) = _nav.update { it.abrirFolha(m.id) }
    fun fecharFolha() = _nav.update { it.fecharFolha() }
    fun maisEmojis(id: Long) = _nav.update { it.abrirFolha(id, emojis = true) }

    fun responderA(id: Long) { mensagem(id)?.let(c.compositor::responder); fecharFolha() }
    fun editarMensagem(id: Long) { mensagem(id)?.let(c.compositor::editar); fecharFolha() }
    fun fixarMensagem(id: Long) { mensagem(id)?.let { repo.fixar(id, !it.pinned) }; fecharFolha() }
    fun pedirApagar(id: Long) { mensagem(id)?.let { m -> _nav.update { it.abrirDialogo(Dialogo.ConfirmarApagar(id, m.content.ifEmpty { "(anexo)" })) } } }
    fun apagarMensagem(id: Long) { repo.apagar(id); fecharDialogo() }

    /** Reage com o emoji; se eu já tinha reagido, tira a reação. */
    fun reagirComEmoji(id: Long, emoji: String) {
        val ja = mensagem(id)?.reactions?.firstOrNull { it.emoji == emoji }?.me == true
        repo.reagir(id, emoji, !ja)
        fecharFolha()
    }

    fun reagir(id: Long, emoji: String, adicionar: Boolean) = repo.reagir(id, emoji, adicionar)
    fun saltarPara(respostaA: Long) { repo.estado.value.canalAberto?.let { repo.saltarPara(it, respostaA) } }
    fun limparDestaque() = repo.limparDestaque()
    fun reenviar(nonce: Long) = repo.reenviar(nonce)
    fun descartar(nonce: Long) = repo.descartarPendente(nonce)
    fun abrirImagem(a: Attachment) = _nav.update { it.abrirDialogo(Dialogo.Imagem(a)) }
    fun abrirTexto(a: Attachment) = _nav.update { it.abrirDialogo(Dialogo.Texto(a)) }

    // ------------------------------------------------------------------ perfil, amizades e bloqueio

    fun abrirPerfil(usuario: Long, nome: String) = _nav.update { it.abrirPerfil(usuario, nome) }
    fun fecharPerfil() = _nav.update { it.fecharPerfil() }

    /** Abre (ou cria) a conversa privada; o `DmOpened` do servidor leva a tela até ela. */
    fun conversarCom(usuario: Long) { fecharPerfil(); repo.abrirDm(usuario) }

    fun adicionarAmigo(nome: String) { repo.pedirAmizade(nome); mostrarAviso("Pedido de amizade enviado") }
    fun pedirDesfazerAmizade(id: Long, nome: String) = _nav.update { it.abrirDialogo(Dialogo.ConfirmarRemoverAmigo(id, nome)) }
    fun desfazerAmizade(id: Long) { repo.removerAmigo(id); fecharDialogo() }
    fun pedirBloquear(id: Long, nome: String) = _nav.update { it.abrirDialogo(Dialogo.ConfirmarBloquear(id, nome)) }
    fun bloquear(id: Long) { repo.bloquear(id); fecharDialogo() }
    fun desbloquear(id: Long) { repo.desbloquear(id); fecharPerfil() }

    /** Nome da pessoa tocada numa menção `<@id>` (ou "usuário"), para a folha abrir mesmo sem snapshot dela. */
    fun abrirPerfilDe(id: Long) = abrirPerfil(id, repo.estado.value.usuario(id)?.name ?: "usuário")

    // ------------------------------------------------------------------ fixadas, busca e ajustes

    fun abrirFixadas() {
        val canal = repo.estado.value.canalAberto ?: return
        repo.pedirFixadas(canal)
        _nav.update { it.abrirDialogo(Dialogo.Fixadas) }
    }

    /** Leva até uma mensagem (resultado de busca ou fixada): salta se é o canal aberto; senão abre o canal e salta ao carregar. */
    fun irParaMensagem(canal: Long, mensagem: Long) {
        fecharDialogo()
        val s = repo.estado.value
        repo.irParaMensagem(canal, mensagem)
        if (s.canalAberto == canal) return
        val sv = s.snapshot?.servers?.firstOrNull { sv -> sv.channels.any { it.id == canal } }
        if (sv != null) abrirCanal(sv.id, canal) else if (s.snapshot?.dms?.any { it.channelId == canal } == true) abrirDm(canal)
    }

    private val _consultaBusca = MutableStateFlow("")
    val consultaDaBusca: StateFlow<String> = _consultaBusca.asStateFlow()
    private val _buscaNoCanal = MutableStateFlow(false)
    val buscaNoCanal: StateFlow<Boolean> = _buscaNoCanal.asStateFlow()

    fun abrirBusca() {
        repo.limparBusca(); _consultaBusca.value = ""
        _nav.update { it.abrirDialogo(Dialogo.Busca) }
    }
    fun fecharTelaCheia() { repo.limparBusca(); fecharDialogo() }
    fun mudarConsultaDaBusca(t: String) { _consultaBusca.value = t }
    fun mudarEscopoDaBusca(noCanal: Boolean) { _buscaNoCanal.value = noCanal }

    fun buscar(mais: Boolean = false) {
        val s = repo.estado.value
        val sn = s.snapshot ?: return
        val eu = s.eu ?: return
        val pedido = Busca.pedido(_consultaBusca.value, sn, s.canalAberto, eu.id, _buscaNoCanal.value) ?: return
        repo.buscar(pedido, mais)
    }

    fun abrirAjustes() { repo.pedirSessoes(); _nav.update { it.abrirDialogo(Dialogo.Ajustes) } }
    fun trocarSenha(atual: String, nova: String) = repo.trocarSenha(atual, nova)
    fun encerrarSessao(id: Long) = repo.encerrarSessao(id)
    fun encerrarOutrasSessoes() = repo.encerrarOutrasSessoes()

    private val _tamanhoDoTexto = MutableStateFlow(c.preferencias.tamanhoDoTexto)
    val tamanhoDoTexto: StateFlow<Int> = _tamanhoDoTexto.asStateFlow()
    fun definirTamanhoDoTexto(n: Int) { c.preferencias.tamanhoDoTexto = n.coerceIn(0, 2); _tamanhoDoTexto.value = c.preferencias.tamanhoDoTexto }

    /** Sobe quando o servidor confirma a troca de senha (a tela limpa os campos). */
    private val _senhaTrocada = MutableStateFlow(0)
    val senhaTrocada: StateFlow<Int> = _senhaTrocada.asStateFlow()

    // ------------------------------------------------------------------ ações de servidor e conta

    fun criarServidor(nome: String) {
        if (nome.isBlank()) return
        repo.criarServidor(nome.trim())
        fecharDialogo()
    }

    fun entrarNoServidor(codigo: String) {
        if (codigo.isBlank()) return
        repo.entrarNoServidor(codigo.trim())
        fecharDialogo()
    }

    fun criarCanal(servidor: Long, nome: String, categoria: Boolean, pai: Long?) {
        if (nome.isBlank()) return
        repo.criarCanal(servidor, nome.trim(), if (categoria) Channel.CATEGORIA else Channel.TEXTO, if (categoria) null else pai)
        fecharDialogo()
    }

    fun definirStatus(s: Status) {
        repo.definirStatus(s)
        _nav.update { it.fecharMenu() }
    }

    // ------------------------------------------------------------------ voz

    val voz: StateFlow<EstadoDeVoz> = c.voz.estado

    /** Abre a tela do canal de voz (não entra na chamada: quem entra é [entrarNaVoz], depois da permissão do microfone). */
    fun abrirVoz(servidor: Long, canal: Long) {
        _nav.update { it.irPara(Destino.Voz(servidor, canal)) }
        repo.fecharCanal()
    }

    /** Toca na faixa ou no cartão da chamada: vai para a tela do canal em que você está. */
    fun abrirTelaDaVoz() {
        val canal = c.voz.estado.value.canal ?: return
        val sv = repo.estado.value.snapshot?.servers?.firstOrNull { s -> s.channels.any { it.id == canal } } ?: return
        abrirVoz(sv.id, canal)
    }

    // ------------------------------------------------------------------ notificações locais e "Manter conectado" (C8)

    val manterConectado: StateFlow<Boolean> = c.manterConectado
    fun definirManterConectado(ligado: Boolean) = c.definirManterConectado(ligado)

    // ------------------------------------------------------------------ atualização do app e relatório de crash (C7)

    val atualizacao: StateFlow<app.telacast.atualizacao.EstadoDaAtualizacao> = c.atualizacao.estado
    fun atualizar() = c.atualizacao.atualizar()
    fun instalarAtualizacao() = c.atualizacao.instalar()
    fun dispensarFolhaDeAtualizacao() = c.atualizacao.dispensarFolha()
    fun abrirPermissaoDeInstalacao() = c.atualizacao.abrirPermissao()
    fun cancelarPermissaoDeInstalacao() = c.atualizacao.cancelarPermissao()
    /** Ao voltar para a frente: consulta de novo (no máximo a cada 10 min) e continua a instalação se a permissão acabou de ser dada. */
    fun aoVoltarParaFrente() { c.atualizacao.verificar(); c.atualizacao.retomar() }

    private val _crash = MutableStateFlow(c.relatorioDeCrash?.pendente())
    /** Texto do relatório do último crash (`null` = não houve ou já foi dispensado). */
    val relatorioDeCrash: StateFlow<String?> = _crash.asStateFlow()
    fun dispensarCrash() { c.relatorioDeCrash?.descartar(); _crash.value = null }

    // ------------------------------------------------------------------ tela compartilhada (C5)

    val transmissao: StateFlow<app.telacast.tela.EstadoDaTransmissao> = c.transmissao.estado
    val player: StateFlow<app.telacast.tela.EstadoDoPlayer> = c.player.estado

    fun abrirOpcoesDeTransmissao() { if (c.voz.estado.value.naChamada) _nav.update { it.abrirTransmitir() } else mostrarAviso("Entre num canal de voz para transmitir a tela.") }
    fun fecharOpcoesDeTransmissao() = _nav.update { it.fecharTransmitir() }

    /** Confirmou o diálogo: no ar, só aplica; senão começa (a interface abre a permissão do Android quando a fase for PedindoPermissao). */
    fun transmitir(opcoes: app.telacast.tela.OpcoesDeTransmissao) {
        _nav.update { it.fecharTransmitir() }
        val t = c.transmissao.estado.value
        if (t.ativa && t.fase != app.telacast.tela.FaseDaTransmissao.PedindoPermissao) { c.transmissao.aplicarOpcoes(opcoes); return }
        val canal = c.voz.estado.value.canal ?: run { mostrarAviso("Entre num canal de voz para transmitir a tela."); return }
        c.transmissao.pedir(canal, opcoes)
    }

    fun permissaoDeTela(codigo: Int, dados: android.content.Intent?) {
        if (codigo == android.app.Activity.RESULT_OK && dados != null) c.transmissao.permissaoConcedida(app.telacast.tela.ConcessaoDeTela(codigo, dados)) else c.transmissao.permissaoNegada()
    }

    fun pararTransmissao() = c.transmissao.parar()
    fun assistir(usuario: Long) {
        val canal = c.voz.estado.value.canal ?: repo.estado.value.snapshot?.live?.firstOrNull { it.userId == usuario }?.channelId ?: return
        _nav.update { it.abrirDialogo(Dialogo.Assistir(canal, usuario)) }
    }
    fun definirSuperficieDoPlayer(s: Any?) = c.player.definirSuperficie(s)

    fun entrarNaVoz(canal: Long) = c.voz.entrar(canal)
    fun sairDaVoz() = c.voz.sair()
    fun alternarMudo() = c.voz.alternarMudo()
    fun alternarSurdo() = c.voz.alternarSurdo()
    fun alternarRotaDoAudio() = c.voz.definirRota(if (c.voz.estado.value.rota == RotaDeAudio.AltoFalante) RotaDeAudio.Fone else RotaDeAudio.AltoFalante)
    fun recusarMicrofone() = c.voz.recusar("Sem a permissão do microfone não dá para entrar na voz. Libere em Ajustes do Android > Apps > Telacast.")
    fun abrirVolume(usuario: Long) = _nav.update { it.abrirVolume(usuario) }
    fun fecharVolume() = _nav.update { it.fecharVolume() }
    fun definirVolumeDe(usuario: Long, volume: Float) = c.voz.definirVolume(usuario, volume)

    fun definirOpcoesDeVoz(f: (OpcoesDeVoz) -> OpcoesDeVoz) {
        val novas = f(c.voz.estado.value.opcoes)
        c.preferencias.opcoesDeVoz = novas
        c.voz.definirOpcoes(novas)
    }

    // amigos
    fun pedirAmizade(usuario: String) { if (usuario.isNotBlank()) repo.pedirAmizade(usuario) }
    fun aceitarAmigo(id: Long) = repo.aceitarAmigo(id)
    fun removerAmigo(id: Long) = repo.removerAmigo(id)
    fun abrirDmCom(usuario: Long) = repo.abrirDm(usuario)

    // ------------------------------------------------------------------ avisos

    private fun aoReceberAviso(a: Aviso) {
        when (a) {
            is Aviso.Erro -> mostrarAviso(a.msg)
            is Aviso.DmAberta -> abrirDm(a.canal)
            is Aviso.ServidorEntrou -> {
                val sv = repo.estado.value.snapshot?.servers?.firstOrNull { it.id == a.servidor }
                val canal = sv?.channels?.filter { it.kind == Channel.TEXTO }?.minByOrNull { it.position }
                if (sv != null && canal != null) abrirCanal(sv.id, canal.id) else _nav.update { it.selecionarServidor(a.servidor) }
            }
            Aviso.SenhaTrocada -> { _senhaTrocada.update { it + 1 }; mostrarAviso("Senha alterada. As outras sessões foram encerradas.") }
            is Aviso.Texto -> mostrarAviso(a.msg)
        }
    }

    fun mostrarAviso(texto: String) {
        _aviso.value = texto
        jobAviso?.cancel()
        jobAviso = escopo.launch { delay(3_500); _aviso.value = null }
    }
}
