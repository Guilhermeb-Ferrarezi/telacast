# Telacast Android: progresso

Plano: `docs/MOBILE-PLAN.md` (raiz do repositório). Este arquivo é o estado da execução: é dele que se retoma.

## Flag

`ANDROID_INSTALL=0`  (nunca rodar `adb install` nem tocar no celular/emulador; só mudar para 1 quando o usuário mandar)

## Passo atual

A1 a A3 e B1 a B8 estão commitados. **C2 (chat de voz) feito, aguardando revisão e commit; falta provar em aparelho.** **C3 (miniaturas de imagem) feito, aguardando revisão e commit.** Próximo: D1 (push por FCM, contrato em definição). Depois: **fase C (vídeo: assistir e transmitir)** e **D (notificações, acabamento)**; B9 do plano original foi cumprido aqui (ajustes).

## Feito

- A1 a A3, B1 a B6: ver os commits (login, gaveta, canais, mensagens, markdown, anexos, compositor, folha de ações, histórico).
- **B7 Início completo** (`ui/Amigos.kt`, `data/ItensDeAmigos.kt`): abas Disponível, Todos, Pendente, Bloqueados e Adicionar amigo; busca local por nome; seções "ONLINE — N", "TODOS OS AMIGOS — N", "RECEBIDOS — N", "ENVIADOS — N", "BLOQUEADOS — N"; linha de pessoa com avatar de 40 (toque abre o perfil), botões redondos (Mensagem e Mais; Aceitar e Recusar; Cancelar), toque na linha abre a DM; Desbloquear; estados vazios com os textos do `docs/ui-spec/07`; Adicionar amigo tira o `@` e só envia com texto; confirmações para desfazer amizade e bloquear.
- **B8 Perfil** (`ui/Perfil.kt`): folha inferior (a mesma `FolhaBase`), aberta ao tocar em avatar ou nome da mensagem, em menção `<@id>`, em membro do painel da direita e no avatar da lista de amigos. Faixa lisa na cor da pessoa, avatar de 80 com presença, nome, `@usuário`, status, "No Telacast desde" (data do snowflake), Mensagem, Adicionar/Desfazer amizade/Aceitar/Cancelar pedido, Bloquear/Desbloquear (com confirmação), Copiar ID, e "Ajustes e conta" no próprio perfil.
- **B8 Fixadas** (`ui/Fixadas.kt`): botão de alfinete na barra do canal e o link "Ver todas as mensagens fixadas" abrem a lista; tocar leva até a mensagem (`Repository.irParaMensagem`, que abre o canal e salta quando o histórico chega). A lista se atualiza sozinha quando alguém fixa ou desafixa.
- **B8 Busca** (`ui/Busca.kt`, `data/Busca.kt`): lupa na barra do canal ou da DM; campo, escopo (No servidor, Neste canal; DM sempre a própria conversa), filtros `de:nome`, `de:eu` e `em:#canal` (o mesmo `parse_search` do desktop), contagem, cartões de resultado (`# canal`, avatar, autor, dia e hora, markdown), "Mostrar mais" (paginação por `before`), estados Buscando, sem resultados e inicial. `AppState.busca` guarda o resultado; resposta de nonce antigo é ignorada.
- **B8 Ajustes** (`ui/Ajustes.kt`): menu do cartão do usuário ganhou "Ajustes". Conta, trocar senha (valida 8 a 128 e igualdade; limpa os campos quando o servidor confirma), sessões abertas (Encerrar, Encerrar todas as outras), aparência mínima (tamanho do texto pequeno/normal/grande, salvo em `Preferencias.tamanhoDoTexto` e aplicado ao Markdown por `LocalEscalaDoTexto`; tema só Grafite escuro) e Sair da conta.
- Imagens: `docs/telas/*.png` (83 com a voz), geradas por `TelasSnapshotTest`, `ConversaSnapshotTest`, `PerfilBuscaSnapshotTest` e `VozSnapshotTest` com `AppState` falso.
- Verificado: 258 testes unitários (imagens: 24 + 24 + 21 + paleta; busca/amigos 9; ViewModel 15 + 23; repository 35; demais como antes) e 14 de integração (novos: amizade, DM e bloqueio; fixadas e busca; senha errada e trocada, sessões); `verifyPaparazziDebug` verde.

## C2: chat de voz (código em `voz/`, telas em `ui/Voz.kt`)

