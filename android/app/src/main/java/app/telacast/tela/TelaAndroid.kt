package app.telacast.tela

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.hardware.display.DisplayManager
import android.hardware.display.VirtualDisplay
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaCodecList
import android.media.MediaFormat
import android.media.projection.MediaProjection
import android.media.projection.MediaProjectionManager
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.util.DisplayMetrics
import android.util.Log
import android.view.Surface
import android.view.WindowManager
import androidx.core.content.ContextCompat
import app.telacast.MainActivity
import app.telacast.TelacastApp
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean

private const val TAG = "TelacastTela"
private const val MIME = MediaFormat.MIMETYPE_VIDEO_AVC

// ---------------------------------------------------------------------------------------------- rede

/** [TransporteDeVideo] sobre o OkHttp. `pendente()` é o `queueSize()` do WebSocket: a medida da lentidão da rede. */
class OkHttpTransporteDeVideo(
    private val cliente: OkHttpClient = OkHttpClient.Builder().pingInterval(20, TimeUnit.SECONDS).readTimeout(0, TimeUnit.MILLISECONDS).connectTimeout(10, TimeUnit.SECONDS).build(),
) : TransporteDeVideo {
    override fun abrir(url: String, ouvinte: OuvinteDeVideo): ConexaoDeVideo {
        val fim = AtomicBoolean(false)
        fun terminou(motivo: String, http: Int? = null) { if (fim.compareAndSet(false, true)) ouvinte.aoFim(motivo, http) }
        val ws = cliente.newWebSocket(Request.Builder().url(url).build(), object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) = ouvinte.aoAbrir()
            override fun onMessage(webSocket: WebSocket, text: String) = ouvinte.aoTexto(text)
            override fun onMessage(webSocket: WebSocket, bytes: ByteString) = ouvinte.aoBinario(bytes.toByteArray())
            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) { webSocket.close(code, null) }
            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) = terminou("fechada ($code)")
            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) = terminou(t.message ?: t.javaClass.simpleName, response?.code)
        })
        return object : ConexaoDeVideo {
            override fun enviar(dados: ByteArray) = ws.send(ByteString.of(*dados))
            override fun pendente() = ws.queueSize()
            override fun fechar() { ws.close(1000, null) }
            override fun cancelar() { ws.cancel() }
        }
    }
}

// ---------------------------------------------------------------------------------------------- transmitir: projeção + codificador

/** Resultado do diálogo "Começar a gravar ou transmitir com o Telacast?" do Android. */
class ConcessaoDeTela(val codigo: Int, val dados: Intent)

/** Serviço em primeiro plano do tipo `mediaProjection` (obrigatório desde o Android 10/14). Separado do da voz; os dois convivem. */
class ServicoDeProjecao : Service() {
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == ACAO_PARAR) {
            (application as TelacastApp).container.transmissao.parar()
            stopSelf()
            return START_NOT_STICKY
        }
        val abrir = PendingIntent.getActivity(this, 0, Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP), PendingIntent.FLAG_IMMUTABLE)
        val parar = PendingIntent.getService(this, 2, Intent(this, ServicoDeProjecao::class.java).setAction(ACAO_PARAR), PendingIntent.FLAG_IMMUTABLE)
        val n = Notification.Builder(this, CANAL).setSmallIcon(app.telacast.R.drawable.ic_notificacao).setColor(0xFFE9B35E.toInt()).setContentTitle("Transmitindo a tela")
            .setContentText("Quem está no canal de voz pode assistir.").setOngoing(true).setCategory(Notification.CATEGORY_SERVICE).setContentIntent(abrir)
            .addAction(Notification.Action.Builder(null, "Parar transmissão", parar).build()).build()
        try {
            if (Build.VERSION.SDK_INT >= 29) startForeground(ID, n, ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PROJECTION) else startForeground(ID, n)
        } catch (e: Exception) {
            Log.w(TAG, "não consegui subir o serviço da transmissão: ${e.message}")
            stopSelf()
        }
        pronto.countDown()
        return START_NOT_STICKY
    }

    companion object {
        private const val CANAL = "transmissao"
        private const val ID = 7002
        private const val ACAO_PARAR = "app.telacast.tela.PARAR"
        @Volatile var pronto = CountDownLatch(1)

        fun criarCanal(ctx: Context) {
            (ctx.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager)
                .createNotificationChannel(NotificationChannel(CANAL, "Transmissão de tela", NotificationManager.IMPORTANCE_LOW).apply { description = "Mostra que a tela está sendo transmitida e deixa parar com um toque."; setShowBadge(false) })
        }

        fun iniciar(ctx: Context): Boolean {
            pronto = CountDownLatch(1)
            ContextCompat.startForegroundService(ctx, Intent(ctx, ServicoDeProjecao::class.java))
            return pronto.await(4, TimeUnit.SECONDS)
        }

        fun parar(ctx: Context) { ctx.stopService(Intent(ctx, ServicoDeProjecao::class.java)) }
    }
}

