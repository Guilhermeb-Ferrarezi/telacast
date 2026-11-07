# Telacast Mobile (Android): plano de execução

Objetivo: o app Android conversa com o **mesmo servidor** (mesmo protocolo JSON/WebSocket do desktop), com a mesma identidade visual (Grafite) e o comportamento do Discord adaptado ao celular. Este plano foi escrito para ser **executado sem você acordado**: cada passo tem um critério de pronto que eu mesmo consigo verificar, e o estado fica num arquivo para eu retomar sem depender de memória de conversa.

## 1. Decisões (padrões assumidos; você pode vetar)

| Tema | Decisão | Motivo |
|---|---|---|
| Linguagem/UI | **Kotlin + Jetpack Compose**, tema próprio Grafite (não Material padrão) | MediaCodec, MediaProjection, notificações e serviços são nativos; Compose desenha tudo com o mesmo tema |
| SDK | compileSdk/targetSdk **34** (o instalado), minSdk **26** | evita baixar outro SDK; Android 8+ cobre quase tudo |
| Build | AGP 8.5.x + Kotlin 2.0.x + Gradle **8.9** (já em `~/Android/gradle-8.9`), JDK 21 | tudo já existe na máquina |
| Rede | OkHttp WebSocket (ping automático) + kotlinx.serialization | reconexão e keepalive prontos |
| Protocolo | **contrato por fixtures**: o Rust grava JSONs de exemplo de cada mensagem em `proto/fixtures/`, o Kotlin lê os mesmos arquivos | mudança no protocolo quebra o teste dos dois lados |
| Verificação visual | **Paparazzi** (renderiza telas Compose em PNG na JVM, sem emulador, sem tocar no seu celular) | cabe na RAM e roda sozinho de madrugada |
| Pacote | `app.telacast`, pasta `~/telacast/android/` (fora do workspace Cargo) | |
| Segredos | token em EncryptedSharedPreferences; keystore de release local e fora do git | |
| Notificações | v1: serviço em primeiro plano opcional + notificações locais. **FCM só depois do deploy** (exige servidor público) | |
| Transmissão | assistir primeiro; transmitir a tela do celular depois (MediaProjection) | |

## 2. O que NÃO faço sozinho

- Tocar na tela do seu celular (Redmi, serial `f9b56d3c`) ou do emulador do port; só `adb install` se o aparelho estiver conectado **e** você tiver deixado a flag `ANDROID_INSTALL=1` no arquivo de progresso. Capturas de aparelho real são apagadas depois.
- Usar seu mouse, teclado ou janelas (regra já combinada). Desktop só em `Xvfb`.
- `git push`, publicar, mexer em conta de loja, gastar dinheiro.
- Compilar com jogo aberto ou com menos de ~3 GB de RAM disponível: nesses casos espero (ver 4).

## 3. Passos (cada um termina com commit; sem coautoria)

### Fase A: fundação (tudo verificável na JVM)
- **A1 Esqueleto + tema.** Projeto Gradle em `android/`, tema Grafite (camadas L0–L4, âmbar, Inter e Newsreader como recursos), tela de amostra. *Pronto quando:* `assembleDebug` gera o APK e um PNG do Paparazzi com a paleta e as fontes é lido por mim e confere com `docs/ui-preview/grafite.html`.
- **A2 Contrato do protocolo.** Teste Rust que grava/compara `proto/fixtures/*.json` (um por variante de `C2S`/`S2C`); `Proto.kt` com serializador de enum "externamente etiquetado" (`{"Login":{...}}` e variantes unitárias como string). *Pronto quando:* `cargo test -p telacast-proto` e `gradle test` passam lendo os mesmos arquivos; alterar um campo no Rust quebra o teste Kotlin.
- **A3 Cliente do gateway.** `Gateway` (OkHttp): conectar, Login/Register(convite)/Resume/Logout, reconexão com recuo exponencial, ping, estado (`Snapshot`, mensagens por canal, não lidos, digitando) num `ViewModel`. Teste de integração JVM contra o servidor real em banco novo (script `android/scripts/it.sh`, igual ao e2e do Rust). *Pronto quando:* cadastrar, criar servidor, enviar, receber, responder, editar, apagar e `Ack` funcionam do Kotlin.

### Fase B: telas (cada uma com PNG do Paparazzi conferido por mim)
- **B1 Login e cadastro** (convite, erros, "Entrando…", retomar sessão). Sessão salva com segurança.
- **B2 Navegação estilo Discord mobile:** gaveta à esquerda com trilho de servidores (40 dp, pílula 8/20/40) + lista de canais; centro = chat; painel de membros pelo gesto à direita. Voltar do sistema fecha gaveta antes de sair.
- **B3 Canais:** cabeçalho do servidor com menu, categorias recolhíveis, canais com `#`, não lido (barra) e pílula no servidor, cartão do usuário.
- **B4 Mensagens:** lista invertida, agrupamento (regra do desktop), divisores de dia, avatar 40 dp, "(editado)", cabeçalho de resposta clicável (salta até a original, com destaque âmbar), markdown básico (negrito, itálico, código, spoiler, menções) conforme `docs/ui-spec/10`.
- **B5 Compositor e ações:** campo multilinha, enviar, "respondendo a", "digitando…", rascunho por canal; **toque longo** abre folha inferior (Responder, Editar, Apagar com confirmação, Copiar); seleção de texto nativa.
- **B6 Histórico:** rolar para cima carrega mais (`History{before}`); o salto até resposta antiga busca a mensagem.
- **B7 Início:** amigos (Disponível/Todos/Pendente/Adicionar), DMs, abrir conversa.
- **B8 Membros e perfil:** painel por cargo/online, perfil em folha inferior.
- **B9 Ajustes:** conta, tamanho do texto, servidor (convite, criar canal/categoria), sair.

