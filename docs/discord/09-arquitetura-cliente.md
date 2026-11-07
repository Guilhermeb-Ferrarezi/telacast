# 09 — Arquitetura do cliente desktop do Discord (e o que o Telacast deve aproveitar)

Análise feita só por leitura leve (ls, cat, grep -a, strings filtrado) no pacote snap do Discord 1.0.168 (Linux x64, canal stable). Nada foi executado e a REA não foi usada.

Legenda: **[F]** = fato observado no disco; **[I]** = inferência.

## 1) Visão geral

- **[F]** O Discord Desktop é um app **Electron/Chromium**. O binário `Discord` pesa ~218 MB (Electron embutido), com `libffmpeg.so`, `libEGL`, `libGLESv2`, `libvulkan`, SwiftShader, `snapshot_blob.bin`, `chrome_crashpad_handler` e `locales/`.
- **[F]** Duas camadas de JS:
  - `resources/app.asar` (3,4 MB): shell fino. Contém o processo principal e o bootstrap. Strings encontradas: `requestSingleInstanceLock` (1x), `autoUpdater`, `crashReporter`, `powerMonitor`, `autostart`, `registerSchemes`, `Notification(`, `XDG_CURRENT_DESKTOP`, `sentry.io`, `updates.discord.com`.
  - `standalone_modules/discord_desktop_core/core.asar` (1,6 MB): o "core" do processo principal. Contém `Tray(`, `Notification(`, `safeStorage`, `nativeImage`, `setBadgeCount`, `crashReporter` (56x), `overlay`/`overlayHandle`, `DISCORD_CLIP_PROTOCOL`, `DISCORD_CODE_INTEGRITY`, e canais IPC `DISCORD_WINDOW_*` e `DISCORD_FILE_MANAGER_*` (caminho de log de módulos, cache de assets).
- **[F]** `build_info.json`: `releaseChannel: stable`, `version: 1.0.168`, `sentryDist: stable-linux-x64`, `sentryRelease: discord-desktop-<hash git>`, `standaloneModules: true`. Esta flag diz que os módulos nativos vêm **empacotados no disco** em `standalone_modules/` (modo snap/deb) em vez de baixados em tempo de execução para `~/.config/discord/<versão>/modules`.
- **[F]** `resources/bootstrap/manifest.json` lista os arquivos que o bootstrap valida (ex.: `node_modules/@grpc/grpc-js/...`, `undici-types`, `@grpc/proto-loader`). Cada módulo tem seu `manifest.json`. O manifest do bootstrap lista versões 0 para `discord_desktop_core`, `discord_erlpack`, `discord_spellcheck`, `discord_utils`, `discord_voice`, `discord_zstd`.
- **[F]** `updater.node` (5,9 MB) e `updater_bootstrap` (4,7 MB) são o atualizador nativo, um binário próprio e fora do Electron. **[I]** No snap o auto-update fica desligado, porque quem atualiza é o snapd. No deb/tar.gz o updater baixa de `updates.discord.com` (URL vista em app.asar) e o `modules.*` vem de `https://discord.com/api/...` (URL de `api/download/` vista).
- **[F]** Estrutura de cada módulo nativo: `index.js` + `.node` (addon N-API) + `manifest.json` + `package.json`. O JS é só um `require('./x.node')` com adaptações (ver `discord_utils/index.js`).
- **[F]** Idiomas dos addons: `discord_utils` é C++ (fontes `discord_desktop/native_modules/discord_utils/src/wayland_global_shortcuts.cpp`, com sd-bus). `discord_dispatch`, `discord_cloudsync` e `discord_sysimg` são **Rust** (std, `rustc 73dc9167…`, `reqwest/http`).
- **[I]** Modelo de processos: main (Electron + `core.asar`) -> renderer (web app React do Discord, carregado de discord.com) -> addons nativos carregados no main e no renderer via `DiscordNative` (preload/contextBridge). O `discord_utils/index.js` detecta `window.DiscordNative.isRenderer` e escolhe o comportamento de cada processo.
- **Dados locais [F]** (`~/.config/discord`, ou o equivalente no snap): `Cache`, `Code Cache`, `GPUCache`, `DawnGraphiteCache`, `Local Storage`, `Session Storage`, `Service Worker`, `Cookies`, `Preferences`, `Local State`, `Crashpad`, `sentry`, `logs`, `discord_asset_cache`, `Dictionaries`, `blob_storage`, `quotes.json`, `settings.json`, `domainMigrated`. Ou seja, é o perfil padrão do Chromium mais uns poucos arquivos próprios. `settings.json` guarda as preferências do shell (ex.: `DESKTOP_PROCESS_POLLING_PERF`, lido via `appSettings`/`DiscordNative.settings`).

