# R5 — Viabilidade técnica do roadmap (crates, egui, transporte de áudio)

Data: 2026-11-07. Somente leitura e pesquisa: versões e datas vêm da API do crates.io (consultada hoje) e das
páginas do GitHub/docs.rs. Nada foi compilado nem instalado. O estado do PC foi checado só com `pkg-config`/`ls`.

## 1. Veredito geral

**O roadmap é viável em Rust quase 100% nativo.** Dos itens pesquisados, só dois pedem algo que não está no PC hoje,
e ambos são opcionais ou de fase tardia.

Achados que mudam o texto do roadmap:

1. **"O crate `opus` exige `libopus` do sistema" está desatualizado.** O `opus` 0.4.0 (23/08/2026) depende só de
   `opusic-sys`, que **compila a libopus 1.6.1 do código-fonte** (feature `bundled`, padrão). Precisa de `cmake` + compilador C.
   **Os dois já estão instalados** (`/usr/bin/cmake`, `gcc`). Resposta à decisão em aberto 2: usar `opus` 0.4 e **não** usar `audiopus_sys`
   (último release em 2021, abandonado).
2. **Pacotes `-dev` já presentes no PC:** ALSA, PipeWire (`libpipewire-0.3`), dbus-1, GTK3, openssl, xkbcommon, sqlite, libclang-22,
   libayatana-appindicator (runtime). **Ausentes:** headers do opus (só existe o `.so.0` de runtime), libpulse-dev, libudev-dev,
   meson, ninja. Nada do plano recomendado abaixo depende dos ausentes.
3. **O gargalo real de áudio sobre WebSocket não é o TCP, é o vídeo.** Se voz e quadros H.264 dividirem a mesma conexão,
   um quadro de vídeo grande atrasa o áudio atrás dele (bloqueio de cabeça de fila). Usar **uma conexão WS exclusiva para mídia de voz**,
   separada do WS de controle e do relay de tela, e `TCP_NODELAY`.
4. **O maior risco de UI não é nenhuma crate, é o egui**: texto rico com elementos inline (menção, emoji customizado),
   lista de mensagens de altura variável e seleção de texto entre mensagens. Detalhes na seção 3.
5. O egui 0.37 (que já usamos) **trouxe emoji coloridos** (changelog "Color emojis", 15/10/2026), o que derruba um risco do R10.

**Confiança:** média-alta nas tabelas de versão e dependências (dados vivos do crates.io); média nos itens de egui e de
runtime (X11, PipeWire, bandeja), que dependem de comportamento que só se confirma rodando.

## 2. Tabela por item

Legenda de risco: B baixo, M médio, A alto. "Libs do sistema" considera **este PC** (Ubuntu com os `-dev` listados acima).