**Codec: Concentus 1.0.2 (Opus em Java puro, `io.github.jaredmdobson:concentus`).** Alternativas pesadas:
- `MediaCodec` tem codificador Opus só a partir do Android 10 e depende do fabricante/ROM (o Redmi roda ROM custom, onde isso varia); não roda na JVM, então não há teste de integração nem de round-trip com o servidor real.
- libopus pelo NDK exigiria NDK e CMake (não há no PC de build), mais um `.so` por ABI e JNI.
- Concentus roda igual na JVM e no aparelho: o mesmo código é testado com Opus de verdade (codifica e decodifica) e na integração. Custo: ~230 KB no APK e CPU; usamos complexidade 5, VBR restrito, 32 kbps, sinal de voz, sem FEC e sem DTX (como o desktop). Medir o consumo de CPU no Redmi é uma das coisas que só o aparelho confirma.

Camadas (tudo puro e testado na JVM, exceto `AudioAndroid.kt` e `ServicoAndroid.kt`):
- `Opus.kt` (interfaces e Concentus), `QuadroDeVoz.kt` (binário `[1][seq u32 BE][opus]` e `[1][remetente i64][seq][opus]`, descarta o que passa de 400 bytes; usa as fixtures hex de `proto/fixtures/binary`).
- `Dsp.kt`: VAD por energia com histerese de 5 dB, hangover de 18 quadros, piso de ruído adaptativo e sensibilidade 0,7 (portado do `voice.rs`); limitador suave (joelho 0,7); suavizador de bordas (declick); medidor de nível.
- `JitterBuffer.kt`: alvo 2 a 5 quadros, ressincroniza, PLC (null no decodificador), volta do contador de sequência, no máximo 32 pacotes por usuário. `Recepcao.kt`: um fluxo por usuário, mistura, volume 0 a 200%, "falando" (pacote nos últimos 250 ms), limpa quem some.
- `Envio.kt`: VAD, preroll de 5 pacotes, quem fica em silêncio não envia, `seq` só avança nos pacotes enviados. `Motor.kt` junta envio e recepção; `capturar(quadro)` e `reproduzir(saida)` são as duas pontas que o dispositivo chama.
- `Controlador.kt`: máquina de estados `Fora -> Entrando -> Conectada -> Reconectando`. Entrando manda `VoiceJoin` e espera o `VoiceUpdate` do próprio usuário (8 s de limite); só então cria o motor e o dispositivo (e manda `VoiceSet` se mudo/surdo foi escolhido antes). Queda da conexão: para o áudio, vira Reconectando e entra de novo sozinho quando o gateway volta. Surdo implica mudo; ligar o mic estando surdo desliga o surdo (como no desktop). Trocar de canal e ser expulso são tratados. O dispositivo, o serviço e o Opus entram por interfaces (testes usam falsos).
- `AudioAndroid.kt`: `AudioRecord(VOICE_COMMUNICATION)` e `AudioTrack(USAGE_VOICE_COMMUNICATION)` a 48 kHz mono 16 bits, duas threads URGENT_AUDIO, `MODE_IN_COMMUNICATION`, foco de áudio, `NoiseSuppressor`, `AcousticEchoCanceler` e `AutomaticGainControl` quando o aparelho tem e as Opções deixam, rota alto-falante/fone (`setCommunicationDevice` no 12+, `isSpeakerphoneOn` antes). Fone Bluetooth/com fio é escolhido sozinho se já estiver conectado ao entrar.
- `ServicoAndroid.kt`: serviço em primeiro plano `foregroundServiceType="microphone"` com notificação contínua (canal "voz", categoria chamada, toque abre o app, ação "Sair da voz"), iniciado quando a chamada conecta e parado ao sair; mantém a voz viva com o app em segundo plano.
- Permissões: `RECORD_AUDIO` só é pedida quando a pessoa toca num canal de voz ou em "Entrar na voz" (junto, `POST_NOTIFICATIONS` no Android 13+; negar a segunda não impede nada). Negou o microfone: aviso e a tela mostra o erro com o caminho em Ajustes do Android.

