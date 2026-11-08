package app.telacast.atualizacao

import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.PackageInstaller
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import app.telacast.TelacastApp
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.Json
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.File
import java.security.MessageDigest
import java.util.concurrent.TimeUnit

/** [ApiDeAtualizacao] sobre o OkHttp: `GET /update/manifest?platform=` e `GET /update/file/<sha256>`. */
class OkHttpAtualizacao(
    private val base: () -> String,
    private val cliente: OkHttpClient = OkHttpClient.Builder().connectTimeout(10, TimeUnit.SECONDS).readTimeout(30, TimeUnit.SECONDS).callTimeout(10, TimeUnit.MINUTES).build(),
) : ApiDeAtualizacao {
    private val json = Json { ignoreUnknownKeys = true }

    override suspend fun manifesto(plataforma: String): ManifestoDeAtualizacao? = withContext(Dispatchers.IO) {
        try {
            cliente.newCall(Request.Builder().url("${base()}/update/manifest?platform=$plataforma").build()).execute().use { r ->
                if (!r.isSuccessful) return@withContext null
                json.decodeFromString(ManifestoDeAtualizacao.serializer(), r.body!!.string())
            }
        } catch (_: Exception) { null }
    }

    override suspend fun baixar(m: ManifestoDeAtualizacao, destino: File, progresso: (Long) -> Unit): ResultadoDoDownload = withContext(Dispatchers.IO) {
        try {
            cliente.newCall(Request.Builder().url("${base()}/update/file/${m.sha256}").build()).execute().use { r ->
                if (!r.isSuccessful) return@withContext ResultadoDoDownload.Falhou("o servidor respondeu ${r.code}")
                val corpo = r.body ?: return@withContext ResultadoDoDownload.Falhou("resposta vazia")
                if (corpo.contentLength() > m.size) return@withContext ResultadoDoDownload.Falhou("o arquivo é maior que o do manifesto")
                val digest = MessageDigest.getInstance("SHA-256")
                var total = 0L
                destino.outputStream().use { saida ->
                    corpo.byteStream().use { entrada ->
                        val buf = ByteArray(64 * 1024)
                        while (true) {
                            val n = entrada.read(buf)
                            if (n < 0) break
                            total += n
                            if (total > m.size) return@withContext ResultadoDoDownload.Falhou("o arquivo é maior que o do manifesto")
                            digest.update(buf, 0, n); saida.write(buf, 0, n); progresso(total)
                        }
                    }
                }
                ResultadoDoDownload.Baixado(total, bytesParaHex(digest.digest()))
            }
        } catch (e: Exception) { ResultadoDoDownload.Falhou(e.message ?: e.javaClass.simpleName) }
    }
}

private const val ACAO_STATUS = "app.telacast.atualizacao.STATUS"

/** Instalação por `PackageInstaller`: sem diálogo quando o Android deixar (Android 12+, mesmo instalador, mesma assinatura); senão ele pede a confirmação. */
class InstaladorAndroid(private val ctx: Context) : InstaladorDeApk {
    override fun podeInstalar(): Boolean = ctx.packageManager.canRequestPackageInstalls()

    @Suppress("DEPRECATION")
    override fun validar(apk: File, versaoNome: String): String? {
        val info = ctx.packageManager.getPackageArchiveInfo(apk.path, 0) ?: return "O arquivo baixado não é um aplicativo Android válido."
        if (info.packageName != ctx.packageName) return "O arquivo baixado é de outro aplicativo (${info.packageName})."
        val atual = ctx.packageManager.getPackageInfo(ctx.packageName, 0)
        val codigoAtual = if (Build.VERSION.SDK_INT >= 28) atual.longVersionCode else atual.versionCode.toLong()
        val codigoNovo = if (Build.VERSION.SDK_INT >= 28) info.longVersionCode else info.versionCode.toLong()
        if (codigoNovo <= codigoAtual) return "A versão baixada não é mais nova que a instalada."
        return null
    }

    override fun instalar(apk: File): String? = try {
        val pi = ctx.packageManager.packageInstaller
        val params = PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL).apply {
            setAppPackageName(ctx.packageName)
            setSize(apk.length())
            if (Build.VERSION.SDK_INT >= 31) setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED)
            if (Build.VERSION.SDK_INT >= 34) setInstallReason(PackageManager.INSTALL_REASON_USER)
        }
        val id = pi.createSession(params)
        pi.openSession(id).use { s ->
            apk.inputStream().use { i -> s.openWrite("telacast.apk", 0, apk.length()).use { o -> i.copyTo(o); s.fsync(o) } }
            val intent = Intent(ctx, ReceptorDeInstalacao::class.java).setAction(ACAO_STATUS).setPackage(ctx.packageName)
            val pendente = PendingIntent.getBroadcast(ctx, id, intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_MUTABLE)
            s.commit(pendente.intentSender)
        }
        null
    } catch (e: Exception) { "Não consegui iniciar a instalação: ${e.message ?: e.javaClass.simpleName}" }

    override fun abrirPermissao() {
        ctx.startActivity(Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:${ctx.packageName}")).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
    }
}

/** Resposta do Android à sessão: pede a confirmação do usuário (`STATUS_PENDING_USER_ACTION`) ou avisa o resultado. */
class ReceptorDeInstalacao : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        val status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE)
        val c = (context.applicationContext as TelacastApp).container
        when (status) {
            PackageInstaller.STATUS_PENDING_USER_ACTION -> {
                @Suppress("DEPRECATION")
                val confirmar = if (Build.VERSION.SDK_INT >= 33) intent.getParcelableExtra(Intent.EXTRA_INTENT, Intent::class.java) else intent.getParcelableExtra(Intent.EXTRA_INTENT)
                confirmar?.let { runCatching { context.startActivity(it.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)) } }
            }
            PackageInstaller.STATUS_SUCCESS -> c.atualizacao.aoResultadoDaInstalacao(true, null)
            PackageInstaller.STATUS_FAILURE_ABORTED -> c.atualizacao.aoResultadoDaInstalacao(false, "A instalação foi cancelada.")
            else -> c.atualizacao.aoResultadoDaInstalacao(false, "O Android não instalou a atualização (código $status).")
        }
    }
}
