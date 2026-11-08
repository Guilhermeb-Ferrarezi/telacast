plugins {
    id("com.android.application") version "8.5.2" apply false
    id("org.jetbrains.kotlin.android") version "2.0.21" apply false
    id("org.jetbrains.kotlin.plugin.compose") version "2.0.21" apply false
    id("org.jetbrains.kotlin.plugin.serialization") version "2.0.21" apply false
    id("app.cash.paparazzi") version "1.3.5" apply false
    // push (D1): só é aplicado no app se app/google-services.json existir
    id("com.google.gms.google-services") version "4.4.2" apply false
}
