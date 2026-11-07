package app.telacast.proto

import kotlinx.serialization.SerializationException
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Test

/**
 * Contrato do protocolo: lê os MESMOS arquivos que o teste Rust (`cargo test -p telacast-proto`) grava e confere,
 * em `proto/fixtures/` (relativo à raiz do repositório; o Gradle passa o caminho em `telacast.fixtures`).
 *
 * Mudou o protocolo no Rust: `UPDATE_FIXTURES=1 cargo test -p telacast-proto -j2`, e este teste mostra o que o
 * Kotlin precisa acompanhar (variante nova sem classe aqui, campo novo, etc.).
 */
class ProtoFixturesTest {
    private fun arvore(texto: String): JsonElement = Json.parseToJsonElement(texto)

    // ---------------------------------------------------------------- S2C

    @Test
    fun cadaFixtureS2cDecodificaERecodificaIgual() {
        val arquivos = Fixtures.arquivos("s2c")
        assertTrue("nenhuma fixture em s2c/", arquivos.isNotEmpty())
        for (f in arquivos) {
            val original = arvore(f.readText())
            val msg = try { Proto.lerS2C(f.readText()) } catch (e: Exception) { fail("${f.name} não decodifica: $e"); return }
            assertFalse("${f.name} caiu em Desconhecida: falta a variante no Kotlin", msg is S2C.Desconhecida)
            assertEquals("${f.name}: recodificação difere do arquivo", original, Proto.s2cParaArvore(msg))
            // e o caminho pelo texto também
            assertEquals("${f.name}: texto recodificado difere", original, arvore(Proto.escreverS2C(msg)))
        }
    }

    @Test
    fun cadaFixtureC2sDecodificaERecodificaIgual() {
        val arquivos = Fixtures.arquivos("c2s")
        assertTrue("nenhuma fixture em c2s/", arquivos.isNotEmpty())
        for (f in arquivos) {
            val original = arvore(f.readText())
            val msg = try { Proto.lerC2S(f.readText()) } catch (e: Exception) { fail("${f.name} não decodifica: $e"); return }
            assertEquals("${f.name}: recodificação difere do arquivo", original, Proto.c2sParaArvore(msg))
            assertEquals("${f.name}: texto recodificado difere", original, arvore(Proto.escreverC2S(msg)))
        }
    }

    /** O contrário do teste Rust: fixture sem variante Kotlin, ou variante Kotlin sem fixture, quebra aqui. */
    @Test
    fun listaDeVariantesNaoDiverge() {
        val s2c = Fixtures.arquivos("s2c").map { Fixtures.variante(it) }.toSet()
        val c2s = Fixtures.arquivos("c2s").map { Fixtures.variante(it) }.toSet()
        assertEquals("S2C: fixtures x variantes do Kotlin", s2c, S2C.ETIQUETAS)
        assertEquals("C2S: fixtures x variantes do Kotlin", c2s, C2S.ETIQUETAS)
    }

    @Test
    fun http() {
        val up = Json.parseToJsonElement(Fixtures.texto("http/upload", "UploadResponse.json"))
        val anexo = Proto.json.decodeFromJsonElement(Attachment.serializer(), up)
        assertEquals(up, Proto.json.encodeToJsonElement(Attachment.serializer(), anexo))

        val um = Json.parseToJsonElement(Fixtures.texto("http/update", "UpdateManifest.json"))
        val manifesto = Proto.json.decodeFromJsonElement(UpdateManifest.serializer(), um)
        assertEquals(um, Proto.json.encodeToJsonElement(UpdateManifest.serializer(), manifesto))
    }

    // ---------------------------------------------------------------- extras (proto/fixtures/extras/LEIA-ME.md)

    private fun extra(nome: String) = Fixtures.texto("extras", nome)

    /** Entrada e `.esperado.json` decodificam para o mesmo objeto, e a entrada recodificada é o esperado. */
    private fun confereS2c(caso: String): S2C {
        val entrada = Proto.lerS2C(extra("$caso.json"))
        val esperadoTexto = extra("$caso.esperado.json")
        assertEquals("$caso: entrada e esperado devem dar o mesmo objeto", Proto.lerS2C(esperadoTexto), entrada)
        assertEquals("$caso: recodificada difere do esperado", arvore(esperadoTexto), Proto.s2cParaArvore(entrada))
        return entrada
    }

    @Test fun extraErrorCodigoDesconhecido() {
        val e = confereS2c("error-codigo-desconhecido") as S2C.Error
        assertEquals(ErrorCode.Unknown, e.code)
        assertTrue(e.msg.isNotEmpty())
    }

    @Test fun extraErrorSemCode() {
        assertEquals(ErrorCode.Unknown, (confereS2c("error-sem-code") as S2C.Error).code)
    }

    @Test fun extraStatusDesconhecidoViraOffline() {
        assertEquals(Status.Offline, (confereS2c("status-desconhecido") as S2C.Auth).me.status)
    }

    @Test fun extraUsuarioSemStatus() {
        assertEquals(Status.Offline, (confereS2c("usuario-sem-status") as S2C.Auth).me.status)
    }

    @Test fun extraCamposNovosIgnorados() {
        val m = (confereS2c("campos-novos-ignorados") as S2C.Message).message
        assertNotNull(m.reply)
    }

