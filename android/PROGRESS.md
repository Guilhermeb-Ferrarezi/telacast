# Telacast Android: progresso

Plano: `docs/MOBILE-PLAN.md` (raiz do repositório). Este arquivo é o estado da execução: é dele que se retoma.

## Flag

`ANDROID_INSTALL=0`  (nunca rodar `adb install` nem tocar no celular/emulador; só mudar para 1 quando o usuário mandar)

## Passo atual

A1, A2 e A3 estão commitados. **B1 (login e cadastro), B2 (navegação com gaveta) e B3 (canais e Início) feitos, aguardando revisão e commit.** Próximo: **B4 (lista de mensagens) e B5 (compositor e ações)**.

## Feito

- A1 a A3: projeto, tema Grafite, protocolo (fixtures), Gateway, Repository, integração contra o servidor real (ver "Achados" abaixo).
- **B1 Login/cadastro** (`ui/Login.kt`, `estado/FormLogin.kt`, `AppViewModel`): servidor editável (padrão em `BuildConfig.SERVIDOR_PADRAO`: `10.0.2.2:8080` no debug, vazio no release), usuário, senha, convite (alterna Entrar / Registrar-se), "Entrando…", erros em português (`TextosDeErro`), retomada da sessão salva ("Entrando…" + "Usar outra conta"). Token cifrado com AES-GCM do Android Keystore (`armazenamento/CifraAesGcm.kt` + `CofreKeystore.kt`, SharedPreferences privadas; sem `security-crypto`); guarda também o último servidor e usuário. `sessaoInvalida` apaga o token e volta ao login.
- **B2 Navegação** (`ui/Principal.kt`, `estado/Navegacao.kt`): gaveta à esquerda própria (trilho 72 dp + painel 240 dp, abre por botão ou arrastando; toque no escurecido fecha), painel de membros à direita (lista simples; por gesto ou botão), `BackHandler` na raiz (`ui/Raiz.kt`) fecha diálogo, menu, membros e gaveta, nessa ordem, antes de sair. Navegação por estado próprio (`Navegacao`, imutável, testado).
- **B3 Canais** (`ui/Trilho.kt`, `ui/Painel.kt`, `ui/Sobreposicoes.kt`): trilho com pílula 8/40 dp, sigla, selo de menções (99+), botão "+"; cabeçalho do servidor em serifa com menu (Convidar pessoas; dono: Criar canal e Criar categoria); categorias recolhíveis (estado salvo; recolhida só mostra canal aberto, não lido ou com gente na voz); canais com `#`, não lido em negrito + barra, pílula de menções; voz com alto-falante e quem está nela ("Canais de voz chegam em breve" ao tocar); cartão do usuário com presença e menu de status (Online, Ausente, Não perturbe, Invisível, Sair da conta); diálogos criar servidor / entrar com convite, criar canal ou categoria, convidar (copiar código), confirmar saída.
- **Início** (`ui/Centro.kt`): "Amigos" com abas Disponível, Todos, Pendente (aceitar, recusar, cancelar) e Adicionar amigo; na gaveta, "Amigos" e as mensagens diretas. DM e canal abrem com cabeçalho e um espaço para a lista (B4).
- Imagens: `docs/telas/*.png` (24, ~1,2 MB), geradas por `TelasSnapshotTest` com `AppState` falso (`Amostras.kt`), sem rede.
- Verificado: 118 testes unitários (proto 21, gateway 13, OkHttp 5, repository 25, ViewModel 15, navegação 6, formulário 4, cifra 4, 24 imagens de telas + paleta) e 10 de integração; `verifyPaparazziDebug` verde.

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
scripts/it.sh                             # INTEGRAÇÃO contra o servidor real; ~55 s (a paginação respeita 12 envios por 10 s)
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

## Sugestões para B4/B5

- A lista de mensagens usa `AppState.lista(canal)` (reais em ordem + provisórias no fim); `Repository.carregarAnteriores(canal)` ao chegar no topo; `LazyColumn(reverseLayout = true)`.
- Compositor: `Repository.enviarMensagem` já valida (vazio, 4000), faz o envio otimista e converte `@nome` em token; `reenviar`/`descartarPendente` para a mensagem que não saiu (limite de 12 envios por 10 s).
- O ponto âmbar do botão da gaveta (`temNovidade`) e as barras de não lido já usam `naoLidos` e `mencoes` do `AppState`.
- `imePadding()` já está na tela de login e nos diálogos; o compositor precisa dele também.
