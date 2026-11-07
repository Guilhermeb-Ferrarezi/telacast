package app.telacast.proto

import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNamingStrategy

/** Ponto de entrada do protocolo: texto JSON <-> [C2S]/[S2C]. Puro Kotlin (roda na JVM, sem Android). */
object Proto {
    /**
     * - `SnakeCase`: `channelId` <-> `channel_id` (os nomes do Rust).
     * - `ignoreUnknownKeys`: servidor mais novo pode ter campos novos.
     * - `encodeDefaults` + `explicitNulls`: escreve também os campos com valor padrão e os `Option` vazios como `null`,
     *   idêntico ao que o `serde_json` do servidor produz (as fixtures são comparadas como árvore).
     */
    @OptIn(ExperimentalSerializationApi::class)
    val json: Json = Json {
        namingStrategy = JsonNamingStrategy.SnakeCase
        ignoreUnknownKeys = true
        encodeDefaults = true
        explicitNulls = true
    }

    /**
     * Lê um quadro de texto do servidor. Variante desconhecida volta como [S2C.Desconhecida].
     * Lança [SerializationException] (ou `IllegalArgumentException`) se o JSON estiver malformado ou uma
     * variante conhecida vier com conteúdo inválido (por exemplo `FriendStatus` novo): quem chama descarta o quadro.
     */
    fun lerS2C(texto: String): S2C = json.decodeFromString(S2CSerializer, texto)

    /** Igual a [lerS2C], mas devolve `null` em vez de lançar. */
    fun lerS2COuNulo(texto: String): S2C? = try { lerS2C(texto) } catch (_: SerializationException) { null } catch (_: IllegalArgumentException) { null }

    fun escreverC2S(msg: C2S): String = json.encodeToString(C2SSerializer, msg)
    fun lerC2S(texto: String): C2S = json.decodeFromString(C2SSerializer, texto)
    fun escreverS2C(msg: S2C): String = json.encodeToString(S2CSerializer, msg)

    fun c2sParaArvore(msg: C2S): JsonElement = json.encodeToJsonElement(C2SSerializer, msg)
    fun s2cParaArvore(msg: S2C): JsonElement = json.encodeToJsonElement(S2CSerializer, msg)
    fun arvoreParaS2C(el: JsonElement): S2C = json.decodeFromJsonElement(S2CSerializer, el)
    fun arvoreParaC2S(el: JsonElement): C2S = json.decodeFromJsonElement(C2SSerializer, el)
}