/**
 * Captura da tela: `MediaProjection` -> `VirtualDisplay` -> `MediaCodec` H.264 por `Surface` (hardware, baseline, CBR, quadro-chave a cada 1 s,
 * SPS/PPS repetidos em todo quadro-chave). A tela gira: o codificador é recriado no tamanho novo e o `VirtualDisplay` troca de superfície,
 * sem derrubar a projeção nem o socket (o primeiro quadro do codificador novo é chave). Sem áudio, como o desktop.
 */
class FonteAndroid(private val ctx: Context) : FonteDeTela {
    private val trava = Any()
    private val principal = Handler(Looper.getMainLooper())
    private var projecao: MediaProjection? = null
    private var vd: VirtualDisplay? = null
    private var codec: MediaCodec? = null
    private var thread: Thread? = null
    private var ouvinte: OuvinteDaFonte? = null
    private var opcoes = OpcoesDeTransmissao()
    private var tamanho = 0 to 0
    private var tamanhoDaTela = 0 to 0
    private var dpi = 320
    @Volatile private var rodando = false
    private val dm = ctx.getSystemService(Context.DISPLAY_SERVICE) as DisplayManager
    private val aoMudarDisplay = object : DisplayManager.DisplayListener {
        override fun onDisplayAdded(displayId: Int) = Unit
        override fun onDisplayRemoved(displayId: Int) = Unit
        override fun onDisplayChanged(displayId: Int) { if (displayId == 0) girou() }
    }

    override fun fpsSuportados(): List<Int> {
        val base = mutableListOf(15, 30)
        val altos = runCatching { codecAvc()?.let { MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos.first { i -> i.name == it }.getCapabilitiesForType(MIME).videoCapabilities.getSupportedFrameRatesFor(1280, 720).upper >= 60.0 } }.getOrNull() == true
        if (altos) base += 60
        return base
    }

    /** Nome do codificador H.264: prefere o de hardware. */
    private fun codecAvc(): String? {
        val todos = MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos.filter { it.isEncoder && MIME in it.supportedTypes }
        val hw = todos.firstOrNull { Build.VERSION.SDK_INT >= 29 && it.isHardwareAccelerated && !it.isSoftwareOnly } ?: todos.firstOrNull { !it.name.startsWith("OMX.google") && !it.name.startsWith("c2.android") }
        return (hw ?: todos.firstOrNull())?.name
    }

    @Suppress("DEPRECATION")
    private fun medirTela(): Pair<Int, Int> {
        val m = DisplayMetrics()
        (ctx.getSystemService(Context.WINDOW_SERVICE) as WindowManager).defaultDisplay.getRealMetrics(m)
        dpi = m.densityDpi
        return m.widthPixels to m.heightPixels
    }

