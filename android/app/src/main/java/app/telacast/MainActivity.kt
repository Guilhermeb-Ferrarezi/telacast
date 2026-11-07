package app.telacast

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.ui.graphics.toArgb
import app.telacast.tema.Grafite
import app.telacast.ui.PaletaScreen

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        // só tema escuro: barras de sistema sempre escuras, ícones claros
        val escuro = SystemBarStyle.dark(Grafite.L2.toArgb())
        enableEdgeToEdge(statusBarStyle = escuro, navigationBarStyle = escuro)
        super.onCreate(savedInstanceState)
        setContent { PaletaScreen() }
    }
}
