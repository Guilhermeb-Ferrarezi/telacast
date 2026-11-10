package app.telacast.chamada

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.media.AudioAttributes
import android.media.Ringtone
import android.media.RingtoneManager
import android.media.ToneGenerator
import android.os.Build
import android.os.VibrationEffect
import android.os.Vibrator
import android.os.VibratorManager
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import app.telacast.MainActivity
import app.telacast.R
import app.telacast.TelacastApp

const val ACAO_ATENDER_CHAMADA = "app.telacast.chamada.ATENDER"
private const val ACAO_RECUSAR_CHAMADA = "app.telacast.chamada.RECUSAR"
private const val CANAL = "chamadas"
private const val TAG = "chamada"

fun criarCanalDeChamadas(ctx: Context) {
    // o toque é do `ToqueAndroid` (também com o app na frente): o canal só leva a notificação, alta e sem som próprio
    (ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager)
        .createNotificationChannel(NotificationChannel(CANAL, "Chamadas", NotificationManager.IMPORTANCE_HIGH).apply {
            description = "Quando alguém liga para você numa conversa direta."
            setSound(null, null); enableVibration(false)
        })
}

/** Toque do sistema para a chamada recebida (com vibração) e o sinal de chamando para quem liga. */
class ToqueAndroid(private val ctx: Context) : ToqueDeChamada {
    private var ringtone: Ringtone? = null
    private var tom: ToneGenerator? = null
    private var tomAtivo = false
    private val vibrador: Vibrator? = if (Build.VERSION.SDK_INT >= 31) (ctx.getSystemService(Context.VIBRATOR_MANAGER_SERVICE) as VibratorManager).defaultVibrator else @Suppress("DEPRECATION") ctx.getSystemService(Context.VIBRATOR_SERVICE) as? Vibrator

    @Synchronized override fun tocarRecebendo() {
        parar()
        runCatching {
            val r = RingtoneManager.getRingtone(ctx, RingtoneManager.getDefaultUri(RingtoneManager.TYPE_RINGTONE))
            if (Build.VERSION.SDK_INT >= 28) r.isLooping = true
            r.audioAttributes = AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_NOTIFICATION_RINGTONE).setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION).build()
            r.play(); ringtone = r
        }
        runCatching { vibrador?.vibrate(VibrationEffect.createWaveform(longArrayOf(0, 800, 800), 0)) }
    }

    @Synchronized override fun tocarChamando() {
        parar()
        runCatching {
            val t = ToneGenerator(android.media.AudioManager.STREAM_VOICE_CALL, 60)
            tom = t; tomAtivo = true
            Thread { while (tomAtivo) { runCatching { t.startTone(ToneGenerator.TONE_SUP_RINGTONE, 1000) }; Thread.sleep(3000) } }.also { it.isDaemon = true }.start()
        }
    }

    @Synchronized override fun parar() {
        runCatching { ringtone?.stop() }; ringtone = null
        tomAtivo = false; runCatching { tom?.stopTone() }; runCatching { tom?.release() }; tom = null
        runCatching { vibrador?.cancel() }
    }

    @Synchronized override fun fim() {
        runCatching { val t = ToneGenerator(android.media.AudioManager.STREAM_VOICE_CALL, 60); t.startTone(ToneGenerator.TONE_PROP_NACK, 300); Thread { Thread.sleep(500); t.release() }.start() }
    }
}

/** Notificação de chamada recebida (alta prioridade, 60 s, com Atender e Recusar) e de chamada perdida. */
class NotificadorDeChamadaAndroid(private val ctx: Context) : NotificadorDeChamada {
    private val nm = NotificationManagerCompat.from(ctx)

    override fun chamadaRecebida(canal: Long, nome: String) {
        if (!nm.areNotificationsEnabled()) return
        val abrir = PendingIntent.getActivity(ctx, canal.hashCode(), Intent(ctx, MainActivity::class.java).setAction(ACAO_ATENDER_CHAMADA).putExtra("canal", canal).putExtra("so_abrir", true)
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        val atender = PendingIntent.getActivity(ctx, canal.hashCode() + 1, Intent(ctx, MainActivity::class.java).setAction(ACAO_ATENDER_CHAMADA).putExtra("canal", canal)
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        val recusar = PendingIntent.getBroadcast(ctx, canal.hashCode() + 2, Intent(ctx, ReceptorDeChamada::class.java).setAction(ACAO_RECUSAR_CHAMADA).putExtra("canal", canal), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        val n = NotificationCompat.Builder(ctx, CANAL).setSmallIcon(R.drawable.ic_notificacao).setColor(0xFFE9B35E.toInt())
            .setContentTitle(nome).setContentText("Chamada de voz recebida")
            .setCategory(NotificationCompat.CATEGORY_CALL).setPriority(NotificationCompat.PRIORITY_MAX).setOngoing(true).setAutoCancel(false)
            .setTimeoutAfter(60_000).setContentIntent(abrir)
            .addAction(0, "Recusar", recusar).addAction(0, "Atender", atender).build()
        runCatching { nm.notify(TAG, canal.hashCode(), n) }
    }

    override fun chamadaPerdida(canal: Long, nome: String) {
        nm.cancel(TAG, canal.hashCode())
        if (!nm.areNotificationsEnabled()) return
        val abrir = PendingIntent.getActivity(ctx, canal.hashCode() + 3, Intent(ctx, MainActivity::class.java).setAction(app.telacast.push.ACAO_ABRIR_PUSH).putExtra("push_destino", "dm").putExtra("push_canal", canal.toString())
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP), PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
        val n = NotificationCompat.Builder(ctx, app.telacast.push.CanalDeNotificacao.Dms.id).setSmallIcon(R.drawable.ic_notificacao).setColor(0xFFE9B35E.toInt())
            .setContentTitle("Chamada perdida").setContentText(nome).setCategory(NotificationCompat.CATEGORY_MISSED_CALL).setAutoCancel(true).setContentIntent(abrir).build()
        runCatching { nm.notify("$TAG-perdida", canal.hashCode(), n) }
    }

    override fun limpar(canal: Long) = nm.cancel(TAG, canal.hashCode())
}

/** "Recusar" da notificação: só para o toque deste aparelho. */
class ReceptorDeChamada : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if (intent.action != ACAO_RECUSAR_CHAMADA) return
        (context.applicationContext as TelacastApp).container.chamadas.recusar(intent.getLongExtra("canal", 0))
    }
}
