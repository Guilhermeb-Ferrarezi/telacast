package app.telacast

import app.telacast.armazenamento.CofreDeSegredos
import app.telacast.armazenamento.Preferencias
import androidx.compose.ui.graphics.ImageBitmap
import app.telacast.arquivos.ApiDeArquivos
import app.telacast.arquivos.Arquivos
import app.telacast.arquivos.CarregadorDeImagens
import app.telacast.arquivos.CarregadorDePrevias
import app.telacast.arquivos.EnviadorDeAnexos
import app.telacast.arquivos.OkHttpArquivos
import app.telacast.data.ControladorDoCompositor
import app.telacast.data.Repository
import app.telacast.net.Gateway
import app.telacast.net.OkHttpTransporte
import app.telacast.net.Transporte
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

/**
 * Contêiner de injeção de dependência manual (sem Hilt, KSP nem kapt): vive no `Application`.
 * Puro Kotlin (sem `Context`), então os testes montam um com armazenamento em memória e transporte de mentira.
 */
class Container(
    val preferencias: Preferencias,
    val cofre: CofreDeSegredos,
    /** Endereço sugerido quando o usuário ainda não digitou nenhum (vem do `BuildConfig`). */
    val servidorPadrao: String = "",
    transporte: Transporte = OkHttpTransporte(),
    val escopo: CoroutineScope = CoroutineScope(SupervisorJob() + Dispatchers.Default),
    relogio: () -> Long = System::currentTimeMillis,
    val api: ApiDeArquivos = OkHttpArquivos(),
    /** Decodifica os bytes de uma imagem (no app: `BitmapFactory`); nos testes, qualquer coisa. */
    decodificarImagem: (ByteArray) -> ImageBitmap? = { null },
) {
    val gateway = Gateway(escopo, transporte, relogio)
    val repositorio = Repository(escopo, gateway, relogio)

    /** Endereço HTTP do servidor (anexos, `/upload`): o mesmo da conexão. */
    fun baseHttp(): String = Arquivos.urlHttp(preferencias.servidor.ifBlank { servidorPadrao })

    val enviador = EnviadorDeAnexos(api, escopo, ::baseHttp, token = { repositorio.token })
    val imagens = CarregadorDeImagens(api, escopo, decodificarImagem, { it.width.toLong() * it.height * 4 })
    val previas = CarregadorDePrevias(api)
    val compositor = ControladorDoCompositor(repositorio, preferencias)
}