Interface:
- Tocar num canal de voz abre a tela do canal e entra (como no desktop); se já está nele, só abre. `Destino.Voz` guarda o centro.
- Tela do canal (`TelaDeVoz`): grade de participantes (avatar com anel verde quando fala, ícones de mudo e surdo; o seu estado vem do controlador, o dos outros do servidor), estado ("Conectando…", "Voz conectada · 01:23", "Sem conexão. Reconectando…", "Você não está neste canal"), faixa de erro, medidor do microfone e botões redondos Mudo, Surdo, Alto-falante/Fone e Sair. Tocar num participante abre a folha de volume (0 a 200%).
- Cartão "Voz conectada" na gaveta, acima do cartão do usuário (estado, canal e tempo, desligar, mudo, surdo); faixa fina no topo do centro quando a pessoa está na chamada e em outra tela (toque abre a tela da voz; mudo e desligar na faixa). A lista de canais da gaveta mostra os participantes com anel de fala e ícones.
- Ajustes ganhou a seção "Voz": supressão de ruído, cancelamento de eco, ganho automático, sensibilidade do microfone e ganho de entrada (guardados em `Preferencias.opcoesDeVoz`, aplicados na hora).
- Erros da voz (permissão, microfone ocupado, queda) também aparecem como aviso curto em qualquer tela. Sair da conta sai da voz.

Difere do desktop (de propósito): sem RNNoise nem AGC próprio (usa os efeitos do Android, que variam por aparelho); reentra sozinho depois de queda; serviço em primeiro plano e notificação; rota de áudio (alto-falante/fone) porque no celular importa; volume por pessoa numa folha em vez de menu de contexto; sem atalho de teclado nem "push to talk".

Frágil: tamanho do buffer e latência do `AudioRecord`/`AudioTrack` (o maior entre o mínimo do aparelho e 4 quadros na captura / 3 quadros na reprodução; quadros de 960 amostras); os efeitos do Android podem não existir ou brigar com o VAD; Bluetooth pode demorar a assumir a rota; Concentus consome CPU (complexidade 5); o ciclo de vida do serviço em primeiro plano no Android 14/17 (tipo microfone exige a permissão já concedida ao iniciar, por isso o serviço só sobe depois de `Conectada`).

Só um aparelho confirma: qualidade e latência reais, eco no alto-falante, se NS/AEC/AGC existem no Redmi, rota Bluetooth, consumo de CPU e bateria, notificação e serviço com o app em segundo plano e tela apagada, comportamento ao receber ligação (foco de áudio), e o diálogo de permissão.

Verificado (na hora do C2; ao fim do C3 são 354 testes unitários): 338 testes unitários (novos: frame, VAD, jitter, envio/recepção, Opus de verdade, 20 cenários do controlador, VM de voz, navegação, 14 imagens de voz) e 15 de integração; o novo `vozDeUmClienteChegaAoOutro` sobe dois clientes reais no servidor real: ana manda seno de 440 Hz codificado em Opus, bob decodifica com energia e vê ana "falando", ana não se ouve, mudo e surdo chegam ao outro lado, e sair tira ana da sala. `verifyPaparazziDebug` verde; imagens novas em `docs/telas/voz_*.png` e `ajustes_voz.png` (83 no total, todas lidas).

Instalar e testar no Redmi (serial `f9b56d3c`): `export PATH=$HOME/Android/Sdk/platform-tools:$PATH`; `cd ~/telacast/android && scripts/gradle.sh assembleDebug`; `adb -s f9b56d3c install -r app/build/outputs/apk/debug/app-debug.apk`; abrir o Telacast, entrar, abrir a gaveta e tocar num canal de voz (o servidor precisa de um; o dono cria em "Criar canal", tipo voz no desktop); aceitar o microfone; para ouvir/falar, entrar no mesmo canal pelo desktop (`cargo run --release -p telacast`). Conferir: anel verde ao falar, mudo/surdo aparecendo no desktop, sair do app para a tela inicial e ver a notificação "Voz conectada" mantendo o áudio, desligar pela notificação, desligar o Wi-Fi por alguns segundos (deve mostrar Reconectando e voltar), alternar Alto-falante/Fone, Ajustes > Voz. Logs: `adb -s f9b56d3c logcat --pid=$(adb -s f9b56d3c shell pidof app.telacast)`.

## C3: miniaturas de imagem (`arquivos/Imagens.kt`, `arquivos/Arquivos.kt`, `ui/Anexos.kt`)

