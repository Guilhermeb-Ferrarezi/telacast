package app.telacast.ui

import androidx.activity.compose.BackHandler
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import app.telacast.AppViewModel
import app.telacast.Fase
import app.telacast.estado.Dialogo
import app.telacast.estado.MenuAberto
import app.telacast.net.EstadoConexao

/** Liga o [AppViewModel] às telas sem estado. Também é onde mora o `BackHandler`. */
@Composable
fun RaizDoApp(vm: AppViewModel) {
    val fase by vm.fase.collectAsState()
    val estado by vm.estado.collectAsState()
    when (fase) {
        Fase.Login -> {
            val form by vm.form.collectAsState()
            TelaDeLogin(form, AcoesLogin(vm::editarForm, vm::entrar, vm::alternarModo))
        }
        Fase.Retomando -> TelaDeCarregamento(estado.conexao is EstadoConexao.Reconectando, vm::usarOutraConta)
        Fase.Principal -> {
            val nav by vm.nav.collectAsState()
            val recolhidas by vm.recolhidas.collectAsState()
            val aviso by vm.aviso.collectAsState()
            // Voltar do sistema fecha diálogo, menu, painel de membros e gaveta (nessa ordem) antes de sair do app
            BackHandler(enabled = nav.podeVoltar) { vm.voltar() }
            TelaPrincipal(estado, nav, recolhidas, aviso, acoesDe(vm))
        }
    }
}

private fun acoesDe(vm: AppViewModel) = AcoesPrincipais(
    painel = AcoesPainel(
        aoAbrirCanal = vm::abrirCanal,
        aoAlternarCategoria = vm::alternarCategoria,
        aoCliqueEmVoz = vm::cliqueEmVoz,
        aoAbrirMenuServidor = { vm.abrirMenu(MenuAberto.Servidor) },
        aoAbrirMenuStatus = { vm.abrirMenu(MenuAberto.Status) },
        aoCriarCanal = { sv, categoria, pai -> vm.abrirDialogo(Dialogo.CriarCanal(sv, categoria, pai)) },
        aoAbrirAmigos = vm::abrirAmigos,
        aoAbrirDm = vm::abrirDm,
    ),
    dialogo = AcoesDialogo(
        aoFechar = vm::fecharDialogo,
        aoCriarServidor = vm::criarServidor,
        aoEntrarNoServidor = vm::entrarNoServidor,
        aoTrocarModo = { criar -> vm.abrirDialogo(Dialogo.AdicionarServidor(criar)) },
        aoCriarCanal = vm::criarCanal,
        aoConfirmarSaida = vm::sair,
    ),
    amigos = AcoesAmigos(
        aoEscolherAba = vm::escolherAba,
        aoPedirAmizade = { vm.pedirAmizade(it); vm.mostrarAviso("Pedido de amizade enviado") },
        aoAceitar = vm::aceitarAmigo,
        aoRemover = vm::removerAmigo,
        aoConversar = vm::abrirDmCom,
    ),
    aoAbrirGaveta = vm::abrirGaveta,
    aoFecharGaveta = vm::fecharGaveta,
    aoAbrirMembros = vm::abrirMembros,
    aoFecharMembros = vm::fecharMembros,
    aoFecharMenu = vm::fecharMenu,
    aoAbrirInicio = { vm.selecionarServidor(null) },
    aoSelecionarServidor = vm::selecionarServidor,
    aoAdicionarServidor = { vm.abrirDialogo(Dialogo.AdicionarServidor(true)) },
    aoConvidar = { vm.abrirDialogo(Dialogo.Convidar(it)) },
    aoEscolherStatus = vm::definirStatus,
    aoPedirSaida = { vm.abrirDialogo(Dialogo.ConfirmarSair) },
)
