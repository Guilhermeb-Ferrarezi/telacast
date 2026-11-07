package app.telacast.estado

import app.telacast.net.Credencial
import app.telacast.proto.ErrorCode

/** Campos da tela de entrada. `erro` já vem em português, pronto para mostrar. */
data class FormLogin(
    val servidor: String = "",
    val usuario: String = "",
    val senha: String = "",
    val convite: String = "",
    val registrar: Boolean = false,
    val entrando: Boolean = false,
    val erro: String? = null,
) {
    /** Mensagem do primeiro campo inválido, ou `null` se pode enviar (o servidor valida de novo). */
    fun validar(): String? = when {
        servidor.isBlank() -> "Informe o endereço do servidor."
        usuario.isBlank() -> "Informe o nome de usuário."
        registrar && usuario.trim().length !in 3..32 -> "O nome de usuário precisa ter de 3 a 32 caracteres."
        senha.isEmpty() -> "Informe a senha."
        registrar && senha.length !in 8..128 -> "A senha precisa ter de 8 a 128 caracteres."
        registrar && convite.isBlank() -> "Informe o código de convite."
        else -> null
    }

    fun credencial(): Credencial =
        if (registrar) Credencial.Registro(usuario.trim(), senha, convite.trim())
        else Credencial.Login(usuario.trim(), senha)
}

/** Textos de erro em português para o que o servidor (ou a rede) pode responder na entrada. */
object TextosDeErro {
    fun paraEntrada(codigo: ErrorCode?, msgDoServidor: String): String = when (codigo) {
        ErrorCode.InvalidCredentials -> "Usuário ou senha incorretos."
        ErrorCode.InvalidInvite -> "Código de convite inválido, expirado ou já usado."
        ErrorCode.UsernameTaken -> "Esse nome de usuário já está em uso."
        ErrorCode.RateLimited -> "Muitas tentativas. Tente de novo em alguns minutos."
        ErrorCode.VersionMismatch -> "Este app é antigo demais para esse servidor. Atualize o Telacast."
        ErrorCode.SessionExpired, ErrorCode.NotAuthenticated -> "Sua sessão expirou. Entre de novo."
        ErrorCode.Internal -> "O servidor teve um problema. Tente de novo daqui a pouco."
        // validação (nome, senha): o servidor já explica em português
        ErrorCode.InvalidInput -> msgDoServidor.ifBlank { "Confira os campos e tente de novo." }
        null -> "Não consegui falar com o servidor. Confira o endereço e a conexão."
        else -> msgDoServidor.ifBlank { "Não foi possível entrar." }
    }
}