- `Arquivos.urlMiniatura` = URL do anexo + `&thumb=1` (contrato: `docs/PROTOCOL.md` 1.3, item 4). O servidor devolve JPEG/PNG de até 1024 px com o cabeçalho `X-Telacast-Thumb: 1`, ou o ORIGINAL sem o cabeçalho (imagem que já cabe, GIF, animada, inválida, servidor antigo). O cliente não olha o cabeçalho: decodifica o que vier e serve igual (tolerante ao fallback).
- `CarregadorDeImagens` tem duas camadas de cache LRU separadas: miniaturas (32 MB) e originais (48 MB), um só download por anexo e por camada. `obterMiniatura(a, urlMiniatura, urlOriginal)` usa o original se já estiver na memória, senão baixa a miniatura, e se ela falhar cai para o original. `miniaturaEmCache` devolve a miniatura, ou o original se for o que há.
- A lista do chat (`AnexoView`) só usa miniatura; o original só é baixado na tela cheia. `VisualizadorDeImagem` mostra primeiro o que já está na memória (a miniatura, quase sempre), baixa o original em seguida e troca sem piscar, com o aviso "Carregando a imagem original…"; se o original falhar (grande demais para o limite de 25 MB, rede), fica com a miniatura. "Baixar" continua baixando o original.
- Tamanho do layout: `Attachment.width/height` são as dimensões CRUAS; em foto com EXIF 5 a 8 a miniatura vem com eixos trocados. `Arquivos.dimensoesDeExibicao(anexo, bmpW, bmpH)` decide pela proporção do bitmap decodificado (bate com o anexo: tamanho do anexo; bate com o anexo trocado: trocado; senão o próprio bitmap; também cobre bitmap reduzido por `inSampleSize`). Enquanto carrega, o espaço reservado usa o anexo cru; numa foto em pé (EXIF) a caixa pode mudar de forma quando a miniatura chega (a lista pode dar um pequeno salto uma vez).
- Verificado: testes unitários (URL, caches separados, um só download, original na memória, servidor antigo, queda para o original, sem nada, `dimensoesDeExibicao` nos três casos) e imagens novas `imagem_tela_cheia_so_miniatura`, `foto_com_eixos_trocados_pelo_exif`, `lista_so_com_miniaturas` (lidas). Integração `miniaturaDeImagemVemReduzidaEOOriginalContinuaIntacto` contra o servidor REAL (o binário `target/release/telacast-server` já tinha as miniaturas): JPEG 2000x1500 volta 1024x768 com o cabeçalho e bem menor; sem `thumb` o original volta byte a byte e sem cabeçalho; 800x600 volta o original sem cabeçalho; JPEG com EXIF 6 vira 768x1024 (anexo diz 2000x1500); token inválido dá 401; e o `CarregadorDeImagens` com `OkHttpArquivos` decodifica 1024x768 (miniatura) e 2000x1500 (original) em camadas separadas. As fotos de teste ficam em `app/src/test/resources/imagens` (geradas uma vez com Pillow; o classpath do Android não tem AWT).
- Só um aparelho confirma: se a rolagem do chat com muitas fotos fica fluida e quanta memória sobra; decodificação de WebP/PNG com alfa no aparelho; o salto de layout na foto em pé.

## D1 (preparação)

- `C2S.PushRegister{token, platform}`, `PushUnregister{token}` e `SetFocus{focused}` já existem no cliente (`proto/C2S.kt`, ida e volta testadas em `PushProtoTest`). Falta acrescentá-las em `C2S.ETIQUETAS` quando as fixtures `proto/fixtures/c2s/*` aparecerem (o teste `listaDeVariantesNaoDiverge` compara os dois conjuntos).

## Bloqueado

Nada.

## Comandos exatos (rodar dentro de `android/`)

Todos passam pela receita de pouca RAM (`scripts/gradle.sh`, que recusa rodar com menos de 3 GB disponíveis e usa `systemd-run` com MemoryMax=2500M, `nice -n 19`, `--no-daemon --max-workers=2`). Um build por vez.

```bash
free -m                                   # precisa de >= 3000 MB em "available"
scripts/gradle.sh assembleDebug           # APK: app/build/outputs/apk/debug/app-debug.apk
scripts/gradle.sh testDebugUnitTest       # unitários (proto, gateway, repository, OkHttp+MockWebServer, snapshot); ~15 s
scripts/gradle.sh verifyPaparazziDebug    # compara a paleta com app/src/test/snapshots
scripts/paleta.sh                         # regrava só a paleta (docs/paleta.png)
scripts/telas.sh                          # regrava TODOS os PNGs (paleta + docs/telas/); só quando o visual mudar de propósito
scripts/it.sh                             # INTEGRAÇÃO contra o servidor real; ~60 s (a paginação respeita 12 envios por 10 s)
scripts/it.sh --tests '*IntegracaoTest.digitando*'   # um teste só (argumentos extras vão para o Gradle)
```