| Item | Crate / técnica | Versão (atualização) | Exige libs do sistema? | Risco | Recomendação |
|---|---|---|---|---|---|
| Captura/saída de áudio | `cpal` | 0.18.2 (16/08/2026) | ALSA `-dev` sempre (presente). `pipewire` feature pede `libpipewire-0.3-dev` (presente); `pulseaudio` pede libpulse-dev (ausente) | M | Usar backend ALSA padrão (no Ubuntu cai no `pipewire-alsa`). Callback sem alocar nem travar: ring buffer (`rtrb`/`ringbuf`). Dispositivo pode vir a 44,1 kHz ou mono: prever `rubato` (Rust puro) |
| Codec Opus | `opus` (via `opusic-sys`) | 0.4.0 (23/08/2026); sys 0.7.7 (04/11/2026) | Não: compila libopus; precisa de **cmake + gcc (presentes)** | B | Adotar. Alternativa sem cmake: `OPUS_LIB_DIR` para o `.so.0` instalado, mas faltam headers/symlink `.so`. Evitar `audiopus_sys` (2021) |
| Supressão de ruído | `nnnoiseless` (RNNoise) | 0.5.2 (18/12/2025) | Não, Rust puro | B | Adotar no P2. Só 48 kHz mono em blocos de 480 amostras (10 ms): dois blocos por quadro Opus de 20 ms; estéreo exige processar por canal ou mixar |
| Eco / AGC | `webrtc-audio-processing` | 2.1.0 (13/05/2026) | **Sim**: ou `libwebrtc-audio-processing-dev` (apt), ou feature `bundled` com **meson + ninja** + C++ (ausentes) | A | **Adiar/descartar.** Eco só importa sem fone. Alternativa: AGC simples em Rust (RMS + ganho com limitador) e orientar uso de fone. O módulo `echo-cancel` do PipeWire é opção do sistema, não verificada aqui |
| Bandeja | `tray-icon` com feature `ksni`, ou `ksni` direto | tray-icon 0.26.1 (07/10/2026); ksni 0.3.7 (20/10/2026) | Backend padrão (appindicator): GTK3 + libappindicator (**presentes aqui**, mas é pacote do sistema). `ksni`: StatusNotifierItem via zbus, **Rust puro** | M | Preferir **`ksni` direto**: sem GTK, thread própria, sem laço de eventos GTK ao lado do winit. No GNOME exige a extensão AppIndicator (o Ubuntu já traz). Confirmar com `cargo tree` se `muda` (dependência obrigatória do tray-icon) puxa GTK no Linux |
| Notificação desktop | `notify-rust` | 4.18.2 (07/10/2026) | Não com feature `z` (zbus). A feature `d` usa libdbus (evitar) | B | Adotar com zbus. Ação ao clicar exige thread aguardando o handle |
| Atalho global / PTT | `global-hotkey` | 0.9.0 (16/10/2026) | Não: usa `x11rb` + `xkeysym` (Rust puro). **Só X11** | M | Serve para mutar/ensurdecer. **Para push-to-talk, preferir XInput2 (raw events) via `x11rb`**: `XGrabKey` rouba a tecla do jogo e some a tecla nos outros apps. Duas versões de `x11rb` (0.13 e nossa 0.14) compilam juntas, só aumenta o build |
| Segredo (token) | `keyring` v4 com store zbus, ou `oo7` | keyring 4.2.0 (29/08/2026); oo7 0.7.0 (05/11/2026) | keyring v4 virou núcleo + stores em crates separados: `zbus-secret-service-keyring-store` (puro) vs `dbus-secret-service` (libdbus) vs `linux-keyutils` (sem daemon, mas **não persiste após reboot**). `oo7`: zbus puro; criptografia `openssl` é opcional | M | Usar **`oo7`** ou keyring v4 + store zbus. Exige daemon Secret Service (gnome-keyring/KWallet) rodando. **Plano B obrigatório:** arquivo `0600` em `~/.local/share/telacast/` quando não houver daemon, avisando o usuário |
| Instância única | `single-instance` | 0.3.3 (**dez/2021**, parado) | Não | M | **Não usar.** Implementar em ~60 linhas: `flock` + socket Unix em `$XDG_RUNTIME_DIR/telacast.sock`. O mesmo socket repassa `telacast://convite/...` à instância viva |
| Área de transferência | `arboard` | 3.7.0 (15/10/2026) | Não: `x11rb`; Wayland é feature opcional | B | **Já vem dentro do eframe/egui-winit** para texto. Ligar a feature `image-data` só para colar imagem. No X11 o app precisa seguir vivo para servir o conteúdo |
| Diretórios XDG | `directories` | 6.0.0 (jan/2025) | Não | B | Adotar (ou `dirs`). Estável, pouco movimento |
| Logs | `tracing` + `tracing-subscriber` + `tracing-appender` | tracing 0.1.44 (dez/2025) | Não | B | Adotar no cliente e no servidor. Rotação diária no `appender` |
| Busca de mensagens | `rusqlite` FTS5 (já `bundled`, 0.41.1) | já no projeto | Não: SQLite embutido compila FTS5 | B | Tabela FTS5 externa (`content=messages`) + gatilhos. Tokenizer `unicode61 remove_diacritics 2` para português; `trigram` se quiser busca por trecho. Consultas do usuário **escapadas** (sintaxe FTS aceita operadores) |
| Cifra de mídia | `chacha20poly1305` | 0.11.0 (05/08/2026) | Não, RustCrypto | B | Adotar para UDP (item 4). Atenção: série 0.11 mudou API, não copiar exemplos 0.10 |
| Câmera | `v4l` (preferido) ou `nokhwa` | v4l 0.14.0 (**mai/2023**); nokhwa 0.10.11 (15/05/2026) | `v4l`: bindings próprios, em princípio sem libv4l (confirmar na build). `nokhwa`: backends extras, mais dependências | M | `v4l` direto (Rust, MJPEG/YUYV → NVENC). Aceitar que o crate está parado; a API V4L2 do kernel é estável |
| E2EE | `openmls` | 0.9.2 (22/10/2026) | Não (provedor `openmls_rust_crypto`) | A | P3 como está. Pesado em compilação e em complexidade; manter a alternativa AEAD por quadro com chave de sala |
| Auto-update | `self_update` | 1.3.3 (27/10/2026) | Não; puxa `reqwest`/TLS (compila mais) | M | Só P3. Alternativa leve: manifesto no servidor + `ureq` + `ed25519-dalek` e troca do binário |
| Portais (Wayland) | `ashpd` | 0.14.0 (05/11/2026) | Não nas features mínimas (zbus 6). GTK4/PipeWire só em features opcionais | B | Fora do uso atual (o PC é X11). Guardar para GlobalShortcuts em Wayland |
| Jogo em execução | `sysinfo` | 0.40.1 (30/10/2026) | Não | B | Para ler `/proc` a cada 5–10 s, **ler `/proc/*/comm` direto**: ~40 linhas, sem compilar a `sysinfo` (grande, pesa a RAM no build) |
| Rate limit | `tower_governor` | 0.8.0 (ago/2025); depende de `axum ^0.8`, `governor ^0.10` | Não | M | Funciona com axum 0.8. Atrás do Caddy, usar `SmartIpKeyExtractor` (lê `X-Forwarded-For`, só confiável se a origem for o proxy). Limite por **conta** no `Send`/login precisa de `governor` direto, não do tower layer |
| TLS no cliente | `tungstenite` 0.30 + `rustls` | tungstenite 0.30.0 | Não, mas escolher o provedor: `ring` (usa `cc`) ou `aws-lc-rs` (**exige cmake**) | B | Features `rustls-tls-webpki-roots` (ou `native-certs`). Preferir provedor **`ring`** para build mais leve. Recusar `ws://` fora de localhost no código, não só na UI |
| Argon2 | `argon2` (já 0.6.0) + `spawn_blocking` | já no projeto | Não | B | `spawn_blocking` + `Semaphore` de 2–3 permissões; hash falso para usuário inexistente. Memória por hash (19–64 MiB) pesa na VPS pequena: dimensionar o semáforo por RAM |
| TLS no servidor | Caddy (roadmap) ou `rustls` no próprio axum | — | Caddy é um binário Go à parte, **não é crate** | M | Caddy não encaminha o UDP do item 4. Alternativa 100% Rust: `axum-server` + `rustls-acme` (não pesquisado a fundo) |