## 2) Inventário dos módulos nativos

Tamanhos de `du -sh`: voice 149M, krisp 86M, ndi 36M, utils 23M, spellcheck 14M.

| Módulo | Observado no disco [F] | Função (inferência [I] quando marcada) |
|---|---|---|
| `discord_desktop_core` | `core.asar`, `app/images`, `index.js` | Núcleo do main: janelas, tray, notificações, IPC, protocolo `discord://`, integridade de código. |
| `discord_modules` | `.node` + manifest | Gerenciador de módulos (instala/atualiza/carrega os demais). [I] |
| `discord_voice` (149M) | `.node`, `libmediapipe.so`, `gpu_encoder_helper`, `selfie_segmentation*.tflite`; símbolos `getInputDevices`, `getCodecCapabilities`, `getEncryptionModes`, `enableDucking`, `enableVideoEffects`, `enableSpatialBlend`, HRTF/HRIR | Motor de voz e vídeo: WebRTC próprio, dispositivos de áudio, codecs, cifragem, ducking, áudio espacial, fundo virtual (MediaPipe), encoder de GPU em helper separado. Contém o stack de captura de tela e Go Live. |
| `discord_krisp` (86M) | modelos `.kef` (`nc-o-lite/med/nb`, `bvc-o-pro`, `vad-o`), pasta `KMS` | Supressão de ruído e VAD da Krisp (proprietário). [I] |
| `discord_utils` (23M) | C++, `wayland_global_shortcuts.cpp`, `input_capture.js`, `input_event.js`, `linux_dom_keycodes.js`; símbolos `PortalShortcut`, `WaylandIdleManager`, `SteamObserver`, `getDiscordMemoryUsage`, `getDiscordIsElevated`, `getExecutableFingerprintForProcess`, `setCandidateGamesCallback`, `inputWatchAll`, `getWaylandPortalTrigger` | Atalhos globais e captura de teclado (X11 + portal Wayland GlobalShortcuts via D-Bus), detecção de ociosidade no Wayland, monitor de processos candidatos a jogo, observador do Steam, telemetria de memória/CPU dos processos Discord, push-to-talk. |
| `discord_game_utils` | `/proc/%d/{cmdline,exe,stat,statm}`, `xdg-mime query default x-scheme-handler/`, `xdg-open`, `CreateShortcuts/RemoveShortcuts` | Utilitários de jogo: lê /proc para identificar o processo, cria/remove atalhos .desktop de jogos, abre URLs/esquemas (`steam://`). [I] |
| `discord_rpc` | `RPCIPC.js` (usa `XDG_RUNTIME_DIR`), `RPCWebSocket.js`, `safeEmitter.js`, node_modules (grpc-js, protobuf, @bufbuild) | Servidor local do Rich Presence/RPC: socket unix `discord-ipc-N` em `$XDG_RUNTIME_DIR` (e variantes de snap/flatpak) e WebSocket em 127.0.0.1 (faixa de portas 6463+). [I] pela convenção pública; só `XDG_RUNTIME_DIR` foi confirmado nas strings. |
| `discord_dispatch` | Rust, reqwest/http | Gerenciador de instalação/atualização de jogos da loja (Dispatch). [I] |
| `discord_cloudsync` | Rust, usa `XDG_DATA_HOME`/`DOCUMENTS` | Sincronização de saves na nuvem (CloudSync). [I] |
| `discord_spellcheck` (14M) | JS + node_modules | Corretor ortográfico (dicionários em `Dictionaries/`). [I] |
| `discord_ndi` (36M) | `discord_ndi` (helper), `libndi.so.6`, `libavahi-*` | Saída NDI (vídeo em rede) para streaming em software profissional; Avahi para descoberta mDNS. [I] |
| `discord_zstd` | `.node` | Compressão zstd do gateway (zstd-stream). [I] |
| `discord_erlpack` | `.node` | (De)serialização ETF (Erlang term format) do gateway. [I] |
| `discord_sysimg` | Rust, mínimo | Imagens/ícones do sistema (ícones de aplicativos para a lista de jogos/janelas). [I] |

