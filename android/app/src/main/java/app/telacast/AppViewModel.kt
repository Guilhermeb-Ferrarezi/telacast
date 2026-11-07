package app.telacast

import androidx.lifecycle.ViewModel
import app.telacast.data.Aviso
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
import app.telacast.proto.Status
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.combine
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
    /** Aviso curto (erro do servidor, "voz em breve"); some sozinho. */
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
        retomarSessaoSalva()
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
        repo.sair()
        c.cofre.apagar()
        retomando.value = false
        navInicializada = false
        _nav.value = Navegacao()
        _form.update { it.copy(senha = "", convite = "", entrando = false, erro = null) }
    }

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
        if (canal != 0L) abrirCanal(servidor, canal) else _nav.update { it.selecionarServidor(null) }
    }

    /** Se o canal aberto sumiu do snapshot (apagado, saí do servidor), volta ao Início. */
    private fun corrigirDestino(s: AppState) {
        val sn = s.snapshot ?: return
        val d = _nav.value.destino
        val existe = when (d) {
            Destino.Amigos -> true
            is Destino.Dm -> sn.dms.any { it.channelId == d.canal }
            is Destino.Canal -> sn.servers.any { sv -> sv.id == d.servidor && sv.channels.any { it.id == d.canal } }
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
    fun escolherAba(a: AbaInicio) = _nav.update { it.escolherAba(a) }

    fun selecionarServidor(id: Long?) = _nav.update { it.selecionarServidor(id) }

    fun abrirCanal(servidor: Long, canal: Long) {
        _nav.update { it.irPara(Destino.Canal(servidor, canal)) }
        repo.abrirCanal(canal)
        c.preferencias.ultimoServidor = servidor
        c.preferencias.ultimoCanal = canal
    }

    fun abrirDm(canal: Long) {
        _nav.update { it.irPara(Destino.Dm(canal)).copy(servidorNoPainel = null) }
        repo.abrirCanal(canal)
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

    fun cliqueEmVoz() = mostrarAviso("Canais de voz chegam em breve.")

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
            Aviso.SenhaTrocada -> mostrarAviso("Senha alterada.")
        }
    }

    fun mostrarAviso(texto: String) {
        _aviso.value = texto
        jobAviso?.cancel()
        jobAviso = escopo.launch { delay(3_500); _aviso.value = null }
    }
}
