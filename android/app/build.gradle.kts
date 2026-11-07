plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
    id("app.cash.paparazzi")
}

android {
    namespace = "app.telacast"
    compileSdk = 34

    defaultConfig {
        applicationId = "app.telacast"
        minSdk = 26
        targetSdk = 34
        versionCode = 1
        versionName = "0.1.0"
        // Servidor sugerido na tela de entrada (o campo continua editável). Em produção fica vazio até haver domínio.
        buildConfigField("String", "SERVIDOR_PADRAO", "\"\"")
    }

    buildTypes {
        release { isMinifyEnabled = false }
        // no emulador, 10.0.2.2 é o PC; no celular real use `adb reverse tcp:8080 tcp:8080` e localhost:8080
        debug { buildConfigField("String", "SERVIDOR_PADRAO", "\"10.0.2.2:8080\"") }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    buildFeatures { compose = true; buildConfig = true }
    packaging { resources.excludes += "/META-INF/{AL2.0,LGPL2.1}" }
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.activity:activity-compose:1.9.2")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.6")
    implementation(platform("androidx.compose:compose-bom:2024.09.03"))
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.animation:animation")
    implementation("androidx.compose.material3:material3")

    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.7.3")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.8.1")
    implementation("com.squareup.okhttp3:okhttp:4.12.0")

    testImplementation("junit:junit:4.13.2")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.8.1")
    testImplementation("com.squareup.okhttp3:mockwebserver:4.12.0")
}

// Testes na JVM: heap enxuto por causa da RAM apertada.
// Os fixtures do protocolo ficam fora do projeto Gradle: ../proto/fixtures (relativo a android/).
// Testes de integração (nome termina em IntegracaoTest) só rodam com -PintegrationTests (ver scripts/it.sh);
// no `verifyPaparazziDebug`/`testDebugUnitTest` normais ficam de fora.
tasks.withType<Test>().configureEach {
    maxHeapSize = "768m"
    systemProperty("telacast.fixtures", rootProject.projectDir.resolve("../proto/fixtures").canonicalPath)
    if (project.hasProperty("integrationTests")) {
        filter.includeTestsMatching("*IntegracaoTest")
        outputs.upToDateWhen { false }
        // repassa o servidor de teste (subido por scripts/it.sh)
        listOf("TELACAST_IT_URL", "TELACAST_IT_INVITE").forEach { k -> System.getenv(k)?.let { environment(k, it) } }
        testLogging { events("passed", "failed", "skipped"); showStandardStreams = true }
    } else {
        filter.excludeTestsMatching("*IntegracaoTest")
        testLogging { events("failed") }
    }
}
