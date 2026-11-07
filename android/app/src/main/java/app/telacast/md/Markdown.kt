package app.telacast.md

/**
 * Markdown do chat (subconjunto do que o Discord entende), porte fiel de `md/src/lib.rs` do desktop.
 * O servidor guarda o texto cru; isto só o interpreta para mostrar. Puro Kotlin, sem dependências.
 *
 * Blocos: parágrafo, título (`#`, `##`, `###`), subtexto (`-# `), citação (`> ` e `>>> `), lista (`- `, `* `, `1. `)
 * e bloco de código (três crases). Dentro dos blocos: negrito, itálico, sublinhado, tachado, código, spoiler, link
 * (`[texto](url)` e URL solta), menções (`<@id>`, `<#id>`), carimbo de tempo (`<t:unix:estilo>`) e `@everyone`/`@here`.
 */
sealed interface Inline {
    data class Texto(val s: String) : Inline
    data class Negrito(val k: List<Inline>) : Inline
    data class Italico(val k: List<Inline>) : Inline
    data class Sublinhado(val k: List<Inline>) : Inline
    data class Tachado(val k: List<Inline>) : Inline
    data class Codigo(val s: String) : Inline
    data class Spoiler(val id: Int, val k: List<Inline>) : Inline
    data class Link(val k: List<Inline>, val url: String) : Inline
    data class Usuario(val id: Long) : Inline
    data class Canal(val id: Long) : Inline
    data class Tempo(val unix: Long, val estilo: Char) : Inline
    /** `@everyone` ou `@here`. */
    data class Todos(val nome: String) : Inline
}

sealed interface Bloco {
    data class Paragrafo(val k: List<Inline>) : Bloco
    data class Titulo(val nivel: Int, val k: List<Inline>) : Bloco
    data class Subtexto(val k: List<Inline>) : Bloco
    data class Citacao(val blocos: List<Bloco>) : Bloco
    /** `inicio`: número do primeiro item numa lista numerada. */
    data class Lista(val inicio: Long?, val itens: List<ItemDeLista>) : Bloco
    data class Codigo(val lang: String, val texto: String) : Bloco
}

data class ItemDeLista(val profundidade: Int, val k: List<Inline>)

object Markdown {
    private const val PROFUNDIDADE_MAX = 10
    private val ESQUEMAS = listOf("http://", "https://", "mailto:", "tel:")

    /** Interpreta uma mensagem inteira. */
    fun parse(texto: String): List<Bloco> = blocos(texto, true, intArrayOf(0))

    private fun ehLinguagem(s: String) =
        s.isNotEmpty() && s.length <= 32 && s.all { (it in 'a'..'z') || (it in 'A'..'Z') || (it in '0'..'9') || it in "_+-.#" }

