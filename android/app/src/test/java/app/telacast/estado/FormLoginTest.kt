package app.telacast.estado

import app.telacast.net.Credencial
import app.telacast.proto.ErrorCode
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class FormLoginTest {
    private val ok = FormLogin(servidor = "h:1", usuario = "ana", senha = "x")

    @Test fun validaCamposNaOrdem() {
        assertEquals("Informe o endereço do servidor.", FormLogin().validar())
        assertEquals("Informe o nome de usuário.", ok.copy(usuario = " ").validar())
        assertEquals("Informe a senha.", ok.copy(senha = "").validar())
        assertNull("entrar aceita qualquer senha não vazia (o servidor confere)", ok.validar())
    }

    @Test fun cadastroExigeNomeSenhaEConviteNosLimites() {
        val r = ok.copy(registrar = true, senha = "12345678", convite = "ABCD-1234")
        assertNull(r.validar())
        assertEquals("O nome de usuário precisa ter de 3 a 32 caracteres.", r.copy(usuario = "ab").validar())
        assertEquals("A senha precisa ter de 8 a 128 caracteres.", r.copy(senha = "1234567").validar())
        assertEquals("A senha precisa ter de 8 a 128 caracteres.", r.copy(senha = "x".repeat(129)).validar())
        assertEquals("Informe o código de convite.", r.copy(convite = "  ").validar())
    }

    @Test fun credencialAparaOsEspacos() {
        assertEquals(Credencial.Login("ana", "x"), ok.copy(usuario = " ana ").credencial())
        assertEquals(Credencial.Registro("ana", "12345678", "AB-12"), ok.copy(registrar = true, senha = "12345678", convite = " AB-12 ").credencial())
    }

    @Test fun errosEmPortugues() {
        assertEquals("Usuário ou senha incorretos.", TextosDeErro.paraEntrada(ErrorCode.InvalidCredentials, "x"))
        assertEquals("Código de convite inválido, expirado ou já usado.", TextosDeErro.paraEntrada(ErrorCode.InvalidInvite, ""))
        assertEquals("Esse nome de usuário já está em uso.", TextosDeErro.paraEntrada(ErrorCode.UsernameTaken, ""))
        assertEquals("Muitas tentativas. Tente de novo em alguns minutos.", TextosDeErro.paraEntrada(ErrorCode.RateLimited, ""))
        assertEquals("Sua sessão expirou. Entre de novo.", TextosDeErro.paraEntrada(ErrorCode.SessionExpired, ""))
        assertEquals("Nome de usuário: 3 a 32 caracteres", TextosDeErro.paraEntrada(ErrorCode.InvalidInput, "Nome de usuário: 3 a 32 caracteres"))
        assertEquals("Não consegui falar com o servidor. Confira o endereço e a conexão.", TextosDeErro.paraEntrada(null, "Sem conexão"))
        assertEquals("Algo novo", TextosDeErro.paraEntrada(ErrorCode.Unknown, "Algo novo"))
    }
}