Arquivos de fora do Electron: `updater.node`, `updater_bootstrap`, `chrome_crashpad_handler`, `discord.desktop` (`MimeType=x-scheme-handler/discord`, `Exec=/usr/bin/discord --url -- %u`, `StartupWMClass=discord`).

## 3) Funcionalidades de integração com o sistema (como fazer no Telacast)

O Telacast é egui/winit e não tem Chromium, então muita coisa se resolve com crates pequenos. Esforço: S = horas, M = 1-3 dias, L = semana ou mais.

### 3.1 Instância única + deep link (`telacast://`)
- **Discord [F]**: `requestSingleInstanceLock`; protocolo `discord://` registrado no `.desktop` (`MimeType`) e `xdg-mime`.
- **Telacast**: crate `single-instance` (ou lock file + socket unix em `$XDG_RUNTIME_DIR/telacast.sock` para repassar o argumento `telacast://convite/...` à instância viva). Registrar `x-scheme-handler/telacast` num `.desktop`. Convites por link são o caso de uso principal.
- Esforço: **S** (lock) + **M** (repasse de URL para a janela já aberta).

### 3.2 Ícone de bandeja (tray) e "fechar para a bandeja"
- **Discord [F]**: `Tray(` no core, `nativeImage`, `setBadgeCount`; ícones alternativos por estado (menções, mudo).
- **Telacast**: crate `tray-icon` (+ `muda` para o menu). No Linux exige GTK ou StatusNotifierItem; o `tray-icon` usa libappindicator/GTK3. O egui roda em loop próprio, então a bandeja precisa de thread GTK separada, com um canal `std::sync::mpsc` para a UI. Alternativa sem GTK: `ksni` (StatusNotifierItem puro em D-Bus, zbus). Interceptar o fechamento da janela com `ViewportCommand::CancelClose` e `Visible(false)`. Menu: Abrir, Mutar/Ensurdecer, Sair. Trocar o ícone no estado "mutado" ou "em chamada".
- Esforço: **M**.

### 3.3 Notificações nativas
- **Discord [F]**: `Notification(` (Electron usa libnotify/D-Bus por baixo); badge de não lidas.
- **Telacast**: crate `notify-rust` (D-Bus `org.freedesktop.Notifications`, ações e ícone), ou `ksni` para o badge. Disparar só quando a janela está sem foco (checar `ViewportInfo::focused`) e respeitar silenciar por canal. Clicar na notificação abre o canal (callback de ação).
- Esforço: **S**.

### 3.4 Atalhos globais (push-to-talk, mutar, ensurdecer)
- **Discord [F]**: `discord_utils` captura teclado (`inputWatchAll`, `linux_dom_keycodes.js`) e, no Wayland, usa o portal `org.freedesktop.portal.GlobalShortcuts` via sd-bus (`PortalShortcut`, `getWaylandPortalTrigger`).
- **Telacast**: no X11, crate `global-hotkey` (XGrabKey), que cobre "mutar" e "ativar/desativar". **Push-to-talk precisa de evento de soltar a tecla**, e o `global-hotkey` entrega Pressed e Released, então serve. Para modificadores soltos ou teclas do mouse, ler `XInput2` raw events com `x11rb` (já usado no projeto) ou `evdev` (exige grupo `input`, desaconselhável). No Wayland, usar `ashpd` (portal GlobalShortcuts), que só existe em compositores recentes (GNOME 48+, KDE 6). Como o Telacast já usa X11, começar só por X11.
- Esforço: **M** (X11) + **L** (portal Wayland).

### 3.5 Iniciar com o sistema
- **Discord [F]**: strings `autostart` (8x) em app.asar; entrada em `~/.config/autostart/`.
- **Telacast**: crate `auto-launch` ou escrever `~/.config/autostart/telacast.desktop` com `X-GNOME-Autostart-enabled` e flag `--minimized` (iniciar na bandeja). Dentro de Flatpak, usar o portal Background.
- Esforço: **S**.

