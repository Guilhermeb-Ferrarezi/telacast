@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package app.telacast.proto

import kotlinx.serialization.KSerializer
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.descriptors.SerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder

/**
 * Tipos do protocolo, espelho de `proto/src/lib.rs` (a fonte da verdade). Os nomes de campo em
 * snake_case do Rust viram camelCase aqui; o [Proto.json] converte com `JsonNamingStrategy.SnakeCase`.
 *
 * Regras (docs/PROTOCOL.md, seção 2): ids são `Long` (snowflakes passam de 2^53), `Option` vira tipo
 * anulável com padrão `null`, `#[serde(default)]` vira valor padrão, campos desconhecidos são ignorados.
 * `u8` e `u32` viram `Int`; `u64` (`nonce`, `size`) vira `Long`.
 */
const val PROTO_VERSION: Int = 2

/** Primeiro byte de um quadro binário de áudio no gateway. */
const val VOICE_FRAME: Int = 1
const val VOICE_MAX_PACKET: Int = 400

/** Enum "etiquetado por nome" que aceita valor novo do servidor, caindo em [padrao] (`#[serde(other)]`). */
open class EnumTolerante<T : Enum<T>>(nome: String, private val valores: Array<T>, private val padrao: T) : KSerializer<T> {
    override val descriptor: SerialDescriptor = PrimitiveSerialDescriptor(nome, PrimitiveKind.STRING)
    override fun deserialize(decoder: Decoder): T {
        val s = decoder.decodeString()
        return valores.firstOrNull { it.name == s } ?: padrao
    }
    override fun serialize(encoder: Encoder, value: T) = encoder.encodeString(value.name)
}

/** Enum "etiquetado por nome" sem valor de reserva: valor desconhecido falha (como `FriendStatus` no Rust). */
open class EnumEstrito<T : Enum<T>>(nome: String, private val valores: Array<T>) : KSerializer<T> {
    override val descriptor: SerialDescriptor = PrimitiveSerialDescriptor(nome, PrimitiveKind.STRING)
    override fun deserialize(decoder: Decoder): T {
        val s = decoder.decodeString()
        return valores.firstOrNull { it.name == s } ?: throw SerializationException("$s não é um valor de ${descriptor.serialName}")
    }
    override fun serialize(encoder: Encoder, value: T) = encoder.encodeString(value.name)
}

/** Motivo estável de um erro. Código desconhecido (servidor mais novo) vira [Unknown]. */
@Serializable(with = ErrorCodeSerializer::class)
enum class ErrorCode {
    InvalidCredentials, WrongPassword, InvalidInvite, UsernameTaken, SessionExpired, NotAuthenticated,
    RateLimited, Forbidden, NotFound, InvalidInput, VersionMismatch, Internal, Unknown,
}
object ErrorCodeSerializer : EnumTolerante<ErrorCode>("ErrorCode", ErrorCode.entries.toTypedArray(), ErrorCode.Unknown)

/** Presença. `Invisible` só aparece para a própria pessoa. Valor desconhecido vira [Offline]. */
@Serializable(with = StatusSerializer::class)
enum class Status { Online, Idle, Dnd, Invisible, Offline }
object StatusSerializer : EnumTolerante<Status>("Status", Status.entries.toTypedArray(), Status.Offline)

/** Sem valor de reserva (igual ao Rust): um valor novo faz o `Snapshot` inteiro falhar. */
@Serializable(with = FriendStatusSerializer::class)
enum class FriendStatus { Friend, Incoming, Outgoing }
object FriendStatusSerializer : EnumEstrito<FriendStatus>("FriendStatus", FriendStatus.entries.toTypedArray())

@Serializable
data class User(
    val id: Long,
    val name: String,
    /** Conectada e visível (online, ausente ou não perturbe). */
    val online: Boolean,
    val status: Status = Status.Offline,
)

@Serializable
data class Channel(
    val id: Long,
    val name: String,
    /** 0 texto, 1 voz, 4 categoria (2 anúncio reservado). */
    val kind: Int,
    val parent: Long? = null,
    val position: Int,
    val lastMessageId: Long,
    val lastReadId: Long,
    val mentions: Int = 0,
) {
    companion object {
        const val TEXTO = 0
        const val VOZ = 1
        const val CATEGORIA = 4
    }
}

@Serializable
data class ServerInfo(
    val id: Long,
    val name: String,
    val owner: Long,
    val invite: String,
    val channels: List<Channel>,
    val members: List<User>,
)

@Serializable
data class Friend(val user: User, val status: FriendStatus)

@Serializable
data class Dm(
    val channelId: Long,
    val user: User,
    val lastMessageId: Long,
    val lastReadId: Long,
    val mentions: Int = 0,
)

@Serializable
data class Live(val channelId: Long, val userId: Long)

@Serializable
data class VoiceState(val userId: Long, val channelId: Long, val muted: Boolean, val deafened: Boolean)

@Serializable
data class Snapshot(
    val me: User,
    val servers: List<ServerInfo>,
    val friends: List<Friend>,
    val dms: List<Dm>,
    val live: List<Live>,
    /** Revisão monotônica do estado das transmissões: snapshot mais velho que o já visto chegou atrasado e é ignorado. */
    val liveRevision: Long = 0,
    val blocked: List<User> = emptyList(),
    val voice: List<VoiceState> = emptyList(),
    /** Tráfego de saída do servidor no mês UTC atual (`2026-11`) em bytes. */
    val egressMonth: String = "",
    val egressBytes: Long = 0,
)

@Serializable
data class Reply(val id: Long, val author: String, val content: String)

@Serializable
data class Attachment(
    val id: Long,
    val name: String,
    val mime: String,
    val size: Long,
    val width: Int,
    val height: Int,
)

@Serializable
data class Reaction(val emoji: String, val count: Int, val me: Boolean)

@Serializable
data class Message(
    val id: Long,
    val channelId: Long,
    val authorId: Long,
    val author: String,
    val content: String,
    /** Milissegundos desde a época Unix (UTC). */
    val ts: Long,
    val edited: Boolean,
    val reply: Reply? = null,
    val nonce: Long = 0,
    val reactions: List<Reaction> = emptyList(),
    /** 0 normal, 2 alguém entrou no servidor, 3 alguém fixou uma mensagem (`content` = id da fixada). */
    val kind: Int = 0,
    val pinned: Boolean = false,
    val attachments: List<Attachment> = emptyList(),
)

@Serializable
data class SearchHit(val message: Message, val channelName: String)

@Serializable
data class SessionInfo(
    val id: Long,
    val device: String,
    val created: Long,
    val lastUsed: Long,
    val current: Boolean,
)

/** Manifesto de atualização do desktop (HTTP). O app Android não usa, mas o contrato é testado. */
@Serializable
data class UpdateManifest(
    val version: String,
    val platform: String,
    val sha256: String,
    val size: Long,
    val notes: String = "",
    val signature: String = "",
)
