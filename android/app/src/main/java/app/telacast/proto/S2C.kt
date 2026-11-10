package app.telacast.proto

import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonEncoder

// Dentro de S2C os nomes `Message` e `Snapshot` apontam para as variantes; estes apelidos apontam para os tipos de dados.
typealias MensagemDados = Message
typealias SnapshotDados = Snapshot

/**
 * Servidor para cliente. As variantes de tupla do Rust (`Snapshot(Snapshot)`, `Message(Message)`,
 * `VoiceUpdate(VoiceState)`) embrulham o dado e no JSON aparecem sem camada extra: `{"Snapshot":{...}}`.
 *
 * [Desconhecida] é o que sai ao ler uma variante que este cliente não conhece (servidor mais novo):
 * o `Gateway` a descarta e segue em frente. Ela não é codificável.
 */
@Serializable(with = S2CSerializer::class)
sealed class S2C {
    @Serializable data class Auth(val token: String, val me: User) : S2C()
    /** Bytes enviados pelo servidor no mês UTC atual (mensagem periódica). */
    @Serializable data class EgressUsage(val month: String, val bytes: Long) : S2C()
    @Serializable data class Viewers(val channelId: Long, val users: List<Long>) : S2C()
    data class VoiceUpdate(val state: VoiceState) : S2C()
    @Serializable data class VoiceLeft(val userId: Long, val channelId: Long) : S2C()
    @Serializable data class Sessions(val list: List<SessionInfo>) : S2C()
    data object PasswordChanged : S2C()
    @Serializable data class Error(val code: ErrorCode = ErrorCode.Unknown, val msg: String) : S2C()
    data class Snapshot(val snapshot: SnapshotDados) : S2C()
    data class Message(val message: MensagemDados) : S2C()
    @Serializable data class History(val channelId: Long, val messages: List<MensagemDados>) : S2C()
    @Serializable data class SearchResults(val nonce: Long, val hits: List<SearchHit>, val more: Boolean) : S2C()
    @Serializable data class Pinned(val channelId: Long, val messageId: Long, val pinned: Boolean) : S2C()
    @Serializable data class Pins(val channelId: Long, val messages: List<MensagemDados>) : S2C()
    @Serializable data class Reaction(val channelId: Long, val messageId: Long, val emoji: String, val userId: Long, val add: Boolean) : S2C()
    @Serializable data class MessageEdited(val channelId: Long, val messageId: Long, val content: String) : S2C()
    @Serializable data class MessageDeleted(val channelId: Long, val messageId: Long) : S2C()
    @Serializable data class Typing(val channelId: Long, val userId: Long) : S2C()
    @Serializable data class DmOpened(val channelId: Long) : S2C()
    @Serializable data class ServerJoined(val serverId: Long) : S2C()

    /** Variante nova do servidor que este cliente ainda não entende. */
    data class Desconhecida(val etiqueta: String) : S2C()

    companion object {
        /** Todas as etiquetas conhecidas (para o teste que confere as fixtures; precisa acompanhar o enum do Rust). */
        val ETIQUETAS: Set<String> = setOf(
            "Auth", "EgressUsage", "Viewers", "VoiceUpdate", "VoiceLeft", "Sessions", "PasswordChanged", "Error", "Snapshot", "Message",
            "History", "SearchResults", "Pinned", "Pins", "Reaction", "MessageEdited", "MessageDeleted", "Typing",
            "DmOpened", "ServerJoined",
        )
    }
}

object S2CSerializer : KSerializer<S2C> {
    override val descriptor: SerialDescriptor = PrimitiveSerialDescriptor("S2C", PrimitiveKind.STRING)
    override fun deserialize(decoder: Decoder): S2C {
        val d = decoder as JsonDecoder
        return S2CCodec.decodificar(d.json, d.decodeJsonElement())
    }
    override fun serialize(encoder: Encoder, value: S2C) {
        val e = encoder as JsonEncoder
        e.encodeJsonElement(S2CCodec.codificar(e.json, value))
    }
}

