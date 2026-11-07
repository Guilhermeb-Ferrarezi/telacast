package app.telacast.data

import app.telacast.data.ItemDaLista.Bloqueada
import app.telacast.data.ItemDaLista.DivisorDeDia
import app.telacast.data.ItemDaLista.Mensagem
import app.telacast.data.ItemDaLista.NovasMensagens
import app.telacast.data.ItemDaLista.Sistema
import app.telacast.proto.Message
import app.telacast.proto.Reply
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.ZoneId
import java.time.ZonedDateTime

class ItensDaListaTest {
    private val zona = ZoneId.of("America/Sao_Paulo")
    private fun ms(dia: Int, h: Int, min: Int) = ZonedDateTime.of(2026, 11, dia, h, min, 0, 0, zona).toInstant().toEpochMilli()
    private val agora = ms(7, 18, 0)
    private val eu = 1L

    private fun m(id: Long, autor: Long, ts: Long, kind: Int = 0, reply: Reply? = null) =
        Message(id, 9, autor, "u$autor", "t$id", ts, false, reply = reply, kind = kind)

    private fun montar(vararg msgs: Message, bloq: Set<Long> = emptySet(), rev: Set<Long> = emptySet(), marcador: Long? = null) =
        ItensDaLista.montar(msgs.toList(), eu, bloq, rev, marcador, agora, zona)

    private fun tipos(l: List<ItemDaLista>) = l.map {
        when (it) {
            is DivisorDeDia -> "dia:${it.rotulo}"
            NovasMensagens -> "novas"
            is Mensagem -> if (it.agrupada) "m${it.msg.id}+" else "m${it.msg.id}"
            is Sistema -> "sis${it.msg.id}"
            is Bloqueada -> "blq${it.msg.id}"
        }
    }

    @Test fun agrupaMesmoAutorAteCincoMinutos() {
        val l = montar(m(1, 2, ms(7, 10, 0)), m(2, 2, ms(7, 10, 4)), m(3, 2, ms(7, 10, 10)), m(4, 3, ms(7, 10, 11)))
        assertEquals(listOf("dia:hoje", "m1", "m2+", "m3", "m4"), tipos(l))
    }

    @Test fun cincoMinutosExatosJaQuebra() {
        assertEquals(listOf("dia:hoje", "m1", "m2"), tipos(montar(m(1, 2, ms(7, 10, 0)), m(2, 2, ms(7, 10, 5)))))
    }

    @Test fun respostaQuebraOAgrupamento() {
        val r = Reply(1, "u2", "x")
        assertEquals(listOf("dia:hoje", "m1", "m2"), tipos(montar(m(1, 2, ms(7, 10, 0)), m(2, 2, ms(7, 10, 1), reply = r))))
    }

    @Test fun mudancaDeDiaQuebraEPoeDivisorComRotulos() {
        val l = montar(m(1, 2, ms(5, 23, 59)), m(2, 2, ms(6, 0, 1)), m(3, 2, ms(7, 0, 2)))
        assertEquals(listOf("dia:05/11/2026", "m1", "dia:ontem", "m2", "dia:hoje", "m3"), tipos(l))
    }

    @Test fun avisoDeSistemaQuebraEAProximaNaoAgrupaComAAnterior() {
        val l = montar(m(1, 2, ms(7, 10, 0)), m(2, 3, ms(7, 10, 1), kind = 2), m(3, 2, ms(7, 10, 2)), m(4, 2, ms(7, 10, 3), kind = 3), m(5, 2, ms(7, 10, 4)))
        assertEquals(listOf("dia:hoje", "m1", "sis2", "m3", "sis4", "m5"), tipos(l))
    }

    @Test fun novasMensagensAntesDaPrimeiraDeOutraPessoaMaisNovaQueOMarcador() {
        val l = montar(m(10, 2, ms(7, 9, 0)), m(11, 3, ms(7, 9, 1)), m(12, 3, ms(7, 9, 2)), m(13, 2, ms(7, 9, 3)), marcador = 10)
        assertEquals(listOf("dia:hoje", "m10", "novas", "m11", "m12+", "m13"), tipos(l))
    }

    @Test fun mensagemMinhaLogoDepoisDoMarcadorNaoAbreODivisor() {
        // igual ao desktop: a regra exige que a mensagem anterior seja <= marcador
        val l = montar(m(10, 2, ms(7, 9, 0)), m(11, 1, ms(7, 9, 1)), m(12, 2, ms(7, 9, 2)), marcador = 10)
        assertEquals(listOf("dia:hoje", "m10", "m11", "m12"), tipos(l))
    }

    @Test fun semNadaMaisNovoNaoHaDivisor() {
        assertEquals(listOf("dia:hoje", "m10"), tipos(montar(m(10, 2, ms(7, 9, 0)), marcador = 10)))
        assertEquals(listOf("dia:hoje", "m10"), tipos(montar(m(10, 2, ms(7, 9, 0)), marcador = null)))
    }

    @Test fun bloqueadaRecolheEMostrarMensagemRevela() {
        val a = montar(m(1, 2, ms(7, 10, 0)), m(2, 5, ms(7, 10, 1)), m(3, 2, ms(7, 10, 2)), bloq = setOf(5))
        assertEquals(listOf("dia:hoje", "m1", "blq2", "m3"), tipos(a))
        val b = montar(m(1, 2, ms(7, 10, 0)), m(2, 5, ms(7, 10, 1)), bloq = setOf(5), rev = setOf(2))
        assertEquals(listOf("dia:hoje", "m1", "m2"), tipos(b))
        // aviso de sistema de bloqueado não é recolhido (kind != 0)
        assertEquals(listOf("dia:hoje", "sis1"), tipos(montar(m(1, 5, ms(7, 10, 0), kind = 2), bloq = setOf(5))))
    }

    @Test fun provisoriaVemMarcadaComoPendente() {
        val l = montar(m(1, 2, ms(7, 10, 0)), m(-77, 1, ms(7, 10, 1)))
        assertTrue((l.last() as Mensagem).pendente)
        assertEquals("m-77", l.last().chave)
    }

    @Test fun formatos() {
        assertEquals("14:05", Formatacao.hora(ms(7, 14, 5), zona))
        assertEquals("07/11/2026", Formatacao.data(ms(7, 14, 5), zona))
        assertEquals("512 B", Formatacao.tamanho(512))
        assertEquals("1.5 KB", Formatacao.tamanho(1536))
        assertEquals("2.0 MB", Formatacao.tamanho(2 shl 20))
        assertEquals("há 5 minutos", Formatacao.relativo(-300))
        assertEquals("em uma hora", Formatacao.relativo(3600))
        assertEquals("há alguns segundos", Formatacao.relativo(-10))
        assertEquals("sábado, 7 de novembro de 2026 14:05", Formatacao.carimbo(ms(7, 14, 5) / 1000, 'F', 0, zona))
    }
}