### Fase C: vídeo (precisa de aparelho para a prova final)
- **C1 Assistir transmissão:** `watch` no WebSocket binário → `MediaCodec` H.264 → `SurfaceView`; tela cheia em paisagem, controles que somem em 1,8 s (spec 08). Na JVM: testes do separador de NAL/quadro-chave. *Prova final* só com aparelho/emulador conectado: marco como "pendente de aparelho" no progresso.
- **C2 Transmitir a tela do celular:** `MediaProjection` + encoder H.264 → WebSocket `publish`, serviço em primeiro plano tipo `mediaProjection`. Mesma regra de prova.

### Fase D: acabamento
- **D1 Notificações locais** (mensagens e menções, canal de notificação por servidor, serviço de conexão opcional).
- **D2 Pronto para o servidor público:** `wss://`, endereço padrão configurável, configuração de rede que só permite texto claro em debug/IP local, ícone, nome, assinatura release.
- **D3 Qualidade:** rotação e tablet, contraste (mínimo 4,5:1), tamanhos de fonte do sistema, reduzir movimento, desempenho da lista.

Servidor (feitos junto, só quando um passo exigir): keepalive/`Ping` explícito se o OkHttp não bastar; `History` por id para saltar a respostas antigas; limites de tamanho para anexos quando chegar a hora.

## 4. Protocolo da execução noturna

1. **Arquivo de estado:** `android/PROGRESS.md` (criado no A1) com: passo atual, feito, bloqueado, comandos exatos para retomar, e a flag `ANDROID_INSTALL`. Atualizo a cada commit. Se o contexto da conversa for compactado, é dali que eu continuo.
2. **Antes de cada build:** `free -m` (>= 3 GB disponíveis) e `pgrep` de jogos (RocketLeague, steam apps, etc.). Se não passar, não compilo: adianto código/testes sem build e tento de novo daqui a 20–30 min.
3. **Receita de build** (a que já funciona nesta máquina): `systemd-run --user --scope -q -p MemoryMax=2500M -p MemorySwapMax=0 nice -n 19 env ANDROID_HOME=$HOME/Android/Sdk GRADLE_OPTS=-Xmx512m ~/Android/gradle-8.9/bin/gradle --no-daemon --max-workers=2 <tarefa>` dentro de `android/`. Disco tem ~41 GB livres; o cache do Gradle deve ficar abaixo de ~6 GB.
4. **Ciclo de cada passo:** escrever → compilar/testar → gerar PNG → **ler o PNG e comparar com a especificação** → corrigir → `git commit` → atualizar `PROGRESS.md`.
5. **Parada segura:** 3 falhas seguidas no mesmo passo = marco como bloqueado com o erro e a hipótese, e sigo para o próximo passo independente. Nunca fico repetindo o mesmo comando.
6. **Servidor de teste:** sempre em banco descartável (como o e2e), nunca no banco do seu uso.
7. **PC acordado:** ao começar, abro um inibidor de suspensão (`systemd-inhibit --what=idle:sleep` em segundo plano) e o encerro no relatório da manhã; o monitor pode apagar normalmente.
8. **Relatório da manhã** (`android/MORNING.md`): o que ficou pronto (com PNGs), o que bloqueou e por quê, o que depende de você ou de um aparelho, quanto de disco o Gradle usou.

## 5. Expectativa realista

Cada passo leva de 20 a 60 minutos com builds. Numa noite de ~8 horas, o esperado é **A1 a B5** (login, navegação, canais, mensagens e compositor funcionando contra o servidor real) com PNGs de cada tela. Vídeo (C) e notificações (D) ficam para a noite seguinte, porque a prova final precisa de aparelho.

## 6. Riscos conhecidos

- **Primeira sincronização do Gradle** baixa ~1–2 GB (AGP, Compose, Paparazzi); se a rede falhar, o passo A1 repete com espera.
- **Paparazzi × AGP:** se a versão não casar, o plano B é Roborazzi (Robolectric), que também roda na JVM.
- **RAM:** emulador qemu aberto ocupa ~4 GB. Sem ele, o plano roda folgado.
- **Fontes variáveis no Compose:** Inter e Newsreader são variáveis; Compose aceita variações de peso, mas o PNG do A1 confirma antes de eu depender disso.
- **Texto claro (`ws://`) no Android 9+** é bloqueado por padrão: o desenvolvimento usa `adb reverse` ou uma exceção só no build debug.
