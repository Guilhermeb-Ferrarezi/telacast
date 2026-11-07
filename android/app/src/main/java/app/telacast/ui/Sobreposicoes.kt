package app.telacast.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.telacast.estado.Dialogo
import app.telacast.proto.Status
import app.telacast.tema.Formas
import app.telacast.tema.Grafite
import app.telacast.tema.Tipos

// ------------------------------------------------------------------ menus (desenhados na própria composição, sem Popup)

/** Caixa de menu: camada 3, filete de 1 px, raio 10, sem sombra. */
@Composable
fun CaixaDeMenu(modifier: Modifier = Modifier, conteudo: @Composable ColumnScope.() -> Unit) {
    Column(
        modifier.width(232.dp).background(Grafite.L3, Formas.Menu).border(1.dp, Grafite.Linha2, Formas.Menu).padding(6.dp),
        content = conteudo,
    )
}

@Composable
fun ItemDeMenu(rotulo: String, aoClicar: () -> Unit, cor: Color = Grafite.Texto, inicio: (@Composable () -> Unit)? = null) {
    Toque(aoClicar, Modifier.fillMaxWidth().height(44.dp), Formas.Linha, fundoPressionado = Grafite.L4) {
        Row(Modifier.fillMaxSize().padding(horizontal = 10.dp), verticalAlignment = Alignment.CenterVertically) {
            if (inicio != null) { Box(Modifier.width(22.dp), contentAlignment = Alignment.CenterStart) { inicio() }; Spacer(Modifier.width(10.dp)) }
            Text(rotulo, style = Tipos.Corpo.copy(color = cor))
        }
    }
}

@Composable
fun SeparadorDeMenu() {
    Box(Modifier.fillMaxWidth().padding(vertical = 4.dp).height(1.dp).background(Grafite.Linha2))
}

@Composable
fun MenuDoServidor(dono: Boolean, aoConvidar: () -> Unit, aoCriarCanal: () -> Unit, aoCriarCategoria: () -> Unit, modifier: Modifier = Modifier) {
    CaixaDeMenu(modifier) {
        ItemDeMenu("Convidar pessoas", aoConvidar)
        if (dono) {
            SeparadorDeMenu()
            ItemDeMenu("Criar canal", aoCriarCanal)
            ItemDeMenu("Criar categoria", aoCriarCategoria)
        }
    }
}

@Composable
fun MenuDeStatus(atual: Status, aoEscolher: (Status) -> Unit, aoSair: () -> Unit, modifier: Modifier = Modifier) {
    CaixaDeMenu(modifier) {
        for (st in listOf(Status.Online, Status.Idle, Status.Dnd, Status.Invisible)) {
            ItemDeMenu(
                st.rotulo(), { aoEscolher(st) }, cor = if (st == atual) Grafite.Ambar else Grafite.Texto,
                inicio = { PontoDePresenca(st.presenca(), Grafite.L3) },
            )
        }
        SeparadorDeMenu()
        ItemDeMenu("Sair da conta", aoSair, cor = Grafite.Vermelho)
    }
}

// ------------------------------------------------------------------ janelas (desenhadas na própria composição)

/** Janela modal: fundo escurecido (clique fora fecha), cartão camada 3 com filete, título em serifa e botão de fechar. */
@Composable
fun Janela(titulo: String, aoFechar: () -> Unit, conteudo: @Composable ColumnScope.() -> Unit) {
    Box(
        Modifier.fillMaxSize().background(Color.Black.copy(alpha = 0.6f)).imePadding()
            .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = aoFechar),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            Modifier.padding(horizontal = 24.dp).widthIn(max = 360.dp).fillMaxWidth()
                .background(Grafite.L3, Formas.Janela).border(1.dp, Grafite.Linha2, Formas.Janela)
                // engole o toque para não fechar ao tocar dentro
                .clickable(interactionSource = remember { MutableInteractionSource() }, indication = null, onClick = {})
                .padding(22.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(titulo, style = Tipos.Subtitulo, modifier = Modifier.weight(1f))
                Toque(aoFechar, Modifier.size(32.dp), Formas.Linha, fundoPressionado = Grafite.L4) {
                    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { IconeFechar(Grafite.Apagado) }
                }
            }
            Spacer(Modifier.height(14.dp))
            conteudo()
        }
    }
}

class AcoesDialogo(
    val aoFechar: () -> Unit = {},
    val aoCriarServidor: (String) -> Unit = {},
    val aoEntrarNoServidor: (String) -> Unit = {},
    val aoTrocarModo: (criar: Boolean) -> Unit = {},
    val aoCriarCanal: (servidor: Long, nome: String, categoria: Boolean, pai: Long?) -> Unit = { _, _, _, _ -> },
    val aoConfirmarSaida: () -> Unit = {},
)

