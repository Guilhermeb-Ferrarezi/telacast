package app.telacast.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.systemBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.telacast.tema.Formas
import app.telacast.tema.Grafite
import app.telacast.tema.GrafiteTheme
import app.telacast.tema.Tipos

/** Tela de amostra do tema, para comparar com o desktop (`docs/ui-preview/grafite.html`). Rola na tela. */
@Composable
fun PaletaScreen(modifier: Modifier = Modifier) {
    GrafiteTheme {
        Box(modifier.fillMaxSize().background(Grafite.L2).systemBarsPadding()) {
            Column(Modifier.verticalScroll(rememberScrollState())) { PaletaConteudo() }
        }
    }
}

/** Só o conteúdo, sem rolagem: é o que o teste de snapshot desenha inteiro. */
@Composable
fun PaletaConteudo() {
    Column(
        Modifier.fillMaxWidth().background(Grafite.L2).padding(horizontal = 16.dp, vertical = 20.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Rotulo("Telacast · Android")
        Text("Paleta Grafite", style = Tipos.Titulo.copy(fontSize = 32.sp, lineHeight = 36.sp))
        Text("Cinzas em camadas, um único acento âmbar, filetes de 1 px, títulos em serifa.", style = Tipos.CorpoApagado)
        Filete()

        // ---- camadas
        Rotulo("Camadas")
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Amostra("L0", "0a0a0b", Grafite.L0, Modifier.weight(1f))
            Amostra("L1", "101012", Grafite.L1, Modifier.weight(1f))
            Amostra("L2", "151517", Grafite.L2, Modifier.weight(1f))
            Amostra("L3", "1c1c1f", Grafite.L3, Modifier.weight(1f))
            Amostra("L4", "252528", Grafite.L4, Modifier.weight(1f))
        }
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Amostra("Linha", "242427", Grafite.Linha, Modifier.weight(1f), alta = false)
            Amostra("Linha2", "303034", Grafite.Linha2, Modifier.weight(1f), alta = false)
        }

        // ---- texto e acento
        Rotulo("Texto")
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            AmostraTexto("Texto", "ecebe7", Grafite.Texto, Modifier.weight(1f))
            AmostraTexto("Texto2", "b7b5ae", Grafite.Texto2, Modifier.weight(1f))
            AmostraTexto("Apagado", "8a8882", Grafite.Apagado, Modifier.weight(1f))
        }
        Rotulo("Âmbar e semânticas")
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Amostra("Âmbar", "e9b35e", Grafite.Ambar, Modifier.weight(1f), tinta = Grafite.AmbarTinta)
            Amostra("Quente", "f0c070", Grafite.AmbarQuente, Modifier.weight(1f), tinta = Grafite.AmbarTinta)
            Amostra("Tinta", "1a1204", Grafite.AmbarTinta, Modifier.weight(1f))
            Amostra("Ok", "74b98f", Grafite.Ok, Modifier.weight(1f), tinta = Grafite.AmbarTinta)
            Amostra("Vermelho", "d9806f", Grafite.Vermelho, Modifier.weight(1f), tinta = Grafite.AmbarTinta)
        }
        Filete()

        // ---- títulos em serifa (Newsreader)
        Rotulo("Títulos em serifa · Newsreader")
        Text("Canais de texto", style = Tipos.Titulo.copy(fontSize = 28.sp, lineHeight = 32.sp))
        Text("Geral e conversas", style = Tipos.Titulo)
        Text("Servidor da turma", style = Tipos.Subtitulo)
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Filete(Modifier.weight(1f))
            Text("ontem", style = Tipos.Divisor)
            Filete(Modifier.weight(1f))
        }
        Row(horizontalArrangement = Arrangement.spacedBy(14.dp), verticalAlignment = Alignment.Bottom) {
            for (w in listOf(400, 500, 600)) {
                Text("Aa $w", style = Tipos.Titulo.copy(fontWeight = FontWeight(w)))
            }
        }
        Filete()

        // ---- texto em Inter, vários pesos
        Rotulo("Texto · Inter")
        for ((w, nome) in listOf(300 to "Leve", 400 to "Regular", 500 to "Médio", 600 to "Seminegrito", 700 to "Negrito")) {
            Text("$nome $w · a rápida raposa marrom 0123456789", style = Tipos.Corpo.copy(fontWeight = FontWeight(w)))
        }
        Text("Texto pequeno de apoio, 12 sp, segunda cor.", style = Tipos.Pequeno)
        Filete()

        // ---- componentes
        Rotulo("Avatares")
        Row(horizontalArrangement = Arrangement.spacedBy(14.dp), verticalAlignment = Alignment.CenterVertically) {
            Avatar("Guilherme", 0, 48.dp, Presenca.Online)
            Avatar("Ana", 1, 48.dp, Presenca.Ausente)
            Avatar("Bruno", 2, 48.dp, Presenca.NaoPerturbe)
            Avatar("Carla", 3, 48.dp, Presenca.Offline)
            Avatar("Davi", 4, 48.dp)
            Avatar("Eva", 0, 32.dp)
        }

        Rotulo("Botões")
        Botao("Entrar", TipoBotao.Primario, Modifier.fillMaxWidth())
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Botao("Cancelar", TipoBotao.Quieto, Modifier.weight(1f))
            Botao("Sair do servidor", TipoBotao.Perigo, Modifier.weight(1f))
        }

        Rotulo("Campo de texto")
        Campo("", {}, "Mensagem em #geral")
        var texto by remember { mutableStateOf("Oi, tudo bem?") }
        Campo(texto, { texto = it }, "", focado = true)

        Rotulo("Canais e não lidos")
        Column(Modifier.fillMaxWidth().clip(Formas.Cartao).background(Grafite.L1).border(1.dp, Grafite.Linha, Formas.Cartao).padding(8.dp)) {
            LinhaDeCanal("geral", selecionado = true)
            LinhaDeCanal("avisos", naoLidas = 3)
            LinhaDeCanal("memes", naoLidas = 42)
            LinhaDeCanal("fotos", naoLidas = 120)
            LinhaDeCanal("off-topic")
        }
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp), verticalAlignment = Alignment.CenterVertically) {
            SeloNaoLido(1); SeloNaoLido(9); SeloNaoLido(42); SeloNaoLido(120)
        }
        Spacer(Modifier.height(8.dp))
    }
}

