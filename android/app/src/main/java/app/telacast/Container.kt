package app.telacast

import app.telacast.armazenamento.CofreDeSegredos
import app.telacast.armazenamento.Preferencias
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
) {
    val gateway = Gateway(escopo, transporte, relogio)
    val repositorio = Repository(escopo, gateway, relogio)
}