### 3.6 Rich Presence local (RPC/IPC) e detecção de jogos
- **Discord [F]**: `discord_rpc` (socket `discord-ipc-N` em `$XDG_RUNTIME_DIR`, WebSocket local); `discord_utils` (`setCandidateGamesCallback`, `SteamObserver`, `getExecutableFingerprintForProcess`) varre processos; `discord_game_utils` lê `/proc/<pid>/{cmdline,exe,stat}`. A lista de jogos conhecidos vem do servidor (API de aplicações detectáveis) [I].
- **Telacast**: (a) **Detecção**: varrer `/proc` a cada 5-10 s com o crate `sysinfo` (ou `procfs`), casando `exe`/`cmdline` com uma tabela do servidor Telacast (nome, exe, ícone). Também checar a janela em foco via x11rb (`_NET_ACTIVE_WINDOW` + `_NET_WM_PID`). Mostra "Jogando X" e já ajuda a sugerir a janela certa no seletor de captura. (b) **RPC local**: socket unix em `$XDG_RUNTIME_DIR/telacast-ipc-0` com JSON em frames (u32 opcode + u32 tamanho + payload, o mesmo formato público do Discord para reaproveitar SDKs). Crate `tokio::net::UnixListener`. Começar só com `SET_ACTIVITY`.
- Esforço: **M** (detecção) + **M** (RPC mínimo).

### 3.7 Overlay em jogo
- **Discord [F]**: strings `overlay`/`overlayHandle` no core; injeção em Windows e Linux via módulo de voz/overlay (não localizado como módulo separado no disco). **[I]** No Linux o Discord praticamente não oferece overlay funcional hoje.
- **Telacast**: não vale a pena agora. Alternativa barata: janela egui secundária "sempre no topo" (`ViewportBuilder::with_always_on_top`, sem decoração, transparente, click-through via `with_mouse_passthrough`) com os falantes. Funciona em jogos em janela sem borda, não em tela cheia exclusiva.
- Esforço: **M** (janela flutuante) / **L** (injeção real, não recomendado).

### 3.8 Atualizador
- **Discord [F]**: `updater.node` + `updater_bootstrap` + `autoUpdater` (Squirrel no Windows) e `updates.discord.com`; módulos versionados por manifest; no snap, quem atualiza é o snapd.
- **Telacast**: se distribuir por AppImage/binário, crate `self_update` (GitHub/HTTP) ou `cargo-dist` com updater; servidor Telacast serve `/updates/manifest.json` (versão, URL, sha256, assinatura minisign/ed25519 com `ed25519-dalek`). Verificar a assinatura antes de trocar o binário (`self-replace`). Se for por pacote de distro, só avisar "nova versão disponível". Checar versão mínima na conexão (o servidor recusa clientes antigos).
- Esforço: **M** (aviso) / **L** (auto-update assinado).

### 3.9 Telemetria e crash reports
- **Discord [F]**: Sentry (`sentryDist`, `sentryRelease`, `sentry.io`), Crashpad (`chrome_crashpad_handler`, pasta `Crashpad`, `sentry`), `crashReporter` com muitas chamadas, telemetria de memória/CPU por processo.
- **Telacast**: panic hook que grava `~/.local/share/telacast/crash/<data>.txt` (backtrace, versão, GPU, driver) + botão "Enviar relatório" **opt-in** para o servidor próprio (sem terceiros, coerente com "não usa Discord"). Crate `sentry` é opcional (aponta para Sentry self-hosted/GlitchTip). Logs com `tracing` + `tracing-appender` com rotação. Incluir no relatório as falhas de NVENC e captura, que são o ponto fraco.
- Esforço: **S** (panic hook + log) / **M** (envio).

### 3.10 Arrastar e soltar arquivos, colar imagem
- **Discord [I]**: nativo do Chromium (DataTransfer); nada específico nos módulos.
- **Telacast**: egui dá `ctx.input(|i| i.raw.dropped_files)` e `hovered_files` prontos (winit). Falta o upload no servidor, preview e colar imagem via `arboard` (`get_image`). Pastas: tratar `path.is_dir()`.
- Esforço: **S**.

### 3.11 Cache e armazenamento local
- **Discord [F]**: perfil Chromium (HTTP cache, IndexedDB/Local Storage, `discord_asset_cache`), `Cookies` e `safeStorage` (token cifrado pelo keyring do SO).
- **Telacast**: usar `directories` (XDG): config em `~/.config/telacast/`, cache em `~/.cache/telacast/` (avatares e emojis por hash de conteúdo, com limite de tamanho e LRU) e estado em `~/.local/share/telacast/`. **Token de sessão no Secret Service** com crate `keyring` (ou `oo7`), nunca no eframe storage em texto puro. Considerar SQLite (`rusqlite`) para mensagens em cache e busca offline.
- Esforço: **S** (diretórios + keyring) / **M** (cache de mídia com LRU).