## 3. Limitações do egui 0.37 por item

| Item | O que o egui faz | Limitação / contorno |
|---|---|---|
| Markdown e rich text | `LayoutJob` com `TextFormat` (cor, fundo, itálico, sublinhado, tachado, monoespaçada) | Não há **widget inline dentro de texto**: menção em "pílula", emoji customizado e spoiler clicável não fluem com o texto. Contornos: cor+fundo no trecho (menção); caractere reservado e pintura da imagem na posição do glifo (`galley` dá a posição do cursor); spoiler como trecho com fundo = cor do texto e revelar no clique. O `egui_commonmark` 0.26 (15/10/2026) serve para CommonMark, **não** para o dialeto do Discord (`\|\|spoiler\|\|`, `__sublinhado__`, `<@id>`, `-#`): escrever parser próprio e **cachear o `Galley` por mensagem** (id + largura). Esforço: **L**, não M |
| Seleção e cópia de texto | Label selecionável só dentro de **um** label | Sem seleção atravessando mensagens. Oferecer "Copiar texto" no menu de contexto e Ctrl+C na mensagem focada. (Comportamento conhecido; não reconfirmado na 0.37) |
| Emoji coloridos | **Novo na 0.37**: renderiza qualquer glifo do sistema (`add_font_provider`/`add_glyph_rasterizer`) | Depende de haver fonte de emoji no PC (`fc-list` mostra 1 fonte Color Emoji aqui). Issue aberta #8731: tesselação de texto ~60% mais lenta na 0.37 — medir em lista longa. Picker com centenas de emojis: usar `show_rows`/grade virtualizada. Emoji customizado de servidor = imagem, não glifo |
| Lista com milhares de mensagens | `ScrollArea::show_rows` só para altura **uniforme**; `show_viewport` para o resto | Mensagens têm altura variável: manter vetor de alturas medidas (cache por id e largura), soma de prefixos, desenhar só as visíveis com espaçador acima e abaixo. Invalidar o cache ao redimensionar. **Ancorar o scroll ao carregar histórico acima** e "colar no fim" (`stick_to_bottom`) são manuais. Imagens: reservar a altura pelos metadados antes de carregar, senão a lista "pula". Com cache de altura, 10 mil mensagens custam só as visíveis por quadro |
| Textarea com autocomplete | `TextEdit::multiline` devolve cursor/seleção (`TextEditOutput`, `TextEditState`) | Sem popup nativo: desenhar `Area` ancorada na posição do cursor do `galley`. Teclas ↑/↓/Tab/Enter precisam ser consumidas **antes** do `TextEdit` (`input_mut` + `consume_key`), senão ele as processa. Menção vira texto `<@id>` no envio; **sem pílula dentro do campo**. Enter envia / Shift+Enter quebra linha: mesmo truque. Testar compose/teclas mortas no X11 (acentos do português) cedo |
| AccessKit | Feature `accesskit` do eframe; 0.37 traz `accessible_name`, `Role`, `SetValue` no `TextEdit` | No Linux fala AT-SPI (Rust, via zbus). Widgets desenhados à mão (mensagens com `Galley` pintado) **não aparecem** sem nome explícito; lista virtualizada expõe só o que está na tela. Qualidade com Orca não foi verificada: prometer "teclado + contraste + zoom" e tratar leitor de tela como melhor esforço (P2/P3 como está) |
| Menus de contexto | `Response::context_menu`; 0.37 adiciona `container_context_menu` | Funciona bem. Não é menu nativo (sem integração de tema do GTK). Submenus existem mas são menos polidos; manter menus rasos |
| Arrastar e soltar | Arquivos do SO: `raw.dropped_files`/`hovered_files` (winit, XDND no X11). Interno: `dnd_drag_source`/`dnd_drop_zone` | **Soltar de fora funciona; arrastar para fora do app não existe.** `hovered_files` dá pouca informação de prévia. Reordenar servidores: viável com DnD interno. Colar imagem: `arboard` `get_image` |
| Modais, tooltips, atalhos | `Modal`, `Popup`, `Area`, `consume_key` | Sem foco/Tab trap automático em modais: implementar para Esc/Enter/Tab |

