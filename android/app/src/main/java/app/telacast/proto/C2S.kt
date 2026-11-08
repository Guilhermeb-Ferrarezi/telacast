package app.telacast.proto

import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonEncoder

/**
 * Cliente para servidor. `proto` ausente vale 0 ao decodificar (igual ao Rust); quem monta `Register`/`Login`/`Resume`
 * deve passar [PROTO_VERSION] (o `Gateway` faz isso).
 *
 * Cada subclasse é uma variante de `C2S` do Rust (mesmo nome = etiqueta do JSON). */
@Serializable(with = C2SSerializer::class)
sealed class C2S {
    @Serializable data class Register(val username: String, val password: String, val invite: String, val proto: Int = 0) : C2S()
    @Serializable data class Login(val username: String, val password: String, val proto: Int = 0) : C2S()
    @Serializable data class Resume(val token: String, val proto: Int = 0) : C2S()
    data object Logout : C2S()
    @Serializable data class VoiceJoin(val channelId: Long) : C2S()
    data object VoiceLeave : C2S()
    @Serializable data class VoiceSet(val muted: Boolean, val deafened: Boolean) : C2S()
    @Serializable data class ChangePassword(val old: String, val new: String) : C2S()
    data object ListSessions : C2S()
    @Serializable data class RevokeSession(val id: Long) : C2S()
    data object RevokeOtherSessions : C2S()
    @Serializable data class CreateServer(val name: String) : C2S()
    @Serializable data class JoinServer(val code: String) : C2S()
    @Serializable data class CreateChannel(val serverId: Long, val name: String, val kind: Int, val parentId: Long? = null) : C2S()
    @Serializable data class Send(
        val channelId: Long,
        val content: String,
        val replyTo: Long? = null,
        val nonce: Long = 0,
        val attachments: List<Long> = emptyList(),
    ) : C2S()
    @Serializable data class Edit(val messageId: Long, val content: String) : C2S()
    @Serializable data class Delete(val messageId: Long) : C2S()
    @Serializable data class SetStatus(val status: Status) : C2S()
    @Serializable data class SetIdle(val idle: Boolean) : C2S()
    @Serializable data class Search(
        val nonce: Long,
        val channelId: Long? = null,
        val serverId: Long? = null,
        val from: Long? = null,
        val text: String,
        val before: Long? = null,
    ) : C2S()
    @Serializable data class Block(val userId: Long) : C2S()
    @Serializable data class Unblock(val userId: Long) : C2S()
    @Serializable data class Pin(val messageId: Long, val pin: Boolean) : C2S()
    @Serializable data class Pins(val channelId: Long) : C2S()
    @Serializable data class React(val messageId: Long, val emoji: String, val add: Boolean) : C2S()
    @Serializable data class Ack(val channelId: Long, val messageId: Long) : C2S()
    @Serializable data class Typing(val channelId: Long) : C2S()
    @Serializable data class History(val channelId: Long, val before: Long? = null) : C2S()
    @Serializable data class FriendRequest(val username: String) : C2S()
    @Serializable data class FriendAccept(val userId: Long) : C2S()
    @Serializable data class FriendRemove(val userId: Long) : C2S()
    @Serializable data class OpenDm(val userId: Long) : C2S()
    /** Push (D1): registra o token do FCM desta instalação; `platform` é "fcm". */
    @Serializable data class PushRegister(val token: String, val platform: String) : C2S()
    @Serializable data class PushUnregister(val token: String) : C2S()
    /** Esta conexão está com o app em primeiro plano e visível? O servidor só manda push a quem não tem conexão com `focused = true`. */
    @Serializable data class SetFocus(val focused: Boolean) : C2S()

    companion object {
        /** Todas as etiquetas de variante (para o teste que confere as fixtures; precisa acompanhar o enum do Rust). */
        val ETIQUETAS: Set<String> = setOf(
            "Register", "Login", "Resume", "Logout", "VoiceJoin", "VoiceLeave", "VoiceSet", "ChangePassword",
            "ListSessions", "RevokeSession", "RevokeOtherSessions", "CreateServer", "JoinServer", "CreateChannel",
            "Send", "Edit", "Delete", "SetStatus", "SetIdle", "Search", "Block", "Unblock", "Pin", "Pins", "React",
            "Ack", "Typing", "History", "FriendRequest", "FriendAccept", "FriendRemove", "OpenDm", "PushRegister", "PushUnregister", "SetFocus",
        )
    }
}

/** Formato "externamente etiquetado" do serde: `{"Login":{...}}`, e a string `"Logout"` nas variantes sem campos. */
object C2SSerializer : KSerializer<C2S> {
    override val descriptor: SerialDescriptor = kotlinx.serialization.descriptors.PrimitiveSerialDescriptor("C2S", kotlinx.serialization.descriptors.PrimitiveKind.STRING)
    override fun deserialize(decoder: Decoder): C2S {
        val d = decoder as JsonDecoder
        return C2SCodec.decodificar(d.json, d.decodeJsonElement())
    }
    override fun serialize(encoder: Encoder, value: C2S) {
        val e = encoder as JsonEncoder
        e.encodeJsonElement(C2SCodec.codificar(e.json, value))
    }
}

