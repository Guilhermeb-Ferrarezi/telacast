import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
    id("app.cash.paparazzi")
}

// Versão: uma fonte só (android/version.properties).
val versaoProps = Properties().apply { rootProject.file("version.properties").inputStream().use { load(it) } }
val versaoNome: String = versaoProps.getProperty("versionName")
val versaoCodigo: Int = versaoProps.getProperty("versionCode").toInt()

// Assinatura do release: a chave NUNCA fica no repositório. Vem de variáveis de ambiente (TELACAST_KEYSTORE, TELACAST_KEYSTORE_PASSWORD,
// TELACAST_KEY_ALIAS, TELACAST_KEY_PASSWORD) ou de ~/.config/telacast/android-signing.properties (storeFile, storePassword, keyAlias, keyPassword).
// Sem nenhuma das duas o assembleRelease ainda roda (CI/testes), mas o APK sai SEM assinatura e o Gradle avisa.
val assinatura: Map<String, String>? = run {
    val env = System.getenv()
    if (env["TELACAST_KEYSTORE"] != null) mapOf("storeFile" to env["TELACAST_KEYSTORE"]!!, "storePassword" to (env["TELACAST_KEYSTORE_PASSWORD"] ?: ""), "keyAlias" to (env["TELACAST_KEY_ALIAS"] ?: "telacast"), "keyPassword" to (env["TELACAST_KEY_PASSWORD"] ?: env["TELACAST_KEYSTORE_PASSWORD"] ?: ""))
    else File(System.getProperty("user.home"), ".config/telacast/android-signing.properties").takeIf { it.exists() }?.let { f ->
        val p = Properties().apply { f.inputStream().use { load(it) } }
        p.stringPropertyNames().associateWith { p.getProperty(it) }
    }
}?.takeIf { File(it["storeFile"] ?: "").exists() }

// Chave pública das atualizações (hex, 32 bytes) em android/update.pub; sem o arquivo a atualização pelo app fica DESLIGADA.
val chaveDeAtualizacao: String = rootProject.file("update.pub").takeIf { it.exists() }?.readText()?.trim()?.takeIf { Regex("[0-9a-fA-F]{64}").matches(it) } ?: ""

android {
    namespace = "app.telacast"
    compileSdk = 34

    defaultConfig {
        applicationId = "app.telacast"
        minSdk = 26
        targetSdk = 34
        versionCode = versaoCodigo
        versionName = versaoNome
        resourceConfigurations += listOf("pt-rBR", "pt")
        // Servidor sugerido na tela de entrada (o campo continua editável). Em produção fica vazio até haver domínio.
        buildConfigField("String", "SERVIDOR_PADRAO", "\"\"")
        buildConfigField("String", "CHAVE_DE_ATUALIZACAO", "\"$chaveDeAtualizacao\"")
    }

    signingConfigs {
        if (assinatura != null) create("release") {
            storeFile = File(assinatura["storeFile"]!!); storePassword = assinatura["storePassword"]; keyAlias = assinatura["keyAlias"]; keyPassword = assinatura["keyPassword"]
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            // só arm64 (a ponte nativa do RNNoise existe só para ele; aparelhos de 32 bits ficam sem suporte)
            ndk { abiFilters += "arm64-v8a" }
            signingConfig = signingConfigs.findByName("release")
            buildConfigField("String", "SERVIDOR_PADRAO", "\"telecast.guilhermebf.dev\"")
        }
        // no emulador, 10.0.2.2 é o PC; no celular real use `adb reverse tcp:8080 tcp:8080` e localhost:8080
        debug { buildConfigField("String", "SERVIDOR_PADRAO", "\"10.0.2.2:8080\"") }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    // telacast-<versão>-arm64.apk (o release; o debug mantém o nome padrão)
    applicationVariants.all {
        if (buildType.name == "release") outputs.all { (this as com.android.build.gradle.internal.api.BaseVariantOutputImpl).outputFileName = "telacast-$versaoNome-arm64${if (assinatura == null) "-sem-assinatura" else ""}.apk" }
    }
    buildFeatures { compose = true; buildConfig = true }
    packaging { resources.excludes += "/META-INF/{AL2.0,LGPL2.1}" }
}

dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    implementation("androidx.core:core-splashscreen:1.0.1")
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
    // Opus em Java puro: roda igual na JVM (testes) e no aparelho, sem NDK nem libs nativas
    implementation("io.github.jaredmdobson:concentus:1.0.2")
    // Ed25519 em Java puro (a verificação da assinatura das atualizações; o JCA do Android só tem Ed25519 a partir do 13)
    implementation("net.i2p.crypto:eddsa:0.3.0")
    // checagem diária de atualização em segundo plano (baixa e verifica o APK novo)
    implementation("androidx.work:work-runtime-ktx:2.9.1")
    // push por FCM (D1). Sem google-services.json o Firebase não inicializa e o app só não registra push
    implementation(platform("com.google.firebase:firebase-bom:33.7.0"))
    implementation("com.google.firebase:firebase-messaging")

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
    // RNNoise na JVM: a versão do PC da ponte nativa (scripts/rnnoise.sh --host). Sem ela os testes do RNNoise são ignorados.
    rootProject.projectDir.resolve("native/rnnoise/target/release").takeIf { it.resolve("libtelacast_rnnoise.so").exists() }
        ?.let { jvmArgs("-Djava.library.path=${it.absolutePath}") }
    if (project.hasProperty("integrationTests")) {
        filter.includeTestsMatching("*IntegracaoTest")
        outputs.upToDateWhen { false }
        // repassa o servidor de teste (subido por scripts/it.sh)
        listOf("TELACAST_IT_URL", "TELACAST_IT_INVITE", "TELACAST_IT_UPDATE_PUBKEY", "TELACAST_IT_UPDATE_SHA").forEach { k -> System.getenv(k)?.let { environment(k, it) } }
        testLogging { events("passed", "failed", "skipped"); showStandardStreams = true }
    } else {
        filter.excludeTestsMatching("*IntegracaoTest")
        testLogging { events("failed") }
    }
}

// Credenciais do Firebase (baixadas do console do projeto, ficam fora do git): sem o arquivo o build e os testes funcionam igual
// e o app simplesmente não registra push.
if (file("google-services.json").exists()) apply(plugin = "com.google.gms.google-services")

gradle.taskGraph.whenReady {
    if (assinatura == null && allTasks.any { it.name.contains("Release", ignoreCase = true) && it.project == project && (it.name.startsWith("assemble") || it.name.startsWith("package")) })
        logger.warn("AVISO: sem chave de assinatura (veja o README): o APK de release sai SEM assinatura e não instala no aparelho.")
}
