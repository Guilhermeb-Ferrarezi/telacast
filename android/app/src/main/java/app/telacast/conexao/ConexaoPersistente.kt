package app.telacast.conexao

import app.telacast.data.AppState
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch

/** O serviço em primeiro plano "Telacast conectado" (no Android, `ServicoDeConexao`); nos testes, um falso. */
interface ServicoPersistente {
    fun iniciar()
    fun parar()
}

object ServicoPersistenteNulo : ServicoPersistente {
    override fun iniciar() = Unit
    override fun parar() = Unit
}

/**
 * "Manter conectado": enquanto a pessoa estiver logada E a opção estiver ligada, um serviço em primeiro plano mantém o processo (e o
 * WebSocket, com o ping do OkHttp e a reconexão com backoff que já existem) vivo com o app fechado ou a tela apagada, e as notificações
 * locais chegam sem Firebase. Sair da conta ou desligar a opção encerra o serviço. Há UMA conexão e UM processo: o serviço não abre
 * socket nenhum, só segura o processo; o da voz e o da tela convivem com ele.
 */
class GerenciadorDaConexaoPersistente(
    escopo: CoroutineScope,
    estadoDoApp: StateFlow<AppState>,
    opcao: StateFlow<Boolean>,
    private val servico: ServicoPersistente,
) {
    init {
        escopo.launch {
            combine(estadoDoApp.map { it.autenticado }.distinctUntilChanged(), opcao) { logada, ligada -> logada && ligada }
                .distinctUntilChanged().collect { querServico -> if (querServico) servico.iniciar() else servico.parar() }
        }
    }
}
