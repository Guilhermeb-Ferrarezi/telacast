package app.telacast.push

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.util.Log
import app.telacast.TelacastApp

/**
 * Só no build de debug. Simula a data message do FCM pelo caminho de verdade (validação, política de primeiro plano,
 * notificação e toque), com as mesmas chaves string: `kind`, `channel_id`, `message_id`, `server_id`, `title`, `body`,
 * `author_id`, `ts`. Extra `forcar` (`--ez forcar true`) mostra mesmo com o app na frente. O resultado vai para o logcat
 * (tag `TelacastPush`) e, se o broadcast for ordenado, no resultado.
 */
class ReceptorDeTesteDePush : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val ex = intent.extras
        val dados = listOf("kind", "channel_id", "message_id", "server_id", "title", "body", "author_id", "ts")
            .mapNotNull { k -> ex?.get(k)?.toString()?.let { k to it } }.toMap()
        val forcar = ex?.getBoolean("forcar", false) ?: false
        val r = (context.applicationContext as TelacastApp).container.receptorDePush.processar(dados, forcar)
        Log.i("TelacastPush", "push de teste $dados forcar=$forcar -> $r")
        if (isOrderedBroadcast) { resultCode = r.ordinal; resultData = r.name } // `am broadcast` comum não é ordenado
    }
}
