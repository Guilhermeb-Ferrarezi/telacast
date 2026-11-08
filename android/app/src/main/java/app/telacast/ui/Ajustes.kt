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
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.telacast.data.Formatacao
import app.telacast.proto.SessionInfo
import app.telacast.proto.User
import app.telacast.tema.Formas
import app.telacast.tema.Grafite
import app.telacast.tema.Tipos
import app.telacast.voz.OpcoesDeVoz

class AcoesDeAjustes(
    val aoFechar: () -> Unit = {},
    val aoTrocarSenha: (atual: String, nova: String) -> Unit = { _, _ -> },
    val aoEncerrarSessao: (Long) -> Unit = {},
    val aoEncerrarOutras: () -> Unit = {},
    val aoEscolherTamanho: (Int) -> Unit = {},
    val aoSair: () -> Unit = {},
    val aoMudarVoz: ((OpcoesDeVoz) -> OpcoesDeVoz) -> Unit = {},
)

/** Multiplicador do tamanho do texto das mensagens: 0 pequeno, 1 normal, 2 grande. */
fun escalaDoTexto(nivel: Int): Float = when (nivel) { 0 -> 0.9f; 2 -> 1.15f; else -> 1f }

/**
 * Ajustes: conta (trocar senha, sessões abertas, sair) e aparência mínima (tamanho do texto). O tema é só o Grafite escuro.
 * `senhaTrocada` muda quando o servidor confirma a troca, e então os campos de senha são limpos.
 */
@Composable
fun TelaDeAjustes(eu: User, sessoes: List<SessionInfo>, agoraMs: Long, tamanho: Int, senhaTrocada: Int, a: AcoesDeAjustes, voz: OpcoesDeVoz = OpcoesDeVoz()) {
    TelaCheia("Ajustes", a.aoFechar) {
        Column(Modifier.verticalScroll(rememberScrollState()).padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Rotulo("Conta")
            Row(Modifier.fillMaxWidth().background(Grafite.L3, Formas.Cartao).padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                Avatar(eu.name, eu.id, 44.dp, eu.status.presenca(), Grafite.L3)
                Spacer(Modifier.width(12.dp))
                Column {
                    Text(eu.name, style = Tipos.Corpo.copy(fontWeight = FontWeight.SemiBold, fontSize = 16.sp))
                    Text("@${eu.name} · ${eu.status.rotulo()}", style = Tipos.Pequeno.copy(color = Grafite.Apagado))
                }
            }

            Spacer(Modifier.height(8.dp))
            Rotulo("Voz")
            SecaoDeVoz(voz, a.aoMudarVoz)

            Spacer(Modifier.height(8.dp))
            Rotulo("Trocar senha")
            key(senhaTrocada) { FormularioDeSenha(a.aoTrocarSenha) }

            Spacer(Modifier.height(8.dp))
            Rotulo("Sessões abertas")
            if (sessoes.isEmpty()) Text("Carregando sessões…", style = Tipos.CorpoApagado)
            for (s in sessoes) {
                Row(Modifier.fillMaxWidth().background(Grafite.L3, Formas.Linha).padding(12.dp), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(
                            if (s.device.isEmpty()) "Origem desconhecida" else if (s.current) "${s.device} · esta sessão" else s.device,
                            style = Tipos.Corpo.copy(fontSize = 14.5.sp),
                        )
                        Text("Último uso ${Formatacao.ha(agoraMs, s.lastUsed)} · aberta ${Formatacao.ha(agoraMs, s.created)}", style = Tipos.Pequeno.copy(fontSize = 12.sp, color = Grafite.Apagado))
                    }
                    if (!s.current) { Spacer(Modifier.width(8.dp)); Botao("Encerrar", TipoBotao.Quieto, altura = 34.dp, aoClicar = { a.aoEncerrarSessao(s.id) }) }
                }
            }
            if (sessoes.any { !it.current }) Botao("Encerrar todas as outras", TipoBotao.Quieto, Modifier.fillMaxWidth(), aoClicar = a.aoEncerrarOutras)

            Spacer(Modifier.height(8.dp))
            Rotulo("Aparência")
            Text("Tema: Grafite (só escuro).", style = Tipos.CorpoApagado.copy(fontSize = 13.sp))
            Row(Modifier.fillMaxWidth().background(Grafite.L3, Formas.Botao).padding(3.dp), horizontalArrangement = Arrangement.spacedBy(3.dp)) {
                for ((i, rotulo) in listOf("Pequeno", "Normal", "Grande").withIndex()) {
                    Toque({ a.aoEscolherTamanho(i) }, Modifier.weight(1f).height(40.dp), Formas.Linha, fundo = if (tamanho == i) Grafite.L4 else androidx.compose.ui.graphics.Color.Transparent, fundoPressionado = Grafite.L4) {
                        Box(Modifier.fillMaxWidth().height(40.dp), contentAlignment = Alignment.Center) {
                            Text(rotulo, style = Tipos.Botao.copy(color = if (tamanho == i) Grafite.Texto else Grafite.Apagado))
                        }
                    }
                }
            }
            Column(Modifier.fillMaxWidth().background(Grafite.L2, Formas.Linha).border(1.dp, Grafite.Linha, Formas.Linha).padding(12.dp)) {
                Text("Assim fica o texto das mensagens.", style = Tipos.Corpo.copy(fontSize = (15.5f * escalaDoTexto(tamanho)).sp, lineHeight = (22f * escalaDoTexto(tamanho)).sp, color = Grafite.Texto2))
            }

            Spacer(Modifier.height(16.dp))
            Botao("Sair da conta", TipoBotao.Perigo, Modifier.fillMaxWidth(), aoClicar = a.aoSair)
            Spacer(Modifier.height(24.dp))
        }
    }
}

