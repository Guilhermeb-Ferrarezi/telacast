package app.telacast.data

import app.telacast.net.Credencial
import app.telacast.net.EventoGateway
import app.telacast.net.Gateway
import app.telacast.proto.Attachment
import app.telacast.proto.C2S
import app.telacast.proto.Message
import app.telacast.proto.Reaction
import app.telacast.proto.Reply
import app.telacast.proto.S2C
import app.telacast.proto.Snapshot
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.receiveAsFlow
import kotlinx.coroutines.launch

/**
 * Dono do estado do app: junta o [Gateway] num único `StateFlow<AppState>` (snapshot, mensagens por canal,
 * não lidos, digitando, envio otimista). A lógica espelha a do desktop (`app/src/main.rs`).
 * Puro Kotlin; as telas só leem [estado] e chamam os métodos daqui.
 *
 * Seguro para chamar de qualquer thread (um cadeado protege as transições de estado).
 */
class Repository(
    private val scope: CoroutineScope,
    private val gateway: Gateway,
    private val relogio: () -> Long = System::currentTimeMillis,
    private val duracaoDigitandoMs: Long = 8_000,
    private val intervaloDigitandoMs: Long = 4_000,
) {
    private val lock = Any()
    private val _estado = MutableStateFlow(AppState())
    val estado: StateFlow<AppState> = _estado.asStateFlow()

    private val _avisos = Channel<Aviso>(Channel.UNLIMITED)
    val avisos: Flow<Aviso> = _avisos.receiveAsFlow()

    private var ultimoNonce = 0L
    private var seqNonce = 0L
    private val jobsDigitando = HashMap<Pair<Long, Long>, Job>()
    private val ultimoDigitando = HashMap<Long, Long>()

    init {
        scope.launch { gateway.estado.collect { c -> modificar { it.copy(conexao = c) } } }
        scope.launch { gateway.eventos.collect { tratar(it) } }
    }

    private inline fun modificar(f: (AppState) -> AppState) = synchronized(lock) { _estado.value = f(_estado.value) }

    // ------------------------------------------------------------------ entrada e saída

    fun entrar(servidor: String, credencial: Credencial) {
        modificar { it.copy(erroDeEntrada = null) }
        gateway.conectar(servidor, credencial)
    }

    /** `Logout` e volta ao estado inicial (limpa mensagens e não lidos). */
    fun sair() = synchronized(lock) {
        gateway.sair()
        jobsDigitando.values.forEach { it.cancel() }
        jobsDigitando.clear()
        ultimoDigitando.clear()
        _estado.value = AppState(conexao = _estado.value.conexao)
    }

    /** Token da sessão atual (para guardar cifrado e usar no HTTP). */
    val token: String? get() = gateway.token

    fun limparErroDeEntrada() = modificar { it.copy(erroDeEntrada = null) }

    // ------------------------------------------------------------------ navegação e leitura

    /**
     * Abre o canal: zera não lido e menções, marca como lido (`Ack`) e pede o histórico se ainda não veio.
     */
    fun abrirCanal(canal: Long) = synchronized(lock) {
        val s = _estado.value
        ack(s, canal)
        // divisor "novas mensagens": só se havia algo mais novo que o último lido no momento de abrir
        val (ultima, lida) = s.canal(canal)?.let { it.lastMessageId to it.lastReadId }
            ?: s.snapshot?.dms?.firstOrNull { it.channelId == canal }?.let { it.lastMessageId to it.lastReadId } ?: (0L to 0L)
        val marcadores = if (ultima > lida) s.marcadores + (canal to lida) else s.marcadores - canal
        var novo = s.copy(canalAberto = canal, naoLidos = s.naoLidos - canal, mencoes = s.mencoes - canal, marcadores = marcadores)
        if (canal !in s.carregados && gateway.enviar(C2S.History(canal, null))) {
            novo = novo.copy(carregados = novo.carregados + canal)
        }
        _estado.value = novo
    }

    /** Esc / "dispensar": tira o divisor "novas mensagens" do canal. */
    fun dispensarMarcador(canal: Long) = modificar { it.copy(marcadores = it.marcadores - canal) }

    fun fecharCanal() = modificar { it.copy(canalAberto = null) }

    private fun ack(s: AppState, canal: Long) {
        val id = s.ultimoId(canal)
        if (id > 0) gateway.enviar(C2S.Ack(canal, id))
    }

    /** Carrega a página mais antiga (`History{before}`), se o servidor ainda tem e não há pedido em andamento. */
    fun carregarAnteriores(canal: Long) = synchronized(lock) { pedirAnteriores(canal) }

    private fun pedirAnteriores(canal: Long): Boolean {
        val s = _estado.value
        if (canal in s.buscandoAntigas || s.temMais[canal] != true) return false
        val maisAntiga = s.mensagens[canal]?.firstOrNull { it.id > 0 }?.id ?: return false
        if (!gateway.enviar(C2S.History(canal, maisAntiga))) return false
        _estado.value = s.copy(buscandoAntigas = s.buscandoAntigas + canal)
        return true
    }

    /**
     * Salta até a mensagem `id` (clique no cabeçalho de uma resposta). Se não estiver carregada, busca páginas
     * mais antigas até achar, no máximo [PAGINAS_DO_SALTO] (igual ao desktop). O resultado aparece em
     * [AppState.destaque] (a tela rola até ela e a realça); se não achar, sai um [Aviso.Texto].
     */
    fun saltarPara(canal: Long, id: Long) = synchronized(lock) {
        val s = _estado.value
        when {
            s.mensagens[canal].orEmpty().any { it.id == id } ->
                _estado.value = s.copy(destaque = Destaque(canal, id, relogio()), salto = null)
            s.temMais[canal] == true -> {
                _estado.value = s.copy(salto = Salto(canal, id, PAGINAS_DO_SALTO))
                if (!pedirAnteriores(canal)) _estado.value = _estado.value.copy(salto = null)
            }
            else -> _avisos.trySend(Aviso.Texto("A mensagem não está mais disponível"))
        }
    }

    /** Pede a busca. `mais` = próxima página (depois da última mensagem já listada). */
    fun buscar(p: PedidoDeBusca, mais: Boolean = false) = synchronized(lock) {
        val s = _estado.value
        val antes = s.busca
        val nonce = antes.nonce + 1
        val before = if (mais) antes.hits.lastOrNull()?.message?.id else null
        val enviado = gateway.enviar(C2S.Search(nonce, p.canal, p.servidor, p.de, p.texto, before))
        _estado.value = s.copy(busca = antes.copy(
            nonce = nonce, consulta = p.original, carregando = enviado, anexando = mais && enviado,
            hits = if (mais) antes.hits else emptyList(), mais = if (mais) antes.mais else false, buscou = antes.buscou && mais,
        ))
    }

    fun limparBusca() = modificar { it.copy(busca = EstadoBusca()) }

    /** Vai até uma mensagem de qualquer canal (resultado de busca ou fixada): salta se já está no canal aberto; senão fica agendado. */
    fun irParaMensagem(canal: Long, id: Long) = synchronized(lock) {
        if (_estado.value.canalAberto == canal) saltarPara(canal, id)
        else _estado.value = _estado.value.copy(saltoAgendado = canal to id)
    }

    fun limparDestaque() = modificar { it.copy(destaque = null) }

    // ------------------------------------------------------------------ envio otimista

    /**
     * Mostra a mensagem na hora (provisória, `id = -nonce`) e manda ao servidor; sem conexão, fica na fila e sai
     * sozinha depois de reconectar. Devolve o `nonce`, ou `null` se o texto não vale (vazio sem anexo, ou
     * acima de 4000 caracteres: o servidor descartaria em silêncio).
     */
    fun enviarMensagem(canal: Long, texto: String, responderA: Reply? = null, anexos: List<Attachment> = emptyList()): Long? =
        synchronized(lock) {
            val s = _estado.value
            val eu = s.eu ?: return null
            val conteudo = trocarMencoesPorToken(texto.trim(), usuariosDoCanal(s, canal))
            if ((conteudo.isEmpty() && anexos.isEmpty()) || conteudo.length > LIMITE_TEXTO) return null
            val nonce = novoNonce()
            val p = Pendente(canal, nonce, conteudo, responderA, anexos, relogio(), depoisDe = s.ultimoId(canal))
            _estado.value = s.copy(pendentes = s.pendentes + p)
            enviarPendente(p)
            ultimoDigitando.remove(canal)
            nonce
        }

    /** Tenta de novo agora (botão "reenviar"). O mesmo `nonce` impede duplicar se a primeira cópia chegou. */
    fun reenviar(nonce: Long) = synchronized(lock) {
        _estado.value.pendentes.firstOrNull { it.nonce == nonce }?.let { enviarPendente(it) }
    }

    /** Desiste de uma pendente (botão "descartar"). */
    fun descartarPendente(nonce: Long) = modificar { s -> s.copy(pendentes = s.pendentes.filter { it.nonce != nonce }) }

    private fun enviarPendente(p: Pendente) {
        gateway.enviar(C2S.Send(p.canal, p.conteudo, p.responderA?.id, p.nonce, p.anexos.map { it.id }))
    }

    /** Positivo e menor que 2^62: `(ms << 8) | contador`, sempre crescente (nunca repete nesta execução). */
    private fun novoNonce(): Long {
        seqNonce++
        val calculado = (((relogio() and 0xFFFF_FFFF_FFFFL) shl 8) or (seqNonce and 0xFF)).coerceAtLeast(1)
        ultimoNonce = maxOf(calculado, ultimoNonce + 1)
        return ultimoNonce
    }

    private fun usuariosDoCanal(s: AppState, canal: Long) =
        s.snapshot?.servers?.firstOrNull { sv -> sv.channels.any { it.id == canal } }?.members
            ?: s.snapshot?.dms?.filter { it.channelId == canal }?.map { it.user }
            ?: emptyList()

    // ------------------------------------------------------------------ ações simples (sem estado local)

    fun editar(mensagem: Long, texto: String) { enviarNaoVazio(texto) { gateway.enviar(C2S.Edit(mensagem, it)) } }
    fun apagar(mensagem: Long) { gateway.enviar(C2S.Delete(mensagem)) }
    fun reagir(mensagem: Long, emoji: String, adicionar: Boolean) { gateway.enviar(C2S.React(mensagem, emoji, adicionar)) }
    fun fixar(mensagem: Long, fixar: Boolean) { gateway.enviar(C2S.Pin(mensagem, fixar)) }
    fun pedirFixadas(canal: Long) { gateway.enviar(C2S.Pins(canal)) }
    fun criarCanal(servidor: Long, nome: String, tipo: Int = 0, categoria: Long? = null) { gateway.enviar(C2S.CreateChannel(servidor, nome, tipo, categoria)) }
    fun criarServidor(nome: String) { gateway.enviar(C2S.CreateServer(nome)) }
    fun entrarNoServidor(codigo: String) { gateway.enviar(C2S.JoinServer(codigo)) }
    fun abrirDm(usuario: Long) { gateway.enviar(C2S.OpenDm(usuario)) }
    fun definirStatus(status: app.telacast.proto.Status) { gateway.enviar(C2S.SetStatus(status)) }
    fun pedirAmizade(usuario: String) { gateway.enviar(C2S.FriendRequest(usuario.trim())) }
    fun aceitarAmigo(usuario: Long) { gateway.enviar(C2S.FriendAccept(usuario)) }
    /** Remove amigo, recusa ou cancela pedido. */
    fun removerAmigo(usuario: Long) { gateway.enviar(C2S.FriendRemove(usuario)) }
    fun bloquear(usuario: Long) { gateway.enviar(C2S.Block(usuario)) }
    fun desbloquear(usuario: Long) { gateway.enviar(C2S.Unblock(usuario)) }
    fun trocarSenha(atual: String, nova: String) { gateway.enviar(C2S.ChangePassword(atual, nova)) }
    fun encerrarSessao(id: Long) { gateway.enviar(C2S.RevokeSession(id)) }
    fun encerrarOutrasSessoes() { gateway.enviar(C2S.RevokeOtherSessions) }
    fun pedirSessoes() { gateway.enviar(C2S.ListSessions) }

    /** Manda `Typing` no máximo a cada [intervaloDigitandoMs] por canal. */
    fun avisarDigitando(canal: Long) = synchronized(lock) {
        val agora = relogio()
        val ultimo = ultimoDigitando[canal]
        if (ultimo != null && agora - ultimo < intervaloDigitandoMs) return
        if (gateway.enviar(C2S.Typing(canal))) ultimoDigitando[canal] = agora
    }

    private inline fun enviarNaoVazio(texto: String, f: (String) -> Unit) {
        val t = texto.trim()
        if (t.isNotEmpty() && t.length <= LIMITE_TEXTO) f(t)
    }

    // ------------------------------------------------------------------ eventos do servidor

    private fun tratar(ev: EventoGateway) = synchronized(lock) {
        when (ev) {
            is EventoGateway.Autenticado -> autenticado(ev)
            is EventoGateway.Falha -> _estado.value = _estado.value.copy(
                erroDeEntrada = ErroDeEntrada(ev.code, ev.msg, ev.sessaoInvalida),
                autenticado = _estado.value.autenticado && !ev.sessaoInvalida,
            )
            is EventoGateway.Binario -> Unit // voz: ainda não
            is EventoGateway.Recebido -> mensagemDoServidor(ev.msg)
        }
    }

    private fun autenticado(ev: EventoGateway.Autenticado) {
        val atual = _estado.value
        // outra conta entrando: nada do estado anterior vale
        val base = if (!ev.reconexao && atual.eu != null && atual.eu.id != ev.eu.id) AppState(conexao = atual.conexao) else atual
        _estado.value = base.copy(autenticado = true, erroDeEntrada = null, eu = ev.eu)
        if (ev.reconexao) {
            // pode ter perdido mensagens enquanto estava fora: recarrega o canal aberto
            val aberto = _estado.value.canalAberto
            _estado.value = _estado.value.copy(carregados = emptySet(), buscandoAntigas = emptySet())
            if (aberto != null && gateway.enviar(C2S.History(aberto, null))) {
                _estado.value = _estado.value.copy(carregados = setOf(aberto))
            }
            // o servidor ignora o `nonce` que já recebeu; o que faltou sai agora
            _estado.value.pendentes.forEach { enviarPendente(it) }
        }
    }

    private fun mensagemDoServidor(m: S2C) {
        val s = _estado.value
        when (m) {
            is S2C.Snapshot -> snapshot(s, m.snapshot)
            is S2C.Message -> mensagemNova(s, m.message)
            is S2C.History -> historico(s, m)
            is S2C.MessageEdited -> _estado.value = s.copy(
                mensagens = s.mensagens.mapMensagem(m.channelId, m.messageId) { it.copy(content = m.content, edited = true) },
                fixadas = s.fixadas.mapMensagem(m.channelId, m.messageId) { it.copy(content = m.content, edited = true) },
            )
            is S2C.MessageDeleted -> _estado.value = s.copy(
                mensagens = s.mensagens.mapValues { (c, l) -> if (c == m.channelId) l.filter { it.id != m.messageId } else l },
                fixadas = s.fixadas.mapValues { (_, l) -> l.filter { it.id != m.messageId } },
            )
            is S2C.Reaction -> _estado.value = s.copy(
                mensagens = s.mensagens.mapMensagem(m.channelId, m.messageId) { reagiu(it, m, s.eu?.id) },
            )
            is S2C.Pinned -> {
                _estado.value = s.copy(mensagens = s.mensagens.mapMensagem(m.channelId, m.messageId) { it.copy(pinned = m.pinned) })
                // a lista de fixadas já pedida fica em dia (igual ao desktop com o painel aberto)
                if (m.channelId in s.fixadas) gateway.enviar(C2S.Pins(m.channelId))
            }
            is S2C.Pins -> _estado.value = s.copy(fixadas = s.fixadas + (m.channelId to m.messages))
            is S2C.Typing -> digitando(s, m)
            is S2C.Viewers -> _estado.value = s.copy(
                espectadores = if (m.users.isEmpty()) s.espectadores - m.channelId else s.espectadores + (m.channelId to m.users),
            )
            is S2C.VoiceUpdate -> _estado.value = s.copy(voz = s.voz.filter { it.userId != m.state.userId } + m.state)
            is S2C.VoiceLeft -> _estado.value = s.copy(voz = s.voz.filter { !(it.userId == m.userId && it.channelId == m.channelId) })
            is S2C.Sessions -> _estado.value = s.copy(sessoes = m.list)
            S2C.PasswordChanged -> _avisos.trySend(Aviso.SenhaTrocada)
            is S2C.DmOpened -> _avisos.trySend(Aviso.DmAberta(m.channelId))
            is S2C.ServerJoined -> _avisos.trySend(Aviso.ServidorEntrou(m.serverId))
            is S2C.Error -> _avisos.trySend(Aviso.Erro(m.code, m.msg))
            is S2C.SearchResults -> {
                val b = s.busca
                if (m.nonce == b.nonce) {
                    _estado.value = s.copy(busca = b.copy(
                        hits = if (b.anexando) b.hits + m.hits else m.hits, mais = m.more, carregando = false, anexando = false, buscou = true,
                    ))
                }
            }
            is S2C.Auth, is S2C.Desconhecida -> Unit // o Gateway já trata
        }
    }

    /** `Snapshot` substitui o anterior inteiro. Não lido = última mensagem mais nova que a última lida (fora o canal aberto). */
    private fun snapshot(s: AppState, sn: Snapshot) {
        val aberto = s.canalAberto
        val canais = sn.servers.flatMap { it.channels }.filter { it.kind == 0 }
            .map { Triple(it.id, it.lastMessageId, it.lastReadId) } +
            sn.dms.map { Triple(it.channelId, it.lastMessageId, it.lastReadId) }
        val naoLidos = canais.filter { (id, ultima, lida) -> ultima > lida && id != aberto }.map { it.first }.toSet()
        val mencoes = (sn.servers.flatMap { sv -> sv.channels.map { it.id to it.mentions } } + sn.dms.map { it.channelId to it.mentions })
            .filter { (id, n) -> n > 0 && id != aberto }.toMap()
        val existe = aberto != null && (sn.servers.any { sv -> sv.channels.any { it.id == aberto } } || sn.dms.any { it.channelId == aberto })
        _estado.value = s.copy(
            snapshot = sn, eu = sn.me, voz = sn.voice, naoLidos = naoLidos, mencoes = mencoes,
            canalAberto = if (existe) aberto else null,
        )
    }

    private fun mensagemNova(s: AppState, m: Message) {
        val ch = m.channelId
        val minha = s.eu?.id == m.authorId
        // o eco da minha mensagem troca a provisória pela definitiva
        val pendentes = if (minha && m.nonce != 0L) s.pendentes.filter { it.nonce != m.nonce } else s.pendentes
        val mensagens = if (ch in s.carregados) s.mensagens + (ch to inserirOrdenada(s.mensagens[ch].orEmpty(), m)) else s.mensagens
        var novo = s.copy(pendentes = pendentes, mensagens = mensagens)
        novo = tirarDigitando(novo, ch, m.authorId)
        if (!minha) {
            if (s.canalAberto == ch) {
                gateway.enviar(C2S.Ack(ch, m.id))
            } else {
                novo = novo.copy(naoLidos = novo.naoLidos + ch)
                val eu = s.eu
                if (eu != null && mencionaUsuario(m.content, eu.id)) {
                    novo = novo.copy(mencoes = novo.mencoes + (ch to (novo.mencoes[ch] ?: 0) + 1))
                }
            }
        }
        _estado.value = novo
    }

    private fun historico(s: AppState, h: S2C.History) {
        val ch = h.channelId
        val cheia = h.messages.size >= TAMANHO_PAGINA
        if (ch in s.buscandoAntigas) {
            // página mais antiga: entra na frente, sem repetir ids
            val atual = s.mensagens[ch].orEmpty()
            val tenho = atual.mapTo(HashSet()) { it.id }
            val frente = h.messages.filter { it.id !in tenho }
            var novo = s.copy(
                mensagens = s.mensagens + (ch to (frente + atual)),
                buscandoAntigas = s.buscandoAntigas - ch,
                temMais = s.temMais + (ch to cheia),
            )
            val salto = s.salto?.takeIf { it.canal == ch }
            if (salto != null) {
                val achou = novo.mensagens[ch].orEmpty().any { it.id == salto.id }
                if (achou) {
                    novo = novo.copy(salto = null, destaque = Destaque(ch, salto.id, relogio()))
                } else if (salto.paginas > 1 && cheia) {
                    novo = novo.copy(salto = salto.copy(paginas = salto.paginas - 1))
                } else {
                    novo = novo.copy(salto = null)
                    _avisos.trySend(Aviso.Texto("Não achei a mensagem original; ela é muito antiga"))
                }
            }
            _estado.value = novo
            if (novo.salto != null && !pedirAnteriores(ch)) _estado.value = _estado.value.copy(salto = null)
            return
        }
        // carga inicial (ou recarga após reconectar): vale o histórico, mais o que chegou ao vivo depois dele
        val ordenadas = h.messages.sortedBy { it.id }
        val ate = ordenadas.lastOrNull()?.id ?: 0L
        val vivas = s.mensagens[ch].orEmpty().filter { it.id > ate }
        _estado.value = s.copy(
            mensagens = s.mensagens + (ch to (ordenadas + vivas)),
            temMais = s.temMais + (ch to cheia),
            pendentes = conciliar(s.pendentes, s.eu?.id, ch, ordenadas),
        )
        // Marca como lido o que acabou de chegar. Sem isto, mensagens que entraram DEPOIS do último `Snapshot` (cujo
        // `last_message_id` fica velho) nunca seriam confirmadas ao servidor ao abrir o canal.
        if (s.canalAberto == ch && ate > 0) gateway.enviar(C2S.Ack(ch, ate))
        // pedi para ir a uma mensagem deste canal (busca/fixadas) antes de ele estar aberto: agora dá
        _estado.value.saltoAgendado?.takeIf { it.first == ch }?.let { (c, id) ->
            _estado.value = _estado.value.copy(saltoAgendado = null)
            saltarPara(c, id)
        }
    }

    /**
     * O histórico não devolve o `nonce` (vem 0). Depois de reconectar, uma pendente cuja primeira cópia chegou mas
     * cujo eco se perdeu apareceria duas vezes. Então: mensagem MINHA, mais nova que tudo o que o canal tinha ao
     * enviar (`depoisDe`) e com o mesmo texto, casa (uma para uma) com a pendente, que sai da fila.
     */
    private fun conciliar(pendentes: List<Pendente>, eu: Long?, canal: Long, historico: List<Message>): List<Pendente> {
        if (eu == null) return pendentes
        val usadas = HashSet<Long>()
        return pendentes.filter { p ->
            if (p.canal != canal) return@filter true
            val achada = historico.firstOrNull {
                it.authorId == eu && it.id > p.depoisDe && it.id !in usadas && it.kind == 0 && it.content.trim() == p.conteudo.trim()
            }
            if (achada != null) usadas += achada.id
            achada == null
        }
    }

    // ------------------------------------------------------------------ digitando

    private fun digitando(s: AppState, t: S2C.Typing) {
        if (t.userId == s.eu?.id) return
        val chave = t.channelId to t.userId
        jobsDigitando.remove(chave)?.cancel()
        _estado.value = s.copy(digitando = s.digitando + (t.channelId to (s.digitando[t.channelId].orEmpty() + t.userId)))
        jobsDigitando[chave] = scope.launch {
            delay(duracaoDigitandoMs)
            synchronized(lock) {
                jobsDigitando.remove(chave)
                _estado.value = tirarDigitando(_estado.value, t.channelId, t.userId)
            }
        }
    }

    private fun tirarDigitando(s: AppState, canal: Long, usuario: Long): AppState {
        jobsDigitando.remove(canal to usuario)?.cancel()
        val quem = s.digitando[canal] ?: return s
        if (usuario !in quem) return s
        val resto = quem - usuario
        return s.copy(digitando = if (resto.isEmpty()) s.digitando - canal else s.digitando + (canal to resto))
    }

    // ------------------------------------------------------------------ auxiliares

    private fun reagiu(m: Message, r: S2C.Reaction, eu: Long?): Message {
        val minha = r.userId == eu
        val i = m.reactions.indexOfFirst { it.emoji == r.emoji }
        val lista = when {
            i >= 0 -> {
                val atual = m.reactions[i]
                if (r.add) {
                    m.reactions.toMutableList().also { it[i] = atual.copy(count = atual.count + 1, me = atual.me || minha) }
                } else {
                    val n = maxOf(0, atual.count - 1)
                    if (n == 0) m.reactions.filterIndexed { j, _ -> j != i }
                    else m.reactions.toMutableList().also { it[i] = atual.copy(count = n, me = if (minha) false else atual.me) }
                }
            }
            r.add -> m.reactions + Reaction(r.emoji, 1, minha)
            else -> m.reactions
        }
        return m.copy(reactions = lista)
    }

    companion object {
        const val LIMITE_TEXTO = 4000
        const val TAMANHO_PAGINA = 50
        const val PAGINAS_DO_SALTO = 10
    }
}

/** O que enviar numa busca (já interpretado: texto livre, `de:` e escopo). */
data class PedidoDeBusca(val original: String, val texto: String, val canal: Long?, val servidor: Long?, val de: Long?)

/** Insere por id mantendo a ordem; id repetido não duplica (o eco e o histórico podem trazer a mesma). */
internal fun inserirOrdenada(lista: List<Message>, m: Message): List<Message> {
    if (lista.any { it.id == m.id }) return lista
    if (lista.isEmpty() || m.id > lista.last().id) return lista + m
    val i = lista.indexOfFirst { it.id > m.id }
    return lista.subList(0, i) + m + lista.subList(i, lista.size)
}

private fun Map<Long, List<Message>>.mapMensagem(canal: Long, id: Long, f: (Message) -> Message): Map<Long, List<Message>> {
    val lista = this[canal] ?: return this
    if (lista.none { it.id == id }) return this
    return this + (canal to lista.map { if (it.id == id) f(it) else it })
}
