package app.telacast.tema

import androidx.compose.ui.graphics.Color

/**
 * Tema "Grafite": cinzas em camadas, um único acento âmbar, filetes de 1 px, só escuro.
 * Os valores são os MESMOS de `app/src/theme.rs` (fonte da verdade no desktop). Mudou lá, muda aqui.
 * Sem gradientes, sem brilho, sem fundo animado.
 */
object Grafite {
    // ---- camadas (só tom)
    val L0 = Color(0xFF0A0A0B) // barra de servidores
    val L1 = Color(0xFF101012) // canais e membros
    val L2 = Color(0xFF151517) // área principal
    val L3 = Color(0xFF1C1C1F) // campos, cartões
    val L4 = Color(0xFF252528) // hover / toque

    // ---- filetes
    val Linha = Color(0xFF242427)
    val Linha2 = Color(0xFF303034)

    // ---- texto
    val Texto = Color(0xFFECEBE7)
    val Texto2 = Color(0xFFB7B5AE)
    val Apagado = Color(0xFF8A8882)

    // ---- acento e semânticas
    val Ambar = Color(0xFFE9B35E) // o único acento
    val AmbarQuente = Color(0xFFF0C070) // botão primário pressionado
    val AmbarTinta = Color(0xFF1A1204) // texto sobre âmbar
    val Ok = Color(0xFF74B98F) // só presença online
    val Vermelho = Color(0xFFD9806F) // só erro e ações destrutivas

    /** Cor do avatar por usuário (fundo, letra), igual a `user_tint` do desktop. */
    val TonsDeAvatar: List<Pair<Color, Color>> = listOf(
        Color(0xFF3B3548) to Color(0xFFCFC6E6),
        Color(0xFF2F4540) to Color(0xFFBFE0D6),
        Color(0xFF4A3A36) to Color(0xFFE8CBBF),
        Color(0xFF33404F) to Color(0xFFC3D4E8),
        Color(0xFF4B4530) to Color(0xFFEAD9A4),
    )

    fun tomDoUsuario(id: Long): Pair<Color, Color> =
        TonsDeAvatar[(if (id < 0) -id else id).toInt() % TonsDeAvatar.size]
}
