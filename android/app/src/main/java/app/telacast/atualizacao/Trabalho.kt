package app.telacast.atualizacao

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import androidx.core.app.NotificationCompat
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.NetworkType
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import app.telacast.MainActivity
import app.telacast.R
import app.telacast.TelacastApp
import java.util.concurrent.TimeUnit

const val ACAO_INSTALAR_ATUALIZACAO = "app.telacast.atualizacao.INSTALAR"
private const val CANAL = "atualizacao"

fun criarCanalDeAtualizacao(ctx: Context) {
    (ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager)
        .createNotificationChannel(NotificationChannel(CANAL, "Atualizações do app", NotificationManager.IMPORTANCE_DEFAULT).apply { description = "Avisa quando uma versão nova do Telacast já foi baixada e está pronta para instalar." })
}

/** Uma vez por dia, só em rede NÃO medida (Wi-Fi) e com o aparelho carregando: baixa e verifica o APK novo e avisa. Instalar fica atrás do toque da pessoa. */
fun agendarChecagemDiaria(ctx: Context) {
    val req = PeriodicWorkRequestBuilder<TrabalhoDeAtualizacao>(1, TimeUnit.DAYS)
        .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.UNMETERED).setRequiresCharging(true).build()).build()
    WorkManager.getInstance(ctx).enqueueUniquePeriodicWork("atualizacao-diaria", ExistingPeriodicWorkPolicy.KEEP, req)
}

class TrabalhoDeAtualizacao(ctx: Context, p: WorkerParameters) : CoroutineWorker(ctx, p) {
    override suspend fun doWork(): Result {
        val c = (applicationContext as TelacastApp).container
        if (!c.atualizacao.ligada) return Result.success()
        val m = c.atualizacao.baixarEmSegundoPlano() ?: return Result.success()
        val abrir = PendingIntent.getActivity(
            applicationContext, 3, Intent(applicationContext, MainActivity::class.java).setAction(ACAO_INSTALAR_ATUALIZACAO).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val n = NotificationCompat.Builder(applicationContext, CANAL).setSmallIcon(R.drawable.ic_notificacao).setColor(0xFFE9B35E.toInt())
            .setContentTitle("Nova versão pronta — toque para instalar").setContentText("Telacast ${m.version}").setContentIntent(abrir).setAutoCancel(true).build()
        runCatching { androidx.core.app.NotificationManagerCompat.from(applicationContext).notify(7003, n) }
        return Result.success()
    }
}