@Composable
private fun Amostra(nome: String, hex: String, cor: Color, modifier: Modifier = Modifier, alta: Boolean = true, tinta: Color = Grafite.Texto2) {
    Column(
        modifier.height(if (alta) 76.dp else 44.dp).clip(Formas.Linha).background(cor).border(1.dp, Grafite.Linha2, Formas.Linha).padding(8.dp),
        verticalArrangement = Arrangement.Bottom,
    ) {
        Text(nome, style = Tipos.Pequeno.copy(color = tinta, fontWeight = FontWeight.SemiBold, fontSize = 11.sp, lineHeight = 14.sp))
        Text(hex, style = Tipos.Pequeno.copy(color = tinta, fontSize = 10.sp, lineHeight = 13.sp))
    }
}

@Composable
private fun AmostraTexto(nome: String, hex: String, cor: Color, modifier: Modifier = Modifier) {
    Column(modifier.clip(Formas.Linha).background(Grafite.L3).padding(10.dp)) {
        Text("Aa", style = Tipos.Titulo.copy(color = cor, fontSize = 26.sp, lineHeight = 30.sp))
        Text(nome, style = Tipos.Pequeno.copy(color = cor, fontWeight = FontWeight.SemiBold))
        Text(hex, style = Tipos.Pequeno.copy(color = cor, fontSize = 10.sp))
    }
}
