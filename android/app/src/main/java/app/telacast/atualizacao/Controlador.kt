package app.telacast.atualizacao

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import java.io.File
import java.security.MessageDigest

/** Rede da atualização (OkHttp no app; falso nos testes). Falhas de rede viram `null`/[ResultadoDoDownload.Falhou] (o app nunca mostra erro de consulta). */
interface ApiDeAtualizacao {
    suspend fun manifesto(plataforma: String): ManifestoDeAtualizacao?
    /** Baixa `/update/file/<sha256>` para [destino], calculando o SHA-256 no caminho. */
    suspend fun baixar(m: ManifestoDeAtualizacao, destino: File, progresso: (Long) -> Unit): ResultadoDoDownload
}

sealed interface ResultadoDoDownload {
    data class Baixado(val tamanho: Long, val sha256: String) : ResultadoDoDownload
    data class Falhou(val motivo: String) : ResultadoDoDownload
}

/** Instala o APK (Android: `PackageInstaller`). Fica atrás de interface para os testes. */
interface InstaladorDeApk {
    /** Pode instalar pacotes? (Android 8+: "instalar apps desconhecidos" liberado para o Telacast.) */
    fun podeInstalar(): Boolean
    /** Confere que o APK é do mesmo pacote e de versão maior (`null` = ok, senão o motivo). */
    fun validar(apk: File, versaoNome: String): String?
    /** Abre a instalação (silenciosa se o Android deixar; senão o Android pede a confirmação). `null` = começou, senão o erro. */
    fun instalar(apk: File): String?
    /** Abre a tela do Android onde a pessoa libera "instalar apps desconhecidos" para o Telacast. */
    fun abrirPermissao()
}

object SemInstalador : InstaladorDeApk {
    override fun podeInstalar() = false
    override fun validar(apk: File, versaoNome: String): String? = "Este ambiente não instala aplicativos."
    override fun instalar(apk: File): String? = "Este ambiente não instala aplicativos."
    override fun abrirPermissao() = Unit
}

enum class FaseDaAtualizacao { Nenhuma, Disponivel, Baixando, Pronta, PrecisaDePermissao, Instalando, Erro }

data class EstadoDaAtualizacao(
    val fase: FaseDaAtualizacao = FaseDaAtualizacao.Nenhuma,
    val manifesto: ManifestoDeAtualizacao? = null,
    /** 0 a 100 enquanto baixa. */
    val progresso: Int = 0,
    val mensagem: String? = null,
    /** A folha "O que mudou" ainda não foi mostrada (nem dispensada) para esta versão. */
    val folhaPendente: Boolean = false,
) {
    val temNovidade: Boolean get() = manifesto != null && fase != FaseDaAtualizacao.Nenhuma
}

/**
 * Atualização pelo próprio app: consulta o manifesto a cada abertura (e ao voltar para a frente depois de 10 min), mostra a faixa
 * "Nova versão X disponível" até atualizar (e a folha "O que mudou" uma vez por versão), baixa com progresso, verifica tamanho, SHA-256 e a
 * assinatura Ed25519 ANTES de instalar, e só instala versão maior do mesmo pacote. Consulta com falha de rede é silenciosa.
 */