    private fun blocos(texto: String, permitirCitacao: Boolean, ids: IntArray): List<Bloco> {
        val linhas = texto.split('\n')
        val out = mutableListOf<Bloco>()
        val para = mutableListOf<String>()
        var i = 0

        fun descarregar() {
            if (para.isNotEmpty()) {
                out += Bloco.Paragrafo(inline(para.joinToString("\n"), 0, ids))
                para.clear()
            }
        }

        while (i < linhas.size) {
            val linha = linhas[i]
            val aparada = linha.trimStart(' ')

            // bloco de código: ``` ... ``` (pode abrir e fechar na mesma linha)
            if (aparada.startsWith("```")) {
                val restoLinhas = linhas.subList(i, linhas.size).joinToString("\n")
                val depois = restoLinhas.trimStart(' ').substring(3)
                val fim = depois.indexOf("```")
                if (fim >= 0) {
                    val corpo = depois.substring(0, fim)
                    val nl = corpo.indexOf('\n')
                    var lang = ""
                    var codigo = corpo
                    if (nl >= 0 && ehLinguagem(corpo.substring(0, nl).trim())) {
                        lang = corpo.substring(0, nl).trim()
                        codigo = corpo.substring(nl + 1)
                    }
                    codigo = codigo.trim('\n')
                    if (codigo.isNotEmpty()) {
                        descarregar()
                        out += Bloco.Codigo(lang, codigo)
                        val consumidas = depois.substring(0, fim + 3).count { it == '\n' }
                        val cauda = depois.substring(fim + 3)
                        i += consumidas
                        if (cauda.isNotBlank()) {
                            // texto depois do fechamento, na mesma linha, vira um parágrafo
                            val linhaCauda = cauda.split('\n').first()
                            if (linhaCauda.isNotBlank()) out += Bloco.Paragrafo(inline(linhaCauda.trimStart(), 0, ids))
                        }
                        i += 1
                        continue
                    }
                }
            }

            // >>> consome o resto da mensagem
            if (permitirCitacao && aparada.startsWith(">>> ")) {
                descarregar()
                val resto = listOf(aparada.substring(4)) + linhas.subList(i + 1, linhas.size)
                out += Bloco.Citacao(blocos(resto.joinToString("\n"), false, ids))
                break
            }
            // > citação (precisa de espaço depois do >)
            if (permitirCitacao && aparada.startsWith("> ")) {
                descarregar()
                val interno = mutableListOf<String>()
                while (i < linhas.size) {
                    val t = linhas[i].trimStart(' ')
                    if (t.startsWith("> ")) { interno += t.substring(2); i++ } else break
                }
                out += Bloco.Citacao(blocos(interno.joinToString("\n"), false, ids))
                continue
            }
            // título: 1 a 3 # e espaço
            val hashes = aparada.takeWhile { it == '#' }.length
            if (hashes in 1..3 && aparada.substring(hashes).startsWith(" ") && aparada.substring(hashes).isNotBlank()) {
                descarregar()
                val corpo = aparada.substring(hashes).trim().trimEnd('#').trimEnd()
                out += Bloco.Titulo(hashes, inline(corpo, 0, ids))
                i++
                continue
            }
            // subtexto
            if (aparada.startsWith("-# ")) {
                val r = aparada.substring(3)
                if (r.isNotBlank()) {
                    descarregar()
                    out += Bloco.Subtexto(inline(r.trim(), 0, ids))
                    i++
                    continue
                }
            }
            // lista
            if (marcadorDeLista(aparada) != null) {
                descarregar()
                val itens = mutableListOf<ItemDeLista>()
                var inicio: Long? = null
                while (i < linhas.size) {
                    val l = linhas[i]
                    val t = l.trimStart(' ')
                    val (num, corpo) = marcadorDeLista(t) ?: break
                    if (itens.isEmpty()) inicio = num
                    val prof = minOf((l.length - t.length) / 2, PROFUNDIDADE_MAX - 1)
                    itens += ItemDeLista(prof, inline(corpo, 0, ids))
                    i++
                }
                out += Bloco.Lista(inicio, itens)
                continue
            }
            para += linha
            i++
        }
        descarregar()
        // parágrafos totalmente vazios (linhas em branco soltas) não viram bloco
        out.removeAll { b -> b is Bloco.Paragrafo && b.k.all { it is Inline.Texto && it.s.isBlank() } }
        return out
    }

    /** `- x`, `* x`, `1. x`: devolve o número (se numerada) e o texto do item. */
    private fun marcadorDeLista(t: String): Pair<Long?, String>? {
        val r = if (t.startsWith("- ")) t.substring(2) else if (t.startsWith("* ")) t.substring(2) else null
        if (r != null && r.isNotBlank() && !t.startsWith("-# ")) return null to r
        val digitos = t.takeWhile { it in '0'..'9' }.length
        if (digitos in 1..9 && t.substring(digitos).startsWith(". ")) {
            val n = t.substring(0, digitos).toLongOrNull() ?: return null
            val resto = t.substring(digitos + 2)
            if (resto.isNotBlank()) return n to resto
        }
        return null
    }

    private fun MutableList<Inline>.empurrarTexto(s: String) {
        if (s.isEmpty()) return
        val ultimo = lastOrNull()
        if (ultimo is Inline.Texto) this[lastIndex] = Inline.Texto(ultimo.s + s) else add(Inline.Texto(s))
    }

