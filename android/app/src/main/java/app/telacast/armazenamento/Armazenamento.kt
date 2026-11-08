package app.telacast.armazenamento

import app.telacast.voz.OpcoesDeVoz

/** Preferências simples do app (nada secreto aqui). Puro Kotlin: o Android entra só em [PreferenciasAndroid]. */
interface Preferencias {
    /** Último servidor digitado (`host:porta` ou URL). */
    var servidor: String
    /** Último nome de usuário. */
    var usuario: String
    /** Categorias recolhidas na lista de canais (ids). */
    var categoriasRecolhidas: Set<Long>
    /** Último canal aberto (para reabrir o app onde parou); 0 = nenhum. */
    var ultimoCanal: Long
    var ultimoServidor: Long
    /** Última conversa direta aberta (0 = a última foi um canal): o app reabre onde parou depois de o processo morrer. */
    var ultimaDm: Long
    /** "Manter conectado": serviço em primeiro plano que mantém o WebSocket vivo para as notificações locais (padrão ligado). */
    var manterConectado: Boolean
    /** Última versão nova cuja folha "O que mudou" já foi mostrada. */
    var versaoVista: String
    /** Tamanho do texto das mensagens: 0 pequeno, 1 normal, 2 grande. */
    var tamanhoDoTexto: Int
    /** Opções de voz (supressão de ruído, eco, ganho, sensibilidade). */
    var opcoesDeVoz: OpcoesDeVoz
    /** Token do FCM desta instalação (vazio = ainda não temos); não é segredo de login. */
    var tokenDePush: String
    /** Já pedimos a permissão de notificações uma vez (não insistimos). */
    var pediuNotificacoes: Boolean
    /** Rascunho do compositor por canal (some ao enviar). */
    fun rascunho(canal: Long): String
    fun salvarRascunho(canal: Long, texto: String)
}

/** Onde mora o token da sessão: cifrado em disco no app, em memória nos testes. */
interface CofreDeSegredos {
    fun guardar(texto: String)
    fun ler(): String?
    fun apagar()
}

class PreferenciasEmMemoria(
    override var servidor: String = "",
    override var usuario: String = "",
    override var categoriasRecolhidas: Set<Long> = emptySet(),
    override var ultimoCanal: Long = 0,
    override var ultimoServidor: Long = 0,
    override var ultimaDm: Long = 0,
    override var manterConectado: Boolean = true,
    override var versaoVista: String = "",
    override var tamanhoDoTexto: Int = 1,
    override var opcoesDeVoz: OpcoesDeVoz = OpcoesDeVoz(),
    override var tokenDePush: String = "",
    override var pediuNotificacoes: Boolean = false,
) : Preferencias {
    private val rascunhos = HashMap<Long, String>()
    override fun rascunho(canal: Long) = rascunhos[canal].orEmpty()
    override fun salvarRascunho(canal: Long, texto: String) { if (texto.isEmpty()) rascunhos.remove(canal) else rascunhos[canal] = texto }
}

class CofreEmMemoria(private var valor: String? = null) : CofreDeSegredos {
    override fun guardar(texto: String) { valor = texto }
    override fun ler(): String? = valor
    override fun apagar() { valor = null }
}
