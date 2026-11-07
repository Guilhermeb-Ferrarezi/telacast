package app.telacast.estado

import app.telacast.proto.Attachment

/** Para onde o centro da tela aponta. */
sealed interface Destino {
    /** Início: amigos (e, na gaveta, as mensagens diretas). */
    data object Amigos : Destino
    data class Dm(val canal: Long) : Destino
    data class Canal(val servidor: Long, val canal: Long) : Destino
}

enum class AbaInicio(val rotulo: String) {
    Disponivel("Disponível"), Todos("Todos"), Pendente("Pendente"), Bloqueados("Bloqueados"), Adicionar("Adicionar amigo"),
}

/** Menus que abrem sobre a gaveta. */
enum class MenuAberto { Servidor, Status }

sealed interface Dialogo {
    /** Botão "+" do trilho: criar um servidor ou entrar com um código de convite. */
    data class AdicionarServidor(val criar: Boolean = true) : Dialogo
    /** `categoria` = criar categoria em vez de canal; `pai` = categoria onde o canal nasce. */
    data class CriarCanal(val servidor: Long, val categoria: Boolean = false, val pai: Long? = null) : Dialogo
    data class Convidar(val codigo: String) : Dialogo
    data object ConfirmarSair : Dialogo
    data class ConfirmarApagar(val mensagem: Long, val previa: String) : Dialogo
    /** Imagem de um anexo em tela cheia. */
    data class Imagem(val anexo: Attachment) : Dialogo
    /** Texto de um anexo em tela cheia. */
    data class Texto(val anexo: Attachment) : Dialogo
    /** Mensagens fixadas do canal aberto. */
    data object Fixadas : Dialogo
    /** Busca de mensagens (no servidor ou neste canal). */
    data object Busca : Dialogo
    /** Ajustes: conta (senha, sessões), aparência. */
    data object Ajustes : Dialogo
    data class ConfirmarBloquear(val usuario: Long, val nome: String) : Dialogo
    data class ConfirmarRemoverAmigo(val usuario: Long, val nome: String) : Dialogo
}

/** Perfil em folha inferior; `nome` serve de reserva quando a pessoa não está no snapshot (autor de mensagem antiga). */
data class PerfilAberto(val usuario: Long, val nome: String)

/** Folha inferior de uma mensagem (toque longo); `emojis` mostra o seletor de reação. */
data class FolhaDeMensagem(val mensagem: Long, val emojis: Boolean = false)

/**
 * Estado de navegação do app, por conta própria (sem biblioteca de navegação). Imutável; cada função devolve o novo
 * estado, então dá para testar na JVM e desenhar qualquer combinação nos testes de imagem.
 */
data class Navegacao(
    val destino: Destino = Destino.Amigos,
    /** Servidor cuja lista de canais aparece na gaveta; `null` = Início (mensagens diretas). */
    val servidorNoPainel: Long? = null,
    val gavetaAberta: Boolean = false,
    val membrosAbertos: Boolean = false,
    val menu: MenuAberto? = null,
    val dialogo: Dialogo? = null,
    val folha: FolhaDeMensagem? = null,
    val perfil: PerfilAberto? = null,
    val abaInicio: AbaInicio = AbaInicio.Disponivel,
) {
    fun abrirGaveta() = copy(gavetaAberta = true, membrosAbertos = false, menu = null)
    fun fecharGaveta() = copy(gavetaAberta = false, menu = null)
    fun abrirMembros() = copy(membrosAbertos = true, gavetaAberta = false, menu = null)
    fun fecharMembros() = copy(membrosAbertos = false)

    /** Trocar o servidor da gaveta não muda o centro nem fecha a gaveta (como no Discord mobile). */
    fun selecionarServidor(id: Long?) = copy(servidorNoPainel = id, menu = null)

    /** Navega e fecha a gaveta. */
    fun irPara(d: Destino): Navegacao {
        val painel = when (d) {
            is Destino.Canal -> d.servidor
            else -> null
        }
        return copy(destino = d, servidorNoPainel = painel, gavetaAberta = false, membrosAbertos = false, menu = null)
    }

    fun abrirMenu(m: MenuAberto) = copy(menu = if (menu == m) null else m)
    fun fecharMenu() = copy(menu = null)
    fun abrirDialogo(d: Dialogo) = copy(dialogo = d, menu = null, folha = null, perfil = null)
    fun abrirPerfil(usuario: Long, nome: String) = copy(perfil = PerfilAberto(usuario, nome), menu = null, folha = null)
    fun fecharPerfil() = copy(perfil = null)
    fun abrirFolha(mensagem: Long, emojis: Boolean = false) = copy(folha = FolhaDeMensagem(mensagem, emojis), menu = null)
    fun fecharFolha() = copy(folha = null)
    fun fecharDialogo() = copy(dialogo = null)
    fun escolherAba(a: AbaInicio) = copy(abaInicio = a)

    /** Algo que o botão Voltar do sistema fecha antes de sair do app? */
    val podeVoltar: Boolean get() = perfil != null || dialogo != null || folha != null || menu != null || membrosAbertos || gavetaAberta

    /** Voltar do sistema: fecha, nesta ordem, diálogo, folha, menu, painel de membros e gaveta. */
    fun voltar(): Navegacao = when {
        perfil != null -> fecharPerfil()
        dialogo != null -> fecharDialogo()
        folha != null -> fecharFolha()
        menu != null -> fecharMenu()
        membrosAbertos -> fecharMembros()
        gavetaAberta -> fecharGaveta()
        else -> this
    }
}
