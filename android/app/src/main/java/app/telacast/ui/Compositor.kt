package app.telacast.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.painter.BitmapPainter
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.telacast.arquivos.AnexoEmEnvio
import app.telacast.arquivos.EstadoDoEnvio
import app.telacast.arquivos.FonteDeArquivo
import app.telacast.data.EstadoCompositor
import app.telacast.data.Formatacao
import app.telacast.data.Sugestao
import app.telacast.data.SugestoesDeMencao
import app.telacast.proto.Status
import app.telacast.tema.Formas
import app.telacast.tema.Grafite
import app.telacast.tema.Tipos

class AcoesDoCompositor(
    val aoMudar: (TextFieldValue) -> Unit = {},
    val aoEnviar: () -> Unit = {},
    val aoAnexar: () -> Unit = {},
    val aoCancelarResposta: () -> Unit = {},
    val aoCancelarEdicao: () -> Unit = {},
    val aoRemoverAnexo: (Long) -> Unit = {},
    val aoTentarAnexo: (Long) -> Unit = {},
    val aoEscolherSugestao: (SugestoesDeMencao, String) -> Unit = { _, _ -> },
)

/**
 * Compositor: linha "fulano está digitando…", autocompletar de `@`, faixas "Respondendo a" e "Editando", e a caixa
 * (camada 3, raio 12, filete âmbar com foco) com bandeja de anexos, "+", campo multilinha (até 6 linhas) e Enviar.
 */
