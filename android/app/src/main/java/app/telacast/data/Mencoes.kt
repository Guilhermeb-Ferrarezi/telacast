package app.telacast.data

import app.telacast.proto.User

/** Menções no texto cru: `<@id>`, `<@!id>`, `@everyone` e `@here` (igual a `mentions_me` do desktop). */
fun mencionaUsuario(conteudo: String, eu: Long): Boolean =
    conteudo.contains("<@$eu>") || conteudo.contains("<@!$eu>") || conteudo.contains("@everyone") || conteudo.contains("@here")

/**
 * Troca `@nome` (nome exato de alguém do canal, sem diferenciar maiúsculas) por `<@id>`, como o desktop faz
 * antes de enviar (o texto cru vai e volta do servidor; a tela mostra o nome atual).
 */
fun trocarMencoesPorToken(conteudo: String, usuarios: List<User>): String {
    val out = StringBuilder(conteudo.length)
    var resto = conteudo
    while (true) {
        val arroba = resto.indexOf('@')
        if (arroba < 0) break
        out.append(resto, 0, arroba)
        val antesOk = arroba == 0 || !resto[arroba - 1].isLetterOrDigit()
        val depois = resto.substring(arroba + 1)
        // o nome mais longo que casa, seguido de algo que não seja letra/número
        val achado = usuarios
            .filter { it.name.isNotEmpty() && depois.startsWith(it.name, ignoreCase = true) }
            .filter { depois.length == it.name.length || !depois[it.name.length].isLetterOrDigit() }
            .maxByOrNull { it.name.length }
        if (achado != null && antesOk) {
            out.append("<@").append(achado.id).append('>')
            resto = depois.substring(achado.name.length)
        } else {
            out.append('@')
            resto = depois
        }
    }
    out.append(resto)
    return out.toString()
}
