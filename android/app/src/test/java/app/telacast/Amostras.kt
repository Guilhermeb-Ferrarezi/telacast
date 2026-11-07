package app.telacast

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import app.telacast.data.AppState
import app.telacast.proto.Channel
import app.telacast.proto.Dm
import app.telacast.proto.Friend
import app.telacast.proto.FriendStatus
import app.telacast.proto.Live
import app.telacast.proto.ServerInfo
import app.telacast.proto.Snapshot
import app.telacast.proto.Status
import app.telacast.proto.User
import app.telacast.proto.VoiceState

/** Dados falsos para desenhar as telas sem rede (testes de imagem do Paparazzi). */
object Amostras {
    val eu = User(1, "guilherme", true, Status.Online)
    val marina = User(2, "marina.souza", true, Status.Idle)
    val joao = User(3, "joao_pedro", true, Status.Dnd)
    val anaClara = User(4, "ana-clara", false, Status.Offline)
    val bruno = User(5, "bruno", true, Status.Online)
    val carla = User(6, "carla.dev", true, Status.Online)

    const val SV_TURMA = 100L
    const val CAT_TEXTO = 110L
    const val CAT_PROJETOS = 111L
    const val CAT_VOZ = 112L
    const val GERAL = 120L
    const val MEMES = 121L
    const val AVISOS = 122L
    const val ANDROID = 123L
    const val DESKTOP = 124L
    const val SALA = 125L
    const val JOGATINA = 126L
    const val DM_MARINA = 300L
    const val DM_JOAO = 301L
    const val DM_ANA = 302L
    const val FUTSAL_GERAL = 221L

    private fun canal(id: Long, nome: String, kind: Int, pai: Long?, pos: Int) =
        Channel(id, nome, kind, pai, pos, lastMessageId = 0, lastReadId = 0)

    val turma = ServerInfo(
        SV_TURMA, "Turma do Telacast", owner = eu.id, invite = "B4NZ-7TQK",
        channels = listOf(
            canal(CAT_TEXTO, "canais de texto", Channel.CATEGORIA, null, 0),
            canal(GERAL, "geral", Channel.TEXTO, CAT_TEXTO, 0),
            canal(MEMES, "memes-e-risadas", Channel.TEXTO, CAT_TEXTO, 1),
            canal(AVISOS, "avisos", Channel.TEXTO, CAT_TEXTO, 2),
            canal(CAT_PROJETOS, "projetos", Channel.CATEGORIA, null, 1),
            canal(ANDROID, "app-android", Channel.TEXTO, CAT_PROJETOS, 0),
            canal(DESKTOP, "app-desktop", Channel.TEXTO, CAT_PROJETOS, 1),
            canal(CAT_VOZ, "canais de voz", Channel.CATEGORIA, null, 2),
            canal(SALA, "sala de reunião", Channel.VOZ, CAT_VOZ, 0),
            canal(JOGATINA, "jogatina", Channel.VOZ, CAT_VOZ, 1),
        ),
        members = listOf(eu, marina, joao, anaClara, bruno, carla),
    )

    private fun servidor(id: Long, nome: String, dono: Long, canal: Long) = ServerInfo(
        id, nome, dono, "XXXX-0000",
        channels = listOf(canal(canal, "geral", Channel.TEXTO, null, 0)), members = listOf(eu, marina),
    )

    val servidores = listOf(
        turma,
        servidor(200, "Clube do Livro", marina.id, 220),
        servidor(201, "Time de Futsal", joao.id, FUTSAL_GERAL),
        servidor(202, "Ação Social", eu.id, 222),
    )

    val dms = listOf(
        Dm(DM_MARINA, marina, 50, 40, mentions = 1),
        Dm(DM_JOAO, joao, 0, 0),
        Dm(DM_ANA, anaClara, 10, 10),
    )

    val amigos = listOf(
        Friend(marina, FriendStatus.Friend),
        Friend(joao, FriendStatus.Friend),
        Friend(anaClara, FriendStatus.Friend),
        Friend(bruno, FriendStatus.Incoming),
        Friend(carla, FriendStatus.Outgoing),
    )

    val snapshot = Snapshot(
        me = eu, servers = servidores, friends = amigos, dms = dms,
        live = listOf(Live(GERAL, marina.id)).take(0),
        voice = listOf(VoiceState(marina.id, SALA, muted = false, deafened = false), VoiceState(joao.id, SALA, muted = true, deafened = false)),
    )