@Composable
fun Compositor(
    estado: EstadoCompositor,
    dica: String,
    anexos: List<AnexoEmEnvio>,
    sugestoes: SugestoesDeMencao?,
    digitando: String?,
    acoes: AcoesDoCompositor,
    modifier: Modifier = Modifier,
    comFoco: Boolean = false,
) {
    val editando = estado.editando != null
    val enviando = anexos.any { it.estado is EstadoDoEnvio.NaFila || it.estado is EstadoDoEnvio.Enviando }
    val temProntos = anexos.any { it.estado is EstadoDoEnvio.Pronto }
    val podeEnviar = !enviando && (estado.valor.text.isNotBlank() || (temProntos && !editando))
    var foco by remember { mutableStateOf(comFoco) }

    Column(modifier.fillMaxWidth()) {
        // a linha reserva a altura mesmo vazia, para a lista não pular quando alguém começa a digitar
        Box(Modifier.fillMaxWidth().height(22.dp).padding(horizontal = 16.dp), contentAlignment = Alignment.CenterStart) {
            if (digitando != null) Text(digitando, style = Tipos.Pequeno.copy(fontSize = 12.5.sp, color = Grafite.Texto2), maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
        if (sugestoes != null) PopupDeMencoes(sugestoes, acoes)
        if (estado.respondendo != null) {
            FaixaDoCompositor(Modifier.padding(horizontal = 12.dp).padding(bottom = 6.dp), "Respondendo a", estado.respondendo.author, "Cancelar", false, acoes.aoCancelarResposta)
        }
        if (editando) {
            FaixaDoCompositor(Modifier.padding(horizontal = 12.dp).padding(bottom = 6.dp), "Editando mensagem", null, "Cancelar", true, acoes.aoCancelarEdicao)
        }
        Column(
            Modifier.padding(start = 12.dp, end = 12.dp, bottom = 10.dp).fillMaxWidth().background(Grafite.L3, Formas.Cartao)
                .border(1.dp, if (foco || editando) Grafite.Ambar else Grafite.Linha2, Formas.Cartao),
        ) {
            if (anexos.isNotEmpty()) {
                BandejaDeAnexos(anexos, acoes)
                Box(Modifier.fillMaxWidth().height(1.dp).background(Grafite.Linha2))
            }
            Row(Modifier.padding(start = 6.dp, end = 6.dp, top = 6.dp, bottom = 6.dp), verticalAlignment = Alignment.Bottom) {
                if (!editando) {
                    Toque(acoes.aoAnexar, Modifier.size(40.dp), CircleShape, fundo = Grafite.L2, fundoPressionado = Grafite.L4) {
                        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { IconeMais(Grafite.Texto2, tamanho = 20.dp) }
                    }
                }
                BasicTextField(
                    value = estado.valor, onValueChange = acoes.aoMudar,
                    textStyle = Tipos.Corpo, cursorBrush = SolidColor(Grafite.Ambar), minLines = 1, maxLines = 6,
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                    modifier = Modifier.weight(1f).onFocusChanged { foco = it.isFocused },
                    decorationBox = { interno ->
                        Box(Modifier.padding(horizontal = 10.dp).heightIn(min = 40.dp), contentAlignment = Alignment.CenterStart) {
                            if (estado.valor.text.isEmpty()) Text(dica, style = Tipos.CorpoApagado, maxLines = 1, overflow = TextOverflow.Ellipsis)
                            Box(Modifier.padding(vertical = 8.dp)) { interno() }
                        }
                    },
                )
                BotaoEnviar(podeEnviar, editando, acoes.aoEnviar)
            }
        }
    }
}

@Composable
private fun BotaoEnviar(ativo: Boolean, editando: Boolean, aoClicar: () -> Unit) {
    if (editando) {
        Botao("Salvar", TipoBotao.Primario, Modifier.height(40.dp), 40.dp, ativo, aoClicar)
        return
    }
    Toque({ if (ativo) aoClicar() }, Modifier.size(40.dp), CircleShape, fundo = if (ativo) Grafite.Ambar else Grafite.L2, fundoPressionado = if (ativo) Grafite.AmbarQuente else Grafite.L2) {
        Box(Modifier.fillMaxSize().drawBehind {
            // seta para cima
            val u = density
            val c = Offset(size.width / 2, size.height / 2)
            val cor = if (ativo) Grafite.AmbarTinta else Grafite.Apagado
            drawLine(cor, c + Offset(0f, 7f) * u, c + Offset(0f, -6f) * u, 2f * u, StrokeCap.Round)
            drawLine(cor, c + Offset(-5.5f, -1.5f) * u, c + Offset(0f, -7f) * u, 2f * u, StrokeCap.Round)
            drawLine(cor, c + Offset(5.5f, -1.5f) * u, c + Offset(0f, -7f) * u, 2f * u, StrokeCap.Round)
        })
    }
}

@Composable
private fun FaixaDoCompositor(modifier: Modifier, rotulo: String, nome: String?, cancelar: String, destaque: Boolean, aoCancelar: () -> Unit) {
    Row(modifier.fillMaxWidth().background(Grafite.L2, Formas.Menu).border(1.dp, Grafite.Linha, Formas.Menu).padding(start = 14.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(rotulo, style = Tipos.Corpo.copy(fontSize = 13.sp, color = if (destaque) Grafite.Ambar else Grafite.Apagado))
        if (nome != null) {
            Spacer(Modifier.width(6.dp))
            Text(nome, style = Tipos.Corpo.copy(fontSize = 13.sp, fontWeight = FontWeight.SemiBold), maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
        } else Spacer(Modifier.weight(1f))
        Toque(aoCancelar, Modifier.height(40.dp), Formas.Linha, fundoPressionado = Grafite.L3) {
            Box(Modifier.height(40.dp).padding(horizontal = 12.dp), contentAlignment = Alignment.Center) {
                Text(cancelar, style = Tipos.Botao.copy(color = Grafite.Texto2, fontSize = 13.sp))
            }
        }
    }
}

/** Autocompletar de `@`: membros (com presença) e `@everyone`/`@here`; tocar completa. */
@Composable
private fun PopupDeMencoes(s: SugestoesDeMencao, acoes: AcoesDoCompositor) {
    Column(Modifier.padding(horizontal = 12.dp).padding(bottom = 6.dp).fillMaxWidth().background(Grafite.L3, Formas.Menu).border(1.dp, Grafite.Linha2, Formas.Menu).padding(8.dp)) {
        Text(
            if (s.consulta.isEmpty()) "Membros" else "Resultados de membros para @${s.consulta}",
            style = Tipos.Corpo.copy(fontSize = 12.5.sp, color = Grafite.Texto2), modifier = Modifier.padding(start = 6.dp, bottom = 4.dp),
        )
        for (item in s.itens) LinhaDeSugestao(item) { acoes.aoEscolherSugestao(s, item.nome) }
    }
}

@Composable
private fun LinhaDeSugestao(s: Sugestao, aoClicar: () -> Unit) {
    Toque(aoClicar, Modifier.fillMaxWidth().height(44.dp), Formas.Linha, fundoPressionado = Grafite.L4) {
        Row(Modifier.fillMaxSize().padding(horizontal = 6.dp), verticalAlignment = Alignment.CenterVertically) {
            val u = s.usuario
            if (u != null) Avatar(u.name, u.id, 28.dp, u.status.presenca(), Grafite.L3)
            else Box(Modifier.size(28.dp).background(Grafite.Ambar.copy(alpha = 0.2f), CircleShape), contentAlignment = Alignment.Center) {
                Text("@", style = Tipos.Corpo.copy(fontSize = 14.sp, color = Grafite.Ambar))
            }
            Spacer(Modifier.width(10.dp))
            Text(s.nome, style = Tipos.Corpo.copy(fontWeight = FontWeight.Medium), maxLines = 1)
            if (s.descricao != null) {
                Spacer(Modifier.width(8.dp))
                Text(s.descricao, style = Tipos.Pequeno.copy(color = Grafite.Apagado), maxLines = 1, overflow = TextOverflow.Ellipsis)
            } else if (u != null) {
                Spacer(Modifier.width(8.dp))
                Text(u.status.rotulo(), style = Tipos.Pequeno.copy(color = Grafite.Apagado))
            }
        }
    }
}

/** Bandeja de anexos pendentes: miniaturas de 120 x 120 dp com progresso, falha e botão de remover. */
@Composable
private fun BandejaDeAnexos(anexos: List<AnexoEmEnvio>, acoes: AcoesDoCompositor) {
    Row(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 10.dp, vertical = 10.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        for (a in anexos) MiniaturaDeAnexo(a, acoes)
    }
}

@Composable
private fun MiniaturaDeAnexo(a: AnexoEmEnvio, acoes: AcoesDoCompositor) {
    val prov = LocalProvedorDeAnexos.current
    var bmp by remember(a.chave) { mutableStateOf(prov.miniaturaEmCache(a.fonte)) }
    LaunchedEffect(a.chave) { if (bmp == null && a.fonte.ehImagem) bmp = prov.miniatura(a.fonte) }
    val forma = RoundedCornerShape(10.dp)
    Box(Modifier.size(120.dp).clip(forma).background(Grafite.L2).border(1.dp, Grafite.Linha2, forma)) {
        val b = bmp
        if (b != null) {
            Image(BitmapPainter(b), a.fonte.nome, Modifier.fillMaxSize(), contentScale = ContentScale.Crop)
        } else {
            Column(Modifier.fillMaxSize().padding(10.dp), verticalArrangement = Arrangement.Bottom) {
                Text(a.fonte.nome, style = Tipos.Corpo.copy(fontSize = 12.5.sp, color = Grafite.Texto), maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(Formatacao.tamanho(a.fonte.tamanho), style = Tipos.Pequeno.copy(fontSize = 11.sp, color = Grafite.Apagado))
            }
        }
        when (val e = a.estado) {
            is EstadoDoEnvio.Enviando, EstadoDoEnvio.NaFila -> {
                Box(Modifier.align(Alignment.BottomStart).fillMaxWidth().height(4.dp).background(Grafite.L4)) {
                    Box(Modifier.fillMaxWidth(a.progresso.coerceIn(0f, 1f)).height(4.dp).background(Grafite.Ambar))
                }
            }
            is EstadoDoEnvio.Falhou -> Toque({ acoes.aoTentarAnexo(a.chave) }, Modifier.fillMaxSize(), fundo = Grafite.L0.copy(alpha = 0.94f), fundoPressionado = Grafite.L0) {
                Column(Modifier.fillMaxSize().padding(8.dp), verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
                    Text("Falhou", style = Tipos.Corpo.copy(fontSize = 12.5.sp, color = Grafite.Vermelho, fontWeight = FontWeight.SemiBold))
                    Text(e.motivo, style = Tipos.Pequeno.copy(fontSize = 10.5.sp, lineHeight = 13.sp, color = Grafite.Texto2), maxLines = 3, overflow = TextOverflow.Ellipsis)
                    if (a.fonte.tamanho <= app.telacast.arquivos.Arquivos.MAX_UPLOAD) {
                        Text("Tentar de novo", style = Tipos.Corpo.copy(fontSize = 11.5.sp, color = Grafite.Ambar, fontWeight = FontWeight.SemiBold), modifier = Modifier.padding(top = 4.dp))
                    }
                }
            }
            is EstadoDoEnvio.Pronto -> Unit
        }
        Toque({ acoes.aoRemoverAnexo(a.chave) }, Modifier.align(Alignment.TopEnd).padding(4.dp).size(28.dp), CircleShape, fundo = Color.Black.copy(alpha = 0.6f), fundoPressionado = Grafite.L4) {
            Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { IconeFechar(Grafite.Texto, tamanho = 16.dp) }
        }
    }
}