internal object C2SCodec {
    fun decodificar(json: Json, el: JsonElement): C2S {
        val (tag, corpo) = Etiquetado.separar(el)
        return when (tag) {
            "Register" -> json.decodeFromJsonElement(C2S.Register.serializer(), Etiquetado.corpo(tag, corpo))
            "Login" -> json.decodeFromJsonElement(C2S.Login.serializer(), Etiquetado.corpo(tag, corpo))
            "Resume" -> json.decodeFromJsonElement(C2S.Resume.serializer(), Etiquetado.corpo(tag, corpo))
            "Logout" -> Etiquetado.unitaria(tag, corpo, C2S.Logout)
            "VoiceJoin" -> json.decodeFromJsonElement(C2S.VoiceJoin.serializer(), Etiquetado.corpo(tag, corpo))
            "VoiceLeave" -> Etiquetado.unitaria(tag, corpo, C2S.VoiceLeave)
            "VoiceSet" -> json.decodeFromJsonElement(C2S.VoiceSet.serializer(), Etiquetado.corpo(tag, corpo))
            "ChangePassword" -> json.decodeFromJsonElement(C2S.ChangePassword.serializer(), Etiquetado.corpo(tag, corpo))
            "ListSessions" -> Etiquetado.unitaria(tag, corpo, C2S.ListSessions)
            "RevokeSession" -> json.decodeFromJsonElement(C2S.RevokeSession.serializer(), Etiquetado.corpo(tag, corpo))
            "RevokeOtherSessions" -> Etiquetado.unitaria(tag, corpo, C2S.RevokeOtherSessions)
            "CreateServer" -> json.decodeFromJsonElement(C2S.CreateServer.serializer(), Etiquetado.corpo(tag, corpo))
            "JoinServer" -> json.decodeFromJsonElement(C2S.JoinServer.serializer(), Etiquetado.corpo(tag, corpo))
            "CreateChannel" -> json.decodeFromJsonElement(C2S.CreateChannel.serializer(), Etiquetado.corpo(tag, corpo))
            "Send" -> json.decodeFromJsonElement(C2S.Send.serializer(), Etiquetado.corpo(tag, corpo))
            "Edit" -> json.decodeFromJsonElement(C2S.Edit.serializer(), Etiquetado.corpo(tag, corpo))
            "Delete" -> json.decodeFromJsonElement(C2S.Delete.serializer(), Etiquetado.corpo(tag, corpo))
            "SetStatus" -> json.decodeFromJsonElement(C2S.SetStatus.serializer(), Etiquetado.corpo(tag, corpo))
            "SetIdle" -> json.decodeFromJsonElement(C2S.SetIdle.serializer(), Etiquetado.corpo(tag, corpo))
            "Search" -> json.decodeFromJsonElement(C2S.Search.serializer(), Etiquetado.corpo(tag, corpo))
            "Block" -> json.decodeFromJsonElement(C2S.Block.serializer(), Etiquetado.corpo(tag, corpo))
            "Unblock" -> json.decodeFromJsonElement(C2S.Unblock.serializer(), Etiquetado.corpo(tag, corpo))
            "Pin" -> json.decodeFromJsonElement(C2S.Pin.serializer(), Etiquetado.corpo(tag, corpo))
            "Pins" -> json.decodeFromJsonElement(C2S.Pins.serializer(), Etiquetado.corpo(tag, corpo))
            "React" -> json.decodeFromJsonElement(C2S.React.serializer(), Etiquetado.corpo(tag, corpo))
            "Ack" -> json.decodeFromJsonElement(C2S.Ack.serializer(), Etiquetado.corpo(tag, corpo))
            "Typing" -> json.decodeFromJsonElement(C2S.Typing.serializer(), Etiquetado.corpo(tag, corpo))
            "History" -> json.decodeFromJsonElement(C2S.History.serializer(), Etiquetado.corpo(tag, corpo))
            "FriendRequest" -> json.decodeFromJsonElement(C2S.FriendRequest.serializer(), Etiquetado.corpo(tag, corpo))
            "FriendAccept" -> json.decodeFromJsonElement(C2S.FriendAccept.serializer(), Etiquetado.corpo(tag, corpo))
            "FriendRemove" -> json.decodeFromJsonElement(C2S.FriendRemove.serializer(), Etiquetado.corpo(tag, corpo))
            "PushRegister" -> json.decodeFromJsonElement(C2S.PushRegister.serializer(), Etiquetado.corpo(tag, corpo))
            "PushUnregister" -> json.decodeFromJsonElement(C2S.PushUnregister.serializer(), Etiquetado.corpo(tag, corpo))
            "SetFocus" -> json.decodeFromJsonElement(C2S.SetFocus.serializer(), Etiquetado.corpo(tag, corpo))
            "OpenDm" -> json.decodeFromJsonElement(C2S.OpenDm.serializer(), Etiquetado.corpo(tag, corpo))
            else -> throw kotlinx.serialization.SerializationException("variante de C2S desconhecida: $tag")
        }
    }

