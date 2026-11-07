# Telacast Android: progresso

Plano: `docs/MOBILE-PLAN.md` (raiz do repositório). Este arquivo é o estado da execução: é dele que se retoma.

## Flag

`ANDROID_INSTALL=0`  (nunca rodar `adb install` nem tocar no celular/emulador; só mudar para 1 quando o usuário mandar)

## Passo atual

**A1 (esqueleto + tema): feito e commitado.** **A2 lado Kotlin (proto + contrato por fixtures) e A3 (Gateway + Repository + testes): feitos, aguardando revisão e commit.** Próximo: **A3b/B1 (login e navegação)**.

## Feito

- A1: projeto Gradle (Kotlin DSL) em `android/`, módulo `app` (`app.telacast`, minSdk 26, compile/targetSdk 34); tema Grafite; `PaletaScreen`; PNG do Paparazzi em `docs/paleta.png`.
- A2 (`app/src/main/java/app/telacast/proto/`): `Tipos.kt` (espelho de `proto/src/lib.rs`: ids `Long`, enums tolerantes), `C2S.kt` e `S2C.kt` (sealed classes com serializador "externamente etiquetado" do serde), `Proto.kt` (`lerS2C`, `escreverC2S`, ...). Variante S2C desconhecida vira `S2C.Desconhecida` (o Gateway descarta); `ErrorCode` desconhecido vira `Unknown`, `Status` desconhecido vira `Offline`; `FriendStatus` desconhecido derruba o Snapshot inteiro (igual ao Rust, contrato de hoje).
- A2 teste: `ProtoFixturesTest` lê `../proto/fixtures/**` (c2s, s2c, http, extras) e confere por árvore JSON, mais "lista de variantes não diverge" nos dois sentidos.
- A3 (`net/` e `data/`, puro Kotlin, sem Android): `Gateway` (OkHttp WebSocket por trás da interface `Transporte`; estados Conectando/Conectado/Reconectando/Desconectado; Login/Registro/Resume; recuo 1-2-4-8-15 s; `SessionExpired` e afins viram `Falha(sessaoInvalida)`), `Repository` (um `StateFlow<AppState>`: snapshot, mensagens por canal, não lidos e menções, digitando 8 s, envio otimista com nonce, eco, paginação, editar/apagar/reagir/fixar, reenvio e conciliação depois de reconectar), `Mencoes.kt`.
- Cleartext: `app/src/debug/` tem `network_security_config` com texto claro liberado SÓ no debug; o release não tem (conferido no manifesto mesclado).
- Verificado: 65 testes unitários (proto 21, gateway 13, OkHttp+MockWebServer 5, repository 25, snapshot 1) e 10 de integração contra o servidor real.

## Bloqueado

Nada.

## Comandos exatos (rodar dentro de `android/`)

Todos passam pela receita de pouca RAM (`scripts/gradle.sh`, que recusa rodar com menos de 3 GB disponíveis e usa `systemd-run` com MemoryMax=2500M, `nice -n 19`, `--no-daemon --max-workers=2`). Um build por vez.

```bash
free -m                                   # precisa de >= 3000 MB em "available"
scripts/gradle.sh assembleDebug           # APK: app/build/outputs/apk/debug/app-debug.apk
scripts/gradle.sh testDebugUnitTest       # unitários (proto, gateway, repository, OkHttp+MockWebServer, snapshot); ~15 s
scripts/gradle.sh verifyPaparazziDebug    # compara a paleta com app/src/test/snapshots
scripts/paleta.sh                         # regrava o snapshot e copia para docs/paleta.png (só quando o tema mudar de propósito)
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

## Sugestões para A3b/B1 (login e navegação)

- `AppViewModel` fino (lifecycle-viewmodel) com um `Repository` criado no `Application` (DI manual); guardar o token com Android Keystore (AES-GCM) e abrir com `Credencial.Retomar`.
- Tela de login/cadastro lê `AppState.erroDeEntrada` e `EstadoConexao`; se `sessaoInvalida`, apaga o token e volta ao login.
- Faixa "Reconectando em Ns..." a partir de `EstadoConexao.Reconectando(tentativa, quandoMs)`.
- Navegação: estado próprio (gaveta + canal) com `BackHandler`; ao abrir canal chamar `Repository.abrirCanal` (já faz `Ack` e `History`), ao sair `fecharCanal`; rolagem da lista com `carregarAnteriores` ao chegar no topo.
- Texto claro (`ws://`) só no debug; o app precisa montar `wss://` em produção (`Gateway.urlDoGateway` já troca `https://` por `wss://`).