@Composable
fun DialogoAtual(d: Dialogo, a: AcoesDialogo) {
    when (d) {
        is Dialogo.AdicionarServidor -> DialogoAdicionarServidor(d.criar, a)
        is Dialogo.CriarCanal -> DialogoCriarCanal(d, a)
        is Dialogo.Convidar -> DialogoConvidar(d.codigo, a.aoFechar)
        Dialogo.ConfirmarSair -> Janela("Sair da conta?", a.aoFechar) {
            Text("Você vai precisar entrar de novo com seu usuário e senha.", style = Tipos.CorpoApagado)
            Spacer(Modifier.height(18.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Botao("Cancelar", TipoBotao.Quieto, Modifier.weight(1f), aoClicar = a.aoFechar)
                Botao("Sair", TipoBotao.Perigo, Modifier.weight(1f), aoClicar = a.aoConfirmarSaida)
            }
        }
    }
}

@Composable
private fun DialogoAdicionarServidor(criar: Boolean, a: AcoesDialogo) {
    var texto by remember(criar) { mutableStateOf("") }
    Janela(if (criar) "Criar um servidor" else "Entrar em um servidor", a.aoFechar) {
        Row(Modifier.fillMaxWidth().background(Grafite.L2, Formas.Botao).padding(3.dp), horizontalArrangement = Arrangement.spacedBy(3.dp)) {
            for ((rotulo, ehCriar) in listOf("Criar" to true, "Tenho um convite" to false)) {
                Toque(
                    { a.aoTrocarModo(ehCriar) }, Modifier.weight(1f).height(38.dp), Formas.Linha,
                    fundo = if (criar == ehCriar) Grafite.L4 else Color.Transparent, fundoPressionado = Grafite.L4,
                ) {
                    Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                        Text(rotulo, style = Tipos.Botao.copy(color = if (criar == ehCriar) Grafite.Texto else Grafite.Apagado))
                    }
                }
            }
        }
        Spacer(Modifier.height(16.dp))
        Rotulo(if (criar) "Nome do servidor" else "Código de convite", Modifier.padding(bottom = 6.dp))
        Campo(texto, { texto = it }, if (criar) "Turma do Telacast" else "ABCD-1234", aoEnviar = { if (criar) a.aoCriarServidor(texto) else a.aoEntrarNoServidor(texto) })
        Spacer(Modifier.height(16.dp))
        Botao(if (criar) "Criar servidor" else "Entrar", TipoBotao.Primario, Modifier.fillMaxWidth()) {
            if (criar) a.aoCriarServidor(texto) else a.aoEntrarNoServidor(texto)
        }
    }
}

@Composable
private fun DialogoCriarCanal(d: Dialogo.CriarCanal, a: AcoesDialogo) {
    var nome by remember { mutableStateOf("") }
    Janela(if (d.categoria) "Criar categoria" else "Criar canal", a.aoFechar) {
        Rotulo(if (d.categoria) "Nome da categoria" else "Nome do canal", Modifier.padding(bottom = 6.dp))
        Campo(nome, { nome = it }, if (d.categoria) "Canais de texto" else "novo-canal", aoEnviar = { a.aoCriarCanal(d.servidor, nome, d.categoria, d.pai) })
        if (!d.categoria) Text("Espaços viram hífens e as letras ficam minúsculas.", style = Tipos.Pequeno.copy(color = Grafite.Apagado), modifier = Modifier.padding(top = 8.dp))
        Spacer(Modifier.height(16.dp))
        Botao("Criar", TipoBotao.Primario, Modifier.fillMaxWidth()) { a.aoCriarCanal(d.servidor, nome, d.categoria, d.pai) }
    }
}

@Composable
private fun DialogoConvidar(codigo: String, aoFechar: () -> Unit) {
    val area = LocalClipboardManager.current
    Janela("Convidar pessoas", aoFechar) {
        Text("Quem tiver este código entra no servidor (em Entrar em um servidor, Tenho um convite).", style = Tipos.CorpoApagado)
        Spacer(Modifier.height(16.dp))
        Box(Modifier.fillMaxWidth().background(Grafite.L2, Formas.Botao).border(1.dp, Grafite.Linha2, Formas.Botao).padding(vertical = 16.dp), contentAlignment = Alignment.Center) {
            Text(codigo, style = Tipos.Subtitulo.copy(fontSize = 26.sp, letterSpacing = 2.sp, fontWeight = FontWeight.Medium, color = Grafite.Ambar))
        }
        Spacer(Modifier.height(16.dp))
        Botao("Copiar código", TipoBotao.Primario, Modifier.fillMaxWidth()) { area.setText(AnnotatedString(codigo)) }
    }
}

/** Aviso curto no pé da tela (erro do servidor, "voz em breve"): camada 3 com filete, sem sombra. */
@Composable
fun AvisoCurto(texto: String, modifier: Modifier = Modifier) {
    Box(modifier.padding(horizontal = 16.dp).background(Grafite.L3, Formas.Menu).border(1.dp, Grafite.Linha2, Formas.Menu).padding(horizontal = 16.dp, vertical = 12.dp)) {
        Text(texto, style = Tipos.Corpo.copy(fontSize = 13.5.sp, lineHeight = 19.sp))
    }
}