@Composable
private fun FormularioDeSenha(aoTrocar: (String, String) -> Unit) {
    var atual by remember { mutableStateOf("") }
    var nova by remember { mutableStateOf("") }
    var repetir by remember { mutableStateOf("") }
    val naoConfere = repetir.isNotEmpty() && nova != repetir
    val pronto = atual.isNotEmpty() && nova.length in 8..128 && nova == repetir
    val teclado = KeyboardOptions(keyboardType = KeyboardType.Password)
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        Campo(atual, { atual = it }, "Senha atual", senha = true, teclado = teclado)
        Campo(nova, { nova = it }, "Nova senha (8 a 128 caracteres)", senha = true, teclado = teclado)
        Campo(repetir, { repetir = it }, "Repita a nova senha", senha = true, teclado = teclado)
        if (naoConfere) Text("As senhas não conferem.", style = Tipos.Corpo.copy(fontSize = 12.5.sp, color = Grafite.Vermelho))
        Botao("Trocar senha", TipoBotao.Primario, Modifier.fillMaxWidth(), habilitado = pronto, aoClicar = { aoTrocar(atual, nova) })
    }
}

/** Opções do chat de voz: efeitos do Android (ruído, eco, ganho automático), sensibilidade do microfone e ganho de entrada. */
@Composable
fun SecaoDeVoz(o: OpcoesDeVoz, aoMudar: ((OpcoesDeVoz) -> OpcoesDeVoz) -> Unit) {
    Column(Modifier.fillMaxWidth().background(Grafite.L3, Formas.Cartao).padding(vertical = 4.dp)) {
        Interruptor("Supressão de ruído", "Tira ruído de fundo da sua voz (RNNoise, o mesmo do desktop; sem ele, o efeito do Android).", o.supressaoDeRuido) { v -> aoMudar { it.copy(supressaoDeRuido = v) } }
        Interruptor("Cancelamento de eco", "Evita que a voz dos outros volte pelo seu microfone.", o.cancelamentoDeEco) { v -> aoMudar { it.copy(cancelamentoDeEco = v) } }
        Interruptor("Ganho automático", "Mantém o volume da sua voz parecido.", o.ganhoAutomatico) { v -> aoMudar { it.copy(ganhoAutomatico = v) } }
        DeslizanteDeVoz("Sensibilidade do microfone", "${(o.sensibilidade * 100).toInt()}%", o.sensibilidade, 0f..1f) { v -> aoMudar { it.copy(sensibilidade = v) } }
        DeslizanteDeVoz("Ganho de entrada", "${"%.1f".format(o.ganhoDeEntrada)}x", o.ganhoDeEntrada, 0.5f..4f) { v -> aoMudar { it.copy(ganhoDeEntrada = v) } }
    }
}

@Composable
private fun Interruptor(titulo: String, descricao: String, ligado: Boolean, aoMudar: (Boolean) -> Unit) {
    Row(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text(titulo, style = Tipos.Corpo.copy(fontSize = 14.5.sp))
            Text(descricao, style = Tipos.Pequeno.copy(fontSize = 12.sp, color = Grafite.Apagado))
        }
        Spacer(Modifier.width(12.dp))
        androidx.compose.material3.Switch(
            ligado, aoMudar,
            colors = androidx.compose.material3.SwitchDefaults.colors(
                checkedThumbColor = Grafite.L0, checkedTrackColor = Grafite.Ambar, checkedBorderColor = Grafite.Ambar,
                uncheckedThumbColor = Grafite.Texto2, uncheckedTrackColor = Grafite.L4, uncheckedBorderColor = Grafite.Linha2,
            ),
        )
    }
}

@Composable
private fun DeslizanteDeVoz(titulo: String, valor: String, atual: Float, faixa: ClosedFloatingPointRange<Float>, aoMudar: (Float) -> Unit) {
    Column(Modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 6.dp)) {
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.SpaceBetween) {
            Text(titulo, style = Tipos.Corpo.copy(fontSize = 14.5.sp))
            Text(valor, style = Tipos.Pequeno.copy(color = Grafite.Apagado))
        }
        androidx.compose.material3.Slider(
            atual, aoMudar, valueRange = faixa,
            colors = androidx.compose.material3.SliderDefaults.colors(thumbColor = Grafite.Ambar, activeTrackColor = Grafite.Ambar, inactiveTrackColor = Grafite.L4),
        )
    }
}
