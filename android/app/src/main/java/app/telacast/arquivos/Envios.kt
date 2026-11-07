package app.telacast.arquivos

import app.telacast.data.Formatacao
import app.telacast.proto.Attachment
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withPermit

sealed interface EstadoDoEnvio {
    data object NaFila : EstadoDoEnvio
    data class Enviando(val enviados: Long) : EstadoDoEnvio
    data class Pronto(val anexo: Attachment) : EstadoDoEnvio
    data class Falhou(val motivo: String) : EstadoDoEnvio
}

/** Um arquivo na bandeja de anexos do compositor. */
data class AnexoEmEnvio(val chave: Long, val canal: Long, val fonte: FonteDeArquivo, val estado: EstadoDoEnvio) {
    val progresso: Float get() = (estado as? EstadoDoEnvio.Enviando)?.let { if (fonte.tamanho > 0) it.enviados.toFloat() / fonte.tamanho else 0f }
        ?: if (estado is EstadoDoEnvio.Pronto) 1f else 0f
}

/**
 * Fila de envio de anexos: até [Arquivos.MAX_SIMULTANEOS] por vez, no máximo [Arquivos.MAX_ANEXOS] por mensagem
 * (por canal), progresso por arquivo, tentar de novo e remover. Puro Kotlin.
 */
class EnviadorDeAnexos(
    private val api: ApiDeArquivos,
    private val escopo: CoroutineScope,
    private val base: () -> String,
    private val token: () -> String?,
    simultaneos: Int = Arquivos.MAX_SIMULTANEOS,
) {
    private val vagas = Semaphore(simultaneos)
    private val _anexos = MutableStateFlow<List<AnexoEmEnvio>>(emptyList())
    val anexos: StateFlow<List<AnexoEmEnvio>> = _anexos.asStateFlow()
    private var proxima = 0L
    private val jobs = HashMap<Long, kotlinx.coroutines.Job>()

    fun doCanal(canal: Long) = _anexos.value.filter { it.canal == canal }

    /** Anexos prontos do canal, na ordem em que foram escolhidos. */
    fun prontos(canal: Long): List<Attachment> = doCanal(canal).mapNotNull { (it.estado as? EstadoDoEnvio.Pronto)?.anexo }

    fun emAndamento(canal: Long) = doCanal(canal).any { it.estado is EstadoDoEnvio.NaFila || it.estado is EstadoDoEnvio.Enviando }

    /** Devolve um aviso em português se algo foi recusado (limite de 10 anexos); o resto entra na fila. */
    @Synchronized
    fun adicionar(canal: Long, fontes: List<FonteDeArquivo>): String? {
        var aviso: String? = null
        for (f in fontes) {
            if (doCanal(canal).size >= Arquivos.MAX_ANEXOS) { aviso = "Você só pode anexar ${Arquivos.MAX_ANEXOS} arquivos por mensagem."; break }
            val chave = ++proxima
            if (f.tamanho > Arquivos.MAX_UPLOAD) {
                val msg = "O arquivo tem ${Formatacao.tamanho(f.tamanho)} e o limite é ${Formatacao.tamanho(Arquivos.MAX_UPLOAD)}."
                _anexos.update { it + AnexoEmEnvio(chave, canal, f, EstadoDoEnvio.Falhou(msg)) }
                continue
            }
            _anexos.update { it + AnexoEmEnvio(chave, canal, f, EstadoDoEnvio.NaFila) }
            iniciar(chave)
        }
        return aviso
    }

    private fun iniciar(chave: Long) {
        jobs[chave] = escopo.launch {
            vagas.withPermit {
                val item = _anexos.value.firstOrNull { it.chave == chave } ?: return@withPermit
                val t = token()
                if (t == null) { atualizar(chave, EstadoDoEnvio.Falhou("Sessão inválida. Entre de novo.")); return@withPermit }
                atualizar(chave, EstadoDoEnvio.Enviando(0))
                try {
                    val a = api.enviar(Arquivos.urlDeUpload(base(), t, item.canal, item.fonte.nome), item.fonte) { n ->
                        atualizar(chave, EstadoDoEnvio.Enviando(n))
                    }
                    atualizar(chave, EstadoDoEnvio.Pronto(a))
                } catch (e: CancellationException) {
                    throw e
                } catch (e: FalhaDeEnvio) {
                    atualizar(chave, EstadoDoEnvio.Falhou(e.message ?: "Falhou."))
                } catch (e: Exception) {
                    atualizar(chave, EstadoDoEnvio.Falhou("Não consegui enviar o arquivo."))
                }
            }
        }
    }

    private fun atualizar(chave: Long, e: EstadoDoEnvio) {
        _anexos.update { l -> l.map { if (it.chave == chave) it.copy(estado = e) else it } }
    }

    fun tentarDeNovo(chave: Long) {
        val item = _anexos.value.firstOrNull { it.chave == chave } ?: return
        if (item.estado !is EstadoDoEnvio.Falhou || item.fonte.tamanho > Arquivos.MAX_UPLOAD) return
        atualizar(chave, EstadoDoEnvio.NaFila)
        iniciar(chave)
    }

    fun remover(chave: Long) {
        jobs.remove(chave)?.cancel()
        _anexos.update { l -> l.filter { it.chave != chave } }
    }

    /** Tira da bandeja os anexos já enviados com a mensagem (os que falharam continuam para tentar de novo). */
    fun concluir(canal: Long) {
        _anexos.update { l -> l.filterNot { it.canal == canal && it.estado is EstadoDoEnvio.Pronto } }
    }
}
