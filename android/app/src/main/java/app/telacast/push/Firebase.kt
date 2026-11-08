package app.telacast.push

import android.content.Context
import android.util.Log
import app.telacast.TelacastApp
import com.google.firebase.FirebaseApp
import com.google.firebase.messaging.FirebaseMessaging
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import kotlin.coroutines.resume
import kotlinx.coroutines.suspendCancellableCoroutine

/**
 * Token do FCM. Sem `app/google-services.json` no build o Firebase não inicializa e [obter] devolve `null`: o app só não
 * registra push (nada quebra). Tudo dentro de `runCatching`: Play Services ausente ou desatualizado também cai aqui.
 */
class ProvedorFirebase(private val ctx: Context) : ProvedorDeToken {
    private fun disponivel() = runCatching { FirebaseApp.getApps(ctx).isNotEmpty() }.getOrDefault(false)

    override suspend fun obter(): String? {
        if (!disponivel()) return null
        return runCatching {
            suspendCancellableCoroutine<String?> { c ->
                FirebaseMessaging.getInstance().token.addOnCompleteListener { t ->
                    if (t.isSuccessful) Log.i("TelacastPush", "token FCM obtido: ${t.result.take(8)}…") else Log.w("TelacastPush", "sem token FCM: ${t.exception?.message}")
                    if (c.isActive) c.resume(if (t.isSuccessful) t.result else null)
                }
            }
        }.getOrNull()
    }

    override suspend fun apagar() {
        if (!disponivel()) return
        runCatching {
            suspendCancellableCoroutine<Unit> { c ->
                FirebaseMessaging.getInstance().deleteToken().addOnCompleteListener { if (c.isActive) c.resume(Unit) }
            }
        }
    }
}

/** Recebe o push do FCM (token novo e data messages). A lógica toda está em [GerenciadorDePush] e [ReceptorDePush]. */
class ServicoDePush : FirebaseMessagingService() {
    private val container get() = (application as TelacastApp).container

    override fun onNewToken(token: String) { Log.i("TelacastPush", "token FCM novo: ${token.take(8)}…"); container.push.aoNovoToken(token) }

    override fun onMessageReceived(message: RemoteMessage) { container.receptorDePush.processar(message.data) }
}