    /** Estado base: autenticado, com não lidos em `geral` (2 menções) e `memes`, e uma menção no servidor de futsal. */
    val estado = AppState(
        autenticado = true, eu = eu, snapshot = snapshot, voz = snapshot.voice,
        naoLidos = setOf(GERAL, MEMES, ANDROID, DM_MARINA, FUTSAL_GERAL),
        mencoes = mapOf(GERAL to 2, DM_MARINA to 1, FUTSAL_GERAL to 120),
    )
}

/** Dados falsos para as telas de conversa (B4/B5): mensagens, anexos e bitmaps gerados em código. */
object AmostrasDeConversa {
    val zona: java.time.ZoneId = java.time.ZoneId.of("America/Sao_Paulo")
    fun ms(dia: Int, h: Int, min: Int, s: Int = 0) = java.time.ZonedDateTime.of(2026, 11, dia, h, min, s, 0, zona).toInstant().toEpochMilli()
    val agora = ms(7, 18, 0)

    private val eu = Amostras.eu
    private val marina = Amostras.marina
    private val joao = Amostras.joao
    private const val CANAL = Amostras.AVISOS

    fun m(
        id: Long, autor: app.telacast.proto.User, ts: Long, texto: String, edited: Boolean = false,
        reply: app.telacast.proto.Reply? = null, reactions: List<app.telacast.proto.Reaction> = emptyList(), pinned: Boolean = false,
        kind: Int = 0, anexos: List<app.telacast.proto.Attachment> = emptyList(), nonce: Long = 0,
    ) = app.telacast.proto.Message(id, CANAL, autor.id, autor.name, texto, ts, edited, reply, nonce, reactions, kind, pinned, anexos)

    fun r(emoji: String, n: Int, eu: Boolean = false) = app.telacast.proto.Reaction(emoji, n, eu)

    // ---------------------------------------------------------------- conversas
    val conversaBasica = listOf(
        m(1001, marina, ms(6, 21, 40), "Pessoal, alguém já viu o **relatório** de ontem?"),
        m(1002, marina, ms(6, 21, 41), "Segue o resumo: tudo certo com a entrega do app."),
        m(1003, joao, ms(6, 21, 44), "Vi sim! Ficou ótimo, só falta o ajuste final.", reply = app.telacast.proto.Reply(1001, "marina.souza", "Pessoal, alguém já viu o relatório de ontem?")),
        m(1004, eu, ms(7, 9, 15), "Bom dia, pessoal! Fechei a revisão e subi as correções.", edited = true, pinned = true,
            reactions = listOf(r("👍", 3, true), r("🎉", 1))),
        m(1005, eu, ms(7, 9, 16), "Se alguém quiser rever, o canal #avisos tem o resumo."),
        m(1006, marina, ms(7, 9, 30), "Perfeito! Obrigada, <@1>.", reactions = listOf(r("❤️", 2))),
        m(1007, joao, ms(7, 17, 52), "Alguém na reunião das 18h?"),
    )

    val conversaMarkdown = listOf(
        m(2001, marina, ms(7, 10, 0), "# Pauta da reunião\n1. Revisão do app\n2. Demo para a turma\n-# enviada às 10h"),
        m(2002, joao, ms(7, 10, 2), "Texto com **negrito**, *itálico*, __sublinhado__, ~~tachado~~ e `código em linha`.\nVeja https://exemplo.com/guia e [o manual](https://exemplo.com/manual)."),
        m(2003, marina, ms(7, 10, 5), "> citação importante\ndepois dela, um spoiler: ||o final do filme|| e outro já revelado: ||a resposta||"),
        m(2004, joao, ms(7, 10, 6), "```kotlin\nfun main() {\n    println(\"olá\")\n}\n```"),
        m(2005, marina, ms(7, 10, 20), "@everyone atenção! <@1> e <@3> (e <@999>) falem em <#122>."),
    )