    override fun iniciar(concessao: Any, opcoes: OpcoesDeTransmissao, ouvinte: OuvinteDaFonte): String? {
        val c = concessao as? ConcessaoDeTela ?: return "Permissão de captura inválida."
        this.ouvinte = ouvinte; this.opcoes = opcoes
        if (!ServicoDeProjecao.iniciar(ctx)) { ServicoDeProjecao.parar(ctx); return "O Android não deixou iniciar o serviço da transmissão." }
        return try {
            val mpm = ctx.getSystemService(Context.MEDIA_PROJECTION_SERVICE) as MediaProjectionManager
            val p = mpm.getMediaProjection(c.codigo, c.dados) ?: return limpar("Não consegui iniciar a captura da tela.")
            projecao = p
            p.registerCallback(object : MediaProjection.Callback() {
                override fun onStop() { if (rodando) falhar("A captura de tela foi encerrada pelo sistema.") }
            }, principal)
            tamanhoDaTela = medirTela()
            val (l, a) = tamanhoDaCodificacao(tamanhoDaTela.first, tamanhoDaTela.second, opcoes.qualidade.ladoMenor)
            val superficie = abrirCodificador(l, a) ?: return limpar("O aparelho não conseguiu abrir o codificador de vídeo H.264.")
            vd = p.createVirtualDisplay("telacast", l, a, dpi, DisplayManager.VIRTUAL_DISPLAY_FLAG_AUTO_MIRROR, superficie, null, null)
            tamanho = l to a
            rodando = true
            iniciarLeitor(codec!!)
            dm.registerDisplayListener(aoMudarDisplay, principal)
            ouvinte.aoTamanho(l, a)
            null
        } catch (e: SecurityException) {
            limpar("O Android negou a captura de tela.")
        } catch (e: Exception) {
            Log.w(TAG, "falha ao iniciar a captura", e)
            limpar("Falha ao iniciar a transmissão: ${e.message ?: e.javaClass.simpleName}")
        }
    }

    private fun limpar(msg: String): String { parar(); return msg }

    /** Cria e inicia um codificador novo no tamanho dado; guarda em [codec] e devolve a superfície de entrada. Tenta formatos cada vez mais simples. */
    private fun abrirCodificador(l: Int, a: Int): Surface? {
        val nome = codecAvc() ?: return null
        val kbps = opcoes.kbpsEfetivo()
        for (nivel in 0..2) {
            var c: MediaCodec? = null
            try {
                val f = MediaFormat.createVideoFormat(MIME, l, a).apply {
                    setInteger(MediaFormat.KEY_COLOR_FORMAT, MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface)
                    setInteger(MediaFormat.KEY_BIT_RATE, kbps * 1000)
                    setInteger(MediaFormat.KEY_FRAME_RATE, opcoes.fps)
                    setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 1)
                    if (nivel <= 1) {
                        setInteger(MediaFormat.KEY_PROFILE, MediaCodecInfo.CodecProfileLevel.AVCProfileBaseline)
                        setInteger(MediaFormat.KEY_BITRATE_MODE, MediaCodecInfo.EncoderCapabilities.BITRATE_MODE_CBR)
                    }
                    if (nivel == 0) {
                        setLong("repeat-previous-frame-after", 500_000L) // tela parada: repete o quadro para o espectador não achar que travou
                        if (Build.VERSION.SDK_INT >= 29) { setInteger("max-fps-to-encoder", opcoes.fps); setInteger("prepend-sps-pps-to-idr-frames", 1) }
                    }
                }
                c = MediaCodec.createByCodecName(nome)
                c.configure(f, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
                val s = c.createInputSurface()
                c.start()
                synchronized(trava) { codec = c; spsPps = null }
                return s
            } catch (e: Exception) {
                Log.w(TAG, "codificador $nome nível $nivel: ${e.message}")
                runCatching { c?.release() }
            }
        }
        return null
    }

    @Volatile private var spsPps: ByteArray? = null

    private fun iniciarLeitor(c: MediaCodec) {
        val t = Thread({
            android.os.Process.setThreadPriority(android.os.Process.THREAD_PRIORITY_VIDEO)
            val info = MediaCodec.BufferInfo()
            try {
                while (rodando && codec === c) {
                    val i = c.dequeueOutputBuffer(info, 100_000)
                    if (i < 0) continue
                    try {
                        val buf = c.getOutputBuffer(i)
                        if (buf != null && info.size > 0) {
                            val bytes = ByteArray(info.size); buf.position(info.offset); buf.get(bytes)
                            if (info.flags and MediaCodec.BUFFER_FLAG_CODEC_CONFIG != 0) spsPps = H264.spsPps(bytes) ?: bytes
                            else {
                                val chave = info.flags and MediaCodec.BUFFER_FLAG_KEY_FRAME != 0
                                ouvinte?.aoQuadro(chave, if (chave) H264.comSpsPps(bytes, spsPps) else bytes)
                            }
                        }
                    } finally { runCatching { c.releaseOutputBuffer(i, false) } }
                }
            } catch (e: Exception) { if (rodando && codec === c) falhar("O codificador de vídeo parou: ${e.message ?: e.javaClass.simpleName}") }
        }, "tela-codificador")
        thread = t; t.start()
    }

