package app.telacast.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.telacast.atualizacao.EstadoDaAtualizacao
import app.telacast.atualizacao.FaseDaAtualizacao
import app.telacast.tema.Formas
import app.telacast.tema.Grafite
import app.telacast.tema.Tipos

class AcoesDeAtualizacao(
    val aoAtualizar: () -> Unit = {},
    val aoInstalar: () -> Unit = {},
    val aoDispensarFolha: () -> Unit = {},
    val aoAbrirPermissao: () -> Unit = {},
    val aoCancelarPermissao: () -> Unit = {},
)

/** Faixa fixa no topo enquanto houver versão nova (some só depois de atualizar): "Nova versão X disponível · Atualizar". */
@Composable
fun FaixaDeAtualizacao(e: EstadoDaAtualizacao, acoes: AcoesDeAtualizacao) {
    val m = e.manifesto ?: return
    if (e.fase == FaseDaAtualizacao.Nenhuma) return
    val texto = when (e.fase) {
        FaseDaAtualizacao.Disponivel -> "Nova versão ${m.version} disponível"
        FaseDaAtualizacao.Baixando -> "Baixando a versão ${m.version} · ${e.progresso}%"
        FaseDaAtualizacao.Pronta -> "A versão ${m.version} está pronta"
        FaseDaAtualizacao.PrecisaDePermissao -> "Falta uma permissão para instalar a versão ${m.version}"
        FaseDaAtualizacao.Instalando -> "Instalando a versão ${m.version}…"
        else -> e.mensagem ?: "Não deu para atualizar."
    }
    val (rotulo, acao) = when (e.fase) {
        FaseDaAtualizacao.Disponivel -> "Atualizar" to acoes.aoAtualizar
        FaseDaAtualizacao.Pronta -> "Instalar" to acoes.aoInstalar
        FaseDaAtualizacao.PrecisaDePermissao -> "Continuar" to acoes.aoInstalar
        FaseDaAtualizacao.Erro -> "Tentar de novo" to acoes.aoAtualizar
        else -> null to {}
    }
    Row(Modifier.fillMaxWidth().background(Grafite.L3).padding(start = 14.dp, end = 8.dp, top = 6.dp, bottom = 6.dp), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(texto, style = Tipos.Corpo.copy(fontSize = 13.sp, color = if (e.fase == FaseDaAtualizacao.Erro) Grafite.Vermelho else Grafite.Ambar, fontWeight = FontWeight.SemiBold), maxLines = 2, overflow = TextOverflow.Ellipsis)
            if (e.fase == FaseDaAtualizacao.Baixando) {
                Box(Modifier.padding(top = 4.dp).fillMaxWidth().height(3.dp).background(Grafite.L4, Formas.Selo)) { Box(Modifier.fillMaxWidth(e.progresso / 100f).height(3.dp).background(Grafite.Ambar, Formas.Selo)) }
            }
        }
        if (rotulo != null) { Spacer(Modifier.width(8.dp)); Botao(rotulo, TipoBotao.Primario, altura = 34.dp, aoClicar = acao) }
    }
}

/** Primeira vez que a versão é detectada: "O que mudou" com Atualizar e Depois (Depois esconde só a folha; a faixa continua). */
@Composable
fun FolhaDeAtualizacao(e: EstadoDaAtualizacao, acoes: AcoesDeAtualizacao) {
    val m = e.manifesto ?: return
    FolhaBase(acoes.aoDispensarFolha) {
        Column(Modifier.padding(horizontal = 20.dp, vertical = 4.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Nova versão ${m.version}", style = Tipos.Subtitulo.copy(fontSize = 22.sp))
            Text("O QUE MUDOU", style = Tipos.Rotulo)
            Column(Modifier.fillMaxWidth().height(140.dp).background(Grafite.L3, Formas.Botao).border(1.dp, Grafite.Linha2, Formas.Botao).padding(12.dp).verticalScroll(rememberScrollState())) {
                Text(m.notes.ifBlank { "Melhorias e correções." }, style = Tipos.Corpo.copy(fontSize = 14.sp, color = Grafite.Texto2))
            }
            Text("${"%.1f".format(m.size / 1_048_576.0).replace('.', ',')} MB · a atualização é conferida (hash e assinatura) antes de instalar.", style = Tipos.Pequeno.copy(color = Grafite.Apagado))
            Spacer(Modifier.height(4.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                Botao("Depois", TipoBotao.Quieto, Modifier.weight(1f), 44.dp, aoClicar = acoes.aoDispensarFolha)
                Botao("Atualizar", TipoBotao.Primario, Modifier.weight(1f), 44.dp, aoClicar = acoes.aoAtualizar)
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}

/** O Android só deixa instalar atualização se a pessoa liberar "instalar apps desconhecidos" para o Telacast: explica o caminho. */
@Composable
fun FolhaDeFonteDesconhecida(acoes: AcoesDeAtualizacao) {
    FolhaBase(acoes.aoCancelarPermissao) {
        Column(Modifier.padding(horizontal = 20.dp, vertical = 4.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text("Permitir a instalação", style = Tipos.Subtitulo.copy(fontSize = 22.sp))
            Text("Como o Telacast não vem da loja de aplicativos, o Android pede uma autorização única para ele poder instalar as próprias atualizações.", style = Tipos.CorpoApagado)
            Text("1. Toque em “Abrir configurações”.\n2. Ative “Permitir desta fonte”.\n3. Volte ao Telacast: a atualização continua sozinha.", style = Tipos.Corpo.copy(fontSize = 14.sp, color = Grafite.Texto2))
            Spacer(Modifier.height(4.dp))
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxWidth()) {
                Botao("Agora não", TipoBotao.Quieto, Modifier.weight(1f), 44.dp, aoClicar = acoes.aoCancelarPermissao)
                Botao("Abrir configurações", TipoBotao.Primario, Modifier.weight(1f), 44.dp, aoClicar = acoes.aoAbrirPermissao)
            }
            Spacer(Modifier.height(8.dp))
        }
    }
}

/** "O app fechou inesperadamente": o relatório do último crash (sem dados pessoais), para copiar ou compartilhar. */
@Composable
fun JanelaDeCrash(relatorio: String, aoCopiar: () -> Unit, aoCompartilhar: () -> Unit, aoFechar: () -> Unit) {
    Janela("O app fechou inesperadamente", aoFechar) {
        Text("Sentimos muito. Se quiser ajudar a corrigir, envie o relatório abaixo para quem cuida do servidor. Ele não tem mensagens, senhas nem dados pessoais.", style = Tipos.CorpoApagado)
        Spacer(Modifier.height(10.dp))
        Text(
            relatorio.lines().take(8).joinToString("\n"), style = Tipos.Pequeno.copy(fontSize = 11.sp, color = Grafite.Texto2, fontFamily = app.telacast.tema.Mono), maxLines = 8, overflow = TextOverflow.Ellipsis,
            modifier = Modifier.fillMaxWidth().background(Grafite.L2, Formas.Botao).border(1.dp, Grafite.Linha2, Formas.Botao).padding(10.dp),
        )
        Spacer(Modifier.height(14.dp))
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Botao("Copiar", TipoBotao.Quieto, Modifier.weight(1f), aoClicar = aoCopiar)
            Botao("Compartilhar", TipoBotao.Quieto, Modifier.weight(1f), aoClicar = aoCompartilhar)
        }
        Spacer(Modifier.height(8.dp))
        Botao("Fechar", TipoBotao.Primario, Modifier.fillMaxWidth(), aoClicar = aoFechar)
    }
}
