package app.telacast.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.telacast.data.EstadoBusca
import app.telacast.tema.Formas
import app.telacast.tema.Grafite
import app.telacast.tema.Tipos

class AcoesDaBusca(
    val aoFechar: () -> Unit = {},
    val aoMudarConsulta: (String) -> Unit = {},
    val aoMudarEscopo: (noCanal: Boolean) -> Unit = {},
    val aoBuscar: () -> Unit = {},
    val aoMostrarMais: () -> Unit = {},
    val aoAbrirResultado: (canal: Long, mensagem: Long) -> Unit = { _, _ -> },
)

/**
 * Busca de mensagens (desktop: `search_panel`): campo, escopo (No servidor / Neste canal), filtros `de:` e `em:`,
 * contagem e resultados em cartões; "Mostrar mais" pede a próxima página.
 */
@Composable
fun TelaDeBusca(
    busca: EstadoBusca,
    consulta: String,
    noCanal: Boolean,
    emServidor: Boolean,
    local: String,
    amb: AmbienteMd,
    agoraMs: Long,
    zona: java.time.ZoneId,
    a: AcoesDaBusca,
) {
    TelaCheia("Resultados de busca", a.aoFechar) {
        Column(Modifier.padding(horizontal = 16.dp, vertical = 12.dp)) {
            Campo(
                consulta, a.aoMudarConsulta, if (emServidor) "Buscar em $local" else "Buscar nesta conversa",
                teclado = KeyboardOptions(imeAction = ImeAction.Search), aoEnviar = a.aoBuscar,
            )
            Spacer(Modifier.height(10.dp))
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                if (emServidor) {
                    for ((rotulo, sel) in listOf("No servidor" to !noCanal, "Neste canal" to noCanal)) {
                        Toque({ a.aoMudarEscopo(rotulo == "Neste canal") }, Modifier.height(34.dp), Formas.Selo, fundo = if (sel) Grafite.L4 else Grafite.L2, fundoPressionado = Grafite.L3) {
                            Box(Modifier.padding(horizontal = 14.dp).height(34.dp), contentAlignment = Alignment.Center) {
                                Text(rotulo, style = Tipos.Botao.copy(color = if (sel) Grafite.Texto else Grafite.Apagado, fontSize = 13.sp))
                            }
                        }
                    }
                }
                Spacer(Modifier.weight(1f))
                Botao("Buscar", TipoBotao.Primario, altura = 36.dp, aoClicar = a.aoBuscar)
            }
            Text("Filtros: de:nome  em:#canal  (de:eu para as suas)", style = Tipos.Pequeno.copy(fontSize = 11.5.sp, color = Grafite.Apagado), modifier = Modifier.padding(top = 10.dp))
        }
        Box(Modifier.fillMaxWidth().height(1.dp).background(Grafite.Linha))
        when {
            busca.carregando && busca.hits.isEmpty() -> Text("Buscando…", style = Tipos.CorpoApagado, modifier = Modifier.fillMaxWidth().padding(24.dp))
            busca.buscou && busca.hits.isEmpty() -> EstadoVazio("Buscamos por toda parte. Infelizmente, não encontramos nada.")
            !busca.buscou -> Text("Escreva e toque em Buscar.", style = Tipos.CorpoApagado, modifier = Modifier.fillMaxWidth().padding(24.dp))
            else -> Unit
        }
        if (busca.hits.isNotEmpty()) {
            Text(
                "${busca.hits.size}${if (busca.mais) "+" else ""} resultado${if (busca.hits.size == 1) "" else "s"}",
                style = Tipos.Pequeno.copy(fontSize = 12.5.sp, color = Grafite.Apagado), modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
            )
            LazyColumn {
                items(busca.hits, key = { it.message.id }) { h -> CartaoDeMensagem(h.message, "# ${h.channelName}", amb, zona, agoraMs) { a.aoAbrirResultado(h.message.channelId, h.message.id) } }
                if (busca.mais) item(key = "mais") {
                    Box(Modifier.fillMaxWidth().padding(vertical = 14.dp), contentAlignment = Alignment.Center) {
                        Botao(if (busca.carregando) "Carregando…" else "Mostrar mais", TipoBotao.Quieto, altura = 36.dp, habilitado = !busca.carregando, aoClicar = a.aoMostrarMais)
                    }
                }
            }
        }
    }
}
