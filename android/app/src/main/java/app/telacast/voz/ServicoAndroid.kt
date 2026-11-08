package app.telacast.voz

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.content.ContextCompat
import app.telacast.MainActivity
import app.telacast.TelacastApp

private const val CANAL = "voz"
private const val ID_NOTIFICACAO = 7001
private const val ACAO_INICIAR = "app.telacast.voz.INICIAR"
private const val ACAO_SAIR = "app.telacast.voz.SAIR"

/** Cria o canal de notificação da voz (importância baixa: sem som nem vibração). Chamar uma vez ao abrir o app. */
fun criarCanalDeNotificacaoDaVoz(ctx: Context) {
    val nm = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
    nm.createNotificationChannel(NotificationChannel(CANAL, "Chamada de voz", NotificationManager.IMPORTANCE_LOW).apply {
        description = "Mostra que você está num canal de voz e deixa sair com um toque."
        setShowBadge(false)
    })
}

/** Liga e desliga o [ServicoAndroidDeVoz] (o controlador não conhece o Android). */
class FachadaDoServicoDeVoz(private val ctx: Context) : ServicoDeVoz {
    override fun iniciar(titulo: String, texto: String) {
        val i = Intent(ctx, ServicoAndroidDeVoz::class.java).setAction(ACAO_INICIAR).putExtra("titulo", titulo).putExtra("texto", texto)
        runCatching { ContextCompat.startForegroundService(ctx, i) }
    }

    override fun parar() { ctx.stopService(Intent(ctx, ServicoAndroidDeVoz::class.java)) }
}

/**
 * Serviço em primeiro plano do tipo `microphone`: sem ele o Android corta o microfone do app em segundo plano. A
 * notificação tem o botão "Sair da voz". O áudio em si não mora aqui: vive no [ControladorDeVoz] do contêiner.
 */
class ServicoAndroidDeVoz : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACAO_SAIR) {
            (application as TelacastApp).container.voz.sair()
            stopSelf()
            return START_NOT_STICKY
        }
        val n = notificacao(intent?.getStringExtra("titulo") ?: "Voz conectada", intent?.getStringExtra("texto") ?: "")
        try {
            if (Build.VERSION.SDK_INT >= 29) startForeground(ID_NOTIFICACAO, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE)
            else startForeground(ID_NOTIFICACAO, n)
        } catch (e: Exception) {
            // sem permissão do microfone ou do tipo de serviço: a voz continua enquanto o app estiver aberto
            stopSelf()
        }
        return START_NOT_STICKY
    }

    private fun notificacao(titulo: String, texto: String): Notification {
        val abrir = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP), PendingIntent.FLAG_IMMUTABLE)
        val sair = PendingIntent.getService(this, 1, Intent(this, ServicoAndroidDeVoz::class.java).setAction(ACAO_SAIR), PendingIntent.FLAG_IMMUTABLE)
        return Notification.Builder(this, CANAL)
            .setSmallIcon(android.R.drawable.stat_sys_phone_call).setContentTitle(titulo).setContentText(texto)
            .setOngoing(true).setCategory(Notification.CATEGORY_CALL).setContentIntent(abrir)
            .addAction(Notification.Action.Builder(null, "Sair da voz", sair).build()).build()
    }
}
