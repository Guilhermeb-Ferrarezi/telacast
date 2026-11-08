package app.telacast.atualizacao

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.currentTime
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import net.i2p.crypto.eddsa.EdDSAEngine
import net.i2p.crypto.eddsa.KeyPairGenerator
import net.i2p.crypto.eddsa.EdDSAPublicKey
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.security.MessageDigest

private val PAR = KeyPairGenerator().generateKeyPair()
private val CHAVE_PUBLICA: ByteArray = (PAR.public as EdDSAPublicKey).abyte
private val OUTRO_PAR = KeyPairGenerator().generateKeyPair()

private fun sha(b: ByteArray) = bytesParaHex(MessageDigest.getInstance("SHA-256").digest(b))

private fun assinar(m: ManifestoDeAtualizacao, par: java.security.KeyPair = PAR): ManifestoDeAtualizacao {
    val e = EdDSAEngine(MessageDigest.getInstance("SHA-512"))
    e.initSign(par.private)
    e.update(mensagemAssinada(m.version, m.platform, m.sha256, m.size))
    return m.copy(signature = bytesParaHex(e.sign()))
}

private fun manifesto(apk: ByteArray, versao: String = "0.2.0", plataforma: String = PLATAFORMA_ANDROID) =
    assinar(ManifestoDeAtualizacao(versao, plataforma, sha(apk), apk.size.toLong(), "Voz melhor e menos travadas."))

class VerificacaoTest {
    private val apk = ByteArray(5000) { (it * 31).toByte() }
    private val v = VerificadorDeAtualizacao(CHAVE_PUBLICA)

    @Test fun manifestoBomPassa() = assertEquals(VerificadorDeAtualizacao.Resultado.Ok, v.conferirManifesto(manifesto(apk), "0.1.0"))

    @Test fun versaoIgualOuMenorNaoEhNovidade() {
        assertEquals(VerificadorDeAtualizacao.Resultado.SemNovidade, v.conferirManifesto(manifesto(apk, "0.1.0"), "0.1.0"))
        assertEquals(VerificadorDeAtualizacao.Resultado.SemNovidade, v.conferirManifesto(manifesto(apk, "0.1.0"), "0.2.0"))
        assertEquals("sufixo é ignorado", VerificadorDeAtualizacao.Resultado.SemNovidade, v.conferirManifesto(manifesto(apk, "0.1.0-beta"), "0.1.0"))
    }

    @Test fun assinaturaRuimEhRecusada() {
        val m = manifesto(apk)
        val ruim = m.copy(signature = m.signature.replaceRange(0, 2, if (m.signature.startsWith("00")) "ff" else "00"))
        assertTrue(v.conferirManifesto(ruim, "0.1.0") is VerificadorDeAtualizacao.Resultado.Recusado)
        assertTrue("sem assinatura", v.conferirManifesto(m.copy(signature = ""), "0.1.0") is VerificadorDeAtualizacao.Resultado.Recusado)
        assertTrue("lixo", v.conferirManifesto(m.copy(signature = "xyz"), "0.1.0") is VerificadorDeAtualizacao.Resultado.Recusado)
        assertTrue("assinatura de outra chave", v.conferirManifesto(assinar(m, OUTRO_PAR), "0.1.0") is VerificadorDeAtualizacao.Resultado.Recusado)
    }

    @Test fun trocarQualquerCampoAssinadoInvalida() {
        val m = manifesto(apk)
        assertTrue(v.conferirManifesto(m.copy(version = "9.9.9"), "0.1.0") is VerificadorDeAtualizacao.Resultado.Recusado)
        assertTrue(v.conferirManifesto(m.copy(sha256 = sha(byteArrayOf(1))), "0.1.0") is VerificadorDeAtualizacao.Resultado.Recusado)
        assertTrue(v.conferirManifesto(m.copy(size = m.size + 1), "0.1.0") is VerificadorDeAtualizacao.Resultado.Recusado)
        assertEquals("as notas não são assinadas (só informam)", VerificadorDeAtualizacao.Resultado.Ok, v.conferirManifesto(m.copy(notes = "outra"), "0.1.0"))
    }

