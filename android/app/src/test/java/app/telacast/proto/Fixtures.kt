package app.telacast.proto

import java.io.File

/**
 * Acha `proto/fixtures` (o mesmo diretório que o teste Rust confere). O Gradle passa o caminho na propriedade
 * `telacast.fixtures` (android/../proto/fixtures); sem ela (rodando da IDE) sobe a partir da pasta atual.
 */
object Fixtures {
    val raiz: File by lazy {
        System.getProperty("telacast.fixtures")?.let { File(it) }?.takeIf { it.isDirectory }
            ?: generateSequence(File("").absoluteFile) { it.parentFile }
                .map { File(it, "proto/fixtures") }.first { it.isDirectory }
    }

    fun arquivos(sub: String): List<File> =
        File(raiz, sub).listFiles { f -> f.isFile && f.name.endsWith(".json") }!!.sortedBy { it.name }

    fun texto(sub: String, nome: String): String = File(File(raiz, sub), nome).readText()

    /** `Send.simples.json` -> `Send`; `Message.json` -> `Message`. */
    fun variante(f: File): String = f.name.substringBefore('.')
}
