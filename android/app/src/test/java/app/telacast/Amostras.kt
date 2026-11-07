package app.telacast

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
