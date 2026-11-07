# Telacast Android: progresso

Plano: `docs/MOBILE-PLAN.md` (raiz do repositório). Este arquivo é o estado da execução: é dele que se retoma.

## Flag

`ANDROID_INSTALL=0`  (nunca rodar `adb install` nem tocar no celular/emulador; só mudar para 1 quando o usuário mandar)

## Passo atual

A1 a A3 e B1 a B6 estão commitados. **B7 (Início completo) e B8 (perfil, fixadas, busca, ajustes) feitos, aguardando revisão e commit.** Próximo: **fase C (vídeo: assistir e transmitir)** e **D (notificações, acabamento)**; B9 do plano original foi cumprido aqui (ajustes).

## Feito

- A1 a A3, B1 a B6: ver os commits (login, gaveta, canais, mensagens, markdown, anexos, compositor, folha de ações, histórico).
- **B7 Início completo** (`ui/Amigos.kt`, `data/ItensDeAmigos.kt`): abas Disponível, Todos, Pendente, Bloqueados e Adicionar amigo; busca local por nome; seções "ONLINE — N", "TODOS OS AMIGOS — N", "RECEBIDOS — N", "ENVIADOS — N", "BLOQUEADOS — N"; linha de pessoa com avatar de 40 (toque abre o perfil), botões redondos (Mensagem e Mais; Aceitar e Recusar; Cancelar), toque na linha abre a DM; Desbloquear; estados vazios com os textos do `docs/ui-spec/07`; Adicionar amigo tira o `@` e só envia com texto; confirmações para desfazer amizade e bloquear.
- **B8 Perfil** (`ui/Perfil.kt`): folha inferior (a mesma `FolhaBase`), aberta ao tocar em avatar ou nome da mensagem, em menção `<@id>`, em membro do painel da direita e no avatar da lista de amigos. Faixa lisa na cor da pessoa, avatar de 80 com presença, nome, `@usuário`, status, "No Telacast desde" (data do snowflake), Mensagem, Adicionar/Desfazer amizade/Aceitar/Cancelar pedido, Bloquear/Desbloquear (com confirmação), Copiar ID, e "Ajustes e conta" no próprio perfil.
- **B8 Fixadas** (`ui/Fixadas.kt`): botão de alfinete na barra do canal e o link "Ver todas as mensagens fixadas" abrem a lista; tocar leva até a mensagem (`Repository.irParaMensagem`, que abre o canal e salta quando o histórico chega). A lista se atualiza sozinha quando alguém fixa ou desafixa.
- **B8 Busca** (`ui/Busca.kt`, `data/Busca.kt`): lupa na barra do canal ou da DM; campo, escopo (No servidor, Neste canal; DM sempre a própria conversa), filtros `de:nome`, `de:eu` e `em:#canal` (o mesmo `parse_search` do desktop), contagem, cartões de resultado (`# canal`, avatar, autor, dia e hora, markdown), "Mostrar mais" (paginação por `before`), estados Buscando, sem resultados e inicial. `AppState.busca` guarda o resultado; resposta de nonce antigo é ignorada.
- **B8 Ajustes** (`ui/Ajustes.kt`): menu do cartão do usuário ganhou "Ajustes". Conta, trocar senha (valida 8 a 128 e igualdade; limpa os campos quando o servidor confirma), sessões abertas (Encerrar, Encerrar todas as outras), aparência mínima (tamanho do texto pequeno/normal/grande, salvo em `Preferencias.tamanhoDoTexto` e aplicado ao Markdown por `LocalEscalaDoTexto`; tema só Grafite escuro) e Sair da conta.
- Imagens: `docs/telas/*.png` (69), geradas por `TelasSnapshotTest`, `ConversaSnapshotTest` e `PerfilBuscaSnapshotTest` com `AppState` falso.
- Verificado: 258 testes unitários (imagens: 24 + 24 + 21 + paleta; busca/amigos 9; ViewModel 15 + 23; repository 35; demais como antes) e 14 de integração (novos: amizade, DM e bloqueio; fixadas e busca; senha errada e trocada, sessões); `verifyPaparazziDebug` verde.

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