    @Test fun plataformaErradaEhRecusada() = assertTrue(v.conferirManifesto(manifesto(apk, plataforma = "linux-x86_64"), "0.1.0") is VerificadorDeAtualizacao.Resultado.Recusado)

    @Test fun tamanhoOuHashInvalidosNoManifestoSaoRecusados() {
        assertTrue(v.conferirManifesto(assinar(ManifestoDeAtualizacao("0.2.0", PLATAFORMA_ANDROID, "abc", 10)), "0.1.0") is VerificadorDeAtualizacao.Resultado.Recusado)
        assertTrue(v.conferirManifesto(assinar(ManifestoDeAtualizacao("0.2.0", PLATAFORMA_ANDROID, sha(apk), 0)), "0.1.0") is VerificadorDeAtualizacao.Resultado.Recusado)
        assertTrue(v.conferirManifesto(assinar(ManifestoDeAtualizacao("0.2.0", PLATAFORMA_ANDROID, sha(apk), 400L shl 20)), "0.1.0") is VerificadorDeAtualizacao.Resultado.Recusado)
    }

    @Test fun arquivoComHashOuTamanhoErradoEhRecusado() {
        val m = manifesto(apk)
        assertEquals(VerificadorDeAtualizacao.Resultado.Ok, v.conferirArquivo(m, apk.size.toLong(), sha(apk)))
        assertEquals(VerificadorDeAtualizacao.Resultado.Ok, v.conferirArquivo(m, apk.size.toLong(), sha(apk).uppercase()))
        assertTrue(v.conferirArquivo(m, apk.size.toLong(), sha(byteArrayOf(9))) is VerificadorDeAtualizacao.Resultado.Recusado)
        assertTrue(v.conferirArquivo(m, apk.size - 1L, sha(apk)) is VerificadorDeAtualizacao.Resultado.Recusado)
    }

    @Test fun semChavePublicaAAtualizacaoFicaDesligada() {
        val sem = VerificadorDeAtualizacao(null)
        assertFalse(sem.ligada)
        assertEquals(VerificadorDeAtualizacao.Resultado.SemNovidade, sem.conferirManifesto(manifesto(apk), "0.1.0"))
        assertFalse(VerificadorDeAtualizacao(ByteArray(5)).ligada)
    }

    @Test fun comparacaoDeVersoes() {
        assertTrue(versaoMaisNova("0.2.0", "0.1.9")); assertTrue(versaoMaisNova("1.0.0", "0.99.99")); assertTrue(versaoMaisNova("0.10.0", "0.9.0"))
        assertFalse(versaoMaisNova("0.1.0", "0.1.0")); assertFalse(versaoMaisNova("0.1.0", "0.2.0")); assertTrue(versaoMaisNova("v0.2", "0.1.5"))
        assertFalse(versaoMaisNova("lixo", "0.0.1"))
    }

    @Test fun hexVaiEVolta() {
        assertEquals("00ff10", bytesParaHex(hexParaBytes("00FF10")!!)); assertNull(hexParaBytes("0")); assertNull(hexParaBytes("zz")); assertNull(hexParaBytes(""))
    }
}

private class ApiFalsa(var manifesto: ManifestoDeAtualizacao?, var conteudo: ByteArray) : ApiDeAtualizacao {
    var consultas = 0; var downloads = 0; var falha: String? = null; var adulterar = false
    override suspend fun manifesto(plataforma: String): ManifestoDeAtualizacao? { consultas++; return manifesto }
    override suspend fun baixar(m: ManifestoDeAtualizacao, destino: File, progresso: (Long) -> Unit): ResultadoDoDownload {
        downloads++
        falha?.let { return ResultadoDoDownload.Falhou(it) }
        val b = if (adulterar) conteudo.copyOf().also { it[0] = (it[0] + 1).toByte() } else conteudo
        destino.writeBytes(b); progresso(b.size.toLong() / 2); progresso(b.size.toLong())
        return ResultadoDoDownload.Baixado(b.size.toLong(), sha(b))
    }
}