## 4. A troca de transporte de áudio (WS para UDP) é realista?

**Sim, e o plano do R04 (mesmo quadro nos dois) é o certo.** Ajustes:

- **Formato de quadro:** `ssrc u32 | seq u16 | ts u32 | payload Opus`, **menos de 1200 bytes** por datagrama. Definir já na versão WS.
- **Segurança:** não usar DTLS. O servidor entrega por TLS/WSS uma chave por sessão de voz; cada pacote vai com
  `chacha20poly1305`, cabeçalho como dado associado e nonce montado de `ssrc+seq` (contador, nunca repetir). Janela anti-replay no receptor.
- **NAT:** o servidor tem IP público (VPS), então **não precisa de ICE/STUN**. O cliente manda um primeiro pacote ("hello" com ticket) e o NAT abre a volta.
- **Operação:** o Caddy não faz proxy deste UDP. Abrir a porta UDP no firewall da VPS (a Oracle tem regra na lista de segurança **e** no iptables da imagem Ubuntu).
- **Fallback:** redes que bloqueiam UDP continuam no WS. O cliente testa UDP por 2–3 s e cai para WS, como já previsto.
- **Esforço:** o roadmap marca **L**; com o formato fixado na etapa WS e o jitter buffer já pronto, cai para **M** (socket tokio no servidor, thread `std::net::UdpSocket` no cliente, cifra, reenvio do ticket).
- **Latência já no WS:** `TCP_NODELAY`, quadros de 20 ms, bitrate 32–64 kbps; em VPS próxima a perda é baixa e o WS é aceitável por muito tempo.

## 5. O que é 100% Rust e o que depende do sistema

**100% Rust, sem nada do sistema (além de gcc/cmake que já existem):** opus (compila a libopus), nnnoiseless, chacha20poly1305,
argon2, rusqlite bundled, tracing, directories, arboard, global-hotkey (X11), notify-rust (zbus), ksni, oo7/keyring+zbus,
tower_governor, tungstenite+rustls(ring), openmls, ashpd (mínimo), sysinfo.

**Dependem de pacote/ferramenta do sistema (hoje presentes ou evitáveis):**