    @Test fun extraMensagemMinima() {
        val m = (confereS2c("mensagem-minima") as S2C.Message).message
        assertEquals(null, m.reply)
        assertEquals(0L, m.nonce)
        assertEquals(emptyList<Reaction>(), m.reactions)
        assertEquals(0, m.kind)
        assertFalse(m.pinned)
        assertEquals(emptyList<Attachment>(), m.attachments)
    }

    @Test fun extraSnapshotMinimo() {
        val s = (confereS2c("snapshot-minimo") as S2C.Snapshot).snapshot
        assertEquals(emptyList<User>(), s.blocked)
        assertEquals(emptyList<VoiceState>(), s.voice)
        assertEquals(Status.Offline, s.me.status)
        assertTrue(s.servers.flatMap { it.members }.all { it.status == Status.Offline })
        assertTrue(s.servers.flatMap { it.channels }.all { it.mentions == 0 && it.parent == null })
        assertTrue(s.dms.all { it.mentions == 0 && it.user.status == Status.Offline })
    }

    @Test fun extraLoginSemProto() {
        val esperado = extra("login-sem-proto.esperado.json")
        val entrada = Proto.lerC2S(extra("login-sem-proto.json"))
        assertEquals(0, (entrada as C2S.Login).proto)
        assertEquals(Proto.lerC2S(esperado), entrada)
        assertEquals(arvore(esperado), Proto.c2sParaArvore(entrada))
    }

    @Test fun extraSendMinimo() {
        val esperado = extra("send-minimo.esperado.json")
        val entrada = Proto.lerC2S(extra("send-minimo.json")) as C2S.Send
        assertEquals(null, entrada.replyTo)
        assertEquals(0L, entrada.nonce)
        assertEquals(emptyList<Long>(), entrada.attachments)
        assertEquals(arvore(esperado), Proto.c2sParaArvore(entrada))
    }

    @Test fun extraManifestoMinimo() {
        val entrada = Proto.json.decodeFromString(UpdateManifest.serializer(), extra("manifesto-minimo.json"))
        assertEquals("", entrada.notes)
        assertEquals("", entrada.signature)
        val esperado = extra("manifesto-minimo.esperado.json")
        assertEquals(arvore(esperado), Proto.json.encodeToJsonElement(UpdateManifest.serializer(), entrada))
    }

    @Test fun extraVarianteDesconhecidaViraDesconhecida() {
        val m = Proto.lerS2C(extra("s2c-variante-desconhecida.json"))
        assertEquals(S2C.Desconhecida("ThreadCreated"), m)
        assertEquals(m, Proto.lerS2COuNulo(extra("s2c-variante-desconhecida.json")))
    }

    /** Contrato de hoje: `FriendStatus` não tem valor de reserva, então o `Snapshot` inteiro falha (igual ao Rust). */
    @Test fun extraFriendStatusDesconhecidoFalhaOSnapshotInteiro() {
        try {
            Proto.lerS2C(extra("friend-status-desconhecido.json"))
            fail("deveria falhar")
        } catch (_: SerializationException) {
        }
        assertEquals(null, Proto.lerS2COuNulo(extra("friend-status-desconhecido.json")))
    }

    // ---------------------------------------------------------------- regras gerais

    @Test fun idsSaoLongSemPerderPrecisao() {
        val m = Proto.lerS2C("""{"Typing":{"channel_id":112624001234567890,"user_id":9007199254740993}}""") as S2C.Typing
        assertEquals(112624001234567890L, m.channelId)
        assertEquals(9007199254740993L, m.userId) // 2^53 + 1: um Double perderia o último dígito
        assertTrue(Proto.escreverS2C(m).contains("9007199254740993"))
    }

    @Test fun varianteUnitariaEhStringPura() {
        assertEquals(JsonPrimitive("Logout"), Proto.c2sParaArvore(C2S.Logout))
        assertEquals("\"Logout\"", Proto.escreverC2S(C2S.Logout))
        assertEquals(C2S.Logout, Proto.lerC2S("\"Logout\""))
        assertEquals(S2C.PasswordChanged, Proto.lerS2C("\"PasswordChanged\""))
    }

    @Test fun varianteDeTuplaNaoTemCamadaExtra() {
        val arv = Proto.s2cParaArvore(Proto.lerS2C(Fixtures.texto("s2c", "Snapshot.vazio.json"))) as JsonObject
        assertEquals(setOf("Snapshot"), arv.keys)
        assertTrue((arv["Snapshot"] as JsonObject).containsKey("me"))
    }

    @Test fun optionVazioSaiComoNull() {
        val txt = Proto.escreverC2S(C2S.Send(channelId = 1, content = "oi", nonce = 5))
        assertTrue(txt, txt.contains("\"reply_to\":null"))
    }

    @Test fun lixoNaoDerrubaOLeitorDeQuadros() {
        assertEquals(null, Proto.lerS2COuNulo("isto não é json"))
        assertEquals(null, Proto.lerS2COuNulo("""{"Auth":{"token":1}}"""))
        assertEquals(null, Proto.lerS2COuNulo("""[1,2]"""))
    }
}