`scripts/it.sh`: sobe `target/release/telacast-server` (se não existir: `cargo build --release -p telacast-server -j2`, com `free -m` antes) num banco `telacast_it_<nanos>` e numa porta livre, cria o convite pela CLI, roda `testDebugUnitTest -PintegrationTests` com `TELACAST_IT_URL` e `TELACAST_IT_INVITE`, e no fim mata o servidor e apaga o banco (mesmo com erro ou Ctrl+C). `scripts/pg.py` é um cliente mínimo de Postgres (só biblioteca padrão; não há `psql` nem docker utilizável) que só aceita nomes `telacast_it_<números>`: nunca toca no banco `telacast` nem na porta 8080. Sem as variáveis, os testes `*IntegracaoTest` são ignorados (e ficam fora do `testDebugUnitTest` normal).

Equivalente sem o script (a receita crua do plano):

```bash
systemd-run --user --scope -q -p MemoryMax=2500M -p MemorySwapMax=0 nice -n 19 env ANDROID_HOME=$HOME/Android/Sdk GRADLE_OPTS=-Xmx512m ~/Android/gradle-8.9/bin/gradle --no-daemon --max-workers=2 assembleDebug
```

Não há `gradlew`: usa-se o Gradle 8.9 de `~/Android/gradle-8.9` (evita baixar outra versão). `local.properties` (sdk.dir) é local e ignorado pelo git.

## Versões fixadas

AGP 8.5.2, Kotlin 2.0.21 (plugins do compilador Compose e de serialização), Gradle 8.9, JDK 21 (alvo 17), Compose BOM 2024.09.03 (foundation, ui, animation, material3), activity-compose 1.9.2, core-ktx 1.13.1, lifecycle-runtime-ktx 2.8.6, kotlinx-serialization-json 1.7.3, kotlinx-coroutines-android 1.8.1 (+ coroutines-test), OkHttp 4.12.0 (+ mockwebserver), Paparazzi 1.3.5, JUnit 4.13.2.

## Notas para os próximos passos

- Paparazzi reduz o PNG para no máximo 1000 px se `useDeviceResolution` for falso; o teste da paleta liga isso e usa um Pixel 5 com 4700 px de altura (se a paleta crescer, aumentar `screenHeight`). `RenderingMode.V_SCROLL` deu imagem de 151 px de largura com Compose: não usar.
- Fontes variáveis funcionam no layoutlib do Paparazzi (pesos 300 a 700 visivelmente diferentes). Itálico do Newsreader é sintetizado (não há arquivo itálico), igual ao egui.
- Tráfego claro `ws://` no Android 9+ exige exceção só no build debug (A3).

## Achados do A2/A3 (para quem mexe no servidor ou no desktop)

- **Servidor manda o `nonce` a TODOS os participantes** do canal (`msg.nonce = nonce` antes do envio em massa), não só ao autor; `docs/PROTOCOL.md` diz "0 para os outros". O cliente só casa o eco quando o autor é ele, então não quebra, mas a doc está imprecisa.
- **O `History` não devolve `nonce`** (vem 0). Depois de reconectar, uma pendente cuja primeira cópia chegou e cujo eco se perdeu apareceria duas vezes (o desktop só compara `nonce`). O Kotlin concilia: mensagem do próprio usuário, mais nova que o maior id conhecido ao enviar (`depoisDe`) e com o mesmo texto, casa com a pendente.
- **Possível bug do desktop:** `ack` ao abrir um canal usa `last_message_id` do `Snapshot`, que fica velho (o `Snapshot` não é reenviado a cada mensagem), e o `History` carregado não confirma nada. Mensagens que chegaram depois do último `Snapshot` ficam não lidas no servidor até outra mensagem chegar com o canal aberto. O Kotlin confirma (`Ack`) o id mais novo do histórico logo que ele chega no canal aberto (teste de integração `naoLidoEAckViajamPeloServidor`).
- Limite de `Send` (12 por 10 s) estoura fácil em teste: uma `Send` recusada vira `Error RateLimited` e a pendente fica na fila; a tela do B-passos precisa de "reenviar/descartar" (o `Repository` já tem `reenviar` e `descartarPendente`).
- Cadastro/login: 10 por 5 min por IP; a suíte de integração usa 9.

