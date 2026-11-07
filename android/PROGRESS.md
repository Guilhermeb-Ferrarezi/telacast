# Telacast Android: progresso

Plano: `docs/MOBILE-PLAN.md` (raiz do repositório). Este arquivo é o estado da execução: é dele que se retoma.

## Flag

`ANDROID_INSTALL=0`  (nunca rodar `adb install` nem tocar no celular/emulador; só mudar para 1 quando o usuário mandar)

## Passo atual

A1 a A3 e B1 a B3 estão commitados. **B4 (mensagens), B5 (compositor e ações) e B6 (histórico) feitos, aguardando revisão e commit.** Próximo: **B7 (Início completo: amigos e DMs), B8 (membros e perfil), B9 (ajustes)**; depois a fase C (vídeo).

## Feito

- A1 a A3: projeto, tema Grafite, protocolo (fixtures), Gateway, Repository, integração contra o servidor real.
- B1 a B3: login/cadastro, gaveta com trilho e canais, menus, diálogos, Início mínimo (ver `docs/telas/`).
- **Markdown** (`md/Markdown.kt`): porte fiel de `md/src/lib.rs` (blocos e inlines), com os mesmos casos de `md/src/tests.rs` em `MarkdownTest`. O desenho (`ui/MarkdownView.kt`) gera `AnnotatedString` com `LinkAnnotation.Clickable`: negrito, itálico, sublinhado, tachado, código em linha e em bloco (JetBrains Mono, `res/font/jetbrainsmono.ttf`), citação, listas, título, subtexto, spoiler que revela ao toque, links, menções `<@id>` com o nome atual, `<#id>`, `@everyone`/`@here` e carimbos `<t:..>`.
- **Lista de mensagens B4** (`ui/Mensagens.kt`, `data/ItensDaLista.kt`): `LazyColumn(reverseLayout = true)`; as regras do desktop estão em `ItensDaLista.montar` (puro, testado): agrupamento (mesmo autor, menos de 5 min, sem resposta, sem quebra de dia/novas/sistema), divisor de dia (hoje, ontem, data), "NOVAS MENSAGENS" (marcador = último lido ao abrir o canal; `AppState.marcadores`), avisos de sistema (kind 2 e 3), mensagens de bloqueado recolhidas ("Mostrar mensagem"), pendentes em opacidade reduzida com "Enviando…", "Sem conexão. Será enviada ao reconectar." ou "Não foi enviada." com Tentar de novo e Descartar, reações em chips (âmbar quando eu reagi; toque alterna; "+" abre o seletor), "Fixada", "(editado)", cabeçalho de resposta que salta até a original.
- **Anexos** (`arquivos/`, `ui/Anexos.kt`): imagem inline até 400x300 dp (cache LRU em memória por bytes, um download por anexo, `BitmapFactory` com `inSampleSize`), tela cheia com zoom (pinça e toque duplo), cartão de arquivo, prévia de texto/código (6 linhas, Expandir, Abrir em tela cheia, Baixar), download pelo `DownloadManager`. Sem Coil.
- **Compositor B5** (`ui/Compositor.kt`, `data/Compositor.kt`): campo multilinha até 6 linhas, Enviar (âmbar com texto), "+" com `GetMultipleContents`, bandeja de miniaturas 120 dp com progresso, falha e remover, fila de 3 uploads por vez e no máximo 10 anexos por mensagem (`arquivos/Envios.kt`), "Respondendo a", "Editando mensagem" com Salvar/Cancelar, "fulano está digitando…", `Typing` no máximo a cada 4 s, rascunho por canal (SharedPreferences), autocompletar de `@` (membros, `@everyone`, `@here`).
- **Folha de ações** (`ui/Folha.kt`): toque longo abre folha inferior desenhada na composição: prévia selecionável, reações rápidas, Responder, Editar (minhas), Copiar texto, Fixar/Desafixar (dono do servidor ou DM), Apagar (minhas ou dono; com confirmação); seletor de emojis.
- **Histórico B6**: perto do topo chama `carregarAnteriores` (indicador "Carregando mensagens antigas…"; a posição não pula); salto até resposta antiga (`Repository.saltarPara`, no máximo 10 páginas, realce âmbar por ~1,5 s em `AppState.destaque`); `Ack` ao abrir o canal, ao chegar o histórico e ao chegar mensagem com o canal aberto.
- Imagens: `docs/telas/*.png` (48), geradas por `TelasSnapshotTest` e `ConversaSnapshotTest` com `AppState` falso e bitmaps gerados em código.
- Verificado: 213 testes unitários (24 `TelasSnapshotTest` + 24 `ConversaSnapshotTest` + paleta incluídos; markdown 15, itens da lista 11, compositor 13, arquivos 14, repository 30, ViewModel 15 + 13 da conversa) e 11 de integração (inclui upload/download de anexo por HTTP e salto até mensagem antiga); `verifyPaparazziDebug` verde.

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
- Cadastro/login: 10 por 5 min por IP; a suíte de integração usa 8.