### 3.12 Outros pontos observados
- **Ociosidade/suspensão [F]**: `powerMonitor` (13x) e `WaylandIdleManager`. Telacast: status "ausente" automático via D-Bus `org.freedesktop.ScreenSaver`/logind `PrepareForSleep` (crate `zbus`), e pausar a captura ao bloquear a tela. **S/M**.
- **Verificação de elevação [F]**: `getDiscordIsElevated`. Telacast: avisar se rodar como root. **S**.
- **Integridade de código [F]**: `DISCORD_CODE_INTEGRITY`. Pouco relevante.
- **Clips [F]**: `DISCORD_CLIP_PROTOCOL`, `getDiscordClipsPid`. Corresponde a um buffer de replay; no Telacast é derivável do encoder NVENC já existente (anel de GOPs). **L**.
- **Efeitos de vídeo [F]**: MediaPipe + `selfie_segmentation.tflite` (fundo virtual). Telacast: crate `ort` (ONNX) ou `tract`. **L**, baixa prioridade.

## 4) Prioridade

| P | Item | Esforço | Motivo |
|---|---|---|---|
| **P0** | Token no keyring, diretórios XDG, log em arquivo + panic hook | S | Base de segurança e depuração; custo baixo. |
| **P0** | Instância única | S | Evita duas conexões e dois dispositivos de áudio disputando a captura. |
| **P1** | Bandeja + fechar para a bandeja | M | Cliente de voz precisa continuar rodando em segundo plano. |
| **P1** | Notificações nativas | S | Mensagens/menções com janela sem foco. |
| **P1** | Arrastar e soltar + colar imagem | S | Pequeno e muito visível. |
| **P1** | Atalhos globais X11 (mutar, PTT) | M | Essencial para voz durante jogos. |
| **P2** | Detecção de jogo por /proc + "Jogando X" | M | Dá contexto social e ajuda a escolher a janela a transmitir. |
| **P2** | Iniciar com o sistema (autostart) | S | Conveniência. |
| **P2** | Deep link `telacast://` + convites | M | Depende do fluxo de convites no servidor. |
| **P2** | Aviso/manifest de atualização | M | Antes do auto-update assinado. |
| **P2** | Ausente automático (idle/suspensão) | S/M | Presença correta. |
| **P3** | RPC local `telacast-ipc` | M | Só vale quando houver ecossistema de jogos. |
| **P3** | Janela de overlay flutuante | M | Funciona só com jogos em janela sem borda. |
| **P3** | Auto-update assinado com troca do binário | L | Depende da forma de distribuição. |
| **P3** | Atalhos no Wayland (portal), clips, fundo virtual, NDI | L | Alto custo e público pequeno. |

Observação de arquitetura: o Discord separa voz/vídeo em um addon nativo gigante (149 MB) e um helper de GPU (`gpu_encoder_helper`) à parte. Para o Telacast faz sentido manter captura+NVENC em uma thread/processo auxiliar (um crash do driver NVENC não derruba a UI), com IPC simples por canal ou socket. Também vale copiar o padrão de **manifest por módulo com hash** apenas se um dia houver plugins; hoje um binário único é mais simples.

## 5) Fontes e caminhos consultados

Raiz: `/snap/discord/current/usr/share/discord/`
- `resources/build_info.json`, `resources/bootstrap/manifest.json`, `discord.desktop`
- `resources/app.asar` e `resources/standalone_modules/discord_desktop_core/core.asar` (grep -a de strings; asar sem compressão)
- `resources/standalone_modules/*/` (ls, `manifest.json`, `package.json`, `discord_utils/index.js`, `discord_game_utils/index.js`, `discord_rpc/RPCIPC.js`; `strings -n 8` filtrado nos `.node` de utils, game_utils, dispatch, cloudsync, sysimg, voice)
- Raiz do pacote: `Discord`, `updater.node`, `updater_bootstrap`, `chrome_crashpad_handler`
- Perfil local: `~/.config/discord` (somente listagem de nomes e tamanhos)

Limites: os `.node` não foram desmontados e o app.asar não foi decompilado; as funções marcadas [I] vêm de nomes de símbolos, strings, convenções públicas e do tamanho dos arquivos. O formato do RPC (opcodes) e a lista de portas 6463+ são conhecimento público, não confirmados no disco. O `settings.json` não existia no perfil consultado.