| Item | Dependência | Estado neste PC | Alternativa |
|---|---|---|---|
| cpal | `libasound2-dev` | presente | nenhuma melhor; já é padrão |
| cpal + PipeWire nativo | `libpipewire-0.3-dev` | presente | usar ALSA via `pipewire-alsa` |
| tray-icon (padrão) | GTK3 + appindicator | presente | `ksni` puro |
| keyring v1 / `dbus-secret-service` | `libdbus-1-dev` | presente | store zbus ou `oo7` |
| webrtc-audio-processing | apt `-dev` **ou** meson+ninja+C++ | ausente | não usar; AGC próprio |
| Captura do áudio do sistema (tela) | PipeWire | `pw-record` presente | ver abaixo |
| Caddy (TLS servidor) | binário à parte | ausente | `rustls-acme` no axum |
| Daemon de segredos | gnome-keyring/KWallet em execução | provável, não verificado | arquivo `0600` |
| Fonte de emoji colorida | Noto Color Emoji | presente (1) | embutir uma fonte no binário (cresce o executável) |

**Áudio do sistema na transmissão:** o caminho de menor custo é **subprocesso `pw-record --target <monitor> -`** lendo PCM cru,
sem compilar nada. O crate `pipewire` exige headers + libclang (bindgen) e a feature `pipewire` do cpal não foi confirmada para captura de monitor.
Subprocesso é gambiarra aceitável como P1; a versão "limpa" é o crate `pipewire` (presente aqui, mas é dependência de sistema em outras máquinas).

## 6. Ajustes sugeridos ao roadmap

1. **Decisão em aberto 2 (Opus):** resolver: usar `opus` 0.4 (compila libopus; cmake e gcc já instalados). Remover a frase "exige `sudo apt install`" e a menção a `audiopus_sys`.
2. **Fase 2, áudio:** acrescentar "conexão WS **separada** para mídia de voz + `TCP_NODELAY`" e "resample (`rubato`) para dispositivos fora de 48 kHz"; fixar o formato de quadro com cifra desde o início.
3. **Fase 2, P2:** retirar `webrtc-audio-processing` (exige meson/ninja ou apt). Manter `nnnoiseless`; trocar eco por "recomendar fone" + AGC próprio.
4. **Fase 2, UDP:** rebaixar de **L** para **M** condicionado ao formato fixo; acrescentar "abrir UDP no firewall da VPS" e "Caddy não serve o UDP".
5. **Fase 1, Markdown:** subir de **M–L** para **L**; escrever o parser próprio do dialeto do Discord e o cache de `Galley`. Acrescentar tarefa explícita **"lista virtualizada com altura variável e ancoragem de scroll"** (hoje diluída) e fazer **o protótipo antes** do resto da fase: é onde o egui mais aperta.
6. **Fase 1, menções:** especificar "popup de autocomplete próprio, sem pílula no campo".
7. **Fase 0.4, instância única:** trocar `single-instance` (parado desde 2021) por `flock` + socket Unix próprio, já servindo o `telacast://`.
8. **Fase 0.4, token:** `oo7`/keyring+zbus com **plano B em arquivo `0600`**; sem esse plano B o login quebra em máquina sem daemon.
9. **Fase 4, sistema:** bandeja via `ksni`; notificação via `notify-rust` com feature zbus; PTT por XInput2 em vez de `global-hotkey` (que fica para mutar/ensurdecer).
10. **Fase 0.2, TLS:** registrar a alternativa sem Caddy (`rustls-acme`) e a escolha do provedor `ring` no cliente. Rate limit: `tower_governor` só por IP; por conta com `governor` direto.
11. **Fase 4, jogo em execução:** ler `/proc` direto, sem `sysinfo`.
12. **RAM no build (15 GB com pouca folga; `free` mostrou ~2 GB disponíveis agora):** adicionar crates grandes **uma por vez** e nunca compilar com jogo aberto. Os mais pesados da lista: `openmls`, `self_update`/`reqwest`, `sysinfo`, `ashpd` com GTK4. Considerar `CARGO_BUILD_JOBS=2` e `[profile.dev] debug = "line-tables-only"`.
13. **Medir cedo:** custo de tesselação com emoji colorido (issue #8731) numa lista de 2 mil mensagens; teclas mortas no `TextEdit` em X11; `cargo tree -i gtk` com `tray-icon`+`ksni` para confirmar que GTK não entra.

## 7. Não verificado

- Se `global-hotkey` 0.9 entrega eventos de **soltar** tecla no X11 (README não diz; checar o código antes de apostar nele para PTT — o plano XInput2 evita a dúvida).
- Se `muda` puxa GTK no Linux quando só a feature `ksni` do tray-icon está ativa.
- Se a feature `pipewire` do cpal captura monitores (áudio do sistema).
- Se `v4l` 0.14 compila sem libv4l no padrão; se `egui` 0.37 ainda não seleciona texto entre labels.
- Existência de daemon Secret Service ativo neste PC.
