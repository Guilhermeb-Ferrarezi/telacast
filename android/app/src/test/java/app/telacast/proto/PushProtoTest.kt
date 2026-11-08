package app.telacast.proto

import org.junit.Assert.assertEquals
import org.junit.Test

/** Variantes de push (D1) do C2S, na forma "externamente etiquetada" do serde. */
class PushProtoTest {
    private fun ida(c: C2S) = Proto.json.encodeToString(C2S.serializer(), c)
    private fun volta(j: String) = Proto.json.decodeFromString(C2S.serializer(), j)

    @Test fun pushRegister() {
        val c = C2S.PushRegister("tok", "fcm")
        assertEquals("""{"PushRegister":{"token":"tok","platform":"fcm"}}""", ida(c))
        assertEquals(c, volta(ida(c)))
    }

    @Test fun pushUnregister() {
        val c = C2S.PushUnregister("tok")
        assertEquals("""{"PushUnregister":{"token":"tok"}}""", ida(c))
        assertEquals(c, volta(ida(c)))
    }

    @Test fun setFocus() {
        assertEquals("""{"SetFocus":{"focused":true}}""", ida(C2S.SetFocus(true)))
        assertEquals(C2S.SetFocus(false), volta("""{"SetFocus":{"focused":false}}"""))
    }
}
