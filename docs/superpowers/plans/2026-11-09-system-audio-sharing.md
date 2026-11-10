# Áudio do sistema no desktop — Plano de implementação

> **Para agentes implementadores:** seguir `superpowers:executing-plans` tarefa por tarefa; marcar as caixas ao concluir. Não delegar sem pedido explícito.

**Goal:** transmitir o áudio da saída do sistema junto com a tela no Windows e Linux, com mute independente e sem misturar o microfone. Android fica em plano próprio, porque o app ainda não existe.

**Architecture:** a captura de cada plataforma converte áudio para PCM comum; o publisher codifica Opus e multiplexa pacotes com timestamps na mesma sessão WebSocket do H.264. O servidor valida e retransmite os pacotes sem decodificar. O espectador decodifica áudio e vídeo em caminhos separados e toca o áudio sincronizado.

**Tech Stack:** Rust, Tokio/Axum, Opus existente, CPAL para saída de áudio desktop, WASAPI loopback no Windows, PipeWire monitor no Linux.

**Spec:** `docs/superpowers/specs/2026-11-09-system-audio-screen-share-design.md`

## Restrições globais

- A faixa de áudio do sistema inicia desligada; vídeo deve continuar se captura de áudio falhar ou for negada.
- O áudio do sistema é independente do microfone/relay de voz; não duplicar nem misturar essas fontes.
- Este plano cobre Windows e Linux. O plano Android depende da base do app e do compartilhamento de tela previstos em `docs/MOBILE-PLAN.md`, fase C; iOS está fora do escopo.
- O espectador pode silenciar o áudio sem fechar o vídeo.
- Pacotes usam Opus e timestamps monotônicos; filas têm limite e descartam áudio velho em vez de acumular atraso.
- Conteúdo protegido, apps que desabilitam captura e permissões negadas não podem derrubar vídeo ou voz.

## Pontos de revisão

- Permissão de captura negada ou API indisponível: a transmissão de vídeo continua e mostra motivo compreensível (Tarefas 3 e 4).
- Endpoint de saída desaparece ou muda durante a transmissão: captura encerra ou se recupera sem travar callback (Tarefa 3).
- Timestamps e perda/reordenação: sincronização relativa ao primeiro PTS de vídeo; jitter buffer limitado a 250 ms e descarte dos pacotes vencidos (Tarefas 1 e 2).
- Áudio do microfone também toca no stream ou captura inicia desligada: manter as fontes isoladas e cumprir o padrão desligado (Tarefas 3 e 4).
- Captura loopback pode incluir voz remota reproduzida pelo próprio Telacast. Avaliar exclusão por processo; se indisponível, explicar a limitação e recomendar fone para evitar eco (Tarefas 3 e 4).

---

### Tarefa 1: Protocolo de mídia e retransmissão do servidor

**Arquivos:**
- Modificar: `proto/src/lib.rs`
- Modificar: `server/src/main.rs`

**Interfaces:**
- Cabeçalho binário: 1 byte `kind` + 8 bytes `pts_us` big-endian + payload.
- `kind`: `0` H.264 delta, `1` H.264 keyframe, `2` Opus de áudio do sistema.
- `MediaKind::{H264Delta, H264Keyframe, SystemAudioOpus}`, `MediaPacketRef { kind: MediaKind, pts_us: u64, payload: &[u8] }`, `MediaPacketError::{TooShort, UnknownKind, EmptyPayload, TooLarge}`.
- `proto::parse_media_packet(data: &[u8]) -> Result<MediaPacketRef<'_>, MediaPacketError>` e `proto::encode_media_packet(kind: MediaKind, pts_us: u64, payload: &[u8]) -> Result<Vec<u8>, MediaPacketError>`.
- `pts_us` é monotônico relativo ao início do stream. H.264 mantém o payload Annex B; Opus usa 48 kHz, 20 ms por pacote.
- O quadro chave continua guardado no `Room` para novos espectadores; pacote Opus nunca substitui esse quadro.

- [x] Implementar o cabeçalho de 9 bytes e parser que rejeita pacote menor que cabeçalho + 1 byte de payload, tipo diferente de 0/1/2 e tamanhos acima do limite existente de 4 MiB.
- [x] Atualizar `Room`/`run_publisher` para encaminhar mídia, armazenar somente quadros H.264 chave e não interpretar/decodear Opus.
- [x] Atualizar `run_viewer` para manter a recuperação no próximo quadro chave sem descartar permanentemente a faixa de áudio após lag.
- [x] Incrementar `PROTO_VERSION` porque o formato do WebSocket de transmissão muda; manter a recusa de clientes incompatíveis no login.
- [x] Revisar os limites e compilar o protocolo/servidor; compilação final conjunta feita ao concluir as tarefas.