internal object S2CCodec {
    fun decodificar(json: Json, el: JsonElement): S2C {
        val (tag, corpo) = Etiquetado.separar(el)
        return when (tag) {
            "Auth" -> json.decodeFromJsonElement(S2C.Auth.serializer(), Etiquetado.corpo(tag, corpo))
            "EgressUsage" -> json.decodeFromJsonElement(S2C.EgressUsage.serializer(), Etiquetado.corpo(tag, corpo))
            "Viewers" -> json.decodeFromJsonElement(S2C.Viewers.serializer(), Etiquetado.corpo(tag, corpo))
            "VoiceUpdate" -> S2C.VoiceUpdate(json.decodeFromJsonElement(VoiceState.serializer(), Etiquetado.corpo(tag, corpo)))
            "VoiceLeft" -> json.decodeFromJsonElement(S2C.VoiceLeft.serializer(), Etiquetado.corpo(tag, corpo))
            "Sessions" -> json.decodeFromJsonElement(S2C.Sessions.serializer(), Etiquetado.corpo(tag, corpo))
            "PasswordChanged" -> Etiquetado.unitaria(tag, corpo, S2C.PasswordChanged)
            "Error" -> json.decodeFromJsonElement(S2C.Error.serializer(), Etiquetado.corpo(tag, corpo))
            "Snapshot" -> S2C.Snapshot(json.decodeFromJsonElement(SnapshotDados.serializer(), Etiquetado.corpo(tag, corpo)))
            "Message" -> S2C.Message(json.decodeFromJsonElement(MensagemDados.serializer(), Etiquetado.corpo(tag, corpo)))
            "History" -> json.decodeFromJsonElement(S2C.History.serializer(), Etiquetado.corpo(tag, corpo))
            "SearchResults" -> json.decodeFromJsonElement(S2C.SearchResults.serializer(), Etiquetado.corpo(tag, corpo))
            "Pinned" -> json.decodeFromJsonElement(S2C.Pinned.serializer(), Etiquetado.corpo(tag, corpo))
            "Pins" -> json.decodeFromJsonElement(S2C.Pins.serializer(), Etiquetado.corpo(tag, corpo))
            "Reaction" -> json.decodeFromJsonElement(S2C.Reaction.serializer(), Etiquetado.corpo(tag, corpo))
            "MessageEdited" -> json.decodeFromJsonElement(S2C.MessageEdited.serializer(), Etiquetado.corpo(tag, corpo))
            "MessageDeleted" -> json.decodeFromJsonElement(S2C.MessageDeleted.serializer(), Etiquetado.corpo(tag, corpo))
            "Typing" -> json.decodeFromJsonElement(S2C.Typing.serializer(), Etiquetado.corpo(tag, corpo))
            "DmOpened" -> json.decodeFromJsonElement(S2C.DmOpened.serializer(), Etiquetado.corpo(tag, corpo))
            "ServerJoined" -> json.decodeFromJsonElement(S2C.ServerJoined.serializer(), Etiquetado.corpo(tag, corpo))
            else -> S2C.Desconhecida(tag)
        }
    }

    fun codificar(json: Json, s: S2C): JsonElement = when (s) {
        is S2C.Auth -> Etiquetado.com("Auth", json.encodeToJsonElement(S2C.Auth.serializer(), s))
        is S2C.EgressUsage -> Etiquetado.com("EgressUsage", json.encodeToJsonElement(S2C.EgressUsage.serializer(), s))
        is S2C.Viewers -> Etiquetado.com("Viewers", json.encodeToJsonElement(S2C.Viewers.serializer(), s))
        is S2C.VoiceUpdate -> Etiquetado.com("VoiceUpdate", json.encodeToJsonElement(VoiceState.serializer(), s.state))
        is S2C.VoiceLeft -> Etiquetado.com("VoiceLeft", json.encodeToJsonElement(S2C.VoiceLeft.serializer(), s))
        is S2C.Sessions -> Etiquetado.com("Sessions", json.encodeToJsonElement(S2C.Sessions.serializer(), s))
        S2C.PasswordChanged -> Etiquetado.unit("PasswordChanged")
        is S2C.Error -> Etiquetado.com("Error", json.encodeToJsonElement(S2C.Error.serializer(), s))
        is S2C.Snapshot -> Etiquetado.com("Snapshot", json.encodeToJsonElement(SnapshotDados.serializer(), s.snapshot))
        is S2C.Message -> Etiquetado.com("Message", json.encodeToJsonElement(MensagemDados.serializer(), s.message))
        is S2C.History -> Etiquetado.com("History", json.encodeToJsonElement(S2C.History.serializer(), s))
        is S2C.SearchResults -> Etiquetado.com("SearchResults", json.encodeToJsonElement(S2C.SearchResults.serializer(), s))
        is S2C.Pinned -> Etiquetado.com("Pinned", json.encodeToJsonElement(S2C.Pinned.serializer(), s))
        is S2C.Pins -> Etiquetado.com("Pins", json.encodeToJsonElement(S2C.Pins.serializer(), s))
        is S2C.Reaction -> Etiquetado.com("Reaction", json.encodeToJsonElement(S2C.Reaction.serializer(), s))
        is S2C.MessageEdited -> Etiquetado.com("MessageEdited", json.encodeToJsonElement(S2C.MessageEdited.serializer(), s))
        is S2C.MessageDeleted -> Etiquetado.com("MessageDeleted", json.encodeToJsonElement(S2C.MessageDeleted.serializer(), s))
        is S2C.Typing -> Etiquetado.com("Typing", json.encodeToJsonElement(S2C.Typing.serializer(), s))
        is S2C.DmOpened -> Etiquetado.com("DmOpened", json.encodeToJsonElement(S2C.DmOpened.serializer(), s))
        is S2C.ServerJoined -> Etiquetado.com("ServerJoined", json.encodeToJsonElement(S2C.ServerJoined.serializer(), s))
        is S2C.Desconhecida -> throw SerializationException("variante desconhecida não pode ser codificada: ${s.etiqueta}")
    }
}
