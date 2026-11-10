package app.telacast.tela

/** Captura e decodificador de ambientes sem tela de verdade (testes, prévias): abrir sempre falha com uma mensagem clara. */
object FonteSemCaptura : FonteDeTela {
    override fun fpsSuportados() = listOf(15, 30)
    override fun iniciar(concessao: Any, opcoes: OpcoesDeTransmissao, ouvinte: OuvinteDaFonte): String? = "Este ambiente não tem captura de tela."
    override fun reconfigurar(opcoes: OpcoesDeTransmissao): String? = "Este ambiente não tem captura de tela."
    override fun pedirQuadroChave() = Unit
    override fun parar() = Unit
}

object DecodificadorSemVideo : DecodificadorDeVideo {
    override fun configurar(superficie: Any, largura: Int, altura: Int): String? = "Este ambiente não tem decodificador de vídeo."
    override fun decodificar(h264: ByteArray, ptsUs: Long, alvoNs: Long) = false
    override fun fechar() = Unit
}
