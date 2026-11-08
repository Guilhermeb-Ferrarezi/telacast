package app.telacast

import android.app.Application
import app.telacast.armazenamento.CofreKeystore
import app.telacast.armazenamento.PreferenciasAndroid

class TelacastApp : Application() {
    lateinit var container: Container
        private set

    override fun onCreate() {
        super.onCreate()
        app.telacast.voz.criarCanalDeNotificacaoDaVoz(this)
        container = Container(
            preferencias = PreferenciasAndroid(this),
            cofre = CofreKeystore(this),
            servidorPadrao = BuildConfig.SERVIDOR_PADRAO,
            decodificarImagem = { app.telacast.arquivos.decodificarBitmap(it) },
            fabricaDeDispositivo = { motor, opcoes, rota, aoFalhar -> app.telacast.voz.DispositivoAndroid(this, motor, opcoes, rota, aoFalhar) },
            servicoDeVoz = app.telacast.voz.FachadaDoServicoDeVoz(this),
            rotaInicialDaVoz = if (app.telacast.voz.temFoneConectado(this)) app.telacast.voz.RotaDeAudio.Fone else app.telacast.voz.RotaDeAudio.AltoFalante,
        )
    }
}
