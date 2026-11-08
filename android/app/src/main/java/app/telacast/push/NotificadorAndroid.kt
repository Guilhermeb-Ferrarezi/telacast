package app.telacast.push

import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.net.Uri
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.app.Person
import app.telacast.MainActivity

/** Ação do `Intent` que a notificação manda para a [MainActivity]; os extras são as chaves de [DestinoDoToque.paraMapa]. */
const val ACAO_ABRIR_PUSH = "app.telacast.push.ABRIR"
private const val PREFIXO_EXTRA = "push_"
private const val ID_DA_NOTIFICACAO = 1

/** Cria os canais de notificação (DMs, menções, canais, amizades). Seguro chamar sempre ao abrir o app. */
fun criarCanaisDeNotificacao(ctx: Context) {
    val nm = ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
    for (c in CanalDeNotificacao.entries) {
        val imp = when (c.importancia) {
            Importancia.Alta -> NotificationManager.IMPORTANCE_HIGH
            Importancia.Padrao -> NotificationManager.IMPORTANCE_DEFAULT
            Importancia.Baixa -> NotificationManager.IMPORTANCE_LOW
        }
        nm.createNotificationChannel(NotificationChannel(c.id, c.nome, imp).apply { description = c.descricao })
    }
}

/** Extras do `Intent` de toque -> destino (a [MainActivity] chama ao abrir). */
fun destinoDoIntent(intent: Intent?): DestinoDoToque? {
    if (intent?.action != ACAO_ABRIR_PUSH) return null
    val ex = intent.extras ?: return null
    val mapa = ex.keySet().filter { it.startsWith(PREFIXO_EXTRA) }.mapNotNull { k -> ex.getString(k)?.let { k.removePrefix(PREFIXO_EXTRA) to it } }.toMap()
    return DestinoDoToque.deMapa(mapa)
}

/** Desenha as notificações: uma por canal/DM (`MessagingStyle`, até 5 linhas) e uma por pedido de amizade. */
class NotificadorAndroid(private val ctx: Context) : Notificador {
    private val nm = NotificationManagerCompat.from(ctx)

    override fun mostrar(modelo: ModeloDeNotificacao) {
        if (!nm.areNotificationsEnabled()) return // sem a permissão (Android 13+) ou desligadas nos Ajustes
        val b = NotificationCompat.Builder(ctx, modelo.canal.id)
            .setSmallIcon(android.R.drawable.stat_notify_chat)
            .setContentIntent(aoTocar(modelo))
            .setAutoCancel(true)
            .setWhen(modelo.ts).setShowWhen(true)
            .setCategory(if (modelo.conversa) NotificationCompat.CATEGORY_MESSAGE else NotificationCompat.CATEGORY_SOCIAL)
            .setPriority(if (modelo.canal.importancia == Importancia.Alta) NotificationCompat.PRIORITY_HIGH else NotificationCompat.PRIORITY_DEFAULT)
        if (modelo.conversa) {
            val linhas = acumularLinhas(linhasAtuais(modelo.tag), modelo.linha)
            val estilo = NotificationCompat.MessagingStyle(Person.Builder().setName("Você").build())
            modelo.tituloDaConversa?.let { estilo.setConversationTitle(it).setGroupConversation(true) }
            for (l in linhas) estilo.addMessage(l.texto, l.ts, Person.Builder().setName(l.autor).build())
            b.setStyle(estilo).setContentTitle(modelo.titulo).setContentText(modelo.texto)
        } else {
            b.setContentTitle(modelo.titulo).setContentText(modelo.texto).setStyle(NotificationCompat.BigTextStyle().bigText(modelo.texto))
        }
        runCatching { nm.notify(modelo.tag, ID_DA_NOTIFICACAO, b.build()) } // SecurityException sem permissão: ignora
    }

    override fun limparCanal(canal: Long) = nm.cancel(ModeloDeNotificacao.tagDoCanal(canal), ID_DA_NOTIFICACAO)

    override fun limparPedidos() {
        val ativas = (ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager).activeNotifications
        for (n in ativas) if (n.tag?.startsWith("f:") == true) nm.cancel(n.tag, n.id)
    }

    /** O que já está empilhado para o canal (lido da notificação ativa, que sobrevive ao processo). */
    private fun linhasAtuais(tag: String): List<LinhaDeMensagem> {
        val ativa = (ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager).activeNotifications
            .firstOrNull { it.tag == tag && it.id == ID_DA_NOTIFICACAO } ?: return emptyList()
        val estilo = NotificationCompat.MessagingStyle.extractMessagingStyleFromNotification(ativa.notification) ?: return emptyList()
        return estilo.messages.map { LinhaDeMensagem(it.person?.name?.toString().orEmpty(), it.text?.toString().orEmpty(), it.timestamp) }
    }

    private fun aoTocar(m: ModeloDeNotificacao): PendingIntent {
        val i = Intent(ctx, MainActivity::class.java).setAction(ACAO_ABRIR_PUSH)
            .setData(Uri.parse("telacast://push/${m.tag}")) // torna o Intent único por canal (o PendingIntent não é reaproveitado)
            .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP)
        for ((k, v) in m.destino.paraMapa()) i.putExtra(PREFIXO_EXTRA + k, v)
        return PendingIntent.getActivity(ctx, m.tag.hashCode(), i, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
    }
}
