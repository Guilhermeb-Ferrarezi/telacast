package app.telacast.conexao

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.content.ContextCompat
import app.telacast.MainActivity
import app.telacast.R
import app.telacast.TelacastApp

private const val CANAL = "conectado"
private const val ID = 7004

fun criarCanalDaConexao(ctx: Context) {
    (ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager)
        .createNotificationChannel(NotificationChannel(CANAL, "Conectado", NotificationManager.IMPORTANCE_MIN).apply {
            description = "Aviso fixo e discreto de que o Telacast está conectado em segundo plano para te avisar das mensagens. Dá para esconder este canal sem perder as notificações."
            setShowBadge(false); setSound(null, null); enableVibration(false)
        })
}

class FachadaDoServicoDeConexao(private val ctx: Context) : ServicoPersistente {
    override fun iniciar() { runCatching { ContextCompat.startForegroundService(ctx, Intent(ctx, ServicoDeConexao::class.java)) }.onFailure { Log.w("TelacastConexao", "não consegui subir o serviço: ${it.message}") } }
    override fun parar() { ctx.stopService(Intent(ctx, ServicoDeConexao::class.java)) }
}

/**
 * Serviço em primeiro plano MÍNIMO: só tem a notificação silenciosa "Telacast conectado" (prioridade mínima) e mantém o processo vivo; a conexão é a
 * do `Gateway` do contêiner (a mesma do app aberto). Tipo `specialUse`: o `dataSync` tem teto de 6 horas por dia a partir do Android 15 para apps
 * que miram o 15, e `specialUse` não tem (exige só a declaração na loja, que não se aplica a um APK instalado direto). `START_STICKY`: se o
 * Android matar o processo, ele volta, retoma a sessão salva e reconecta sozinho.
 */
class ServicoDeConexao : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val abrir = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP), PendingIntent.FLAG_IMMUTABLE)
        val n = Notification.Builder(this, CANAL).setSmallIcon(R.drawable.ic_notificacao).setColor(0xFFE9B35E.toInt()).setContentTitle("Telacast conectado")
            .setOngoing(true).setShowWhen(false).setCategory(Notification.CATEGORY_SERVICE).setContentIntent(abrir).build()
        try {
            if (Build.VERSION.SDK_INT >= 34) startForeground(ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_SPECIAL_USE)
            else startForeground(ID, n)
        } catch (e: Exception) {
            Log.w("TelacastConexao", "serviço em primeiro plano recusado: ${e.message}")
            stopSelf(); return START_NOT_STICKY
        }
        // reiniciado pelo sistema sem o app aberto: retoma a sessão salva
        (application as TelacastApp).container.retomarSessaoSalva()
        return START_STICKY
    }
}

/** Depois de ligar o aparelho: se a pessoa estava logada e deixou "Manter conectado" ligado, o serviço sobe e a conexão volta sem abrir o app. */
class ReceptorDeInicializacao : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != Intent.ACTION_BOOT_COMPLETED && intent.action != Intent.ACTION_MY_PACKAGE_REPLACED) return
        val c = (context.applicationContext as TelacastApp).container
        if (c.manterConectado.value && c.cofre.ler() != null) FachadaDoServicoDeConexao(context).iniciar()
    }
}
