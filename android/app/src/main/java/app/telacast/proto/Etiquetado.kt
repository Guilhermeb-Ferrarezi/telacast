package app.telacast.proto

import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/** Ajudantes do formato "externamente etiquetado" do serde (docs/PROTOCOL.md, seção 2). */
internal object Etiquetado {
    /** Separa a etiqueta do corpo: `"Logout"` -> ("Logout", null); `{"Login":{...}}` -> ("Login", {...}). */
    fun separar(el: JsonElement): Pair<String, JsonElement?> = when {
        el is JsonPrimitive && el.isString -> el.content to null
        el is JsonObject && el.size == 1 -> el.entries.first().let { it.key to it.value }
        else -> throw SerializationException("mensagem fora do formato etiquetado: ${el.toString().take(80)}")
    }

    fun corpo(tag: String, corpo: JsonElement?): JsonElement =
        corpo ?: throw SerializationException("a variante $tag precisa de campos")

    /** Variante sem campos: aceita a string pura (e, por tolerância, `{"Tag":null}`). */
    fun <T> unitaria(tag: String, corpo: JsonElement?, valor: T): T {
        if (corpo == null || corpo is JsonNull) return valor
        throw SerializationException("a variante $tag não tem campos")
    }

    fun com(tag: String, corpo: JsonElement): JsonElement = JsonObject(mapOf(tag to corpo))
    fun unit(tag: String): JsonElement = JsonPrimitive(tag)
}
