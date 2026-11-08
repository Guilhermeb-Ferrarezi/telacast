package app.telacast.voz

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.media.AudioAttributes
import android.media.AudioDeviceInfo
import android.media.AudioFocusRequest
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioRecord
import android.media.AudioTrack
import android.media.MediaRecorder
import android.media.audiofx.AcousticEchoCanceler
import android.media.audiofx.AutomaticGainControl
import android.media.audiofx.NoiseSuppressor
import android.os.Build
import android.os.Process
import android.util.Log
import java.util.concurrent.atomic.AtomicInteger

/** Há fone (com fio, USB ou Bluetooth) conectado agora? Decide a rota inicial da chamada. */
fun temFoneConectado(ctx: Context): Boolean {
    val am = ctx.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    return am.getDevices(AudioManager.GET_DEVICES_OUTPUTS).any { it.type in TIPOS_DE_FONE }
}

private val TIPOS_DE_FONE = buildSet {
    add(AudioDeviceInfo.TYPE_WIRED_HEADSET); add(AudioDeviceInfo.TYPE_WIRED_HEADPHONES)
    add(AudioDeviceInfo.TYPE_BLUETOOTH_SCO); add(AudioDeviceInfo.TYPE_BLUETOOTH_A2DP); add(AudioDeviceInfo.TYPE_USB_HEADSET)
    if (Build.VERSION.SDK_INT >= 31) add(AudioDeviceInfo.TYPE_BLE_HEADSET)
}

/**
 * Microfone e alto-falante de verdade: `AudioRecord` (fonte VOICE_COMMUNICATION, que já liga o processamento de voz do
 * aparelho) e `AudioTrack` (uso VOICE_COMMUNICATION), os dois a 48 kHz mono de 16 bits, cada um na sua thread de áudio.
 * Ligam AGC, supressão de ruído e cancelamento de eco do Android quando o aparelho os oferece. A lógica toda mora no
 * [MotorDeVoz]; aqui só se move PCM de um lado para o outro a cada 20 ms.
 */