private class InstaladorFalso : InstaladorDeApk {
    var pode = true; var erroValidar: String? = null; var instalados = ArrayList<File>(); var permissoes = 0
    override fun podeInstalar() = pode
    override fun validar(apk: File, versaoNome: String) = erroValidar
    override fun instalar(apk: File): String? { instalados += apk; return null }
    override fun abrirPermissao() { permissoes++ }
}

@OptIn(ExperimentalCoroutinesApi::class)
class ControladorDeAtualizacaoTest {
    private val apk = ByteArray(8000) { (it * 7).toByte() }
    private val pasta = java.nio.file.Files.createTempDirectory("tc-atu").toFile()

    private inner class Amb(t: TestScope, val api: ApiFalsa = ApiFalsa(manifesto(apk), apk), chave: ByteArray? = CHAVE_PUBLICA, var vista: String = "") {
        val inst = InstaladorFalso()
        val c = ControladorDeAtualizacao(t.backgroundScope, api, VerificadorDeAtualizacao(chave), inst, "0.1.0", pasta, { vista }, { vista = it }, { t.currentTime })
        val t0 = t
        fun rodar() = t0.runCurrent()
    }

    private fun TestScope.amb(api: ApiFalsa = ApiFalsa(manifesto(apk), apk), chave: ByteArray? = CHAVE_PUBLICA, vista: String = "") = Amb(this, api, chave, vista)

    @Test fun versaoNovaAparecePrimeiraVezComAFolha() = runTest {
        val a = amb(); a.c.verificar(true); a.rodar()
        val e = a.c.estado.value
        assertEquals(FaseDaAtualizacao.Disponivel, e.fase); assertEquals("0.2.0", e.manifesto!!.version); assertTrue(e.folhaPendente)
    }

    @Test fun dispensarEscondeAFolhaMasMantemAFaixaEMarcaComoVista() = runTest {
        val a = amb(); a.c.verificar(true); a.rodar()
        a.c.dispensarFolha()
        assertFalse(a.c.estado.value.folhaPendente); assertEquals(FaseDaAtualizacao.Disponivel, a.c.estado.value.fase); assertEquals("0.2.0", a.vista)
        val outra = amb(vista = "0.2.0"); outra.c.verificar(true); outra.rodar()
        assertFalse("na abertura seguinte a folha não volta", outra.c.estado.value.folhaPendente); assertEquals(FaseDaAtualizacao.Disponivel, outra.c.estado.value.fase)
    }

    @Test fun atualizarBaixaVerificaEInstala() = runTest {
        val a = amb(); a.c.verificar(true); a.rodar()
        a.c.atualizar(); a.rodar()
        assertEquals(FaseDaAtualizacao.Instalando, a.c.estado.value.fase)
        assertEquals(1, a.api.downloads)
        val f = a.inst.instalados.single()
        assertEquals(apk.toList(), f.readBytes().toList())
        assertFalse("o arquivo parcial não fica", File(pasta, "telacast-0.2.0.apk.parte").exists())
    }

    @Test fun hashDiferenteNaoInstalaEApagaTudo() = runTest {
        val a = amb(); a.api.adulterar = true; a.c.verificar(true); a.rodar()
        a.c.atualizar(); a.rodar()
        assertEquals(FaseDaAtualizacao.Erro, a.c.estado.value.fase)
        assertTrue(a.c.estado.value.mensagem!!.contains("hash"))
        assertTrue(a.inst.instalados.isEmpty())
        assertFalse(File(pasta, "telacast-0.2.0.apk").exists())
    }

    @Test fun falhaDeRedeNoDownloadMostraErroComMensagem() = runTest {
        val a = amb(); a.api.falha = "sem rede"; a.c.verificar(true); a.rodar(); a.c.atualizar(); a.rodar()
        assertEquals(FaseDaAtualizacao.Erro, a.c.estado.value.fase); assertTrue(a.c.estado.value.mensagem!!.contains("sem rede"))
        a.api.falha = null; a.c.atualizar(); a.rodar()
        assertEquals("tentar de novo funciona", FaseDaAtualizacao.Instalando, a.c.estado.value.fase)
    }

