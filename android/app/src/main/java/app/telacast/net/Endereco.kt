package app.telacast.net

/**
 * Endereço digitado sem esquema. Um nome de domínio sem porta (`telecast.guilhermebf.dev`) é um servidor público: vale `https`/`wss`.
 * Endereço de rede local ou de teste (`10.0.2.2:8080`, IP, `localhost`, nome sem ponto, qualquer `:porta`) vale `http`/`ws`.
 * O release do app não aceita texto claro de qualquer jeito (o Android bloqueia); isso só evita que o amigo precise digitar `https://`.
 */
object Endereco {
    fun publico(servidor: String): Boolean {
        val s = servidor.trim().trimEnd('/').substringBefore('/')
        if (s.isEmpty() || ':' in s) return false
        if (s.equals("localhost", true) || s.endsWith(".local", true) || s.endsWith(".lan", true)) return false
        if (s.matches(Regex("""\d{1,3}(\.\d{1,3}){3}"""))) return false
        return '.' in s
    }
}
