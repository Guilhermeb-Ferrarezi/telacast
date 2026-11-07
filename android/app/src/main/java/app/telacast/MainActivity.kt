package app.telacast

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.ui.graphics.toArgb
import androidx.lifecycle.ViewModel
import androidx.lifecycle.ViewModelProvider
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
        // só tema escuro: barras de sistema sempre escuras, ícones claros
        val escuro = SystemBarStyle.dark(Grafite.L2.toArgb())
        enableEdgeToEdge(statusBarStyle = escuro, navigationBarStyle = escuro)
        super.onCreate(savedInstanceState)
        setContent { GrafiteTheme { RaizDoApp(vm) } }
    }
}
