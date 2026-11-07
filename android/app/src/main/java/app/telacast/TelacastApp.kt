package app.telacast

import android.app.Application
import app.telacast.armazenamento.CofreKeystore
import app.telacast.armazenamento.PreferenciasAndroid

class TelacastApp : Application() {
    lateinit var container: Container
        private set

    override fun onCreate() {
        super.onCreate()
        container = Container(
            preferencias = PreferenciasAndroid(this),
            cofre = CofreKeystore(this),
            servidorPadrao = BuildConfig.SERVIDOR_PADRAO,
        )
    }
}