    /** A tela girou (ou a densidade mudou): recria o codificador no tamanho novo. */
    private fun girou() {
        if (!rodando) return
        val nova = medirTela()
        if (nova == tamanhoDaTela) return
        tamanhoDaTela = nova
        recriar()
    }

    private fun recriar(): String? = synchronized(trava) {
        if (!rodando) return null
        val (l, a) = tamanhoDaCodificacao(tamanhoDaTela.first, tamanhoDaTela.second, opcoes.qualidade.ladoMenor)
        val velho = codec
        val superficie = abrirCodificador(l, a) ?: return "O aparelho não conseguiu reabrir o codificador de vídeo."
        val novo = codec!!
        runCatching { vd?.resize(l, a, dpi); vd?.setSurface(superficie) }
        tamanho = l to a
        iniciarLeitor(novo)
        runCatching { velho?.stop() }; runCatching { velho?.release() }
        ouvinte?.aoTamanho(l, a)
        null
    }

    override fun reconfigurar(opcoes: OpcoesDeTransmissao): String? { this.opcoes = opcoes; return recriar() }

    override fun pedirQuadroChave() {
        runCatching { codec?.setParameters(Bundle().apply { putInt(MediaCodec.PARAMETER_KEY_REQUEST_SYNC_FRAME, 0) }) }
    }

    private fun falhar(msg: String) { val o = ouvinte; if (rodando) { rodando = false; o?.aoFalhar(msg) } }

    override fun parar() {
        val estavaRodando = rodando
        rodando = false
        runCatching { dm.unregisterDisplayListener(aoMudarDisplay) }
        runCatching { vd?.release() }; vd = null
        synchronized(trava) { runCatching { codec?.stop() }; runCatching { codec?.release() }; codec = null }
        thread?.takeIf { it !== Thread.currentThread() }?.join(500); thread = null
        runCatching { projecao?.stop() }; projecao = null
        if (estavaRodando || true) ServicoDeProjecao.parar(ctx)
    }
}

// ---------------------------------------------------------------------------------------------- assistir: decodificador

/** `MediaCodec` H.264 desenhando direto numa `Surface` (a do `SurfaceView`); baixa latência (sem fila: entrada cheia = descarta). */
class DecodificadorAndroid : DecodificadorDeVideo {
    private var codec: MediaCodec? = null
    private var thread: Thread? = null
    @Volatile private var rodando = false

    override fun configurar(superficie: Any, largura: Int, altura: Int): String? {
        val s = superficie as? Surface ?: return "Superfície de vídeo inválida."
        return try {
            val f = MediaFormat.createVideoFormat(MIME, largura, altura).apply {
                if (Build.VERSION.SDK_INT >= 30) setInteger(MediaFormat.KEY_LOW_LATENCY, 1)
                if (Build.VERSION.SDK_INT >= 23) setInteger(MediaFormat.KEY_PRIORITY, 0)
            }
            val c = MediaCodec.createDecoderByType(MIME)
            c.configure(f, s, null, 0)
            c.start()
            codec = c; rodando = true
            thread = Thread({
                val info = MediaCodec.BufferInfo()
                try { while (rodando) { val i = c.dequeueOutputBuffer(info, 20_000); if (i >= 0) c.releaseOutputBuffer(i, true) } } catch (_: Exception) { }
            }, "tela-decodificador").also { it.start() }
            null
        } catch (e: Exception) {
            Log.w(TAG, "decodificador: ${e.message}")
            "Este aparelho não conseguiu abrir o decodificador de vídeo."
        }
    }

    override fun decodificar(h264: ByteArray, instanteUs: Long): Boolean {
        val c = codec ?: return false
        return try {
            val i = c.dequeueInputBuffer(0)
            if (i < 0) return false // sem buffer livre: descarta (e o controlador espera a próxima chave)
            val buf = c.getInputBuffer(i) ?: return false
            if (h264.size > buf.capacity()) { c.queueInputBuffer(i, 0, 0, instanteUs, 0); return false }
            buf.clear(); buf.put(h264)
            c.queueInputBuffer(i, 0, h264.size, instanteUs, 0)
            true
        } catch (e: Exception) { false }
    }

    override fun fechar() {
        rodando = false
        thread?.takeIf { it !== Thread.currentThread() }?.join(300); thread = null
        runCatching { codec?.stop() }; runCatching { codec?.release() }; codec = null
    }
}