class ControladorDeAtualizacao(
    private val escopo: CoroutineScope,
    private val api: ApiDeAtualizacao,
    private val verificador: VerificadorDeAtualizacao,
    private val instalador: InstaladorDeApk,
    private val versaoAtual: String,
    private val pasta: File,
    private val ultimaVersaoVista: () -> String,
    private val marcarVersaoVista: (String) -> Unit,
    private val relogio: () -> Long = System::currentTimeMillis,
    private val intervaloMinimoMs: Long = 10 * 60_000,
) {
    private val _estado = MutableStateFlow(EstadoDaAtualizacao())
    val estado: StateFlow<EstadoDaAtualizacao> = _estado.asStateFlow()
    private var ultimaConsulta: Long? = null
    private var consultando = false

    val ligada: Boolean get() = verificador.ligada

    private fun arquivoDe(m: ManifestoDeAtualizacao) = File(pasta, "telacast-${m.version}.apk")

    /** Consulta o servidor. `forcar` ignora o intervalo mínimo (a abertura do app). */
    fun verificar(forcar: Boolean = false) {
        if (!verificador.ligada) return
        val agora = relogio()
        val u = ultimaConsulta
        if (consultando || (!forcar && u != null && agora - u < intervaloMinimoMs)) return
        if (_estado.value.fase == FaseDaAtualizacao.Baixando || _estado.value.fase == FaseDaAtualizacao.Instalando) return
        consultando = true; ultimaConsulta = agora
        escopo.launch {
            try {
                val m = api.manifesto(PLATAFORMA_ANDROID) ?: return@launch // sem rede ou sem versão publicada: silêncio
                when (val r = verificador.conferirManifesto(m, versaoAtual)) {
                    VerificadorDeAtualizacao.Resultado.Ok -> {
                        val ja = _estado.value.manifesto
                        if (ja?.sha256 != m.sha256) {
                            val vista = ultimaVersaoVista() == m.version
                            val pronta = arquivoVerificado(m)
                            _estado.value = EstadoDaAtualizacao(if (pronta) FaseDaAtualizacao.Pronta else FaseDaAtualizacao.Disponivel, m, folhaPendente = !vista)
                        }
                    }
                    VerificadorDeAtualizacao.Resultado.SemNovidade -> if (_estado.value.fase == FaseDaAtualizacao.Nenhuma) Unit else _estado.value = EstadoDaAtualizacao()
                    is VerificadorDeAtualizacao.Resultado.Recusado -> Unit // manifesto estranho: não mostra nada (e não instala nada)
                }
            } finally { consultando = false }
        }
    }

    /** O APK desta versão já está na pasta (baixado em segundo plano) e confere? */
    private fun arquivoVerificado(m: ManifestoDeAtualizacao): Boolean {
        val f = arquivoDe(m)
        if (!f.exists() || f.length() != m.size) return false
        val h = runCatching { sha256De(f) }.getOrNull() ?: return false
        return verificador.conferirArquivo(m, f.length(), h) == VerificadorDeAtualizacao.Resultado.Ok
    }

    /** Para o trabalho diário (WorkManager): consulta, baixa e verifica sem instalar. Devolve a versão pronta, ou `null` (sem novidade, sem rede ou falha). */
    suspend fun baixarEmSegundoPlano(): ManifestoDeAtualizacao? {
        if (!verificador.ligada) return null
        val m = api.manifesto(PLATAFORMA_ANDROID) ?: return null
        if (verificador.conferirManifesto(m, versaoAtual) != VerificadorDeAtualizacao.Resultado.Ok) return null
        pasta.mkdirs()
        val destino = arquivoDe(m)
        if (arquivoVerificado(m)) return m
        val parcial = File(pasta, destino.name + ".parte")
        val r = api.baixar(m, parcial) { }
        if (r !is ResultadoDoDownload.Baixado || verificador.conferirArquivo(m, r.tamanho, r.sha256) != VerificadorDeAtualizacao.Resultado.Ok || !parcial.renameTo(destino)) { parcial.delete(); return null }
        return m
    }

    /** "Agora não" na folha da permissão: volta a oferecer a instalação. */
    fun cancelarPermissao() { if (_estado.value.fase == FaseDaAtualizacao.PrecisaDePermissao) _estado.value = _estado.value.copy(fase = FaseDaAtualizacao.Pronta) }

    /** Voltou das configurações do Android: se a permissão foi dada, continua a instalação. */
    fun retomar() { if (_estado.value.fase == FaseDaAtualizacao.PrecisaDePermissao && instalador.podeInstalar()) instalar() }

    /** "Depois": esconde a folha, mantém a faixa. */
    fun dispensarFolha() {
        val m = _estado.value.manifesto ?: return
        marcarVersaoVista(m.version)
        _estado.value = _estado.value.copy(folhaPendente = false)
    }

    /** "Atualizar": baixa (se preciso), verifica e abre a instalação. */
    fun atualizar() {
        val s = _estado.value
        val m = s.manifesto ?: return
        if (s.fase == FaseDaAtualizacao.Baixando || s.fase == FaseDaAtualizacao.Instalando) return
        marcarVersaoVista(m.version)
        _estado.value = s.copy(fase = FaseDaAtualizacao.Baixando, progresso = 0, mensagem = null, folhaPendente = false)
        escopo.launch {
            pasta.mkdirs()
            val destino = arquivoDe(m)
            if (!arquivoVerificado(m)) {
                destino.delete()
                val parcial = File(pasta, destino.name + ".parte")
                var ultimo = -1
                when (val r = api.baixar(m, parcial) { lidos -> val p = (lidos * 100 / m.size).toInt().coerceIn(0, 100); if (p != ultimo) { ultimo = p; _estado.value = _estado.value.copy(progresso = p) } }) {
                    is ResultadoDoDownload.Falhou -> { parcial.delete(); falhar("Não consegui baixar a atualização: ${r.motivo}"); return@launch }
                    is ResultadoDoDownload.Baixado -> {
                        val c = verificador.conferirArquivo(m, r.tamanho, r.sha256)
                        if (c is VerificadorDeAtualizacao.Resultado.Recusado) { parcial.delete(); falhar(c.motivo); return@launch }
                        if (!parcial.renameTo(destino)) { parcial.delete(); falhar("Não consegui guardar a atualização."); return@launch }
                    }
                }
            }
            instalarPronta(m, destino)
        }
    }

    /** Toque em "Instalar" (APK já baixado e verificado, ou o que o WorkManager deixou pronto). */
    fun instalar() {
        val m = _estado.value.manifesto ?: return
        val f = arquivoDe(m)
        escopo.launch { if (arquivoVerificado(m)) instalarPronta(m, f) else atualizar() }
    }

    private fun instalarPronta(m: ManifestoDeAtualizacao, f: File) {
        val erro = instalador.validar(f, m.version)
        if (erro != null) { f.delete(); falhar(erro); return }
        if (!instalador.podeInstalar()) { _estado.value = _estado.value.copy(fase = FaseDaAtualizacao.PrecisaDePermissao, mensagem = null); return }
        _estado.value = _estado.value.copy(fase = FaseDaAtualizacao.Instalando, progresso = 100)
        val e = instalador.instalar(f)
        if (e != null) falhar(e)
    }

    fun abrirPermissao() { instalador.abrirPermissao() }

    /** O Android respondeu à sessão de instalação (status de `PackageInstaller`). */
    fun aoResultadoDaInstalacao(sucesso: Boolean, mensagem: String?) {
        if (sucesso) return // o Android reinicia o app com a versão nova
        falhar(mensagem ?: "O Android não instalou a atualização.")
    }

    private fun falhar(msg: String) { _estado.value = _estado.value.copy(fase = FaseDaAtualizacao.Erro, mensagem = msg) }

    companion object {
        fun sha256De(f: File): String {
            val d = MessageDigest.getInstance("SHA-256")
            f.inputStream().use { i -> val b = ByteArray(64 * 1024); while (true) { val n = i.read(b); if (n < 0) break; d.update(b, 0, n) } }
            return bytesParaHex(d.digest())
        }
    }
}