## Decisões do B4 a B6

- Toque longo abre a folha (não há SelectionContainer na lista: ele roubaria o toque longo); o texto da mensagem fica selecionável na prévia da folha, e há "Copiar texto".
- Enviar com anexo ainda subindo: o botão Enviar fica apagado até todos terminarem (o desktop "arma" a mensagem e a envia sozinho depois). Mais simples e sem estado escondido; anexo que falhou continua na bandeja com "Tentar de novo" ou remover.
- O `History` não traz `nonce`; a conciliação de pendentes (A3) continua valendo.
- Mensagem nova só rola para o fim se a pessoa já estava no fim (ou se a nova é minha).
- O seletor de emojis tem 36 emojis fixos (sem busca). O servidor só aceita emoji sem letras, números ASCII nem espaço.
- Compartilhar anexo não foi feito (compartilharia o endereço com o token); há Baixar (DownloadManager) e abrir em tela cheia.
- Moldura de sistema: a lista usa `windowInsetsPadding(navigationBars union ime)` no compositor; a barra de status é tratada na raiz da tela principal.

## Pendências e fora do escopo (depois do B8)

- Busca: sem destaque dos termos nos resultados (o spec pede, o desktop não faz), sem histórico de busca, sem os filtros `tem:`, `antes:`, `depois:`, `fixadas:` (o protocolo só tem `de` e escopo) e sem ordenar; sem busca em todas as DMs.
- Perfil: sem "amigos e servidores em comum" nem cargos (o protocolo não traz); sem bloquear por menu de contexto; "dono" só aparece quando a pessoa é dona do servidor do canal aberto.
- Amigos: sem "Sugestões", sem nota no pedido, sem "Limpar tudo" e sem chamadas (voz ainda não existe). Remover a amizade de dentro da lista passa pelo perfil (o botão redondo "Mais" abre o perfil).
- Ajustes: sem trocar nome ou avatar (o servidor não tem), sem notificações (fase D), sem tema claro (só Grafite), sem reduzir movimento.
- Voz, vídeo e notificações (fases C e D). Prova em aparelho: nada foi instalado; só imagens na JVM e integração contra o servidor real.
- Fixadas que o servidor traz em `Pins` ordenadas da mais recente; a tela não reordena.

## Decisões do B1 a B3

- Linhas de canal com 40 dp (o desktop usa 34) e menus com itens de 44 dp: alvo de toque. Botões de formulário com 46 a 48 dp.
- Gaveta própria em vez de `ModalNavigationDrawer`: controle total do visual (sem sombra, escurecido preto a 55%) e do que o Paparazzi desenha. Menus e diálogos são desenhados na própria composição (sem `Popup`/`Dialog`), porque o layoutlib não desenha janelas separadas.
- Selo de menções do trilho pendurado no canto do ícone (o desktop o põe dentro do ícone, por cima da sigla).
- "Sair do servidor" **não existe**: o protocolo não tem mensagem de sair de servidor (só `Logout` da conta). O menu do servidor tem Convidar, Criar canal e Criar categoria.
- Início na gaveta: o botão "DM" do trilho troca o painel para Amigos + DMs sem mudar o centro; tocar num servidor do trilho troca o painel sem mudar o centro (como no Discord mobile).
- Primeira tela depois do login: último canal aberto (salvo), senão o primeiro canal de texto do primeiro servidor, senão Amigos.

## Sugestões para a fase C e D

- C1 (assistir transmissão): `Snapshot.live` diz quem transmite e em qual canal; o vídeo usa conexão própria `/watch/{canal}?token=` (OkHttp, quadros `[flag][H.264 Annex B]`); começar a decodificar só em quadro-chave. O `Gateway` já entrega quadros binários do `/ws` (áudio), ainda sem uso.
- D1 (notificações): `Repository` já separa `naoLidos` e `mencoes`; falta o serviço em primeiro plano e o canal de notificação por servidor.
- Perfil e amigos reaproveitam `FolhaBase` e `EstadoVazio`; o "mais" de cada amigo pode virar menu de contexto no futuro.
