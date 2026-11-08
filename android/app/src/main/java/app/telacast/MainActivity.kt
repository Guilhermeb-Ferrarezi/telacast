package app.telacast

import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import androidx.compose.ui.graphics.toArgb
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
import app.telacast.push.destinoDoIntent
import app.telacast.tema.Grafite
import app.telacast.tema.GrafiteTheme
import app.telacast.ui.RaizDoApp

class MainActivity : ComponentActivity() {
    private val vm: AppViewModel by viewModels {
        object : ViewModelProvider.Factory {
            @Suppress("UNCHECKED_CAST")
            override fun <T : ViewModel> create(modelClass: Class<T>): T =
                AppViewModel((application as TelacastApp).container) as T
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        installSplashScreen() // troca o tema de splash pelo Theme.Telacast
        // só tema escuro: barras de sistema sempre escuras, ícones claros
        val escuro = SystemBarStyle.dark(Grafite.L2.toArgb())
        enableEdgeToEdge(statusBarStyle = escuro, navigationBarStyle = escuro)
        super.onCreate(savedInstanceState)
        setContent { GrafiteTheme { RaizDoApp(vm) } }
        // toque numa notificação de push com o app fechado; depois de rotação/restauração o Intent antigo não vale de novo
        if (savedInstanceState == null) tratarToque(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        tratarToque(intent)
    }

    override fun onStart() {
        super.onStart()
        vm.aoVoltarParaFrente()
    }

    private fun tratarToque(intent: Intent?) {
        if (intent?.action == app.telacast.atualizacao.ACAO_INSTALAR_ATUALIZACAO) { vm.instalarAtualizacao(); return }
        destinoDoIntent(intent)?.let { vm.abrirPorNotificacao(it) }
    }
}