    // ---------------------------------------------------------------- anexos
    const val ID_IMAGEM = 5001L
    const val ID_IMAGEM_VERTICAL = 5002L
    const val ID_ARQUIVO = 5003L
    const val ID_TEXTO = 5004L
    val anexoImagem = app.telacast.proto.Attachment(ID_IMAGEM, "praia.png", "image/png", 482_913, 1280, 720)
    val anexoVertical = app.telacast.proto.Attachment(ID_IMAGEM_VERTICAL, "retrato.jpg", "image/jpeg", 220_000, 600, 900)
    val anexoPdf = app.telacast.proto.Attachment(ID_ARQUIVO, "relatório-final.pdf", "application/octet-stream", 1_048_576, 0, 0)
    val anexoTexto = app.telacast.proto.Attachment(ID_TEXTO, "Main.kt", "application/octet-stream", 1_900, 0, 0)

    val textoDoAnexo = (1..14).joinToString("\n") { n ->
        when (n) { 1 -> "package app.telacast"; 2 -> ""; 3 -> "fun main() {"; 14 -> "}"; else -> "    println(\"linha $n\")" }
    }
    val previaDeTexto = app.telacast.arquivos.PreviaDeTexto(textoDoAnexo, 14, 0)

    val conversaAnexos = listOf(
        m(3001, marina, ms(7, 11, 0), "Fotos da viagem e o relatório:", anexos = listOf(anexoImagem, anexoPdf)),
        m(3002, joao, ms(7, 11, 5), "", anexos = listOf(anexoVertical)),
        m(3003, marina, ms(7, 11, 9), "Esse é o código que comentei:", anexos = listOf(anexoTexto)),
    )

    // ---------------------------------------------------------------- bitmaps gerados em código (sem gradiente)
    fun bitmap(w: Int, h: Int, fundo: Color, faixa: Color, sol: Color): ImageBitmap {
        val b = ImageBitmap(w, h)
        val c = androidx.compose.ui.graphics.Canvas(b)
        val p = androidx.compose.ui.graphics.Paint()
        p.color = fundo; c.drawRect(0f, 0f, w.toFloat(), h.toFloat(), p)
        p.color = faixa; c.drawRect(0f, h * 0.68f, w.toFloat(), h.toFloat(), p)
        p.color = sol; c.drawCircle(androidx.compose.ui.geometry.Offset(w * 0.72f, h * 0.30f), h * 0.12f, p)
        return b
    }

    val bmpPaisagem by lazy { bitmap(640, 360, Color(0xFF33404F), Color(0xFF2F4540), Color(0xFFE9B35E)) }
    val bmpRetrato by lazy { bitmap(300, 450, Color(0xFF3B3548), Color(0xFF4A3A36), Color(0xFFCFC6E6)) }
    val bmpMiniatura by lazy { bitmap(240, 240, Color(0xFF4B4530), Color(0xFF33404F), Color(0xFFEAD9A4)) }

    fun provedor() = app.telacast.ui.ProvedorDeAnexos(
        imagemEmCache = { when (it.id) { ID_IMAGEM -> bmpPaisagem; ID_IMAGEM_VERTICAL -> bmpRetrato; else -> null } },
        previaEmCache = { if (it.id == ID_TEXTO) previaDeTexto else null },
        miniaturaEmCache = { if (it.nome.endsWith(".png")) bmpMiniatura else null },
    )

    // ---------------------------------------------------------------- estado
    fun estado(msgs: List<app.telacast.proto.Message>, pendentes: List<app.telacast.data.Pendente> = emptyList(), bloqueados: List<app.telacast.proto.User> = emptyList(),
               marcador: Long? = null, destaque: app.telacast.data.Destaque? = null, temMais: Boolean = false, buscando: Boolean = false,
               digitando: Set<Long> = emptySet(), conexao: app.telacast.net.EstadoConexao = app.telacast.net.EstadoConexao.Conectado): app.telacast.data.AppState {
        val base = Amostras.estado
        return base.copy(
            snapshot = base.snapshot!!.copy(blocked = bloqueados),
            canalAberto = CANAL, carregados = setOf(CANAL), mensagens = mapOf(CANAL to msgs), pendentes = pendentes,
            naoLidos = base.naoLidos - CANAL, mencoes = base.mencoes - CANAL,
            marcadores = marcador?.let { mapOf(CANAL to it) } ?: emptyMap(), destaque = destaque,
            temMais = mapOf(CANAL to temMais), buscandoAntigas = if (buscando) setOf(CANAL) else emptySet(),
            digitando = if (digitando.isEmpty()) emptyMap() else mapOf(CANAL to digitando), conexao = conexao,
        )
    }
}
