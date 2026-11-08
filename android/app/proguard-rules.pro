# Regras do R8 para o release do Telacast. Conferidas lendo `app/build/outputs/mapping/release/seeds.txt` (o que ficou) e na fumaça.

# --- ponte JNI do RNNoise: os nomes das funções nativas são ligados pelo nome Java_app_telacast_voz_RnnoiseNativo_*
-keep class app.telacast.voz.RnnoiseNativo { *; }
-keepclasseswithmembernames class * { native <methods>; }

# --- kotlinx.serialization (o protocolo inteiro; serializadores gerados e Companion.serializer())
-keepattributes *Annotation*, InnerClasses, Signature, Exceptions, RuntimeVisibleAnnotations, EnclosingMethod
-keepclassmembers class app.telacast.** { *** Companion; }
-keepclasseswithmembers class app.telacast.** { kotlinx.serialization.KSerializer serializer(...); }
-keep,includedescriptorclasses class app.telacast.**$$serializer { *; }
-keepclassmembers class app.telacast.** { *** INSTANCE; }
-keep class app.telacast.proto.** { *; }

# --- OkHttp / Okio (já trazem regras próprias; só silencia o que é opcional)
-dontwarn okhttp3.internal.platform.**
-dontwarn org.conscrypt.**
-dontwarn org.bouncycastle.**
-dontwarn org.openjsse.**

# --- Opus em Java puro e Ed25519 (carregam classes e provedores por nome)
-keep class io.github.jaredmdobson.concentus.** { *; }
-keep class net.i2p.crypto.eddsa.** { *; }
-dontwarn sun.security.x509.**

# --- Firebase / WorkManager trazem regras de consumidor; os serviços e receivers do app estão no manifesto (o AGP os mantém)

# --- crash: stack legível no relatório (sem mapping no aparelho, o nome do arquivo e a linha bastam)
-keepattributes SourceFile, LineNumberTable
-renamesourcefileattribute SourceFile

# --- sem logs no release (o log de voz e os de depuração somem; só warn e error ficam)
-assumenosideeffects class android.util.Log {
    public static int v(...);
    public static int d(...);
    public static int w(...);
    public static int i(...);
}