class DispositivoAndroid(
    private val ctx: Context,
    private val motor: MotorDeVoz,
    private var opcoes: OpcoesDeVoz,
    private var rota: RotaDeAudio,
    private val aoFalhar: (String) -> Unit,
) : DispositivoDeAudio {
    private val am = ctx.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    @Volatile private var gravador: AudioRecord? = null
    @Volatile private var toca: AudioTrack? = null
    private var efeitos: List<android.media.audiofx.AudioEffect> = emptyList()
    private var foco: AudioFocusRequest? = null
    private var modoAnterior = AudioManager.MODE_NORMAL
    @Volatile private var rodando = false
    private var threadMic: Thread? = null
    private var threadSaida: Thread? = null
    /** Reaberturas seguidas do microfone / do alto-falante (zera depois de ~1 s de áudio fluindo). */
    private val reaberturasMic = AtomicInteger(0)
    private val reaberturasSaida = AtomicInteger(0)
    private val travaDeReabertura = Any()

    override fun iniciar(): String? {
        if (ctx.checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) return "Sem a permissão do microfone."
        try {
            // Modo de comunicação, foco e ROTA primeiro, depois o AudioRecord/AudioTrack: com fone Bluetooth (SCO) a troca
            // de rota depois de o microfone abrir invalida o AudioRecord (read devolve erro) e a chamada caía em loop.
            modoAnterior = am.mode
            am.mode = AudioManager.MODE_IN_COMMUNICATION
            pedirFoco()
            aplicarRota()
            val rec = abrirGravador() ?: run { parar(); return "Não consegui abrir o microfone (talvez outro app o esteja usando)." }
            val track = abrirTrilha() ?: run { parar(); return "Não consegui abrir o alto-falante." }
            gravador = rec; toca = track
            ligarEfeitos(rec.audioSessionId)
            // um pouco de silêncio antes de tocar evita estalo e "fome" do buffer no começo
            track.write(ShortArray(QUADRO * 2), 0, QUADRO * 2)
            rec.startRecording(); track.play()
            rodando = true
            threadMic = Thread({ laçoDoMicrofone() }, "voz-microfone").also { it.start() }
            threadSaida = Thread({ laçoDoAltoFalante() }, "voz-alto-falante").also { it.start() }
            return null
        } catch (e: SecurityException) {
            parar(); return "O Android negou o acesso ao microfone."
        } catch (e: Exception) {
            parar(); return "Falha ao abrir o áudio: ${e.message ?: e.javaClass.simpleName}"
        }
    }

    private fun abrirGravador(): AudioRecord? {
        val formato = AudioFormat.Builder().setEncoding(AudioFormat.ENCODING_PCM_16BIT).setSampleRate(TAXA_DE_AMOSTRAGEM).setChannelMask(AudioFormat.CHANNEL_IN_MONO).build()
        val min = AudioRecord.getMinBufferSize(TAXA_DE_AMOSTRAGEM, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
        if (min <= 0) return null
        val rec = AudioRecord.Builder().setAudioSource(MediaRecorder.AudioSource.VOICE_COMMUNICATION).setAudioFormat(formato)
            .setBufferSizeInBytes(maxOf(min, QUADRO * 2 * 4)).build()
        if (rec.state != AudioRecord.STATE_INITIALIZED) { rec.release(); return null }
        return rec
    }

    private fun abrirTrilha(): AudioTrack? {
        val formato = AudioFormat.Builder().setEncoding(AudioFormat.ENCODING_PCM_16BIT).setSampleRate(TAXA_DE_AMOSTRAGEM).setChannelMask(AudioFormat.CHANNEL_OUT_MONO).build()
        val min = AudioTrack.getMinBufferSize(TAXA_DE_AMOSTRAGEM, AudioFormat.CHANNEL_OUT_MONO, AudioFormat.ENCODING_PCM_16BIT)
        val track = AudioTrack.Builder()
            .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION).setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
            .setAudioFormat(formato).setBufferSizeInBytes(maxOf(min, QUADRO * 2 * 3))
            .setTransferMode(AudioTrack.MODE_STREAM).setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY).build()
        if (track.state != AudioTrack.STATE_INITIALIZED) { track.release(); return null }
        return track
    }

    /**
     * Troca de rota (Bluetooth entrando ou saindo, fone, viva-voz) invalida o AudioRecord/AudioTrack e `read`/`write` devolvem
     * erro. Isso NÃO derruba a chamada: reabre o dispositivo (até [MAX_REABERTURAS] vezes seguidas, com uma pausa) e só avisa
     * a falha se não houver jeito. Devolve o novo gravador, ou `null` se desistiu.
     */
    private fun reabrirGravador(erro: Int): AudioRecord? {
        if (!rodando) return null
        if (reaberturasMic.incrementAndGet() > MAX_REABERTURAS) return null
        Log.w(TAG, "microfone: read=$erro, reabrindo (${reaberturasMic.get()}/$MAX_REABERTURAS)")
        synchronized(travaDeReabertura) {
            if (!rodando) return null
            runCatching { gravador?.stop() }; gravador?.release(); gravador = null
            for (e in efeitos) runCatching { e.release() }; efeitos = emptyList()
            Thread.sleep(PAUSA_DE_REABERTURA_MS)
            runCatching { aplicarRota() }
            val novo = runCatching { abrirGravador() }.getOrNull() ?: return null
            ligarEfeitos(novo.audioSessionId)
            runCatching { novo.startRecording() }
            if (novo.recordingState != AudioRecord.RECORDSTATE_RECORDING) { novo.release(); return null }
            gravador = novo
            return novo
        }
    }

    private fun reabrirTrilha(erro: Int): AudioTrack? {
        if (!rodando) return null
        if (reaberturasSaida.incrementAndGet() > MAX_REABERTURAS) return null
        Log.w(TAG, "alto-falante: write=$erro, reabrindo (${reaberturasSaida.get()}/$MAX_REABERTURAS)")
        synchronized(travaDeReabertura) {
            if (!rodando) return null
            runCatching { toca?.stop() }; toca?.release(); toca = null
            Thread.sleep(PAUSA_DE_REABERTURA_MS)
            runCatching { aplicarRota() }
            val novo = runCatching { abrirTrilha() }.getOrNull() ?: return null
            runCatching { novo.write(ShortArray(QUADRO * 2), 0, QUADRO * 2); novo.play() }
            toca = novo
            return novo
        }
    }

    private fun laçoDoMicrofone() {
        Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_AUDIO)
        var rec = gravador ?: return
        val quadro = ShortArray(QUADRO)
        var estaveis = 0
        while (rodando) {
            var lidos = 0
            while (rodando && lidos < QUADRO) {
                val n = rec.read(quadro, lidos, QUADRO - lidos)
                if (n < 0) {
                    rec = reabrirGravador(n) ?: run { falhar("O microfone parou de responder."); return }
                    lidos = 0; estaveis = 0
                    continue
                }
                lidos += n
            }
            if (lidos == QUADRO) {
                motor.capturar(quadro)
                if (++estaveis == QUADROS_ESTAVEIS) reaberturasMic.set(0)
            }
        }
    }

    private fun laçoDoAltoFalante() {
        Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_AUDIO)
        var track = toca ?: return
        val saida = ShortArray(QUADRO)
        var estaveis = 0
        while (rodando) {
            motor.reproduzir(saida)
            val n = track.write(saida, 0, QUADRO) // bloqueia até haver espaço: é o relógio de 20 ms
            if (n < 0) {
                track = reabrirTrilha(n) ?: run { falhar("O alto-falante parou de responder."); return }
                estaveis = 0
            } else if (++estaveis == QUADROS_ESTAVEIS) reaberturasSaida.set(0)
        }
    }

    private fun falhar(msg: String) { if (rodando) { rodando = false; aoFalhar(msg) } }

    private fun ligarEfeitos(sessao: Int) {
        val lista = ArrayList<android.media.audiofx.AudioEffect>()
        runCatching {
            if (AutomaticGainControl.isAvailable()) AutomaticGainControl.create(sessao)?.also { it.enabled = opcoes.ganhoAutomatico; lista += it }
            if (NoiseSuppressor.isAvailable()) NoiseSuppressor.create(sessao)?.also { it.enabled = suprimirPeloSistema(); lista += it }
            if (AcousticEchoCanceler.isAvailable()) AcousticEchoCanceler.create(sessao)?.also { it.enabled = opcoes.cancelamentoDeEco; lista += it }
        }
        efeitos = lista
    }

    /** O NoiseSuppressor do sistema só vale quando o RNNoise não está lá (senão o ruído seria suprimido duas vezes). */
    private fun suprimirPeloSistema() = opcoes.supressaoDeRuido && !motor.supressorProprio

    override fun aplicarOpcoes(opcoes: OpcoesDeVoz) {
        this.opcoes = opcoes
        for (e in efeitos) runCatching {
            e.enabled = when (e) {
                is AutomaticGainControl -> opcoes.ganhoAutomatico
                is NoiseSuppressor -> suprimirPeloSistema()
                is AcousticEchoCanceler -> opcoes.cancelamentoDeEco
                else -> e.enabled
            }
        }
    }

    override fun definirRota(rota: RotaDeAudio) { this.rota = rota; aplicarRota() }

    /**
     * Alto-falante: força a saída do viva-voz. Fone: libera para o sistema usar fone/Bluetooth ou o receptor. Nunca lança:
     * se o Android recusar a rota, a chamada segue com a que o sistema escolheu.
     */
    @Suppress("DEPRECATION")
    private fun aplicarRota() {
        try {
            if (Build.VERSION.SDK_INT >= 31) {
                if (rota == RotaDeAudio.AltoFalante) {
                    val alto = am.availableCommunicationDevices.firstOrNull { it.type == AudioDeviceInfo.TYPE_BUILTIN_SPEAKER }
                    if (alto == null || !am.setCommunicationDevice(alto)) am.isSpeakerphoneOn = true
                } else {
                    val fone = am.availableCommunicationDevices.firstOrNull { it.type in TIPOS_DE_FONE } ?: am.availableCommunicationDevices.firstOrNull { it.type == AudioDeviceInfo.TYPE_BUILTIN_EARPIECE }
                    if (fone == null || !am.setCommunicationDevice(fone)) am.clearCommunicationDevice()
                }
            } else {
                am.isSpeakerphoneOn = rota == RotaDeAudio.AltoFalante
            }
        } catch (e: Exception) {
            Log.w(TAG, "rota de áudio recusada: ${e.message}")
        }
    }

    /**
     * Foco de áudio: ligação, outro app de mídia ou o próprio sistema o tiram e devolvem. Numa chamada de voz isso NUNCA
     * encerra nada (a captura e a reprodução seguem; se o sistema calar o microfone, o `read` volta a fluir quando o foco volta).
     */
    private val aoMudarFoco = AudioManager.OnAudioFocusChangeListener { mudanca ->
        Log.i(TAG, "foco de áudio: ${nomeDoFoco(mudanca)}")
    }

    private fun nomeDoFoco(m: Int) = when (m) {
        AudioManager.AUDIOFOCUS_GAIN -> "ganhou"
        AudioManager.AUDIOFOCUS_LOSS -> "perdeu (permanente)"
        AudioManager.AUDIOFOCUS_LOSS_TRANSIENT -> "perdeu (temporário)"
        AudioManager.AUDIOFOCUS_LOSS_TRANSIENT_CAN_DUCK -> "perdeu (pode abaixar)"
        else -> m.toString()
    }

    private fun pedirFoco() {
        val req = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
            .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION).setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
            .setOnAudioFocusChangeListener(aoMudarFoco).build()
        am.requestAudioFocus(req); foco = req
    }

    @Suppress("DEPRECATION")
    override fun parar() {
        rodando = false
        threadMic?.takeIf { it !== Thread.currentThread() }?.join(300)
        threadSaida?.takeIf { it !== Thread.currentThread() }?.join(300)
        threadMic = null; threadSaida = null
        synchronized(travaDeReabertura) {
            for (e in efeitos) runCatching { e.release() }
            efeitos = emptyList()
            gravador?.let { runCatching { it.stop() }; it.release() }; gravador = null
            toca?.let { runCatching { it.stop() }; it.release() }; toca = null
        }
        runCatching {
            if (Build.VERSION.SDK_INT >= 31) am.clearCommunicationDevice() else am.isSpeakerphoneOn = false
            am.mode = modoAnterior
            foco?.let { am.abandonAudioFocusRequest(it) }
        }
        foco = null
    }

    private companion object {
        const val TAG = "TelacastVoz"
        const val MAX_REABERTURAS = 3
        const val PAUSA_DE_REABERTURA_MS = 250L
        /** Depois de ~1 s de quadros bons seguidos a fila de reaberturas zera. */
        const val QUADROS_ESTAVEIS = 50
    }
}
