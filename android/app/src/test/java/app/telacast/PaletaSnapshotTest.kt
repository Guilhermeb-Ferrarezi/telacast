package app.telacast

import app.cash.paparazzi.DeviceConfig
import app.cash.paparazzi.Paparazzi
import app.telacast.tema.GrafiteTheme
import app.telacast.ui.PaletaConteudo
import org.junit.Rule
import org.junit.Test

/** Renderiza a paleta na JVM (sem emulador). Saída: app/build/reports/paparazzi (ou src/test/snapshots). */
class PaletaSnapshotTest {
    @get:Rule
    val paparazzi = Paparazzi(
        // Pixel 5 (1080 px, 393 dp de largura), mas bem alto para caber a paleta inteira em um PNG só
        deviceConfig = DeviceConfig.PIXEL_5.copy(screenHeight = 4700),
        theme = "Theme.Telacast",
        useDeviceResolution = true, // sem isto o Paparazzi reduz o PNG para no máximo 1000 px
    )

    @Test
    fun paleta() {
        paparazzi.snapshot { GrafiteTheme { PaletaConteudo() } }
    }
}