## Decisões do B4 a B6

- Toque longo abre a folha (não há SelectionContainer na lista: ele roubaria o toque longo); o texto da mensagem fica selecionável na prévia da folha, e há "Copiar texto".
- Enviar com anexo ainda subindo: o botão Enviar fica apagado até todos terminarem (o desktop "arma" a mensagem e a envia sozinho depois). Mais simples e sem estado escondido; anexo que falhou continua na bandeja com "Tentar de novo" ou remover.
- O `History` não traz `nonce`; a conciliação de pendentes (A3) continua valendo.
- Mensagem nova só rola para o fim se a pessoa já estava no fim (ou se a nova é minha).
- O seletor de emojis tem 36 emojis fixos (sem busca). O servidor só aceita emoji sem letras, números ASCII nem espaço.
- Compartilhar anexo não foi feito (compartilharia o endereço com o token); há Baixar (DownloadManager) e abrir em tela cheia.
- Moldura de sistema: a lista usa `windowInsetsPadding(navigationBars union ime)` no compositor; a barra de status é tratada na raiz da tela principal.

## Pendências e fora do escopo (B7 em diante)

- Fixadas do canal (`Pins`) tem estado no Repository mas não tem tela; "Ver todas as mensagens fixadas" ainda não faz nada.
- Perfil ao tocar no avatar ou na menção (B8), busca (`Search`), bloquear/desbloquear pela interface, ajustes (trocar senha, sessões) e voz.
- Prova em aparelho: nada foi instalado; só imagens na JVM e integração contra o servidor real. Falta conferir na prática: teclado e `imePadding`, `GetMultipleContents`, `DownloadManager`, gestos de zoom e a rolagem do salto (o deslocamento do centro usa um valor negativo em `animateScrollToItem` com `reverseLayout`; está certo na teoria, mas só o aparelho confirma).

## Decisões do B1 a B3

- Linhas de canal com 40 dp (o desktop usa 34) e menus com itens de 44 dp: alvo de toque. Botões de formulário com 46 a 48 dp.
- Gaveta própria em vez de `ModalNavigationDrawer`: controle total do visual (sem sombra, escurecido preto a 55%) e do que o Paparazzi desenha. Menus e diálogos são desenhados na própria composição (sem `Popup`/`Dialog`), porque o layoutlib não desenha janelas separadas.
- Selo de menções do trilho pendurado no canto do ícone (o desktop o põe dentro do ícone, por cima da sigla).
- "Sair do servidor" **não existe**: o protocolo não tem mensagem de sair de servidor (só `Logout` da conta). O menu do servidor tem Convidar, Criar canal e Criar categoria.
- Início na gaveta: o botão "DM" do trilho troca o painel para Amigos + DMs sem mudar o centro; tocar num servidor do trilho troca o painel sem mudar o centro (como no Discord mobile).
- Primeira tela depois do login: último canal aberto (salvo), senão o primeiro canal de texto do primeiro servidor, senão Amigos.

## Pendências e fora do escopo (B4 em diante)

- Lista de mensagens, compositor, ações de mensagem, markdown, perfil em folha inferior, painel de membros por cargo, ajustes (trocar senha, sessões) e voz.
- Busca (`Search`) não tem estado nem tela.
- Sem prova em aparelho: nada foi instalado; só imagens na JVM.

## Sugestões para B7 em diante

- B7: amigos e DMs completos reaproveitam `Repository.abrirDm`, `pedirAmizade`, `aceitarAmigo`, `removerAmigo`; falta bloquear (`C2S.Block`) e a lista "Bloqueados" (o `Snapshot.blocked` já alimenta a lista de mensagens).
- B8: o perfil em folha inferior pode reusar `FolhaBase` de `ui/Folha.kt`; `AcoesMd.aoUsuario` e o avatar da mensagem já têm o gancho.
- Fixadas: `Repository.pedirFixadas` e `AppState.fixadas` prontos; falta a tela (lista com `MensagemView`) e ligar o link de `LinhaDeSistema`.
- C1: o `Gateway` já entrega quadros binários (`EventoGateway.Binario`); a transmissão usa conexão própria `/watch/{canal}?token=` (OkHttp) e `Snapshot.live` diz quem transmite.