### Tarefa 2: Reprodução e sincronização no cliente desktop

**Arquivos:**
- Criar: `app/src/system_audio_playback.rs`
- Modificar: `app/src/viewer.rs`
- Modificar: `app/src/main.rs`
- Modificar: `app/Cargo.toml` somente se uma dependência direta for necessária

**Interfaces:**
- `system_audio_playback::Player::new() -> Result<Player, String>` cria um decoder Opus, fila limitada e saída CPAL.
- `Player::push(timestamp_us: u64, packet: &[u8])` agenda áudio decodificado sem bloquear a thread WebSocket; PTS se ancora ao primeiro quadro de vídeo e o jitter buffer não passa de 250 ms.
- `Player::mute(muted: bool)` silencia somente a faixa recebida do compartilhamento.

- [x] Separar, no viewer, os pacotes H.264 dos pacotes Opus por `MediaKind`; enviar cada payload para o decoder correto.
- [x] Converter PCM decodificado Opus (48 kHz) para taxa/canais da saída CPAL e agendar a reprodução pelo mesmo PTS de vídeo.
- [x] Limitar a fila de reprodução e descartar amostras novas quando a fila cheia impedir crescimento ilimitado da latência.
- [x] Adicionar botão de mute da faixa ao overlay de transmissão; não mudar o estado do microfone nem desconectar o vídeo.
- [x] Compilar com `cargo check -p telacast`; a validação manual com dois clientes continua pendente.

### Tarefa 3: Captura do áudio de saída no Windows e Linux

**Arquivos:**
- Criar: `app/src/system_audio.rs`
- Criar: `app/src/system_audio_windows.rs`
- Criar: `app/src/system_audio_linux.rs`
- Modificar: `app/Cargo.toml`
- Modificar: `app/src/main.rs` somente para enumerar/apresentar fontes de saída

**Interfaces:**
- `system_audio::outputs() -> Result<Vec<AudioOutput>, CaptureError>`; `AudioOutput { id: String, name: String }`; `Capture::start(output: Option<&AudioOutput>) -> Result<Capture, CaptureError>`, `None` seleciona a saída padrão.
- Captura entrega PCM float intercalado, 48 kHz, mono/estéreo informado; worker forma quadros Opus de 960 amostras/canal (20 ms) via fila bounded de 500 ms. Callbacks nativos não fazem I/O, bloqueio de mutex nem rede.

- [x] Capturar via WASAPI loopback; usar loopback por processo para excluir o próprio Telacast em Windows 10 build 20348 ou posterior, com fallback ao endpoint selecionado em versões antigas.
- [x] Capturar monitor do sink PipeWire selecionado e enumerar saídas; ausência de PipeWire/`pw-cat` vira erro de faixa.
- [x] Normalizar as amostras para PCM float intercalado, 48 kHz, estéreo.
- [x] Tratar falha de inicialização/captura como erro de faixa, mantendo o vídeo ativo.
- [ ] Compilar para Linux e validar manualmente captura; só o alvo Windows foi compilado neste ambiente.

### Tarefa 4: Codificação Opus, envio e opção de transmissão no desktop

**Arquivos:**
- Modificar: `app/src/publisher.rs`
- Modificar: `app/src/main.rs`
- Modificar: `app/src/system_audio.rs`

**Interfaces:**
- Publisher recebe `share_system_audio: bool` e fonte de saída opcional, além da qualidade de vídeo atual.
- `false` não inicia captura nem codificador de áudio; `true` inicia a captura e publica pacotes Opus com timestamp na sessão do stream.

- [x] Adicionar checkbox **Compartilhar áudio do sistema**, desligado por padrão, e seletor de saída no diálogo de transmissão.
- [x] Quando habilitado, codificar blocos de 20 ms em Opus a 48 kHz; atribuir PTS monotônico a partir do relógio da transmissão.
- [x] Enviar áudio em fila limitada sem bloquear o loop do vídeo; se a captura falhar, informar estado e manter H.264 ativo.
- [x] Encerrar captura junto do stream; opção desligada não inicia captura.
- [ ] Compilar e validar a captura manualmente em Windows e Linux com dois clientes: Linux ainda precisa ser compilado, e o fluxo de áudio requer uma sessão real para validar.

## Sequência de entrega

As Tarefas 1 a 4 formam a entrega desktop. A implementação Android é um plano separado, dependente da base de app e transmissão de tela mobile. Nenhuma alteração para iPhone será feita.
