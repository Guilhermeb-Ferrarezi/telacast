package app.telacast.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.telacast.estado.FormLogin
import app.telacast.tema.Grafite
import app.telacast.tema.Tipos

class AcoesLogin(
    val aoEditar: ((FormLogin) -> FormLogin) -> Unit = {},
    val aoEntrar: () -> Unit = {},
    val aoAlternarModo: () -> Unit = {},
)

/** Marca do app: "Telacast" em serifa. Sem logotipo nem imagem (a identidade Grafite é só tipografia e camadas). */
@Composable
private fun Marca(subtitulo: String) {
    Text("Telacast", style = Tipos.Titulo.copy(fontSize = 44.sp, lineHeight = 48.sp, letterSpacing = (-0.5).sp), modifier = Modifier.fillMaxWidth(), textAlign = TextAlign.Start)
    Text(subtitulo, style = Tipos.CorpoApagado, modifier = Modifier.padding(top = 4.dp))
}

/** Entrar / Registrar-se (com convite). Sem lógica: o [FormLogin] e as ações vêm de fora. */
@Composable
fun TelaDeLogin(form: FormLogin, acoes: AcoesLogin, modifier: Modifier = Modifier) {
    Box(modifier.fillMaxSize().background(Grafite.L2).systemBarsPadding().imePadding(), contentAlignment = Alignment.TopCenter) {
        Column(
            Modifier.widthIn(max = 420.dp).fillMaxWidth().verticalScroll(rememberScrollState()).padding(horizontal = 24.dp, vertical = 32.dp),
        ) {
            Spacer(Modifier.height(24.dp))
            Marca(if (form.registrar) "Crie sua conta com um convite." else "Conversa, voz e transmissão de tela.")
            Spacer(Modifier.height(36.dp))
            Text(if (form.registrar) "Criar conta" else "Entrar", style = Tipos.Titulo)
            Spacer(Modifier.height(18.dp))

            val livre = !form.entrando
            Rotulo("Servidor", Modifier.padding(bottom = 6.dp))
            Campo(
                form.servidor, { v -> acoes.aoEditar { it.copy(servidor = v) } }, "endereço.do.servidor:8080",
                habilitado = livre, teclado = KeyboardOptions(keyboardType = KeyboardType.Uri, imeAction = ImeAction.Next, autoCorrectEnabled = false),
            )
            Spacer(Modifier.height(14.dp))
            Rotulo("Usuário", Modifier.padding(bottom = 6.dp))
            Campo(
                form.usuario, { v -> acoes.aoEditar { it.copy(usuario = v) } }, "seu_nome",
                habilitado = livre, teclado = KeyboardOptions(keyboardType = KeyboardType.Text, imeAction = ImeAction.Next, autoCorrectEnabled = false),
            )
            Spacer(Modifier.height(14.dp))
            Rotulo("Senha", Modifier.padding(bottom = 6.dp))
            Campo(
                form.senha, { v -> acoes.aoEditar { it.copy(senha = v) } }, if (form.registrar) "de 8 a 128 caracteres" else "sua senha",
                senha = true, habilitado = livre,
                teclado = KeyboardOptions(keyboardType = KeyboardType.Password, imeAction = if (form.registrar) ImeAction.Next else ImeAction.Go),
                aoEnviar = if (form.registrar) null else acoes.aoEntrar,
            )
            if (form.registrar) {
                Spacer(Modifier.height(14.dp))
                Rotulo("Código de convite", Modifier.padding(bottom = 6.dp))
                Campo(
                    form.convite, { v -> acoes.aoEditar { it.copy(convite = v) } }, "ABCD-1234", habilitado = livre,
                    teclado = KeyboardOptions(keyboardType = KeyboardType.Ascii, imeAction = ImeAction.Go, autoCorrectEnabled = false),
                    aoEnviar = acoes.aoEntrar,
                )
            }

            if (form.erro != null) {
                Text(form.erro, style = Tipos.Corpo.copy(fontSize = 13.5.sp, lineHeight = 19.sp, color = Grafite.Vermelho), modifier = Modifier.padding(top = 14.dp))
            }

            Spacer(Modifier.height(22.dp))
            Botao(
                if (form.entrando) "Entrando…" else if (form.registrar) "Criar conta" else "Entrar",
                TipoBotao.Primario, Modifier.fillMaxWidth(), altura = 46.dp, habilitado = livre, aoClicar = acoes.aoEntrar,
            )
            Spacer(Modifier.height(18.dp))
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) {
                Text(if (form.registrar) "Já tem uma conta? " else "Ainda não tem conta? ", style = Tipos.CorpoApagado)
                Text(
                    if (form.registrar) "Entrar" else "Registrar-se",
                    style = Tipos.Corpo.copy(color = if (livre) Grafite.Ambar else Grafite.Apagado, fontWeight = FontWeight.SemiBold),
                    modifier = Modifier.clickable(enabled = livre, interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = acoes.aoAlternarModo).padding(vertical = 10.dp),
                )
            }
            if (!form.registrar) {
                Text("O cadastro é fechado: só entra quem tem um código de convite.", style = Tipos.Pequeno.copy(color = Grafite.Apagado), textAlign = TextAlign.Center, modifier = Modifier.fillMaxWidth())
            }
        }
    }
}

/** Retomando a sessão salva: "Entrando…" (e, se a rede demora, a saída para outra conta). */
@Composable
fun TelaDeCarregamento(semConexao: Boolean, aoUsarOutraConta: () -> Unit, modifier: Modifier = Modifier) {
    Box(modifier.fillMaxSize().background(Grafite.L2).systemBarsPadding(), contentAlignment = Alignment.Center) {
        Column(Modifier.padding(32.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            Text("Telacast", style = Tipos.Titulo.copy(fontSize = 44.sp, lineHeight = 48.sp, letterSpacing = (-0.5).sp))
            Spacer(Modifier.height(10.dp))
            Text(if (semConexao) "Sem conexão. Tentando de novo…" else "Entrando…", style = Tipos.CorpoApagado.copy(color = if (semConexao) Grafite.Ambar else Grafite.Apagado))
            Spacer(Modifier.height(28.dp))
            Botao("Usar outra conta", TipoBotao.Quieto, aoClicar = aoUsarOutraConta)
        }
    }
}
