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
    private var gravador: AudioRecord? = null
    private var toca: AudioTrack? = null
    private var efeitos: List<android.media.audiofx.AudioEffect> = emptyList()
    private var foco: AudioFocusRequest? = null
    private var modoAnterior = AudioManager.MODE_NORMAL
    @Volatile private var rodando = false
    private var threadMic: Thread? = null
    private var threadSaida: Thread? = null

    override fun iniciar(): String? {
        if (ctx.checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) return "Sem a permissão do microfone."
        try {
            val formatoEntrada = AudioFormat.Builder().setEncoding(AudioFormat.ENCODING_PCM_16BIT).setSampleRate(TAXA_DE_AMOSTRAGEM).setChannelMask(AudioFormat.CHANNEL_IN_MONO).build()
            val minEntrada = AudioRecord.getMinBufferSize(TAXA_DE_AMOSTRAGEM, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
            if (minEntrada <= 0) return "Este aparelho não grava áudio a 48 kHz."
            val rec = AudioRecord.Builder().setAudioSource(MediaRecorder.AudioSource.VOICE_COMMUNICATION).setAudioFormat(formatoEntrada)
                .setBufferSizeInBytes(maxOf(minEntrada, QUADRO * 2 * 4)).build()
            if (rec.state != AudioRecord.STATE_INITIALIZED) { rec.release(); return "Não consegui abrir o microfone (talvez outro app o esteja usando)." }

            val formatoSaida = AudioFormat.Builder().setEncoding(AudioFormat.ENCODING_PCM_16BIT).setSampleRate(TAXA_DE_AMOSTRAGEM).setChannelMask(AudioFormat.CHANNEL_OUT_MONO).build()
            val minSaida = AudioTrack.getMinBufferSize(TAXA_DE_AMOSTRAGEM, AudioFormat.CHANNEL_OUT_MONO, AudioFormat.ENCODING_PCM_16BIT)
            val track = AudioTrack.Builder()
                .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION).setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
                .setAudioFormat(formatoSaida).setBufferSizeInBytes(maxOf(minSaida, QUADRO * 2 * 3))
                .setTransferMode(AudioTrack.MODE_STREAM).setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY).build()
            if (track.state != AudioTrack.STATE_INITIALIZED) { rec.release(); track.release(); return "Não consegui abrir o alto-falante." }

            gravador = rec; toca = track
            modoAnterior = am.mode
            am.mode = AudioManager.MODE_IN_COMMUNICATION
            pedirFoco()
            aplicarRota()
            ligarEfeitos(rec.audioSessionId)
            // um pouco de silêncio antes de tocar evita estalo e "fome" do buffer no começo
            track.write(ShortArray(QUADRO * 2), 0, QUADRO * 2)
            rec.startRecording(); track.play()
            rodando = true
            threadMic = Thread({ laçoDoMicrofone(rec) }, "voz-microfone").also { it.start() }
            threadSaida = Thread({ laçoDoAltoFalante(track) }, "voz-alto-falante").also { it.start() }
            return null
        } catch (e: SecurityException) {
            parar(); return "O Android negou o acesso ao microfone."
        } catch (e: Exception) {
            parar(); return "Falha ao abrir o áudio: ${e.message ?: e.javaClass.simpleName}"
        }
    }

    private fun laçoDoMicrofone(rec: AudioRecord) {
        Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_AUDIO)
        val quadro = ShortArray(QUADRO)
        while (rodando) {
            var lidos = 0
            while (rodando && lidos < QUADRO) {
                val n = rec.read(quadro, lidos, QUADRO - lidos)
                if (n < 0) { if (rodando) falhar("O microfone parou de responder."); return }
                lidos += n
            }
            if (lidos == QUADRO) motor.capturar(quadro)
        }
    }

    private fun laçoDoAltoFalante(track: AudioTrack) {
        Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_AUDIO)
        val saida = ShortArray(QUADRO)
        while (rodando) {
            motor.reproduzir(saida)
            val n = track.write(saida, 0, QUADRO) // bloqueia até haver espaço: é o relógio de 20 ms
            if (n < 0) { if (rodando) falhar("O alto-falante parou de responder."); return }
        }
    }

    private fun falhar(msg: String) { if (rodando) { rodando = false; aoFalhar(msg) } }

    private fun ligarEfeitos(sessao: Int) {
        val lista = ArrayList<android.media.audiofx.AudioEffect>()
        if (AutomaticGainControl.isAvailable()) AutomaticGainControl.create(sessao)?.also { it.enabled = opcoes.ganhoAutomatico; lista += it }
        if (NoiseSuppressor.isAvailable()) NoiseSuppressor.create(sessao)?.also { it.enabled = opcoes.supressaoDeRuido; lista += it }
        if (AcousticEchoCanceler.isAvailable()) AcousticEchoCanceler.create(sessao)?.also { it.enabled = opcoes.cancelamentoDeEco; lista += it }
        efeitos = lista
    }

    override fun aplicarOpcoes(opcoes: OpcoesDeVoz) {
        this.opcoes = opcoes
        for (e in efeitos) e.enabled = when (e) {
            is AutomaticGainControl -> opcoes.ganhoAutomatico
            is NoiseSuppressor -> opcoes.supressaoDeRuido
            is AcousticEchoCanceler -> opcoes.cancelamentoDeEco
            else -> e.enabled
        }
    }

    override fun definirRota(rota: RotaDeAudio) { this.rota = rota; aplicarRota() }

    /** Alto-falante: força a saída do viva-voz. Fone: libera para o sistema usar fone/Bluetooth ou o receptor. */
    @Suppress("DEPRECATION")
    private fun aplicarRota() {
        if (Build.VERSION.SDK_INT >= 31) {
            if (rota == RotaDeAudio.AltoFalante) {
                val alto = am.availableCommunicationDevices.firstOrNull { it.type == AudioDeviceInfo.TYPE_BUILTIN_SPEAKER }
                if (alto != null) am.setCommunicationDevice(alto) else am.isSpeakerphoneOn = true
            } else {
                val fone = am.availableCommunicationDevices.firstOrNull { it.type in TIPOS_DE_FONE } ?: am.availableCommunicationDevices.firstOrNull { it.type == AudioDeviceInfo.TYPE_BUILTIN_EARPIECE }
                if (fone != null) am.setCommunicationDevice(fone) else am.clearCommunicationDevice()
            }
        } else {
            am.isSpeakerphoneOn = rota == RotaDeAudio.AltoFalante
        }
    }

    private fun pedirFoco() {
        val req = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT)
            .setAudioAttributes(AudioAttributes.Builder().setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION).setContentType(AudioAttributes.CONTENT_TYPE_SPEECH).build())
            .setOnAudioFocusChangeListener { }.build()
        am.requestAudioFocus(req); foco = req
    }

    @Suppress("DEPRECATION")
    override fun parar() {
        rodando = false
        threadMic?.join(300); threadSaida?.join(300)
        threadMic = null; threadSaida = null
        for (e in efeitos) runCatching { e.release() }
        efeitos = emptyList()
        gravador?.let { runCatching { it.stop() }; it.release() }; gravador = null
        toca?.let { runCatching { it.stop() }; it.release() }; toca = null
        runCatching {
            if (Build.VERSION.SDK_INT >= 31) am.clearCommunicationDevice() else am.isSpeakerphoneOn = false
            am.mode = modoAnterior
            foco?.let { am.abandonAudioFocusRequest(it) }
        }
        foco = null
    }
}