    fun codificar(json: Json, c: C2S): JsonElement = when (c) {
        is C2S.Register -> Etiquetado.com("Register", json.encodeToJsonElement(C2S.Register.serializer(), c))
        is C2S.Login -> Etiquetado.com("Login", json.encodeToJsonElement(C2S.Login.serializer(), c))
        is C2S.Resume -> Etiquetado.com("Resume", json.encodeToJsonElement(C2S.Resume.serializer(), c))
        C2S.Logout -> Etiquetado.unit("Logout")
        is C2S.VoiceJoin -> Etiquetado.com("VoiceJoin", json.encodeToJsonElement(C2S.VoiceJoin.serializer(), c))
        C2S.VoiceLeave -> Etiquetado.unit("VoiceLeave")
        is C2S.VoiceSet -> Etiquetado.com("VoiceSet", json.encodeToJsonElement(C2S.VoiceSet.serializer(), c))
        is C2S.ChangePassword -> Etiquetado.com("ChangePassword", json.encodeToJsonElement(C2S.ChangePassword.serializer(), c))
        C2S.ListSessions -> Etiquetado.unit("ListSessions")
        is C2S.RevokeSession -> Etiquetado.com("RevokeSession", json.encodeToJsonElement(C2S.RevokeSession.serializer(), c))
        C2S.RevokeOtherSessions -> Etiquetado.unit("RevokeOtherSessions")
        is C2S.CreateServer -> Etiquetado.com("CreateServer", json.encodeToJsonElement(C2S.CreateServer.serializer(), c))
        is C2S.JoinServer -> Etiquetado.com("JoinServer", json.encodeToJsonElement(C2S.JoinServer.serializer(), c))
        is C2S.CreateChannel -> Etiquetado.com("CreateChannel", json.encodeToJsonElement(C2S.CreateChannel.serializer(), c))
        is C2S.Send -> Etiquetado.com("Send", json.encodeToJsonElement(C2S.Send.serializer(), c))
        is C2S.Edit -> Etiquetado.com("Edit", json.encodeToJsonElement(C2S.Edit.serializer(), c))
        is C2S.Delete -> Etiquetado.com("Delete", json.encodeToJsonElement(C2S.Delete.serializer(), c))
        is C2S.SetStatus -> Etiquetado.com("SetStatus", json.encodeToJsonElement(C2S.SetStatus.serializer(), c))
        is C2S.SetIdle -> Etiquetado.com("SetIdle", json.encodeToJsonElement(C2S.SetIdle.serializer(), c))
        is C2S.Search -> Etiquetado.com("Search", json.encodeToJsonElement(C2S.Search.serializer(), c))
        is C2S.Block -> Etiquetado.com("Block", json.encodeToJsonElement(C2S.Block.serializer(), c))
        is C2S.Unblock -> Etiquetado.com("Unblock", json.encodeToJsonElement(C2S.Unblock.serializer(), c))
        is C2S.Pin -> Etiquetado.com("Pin", json.encodeToJsonElement(C2S.Pin.serializer(), c))
        is C2S.Pins -> Etiquetado.com("Pins", json.encodeToJsonElement(C2S.Pins.serializer(), c))
        is C2S.React -> Etiquetado.com("React", json.encodeToJsonElement(C2S.React.serializer(), c))
        is C2S.Ack -> Etiquetado.com("Ack", json.encodeToJsonElement(C2S.Ack.serializer(), c))
        is C2S.Typing -> Etiquetado.com("Typing", json.encodeToJsonElement(C2S.Typing.serializer(), c))
        is C2S.History -> Etiquetado.com("History", json.encodeToJsonElement(C2S.History.serializer(), c))
        is C2S.FriendRequest -> Etiquetado.com("FriendRequest", json.encodeToJsonElement(C2S.FriendRequest.serializer(), c))
        is C2S.FriendAccept -> Etiquetado.com("FriendAccept", json.encodeToJsonElement(C2S.FriendAccept.serializer(), c))
        is C2S.FriendRemove -> Etiquetado.com("FriendRemove", json.encodeToJsonElement(C2S.FriendRemove.serializer(), c))
        is C2S.OpenDm -> Etiquetado.com("OpenDm", json.encodeToJsonElement(C2S.OpenDm.serializer(), c))
        is C2S.PushRegister -> Etiquetado.com("PushRegister", json.encodeToJsonElement(C2S.PushRegister.serializer(), c))
        is C2S.PushUnregister -> Etiquetado.com("PushUnregister", json.encodeToJsonElement(C2S.PushUnregister.serializer(), c))
        is C2S.SetFocus -> Etiquetado.com("SetFocus", json.encodeToJsonElement(C2S.SetFocus.serializer(), c))
    }
}