    private fun acharFechamento(s: String, delim: String, minInterno: Int): Int? {
        val de = delim.length + minInterno
        if (de > s.length) return null
        val p = s.indexOf(delim, de)
        return if (p < 0) null else p
    }

    /** Posição do `*` que fecha um itálico aberto em `rest[0]`. */
    private fun acharEstrelaFecha(rest: String): Int? {
        var de = 1
        while (true) {
            val p = rest.indexOf('*', de)
            if (p < 0) return null
            val sepAntes = p > 0 && (rest[p - 1] == ' ' || rest[p - 1] == '\n')
            val estrelaAntes = p > 0 && rest[p - 1] == '*'
            val estrelaDepois = p + 1 < rest.length && rest[p + 1] == '*'
            if (p > 1 && !sepAntes && !estrelaAntes && !estrelaDepois) return p
            de = p + 1
        }
    }

    private fun Char.alfanumerico() = isLetterOrDigit()

    private fun inline(s: String, profundidade: Int, ids: IntArray): List<Inline> {
        val out = mutableListOf<Inline>()
        if (profundidade >= PROFUNDIDADE_MAX) { out.empurrarTexto(s); return out }
        var i = 0
        while (i < s.length) {
            val rest = s.substring(i)
            val c = rest[0]
            val prev: Char? = if (i > 0) s[i - 1] else null

            // escape
            if (c == '\\' && rest.length > 1) {
                val cp = rest.codePointAt(1)
                if (!Character.isLetterOrDigit(cp) && !Character.isWhitespace(cp)) {
                    val n = String(Character.toChars(cp))
                    out.empurrarTexto(n)
                    i += 1 + n.length
                    continue
                }
            }
            // spoiler
            if (rest.startsWith("||")) {
                val fim = acharFechamento(rest, "||", 1)
                if (fim != null) {
                    val id = ids[0]
                    ids[0] += 1
                    out += Inline.Spoiler(id, inline(rest.substring(2, fim), profundidade + 1, ids))
                    i += fim + 2
                    continue
                }
            }
            // código em linha: crases simétricas
            if (c == '`') {
                val n = rest.takeWhile { it == '`' }.length
                if (n < 3) {
                    val cerca = rest.substring(0, n)
                    val e = rest.indexOf(cerca, n)
                    // o fechamento não pode fazer parte de uma sequência maior de crases
                    if (e >= 0 && e > n && rest.getOrNull(e + n) != '`') {
                        var corpo = rest.substring(n, e)
                        corpo = corpo.removePrefix(" ").removeSuffix(" ")
                        out += Inline.Codigo(corpo)
                        i += e + n
                        continue
                    }
                }
            }
            // negrito + itálico, negrito
            if (rest.startsWith("***")) {
                val fim = acharFechamento(rest, "***", 1)
                if (fim != null) {
                    out += Inline.Negrito(listOf(Inline.Italico(inline(rest.substring(3, fim), profundidade + 1, ids))))
                    i += fim + 3
                    continue
                }
            }
            if (rest.startsWith("**")) {
                val fim = acharFechamento(rest, "**", 1)
                if (fim != null && rest.getOrNull(fim + 2) != '*') {
                    out += Inline.Negrito(inline(rest.substring(2, fim), profundidade + 1, ids))
                    i += fim + 2
                    continue
                }
            }
            // sublinhado (não é negrito)
            if (rest.startsWith("__")) {
                val fim = acharFechamento(rest, "__", 1)
                if (fim != null && rest.getOrNull(fim + 2) != '_') {
                    out += Inline.Sublinhado(inline(rest.substring(2, fim), profundidade + 1, ids))
                    i += fim + 2
                    continue
                }
            }
            // tachado
            if (rest.startsWith("~~")) {
                val fim = acharFechamento(rest, "~~", 1)
                if (fim != null) {
                    out += Inline.Tachado(inline(rest.substring(2, fim), profundidade + 1, ids))
                    i += fim + 2
                    continue
                }
            }
            // itálico: *x* (sem espaço colado ao delimitador)
            if (c == '*' && rest.getOrNull(1).let { it != '*' && it != ' ' && it != '\n' }) {
                val p = acharEstrelaFecha(rest)
                if (p != null) {
                    out += Inline.Italico(inline(rest.substring(1, p), profundidade + 1, ids))
                    i += p + 1
                    continue
                }
            }
            if (c == '_' && !(prev != null && prev.alfanumerico()) && rest.getOrNull(1).let { it != '_' && it != ' ' && it != '\n' }) {
                var de = 1
                var achou = false
                while (true) {
                    val p = rest.indexOf('_', de)
                    if (p < 0) break
                    val depois = rest.getOrNull(p + 1)
                    val antes = if (p > 0) rest[p - 1] else null
                    if (p > 1 && antes != ' ' && antes != '\n' && antes != '_' && depois != '_' && !(depois != null && depois.alfanumerico())) {
                        out += Inline.Italico(inline(rest.substring(1, p), profundidade + 1, ids))
                        i += p + 1
                        achou = true
                        break
                    }
                    de = p + 1
                }
                if (achou) continue
            }
            // link mascarado [texto](url)
            if (c == '[') {
                val fecha = rest.indexOf("](")
                if (fecha >= 0) {
                    val parente = rest.indexOf(')', fecha + 2)
                    if (parente >= 0) {
                        val url = rest.substring(fecha + 2, parente)
                        if (ESQUEMAS.any { url.startsWith(it) } && url.none { it.isISOControl() || it == ' ' }) {
                            val rotulo = rest.substring(1, fecha)
                            if (rotulo.isNotEmpty() && !rotulo.contains('[')) {
                                out += Inline.Link(inline(rotulo, profundidade + 1, ids), url)
                                i += parente + 1
                                continue
                            }
                        }
                    }
                }
            }
            // URL solta
            if (rest.startsWith("http://") || rest.startsWith("https://")) {
                var fim = rest.indexOfFirst { it.isWhitespace() || it == '<' }
                if (fim < 0) fim = rest.length
                var url = rest.substring(0, fim)
                while (url.isNotEmpty() && (url.last() in ".,:;\"']!?" || (url.last() == ')' && !url.contains('(')))) url = url.dropLast(1)
                if (url.length > "https://".length) {
                    out += Inline.Link(listOf(Inline.Texto(url)), url)
                    i += url.length
                    continue
                }
            }
            // tokens entre ângulos
            if (c == '<') {
                val fim = rest.indexOf('>')
                if (fim >= 0) {
                    val tok = rest.substring(1, fim)
                    val no: Inline? = run {
                        val usuario = (if (tok.startsWith("@!")) tok.substring(2) else if (tok.startsWith("@")) tok.substring(1) else null)?.toLongOrNull()
                        val canal = if (tok.startsWith("#")) tok.substring(1).toLongOrNull() else null
                        when {
                            usuario != null -> Inline.Usuario(usuario)
                            canal != null -> Inline.Canal(canal)
                            tok.startsWith("t:") -> {
                                val t = tok.substring(2)
                                val dois = t.indexOf(':')
                                val num = if (dois >= 0) t.substring(0, dois) else t
                                val estilo = if (dois >= 0) t.substring(dois + 1) else "f"
                                val unix = num.toLongOrNull()
                                if (unix != null && estilo.length == 1 && estilo[0] in "tTdDfFsSR") Inline.Tempo(unix, estilo[0]) else null
                            }
                            else -> null
                        }
                    }
                    if (no != null) {
                        out += no
                        i += fim + 1
                        continue
                    }
                }
            }
            // @everyone / @here
            val todos = listOf("@everyone", "@here").firstOrNull { rest.startsWith(it) }
            if (todos != null && !(prev != null && prev.alfanumerico())) {
                out += Inline.Todos(todos)
                i += todos.length
                continue
            }
            // caractere comum (um par substituto UTF-16 conta como um só)
            val tam = if (Character.isHighSurrogate(c) && rest.length > 1) 2 else 1
            out.empurrarTexto(rest.substring(0, tam))
            i += tam
        }
        return out
    }
}
