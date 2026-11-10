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

class AcoesDeChamada(val aoAtender: (Long) -> Unit = {}, val aoRecusar: (Long) -> Unit = {})

/** Faixa "fulano está ligando…" com Atender e Recusar (a notificação faz o mesmo com o app fechado). */
@Composable
fun FaixaDeChamada(e: app.telacast.chamada.EstadoDaChamada, acoes: AcoesDeChamada) {
    val r = e as? app.telacast.chamada.EstadoDaChamada.Recebendo ?: return
    Row(Modifier.fillMaxWidth().background(Grafite.L4).padding(start = 14.dp, end = 8.dp, top = 8.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Avatar(r.nome, r.usuario, 36.dp, fundo = Grafite.L4)
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f)) {
            Text(r.nome, style = Tipos.Corpo.copy(fontSize = 14.5.sp, fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis)
            Text("Está ligando para você…", style = Tipos.Pequeno.copy(color = Grafite.Ambar))
        }
        Botao("Recusar", TipoBotao.Perigo, altura = 36.dp, aoClicar = { acoes.aoRecusar(r.canal) })
        Spacer(Modifier.width(6.dp))
        Botao("Atender", TipoBotao.Primario, altura = 36.dp, aoClicar = { acoes.aoAtender(r.canal) })
    }
}

/**
 * O servidor recusou este app (versão do protocolo): NÃO desloga (o token fica guardado). Mostra "Atualize o app" com o botão. Se a atualização pelo app
 * não está disponível (sem chave, sem rede), oferece baixar o APK pelo navegador.
 */
@Composable
fun TelaDesatualizada(e: EstadoDaAtualizacao, atualizacaoLigada: Boolean, acoes: AcoesDeAtualizacao, aoBaixarPeloNavegador: () -> Unit, aoTentarDeNovo: () -> Unit) {
    Box(Modifier.fillMaxSize().background(Grafite.L2), contentAlignment = Alignment.Center) {
        Column(Modifier.padding(32.dp), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text("Telacast", style = Tipos.Titulo.copy(fontSize = 44.sp, lineHeight = 48.sp, letterSpacing = (-0.5).sp))
            Text("Atualize o app", style = Tipos.Subtitulo.copy(fontSize = 24.sp))
            Text("Esta versão do Telacast é antiga demais para falar com o servidor. Sua conta continua salva: depois de atualizar, você volta direto para as conversas.", style = Tipos.CorpoApagado, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
            Spacer(Modifier.height(8.dp))
            val m = e.manifesto
            when {
                m != null && e.fase == FaseDaAtualizacao.Baixando -> Text("Baixando a versão ${m.version} · ${e.progresso}%", style = Tipos.Corpo.copy(color = Grafite.Ambar))
                m != null && e.fase == FaseDaAtualizacao.Instalando -> Text("Instalando a versão ${m.version}…", style = Tipos.Corpo.copy(color = Grafite.Ambar))
                m != null -> Botao(if (e.fase == FaseDaAtualizacao.Pronta) "Instalar a versão ${m.version}" else "Atualizar para a ${m.version}", TipoBotao.Primario, Modifier.fillMaxWidth(), 48.dp, aoClicar = if (e.fase == FaseDaAtualizacao.Pronta) acoes.aoInstalar else acoes.aoAtualizar)
                else -> Text(if (atualizacaoLigada) "Procurando a versão nova…" else "A atualização pelo app não está disponível.", style = Tipos.CorpoApagado)
            }
            if (e.fase == FaseDaAtualizacao.Erro && e.mensagem != null) Text(e.mensagem, style = Tipos.Corpo.copy(fontSize = 13.sp, color = Grafite.Vermelho), textAlign = androidx.compose.ui.text.style.TextAlign.Center)
            Botao("Baixar pelo navegador", TipoBotao.Quieto, Modifier.fillMaxWidth(), 44.dp, aoClicar = aoBaixarPeloNavegador)
            Botao("Tentar de novo", TipoBotao.Quieto, Modifier.fillMaxWidth(), 44.dp, aoClicar = aoTentarDeNovo)
        }
    }
}