    @Test fun apkDeOutroPacoteOuVersaoMenorNaoInstala() = runTest {
        val a = amb(); a.inst.erroValidar = "O arquivo baixado é de outro aplicativo (x)."; a.c.verificar(true); a.rodar(); a.c.atualizar(); a.rodar()
        assertEquals(FaseDaAtualizacao.Erro, a.c.estado.value.fase); assertTrue(a.inst.instalados.isEmpty())
    }

    @Test fun semPermissaoDeInstalarPedeEContinuaAoVoltar() = runTest {
        val a = amb(); a.inst.pode = false; a.c.verificar(true); a.rodar(); a.c.atualizar(); a.rodar()
        assertEquals(FaseDaAtualizacao.PrecisaDePermissao, a.c.estado.value.fase); assertTrue(a.inst.instalados.isEmpty())
        a.c.abrirPermissao(); assertEquals(1, a.inst.permissoes)
        a.c.retomar(); a.rodar()
        assertEquals("ainda sem permissão: continua esperando", FaseDaAtualizacao.PrecisaDePermissao, a.c.estado.value.fase)
        a.inst.pode = true; a.c.retomar(); a.rodar()
        assertEquals(FaseDaAtualizacao.Instalando, a.c.estado.value.fase); assertEquals(1, a.inst.instalados.size)
        assertEquals("não baixa de novo", 1, a.api.downloads)
    }

    @Test fun agoraNaoNaFolhaDaPermissaoVoltaAOferecer() = runTest {
        val a = amb(); a.inst.pode = false; a.c.verificar(true); a.rodar(); a.c.atualizar(); a.rodar()
        a.c.cancelarPermissao()
        assertEquals(FaseDaAtualizacao.Pronta, a.c.estado.value.fase)
    }

    @Test fun consultaSemRedeEhSilenciosa() = runTest {
        val a = amb(ApiFalsa(null, apk)); a.c.verificar(true); a.rodar()
        assertEquals(FaseDaAtualizacao.Nenhuma, a.c.estado.value.fase); assertNull(a.c.estado.value.mensagem)
    }

    @Test fun manifestoComAssinaturaRuimNaoMostraNada() = runTest {
        val a = amb(ApiFalsa(manifesto(apk).copy(signature = "00".repeat(64)), apk)); a.c.verificar(true); a.rodar()
        assertEquals(FaseDaAtualizacao.Nenhuma, a.c.estado.value.fase)
    }

    @Test fun semChaveAAtualizacaoNemConsulta() = runTest {
        val a = amb(chave = null); a.c.verificar(true); a.rodar()
        assertEquals(0, a.api.consultas); assertFalse(a.c.ligada)
    }

    @Test fun consultaTemIntervaloMinimoDe10MinutosMasAberturaForca() = runTest {
        val a = amb(); a.c.verificar(true); a.rodar()
        a.c.verificar(); a.rodar(); assertEquals(1, a.api.consultas)
        a.t0.advanceTimeBy(9 * 60_000); a.c.verificar(); a.rodar(); assertEquals(1, a.api.consultas)
        a.t0.advanceTimeBy(2 * 60_000); a.c.verificar(); a.rodar(); assertEquals(2, a.api.consultas)
        a.c.verificar(true); a.rodar(); assertEquals(3, a.api.consultas)
    }

    @Test fun baixadoEmSegundoPlanoFicaProntoEVerificado() = runTest {
        val a = amb()
        val m = kotlinx.coroutines.runBlocking { a.c.baixarEmSegundoPlano() }
        assertNotNull(m); assertEquals(1, a.api.downloads); assertTrue(a.inst.instalados.isEmpty())
        val depois = amb(); depois.c.verificar(true); depois.rodar()
        assertEquals("o app, ao abrir, já acha o APK pronto", FaseDaAtualizacao.Pronta, depois.c.estado.value.fase)
        depois.c.instalar(); depois.rodar()
        assertEquals("instala sem baixar de novo", 0, depois.api.downloads); assertEquals(1, depois.inst.instalados.size)
    }

    @Test fun trabalhoEmSegundoPlanoComHashRuimNaoDeixaNada() = runTest {
        val a = amb(); a.api.adulterar = true
        assertNull(kotlinx.coroutines.runBlocking { a.c.baixarEmSegundoPlano() })
        assertFalse(File(pasta, "telacast-0.2.0.apk").exists())
    }
}
