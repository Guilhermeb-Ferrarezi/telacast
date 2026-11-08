package app.telacast

import android.app.Activity
import android.app.Application
import android.os.Bundle
import app.telacast.armazenamento.CofreKeystore
import app.telacast.armazenamento.PreferenciasAndroid

class TelacastApp : Application() {
    lateinit var container: Container
        private set
    lateinit var crash: app.telacast.diagnostico.RelatorioDeCrash
        private set

    override fun onCreate() {
        super.onCreate()
        // relatório do último crash (ver diagnostico/Crash.kt): grava antes de o processo morrer
        crash = app.telacast.diagnostico.RelatorioDeCrash(java.io.File(filesDir, "ultimo-crash.txt"))
        crash.instalar("${BuildConfig.VERSION_NAME} (${BuildConfig.VERSION_CODE})", "${android.os.Build.MANUFACTURER} ${android.os.Build.MODEL} · Android ${android.os.Build.VERSION.RELEASE} (API ${android.os.Build.VERSION.SDK_INT})")
        app.telacast.voz.criarCanalDeNotificacaoDaVoz(this)
        app.telacast.push.criarCanaisDeNotificacao(this)
        app.telacast.tela.ServicoDeProjecao.criarCanal(this)
        app.telacast.atualizacao.criarCanalDeAtualizacao(this)
        app.telacast.conexao.criarCanalDaConexao(this)
        container = Container(
            preferencias = PreferenciasAndroid(this),
            cofre = CofreKeystore(this),
            servidorPadrao = BuildConfig.SERVIDOR_PADRAO,
            decodificarImagem = { app.telacast.arquivos.decodificarBitmap(it) },
            fabricaDeDispositivo = { motor, opcoes, rota, aoFalhar -> app.telacast.voz.DispositivoAndroid(this, motor, opcoes, rota, aoFalhar) },
            servicoDeVoz = app.telacast.voz.FachadaDoServicoDeVoz(this),
            notificador = app.telacast.push.NotificadorAndroid(this),
            provedorDeToken = app.telacast.push.ProvedorFirebase(this),
            fabricaDeSupressor = app.telacast.voz.SupressorRnnoise,
            servicoDeConexao = app.telacast.conexao.FachadaDoServicoDeConexao(this), relatorioDeCrash = crash, chaveDeAtualizacao = BuildConfig.CHAVE_DE_ATUALIZACAO, versaoDoApp = BuildConfig.VERSION_NAME,
            instaladorDeApk = app.telacast.atualizacao.InstaladorAndroid(this), pastaDeAtualizacao = java.io.File(cacheDir, "atualizacao"),
            fabricaDeFonte = { app.telacast.tela.FonteAndroid(this) },
            fabricaDeDecodificador = { app.telacast.tela.DecodificadorAndroid() },
            rotaInicialDaVoz = if (app.telacast.voz.temFoneConectado(this)) app.telacast.voz.RotaDeAudio.Fone else app.telacast.voz.RotaDeAudio.AltoFalante,
        )
        if (container.atualizacao.ligada) app.telacast.atualizacao.agendarChecagemDiaria(this)
        // SetFocus: o app está "na frente" enquanto alguma activity está iniciada (rotação não conta)
        registerActivityLifecycleCallbacks(object : ActivityLifecycleCallbacks {
            override fun onActivityStarted(activity: Activity) = container.foco.atividadeIniciou()
            override fun onActivityStopped(activity: Activity) = container.foco.atividadeParou(activity.isChangingConfigurations)
            override fun onActivityCreated(activity: Activity, savedInstanceState: Bundle?) = Unit
            override fun onActivityResumed(activity: Activity) = Unit
            override fun onActivityPaused(activity: Activity) = Unit
            override fun onActivitySaveInstanceState(activity: Activity, outState: Bundle) = Unit
            override fun onActivityDestroyed(activity: Activity) = Unit
        })
    }
}
