# 04 - Voz, vídeo e transmissão de tela (Discord) e proposta para o Telacast

Legenda: **[DOC]** = fato de documentação oficial/suporte; **[CLI]** = observado no cliente instalado (index.js do discord_voice, só nomes de funções); **[INF]** = inferência minha; **[3P]** = fonte de terceiros, não confirmada.

## 1) Visão geral

- O Discord separa **sinalização** (WebSocket "voice gateway") e **mídia** (UDP). O cliente precisa receber UDP, inclusive atrás de NAT. [DOC]
- Fluxo: o gateway principal recebe Opcode 4 (Voice State Update: guild, canal, self_mute, self_deaf) e devolve `VOICE_STATE_UPDATE` (session_id) e `VOICE_SERVER_UPDATE` (token, endpoint). Depois abre-se `wss://endpoint?v=8`, Identify (op 0), Hello (op 8), Ready (op 2: ssrc, ip, porta, modos), IP Discovery por UDP, Select Protocol (op 1), Session Description (op 4: modo + chave de 32 bytes), Speaking (op 5). [DOC]
- Áudio: **Opus 48 kHz estéreo**, pacote RTP (payload type 0x78) cifrado. Vídeo viaja no mesmo esquema RTP, com SSRCs próprios. [DOC; vídeo por SSRC é INF]
- O servidor de voz é um **SFU** (recebe um fluxo por participante e o reencaminha), não mistura nem faz P2P em canais. [INF, consistente com o modelo de SSRC por usuário e com op "video sink wants" da v5]
- Desde 01/03/2026 DMs, chamadas em grupo, canais de voz e Go Live exigem E2EE (**DAVE**, baseado em MLS). [DOC]
- O cliente desktop usa um módulo nativo (`discord_voice.node`, 121 MB) com engine WebRTC própria, Krisp, captura de tela por GPU (`gpu_encoder_helper`) e `libmediapipe.so`. [CLI]

## 2) Lista de funcionalidades

### 2.1 Canais e conexão
| Função | Como o usuário vê | Tecnologia |
|---|---|---|
| Canal de voz | Clique no canal, entra, lista de participantes embaixo do canal e painel de voz com ping | Op 4 do gateway + voice WS v8 [DOC] |
| Canal de palco (Stage) | Oradores x plateia; plateia ouve e "levanta a mão"; moderador convida | Mesmo transporte, permissão de falar por papel [INF]. Limites exatos não confirmados |
| Sair / mover canal | Botão desconectar; arrastar usuário entre canais | Op 4 com `channel_id` null / novo. Troca de canal gera novo token [DOC] |
| Região do servidor de voz | Antes era seleção manual por canal (override de região); hoje automática | `endpoint` vem do VOICE_SERVER_UPDATE [DOC]; "automática" é [INF] |
| Latência / ping | Indicador verde/amarelo/vermelho e "ping de voz" | Heartbeat op 3/6 com nonce [DOC]; `setPingCallback`, `getStats` [CLI] |
| Reconexão | "RTC conectando..." e volta sozinho | Resume op 7 / Resumed op 9, v8 com `seq_ack` (buffered resume) [DOC] |

### 2.2 Áudio
| Função | Como vê | Tecnologia |
|---|---|---|
| Mudo / ensurdecer | Botões microfone e fone; ícones ao lado do nome | `self_mute`/`self_deaf` no op 4 [DOC]; `setSelfMute/Deafen` [CLI] |
| Mudo/ensurdecer pelo servidor | Ícone vermelho; só moderador | Permissões (Mute/Deafen Members) [INF] |
| Detecção de voz (VAD) | Anel verde no avatar quando fala; sensibilidade automática ou slider | Callback de fala `setOnSpeakingCallback`, flags do op 5 [DOC/CLI]; algoritmo local [INF] |
| Push-to-talk | Tecla configurável, atalho global, delay de soltura | `setPTTActive` [CLI]; atalho global é função do app [INF] |
| Fala enquanto mudo | Aviso "você está mudo" | `setOnSpeakingWhileMutedCallback` [CLI] |
| Supressão de ruído | Opções Nenhum / Krisp / Padrão | Krisp (IA local) [3P]; módulo/CLI presente (`manifest.json` lista arquivos) |
| Cancelamento de eco, AGC | Toggles em Voz e Vídeo | Pipeline estilo WebRTC APM [INF] |
| Volume por usuário | Botão direito no usuário: slider 0-200% e "mutar para mim" | `setLocalVolume`, `setLocalMute` (só local) [CLI] |
| Pan/posição | Não exposto ao usuário comum | `setLocalPan`, `setUserPosition` [CLI] |
| Prioridade de fala | Baixa o volume dos outros | Flag `1<<2` do Speaking [DOC]; `setRemoteUserCanHavePriority` [CLI] |
| Dispositivos | Seleção de entrada/saída, teste de microfone | `audioSubsystem` (padrão no Linux) [CLI] |
| Soundboard / replay local | Sons curtos no canal | `startSamplesLocalPlayback`, `startReplay` [CLI]; resto [INF] |
| Bitrate do canal | Slider do admin; teto por nível de boost | 64 kbps sem boost, 128 / 256 / 384 kbps nos níveis 1 / 2 / 3 [3P] |

### 2.3 Vídeo e tela
| Função | Como vê | Tecnologia |
|---|---|---|
| Câmera | Botão "Vídeo"; mosaico de participantes | Vídeo RTP, codecs negociados [INF]; `setOnVideoCallback`, `setDisableLocalVideo` [CLI] |
| Compartilhar tela / janela (Go Live) | Seletor de tela ou janela, qualidade (gear), até 50 pessoas com vídeo/tela por canal | `setDesktopSource(WithOptions)`, `setGoLiveDevices`, `setVideoBroadcast` [CLI]; limite de 50 [DOC suporte] |
| Qualidade / fps / resolução | Presets 720p/1080p/1440p/fonte e 15/30/60 fps | Gratuito até 720p 30 fps [3P]; Nitro até 4K 60 fps [DOC suporte, via busca; página 403 no fetch direto] |
| Áudio do sistema/aplicativo | Opção "Compartilhar áudio" | Speaking com flag Soundshare `1<<1` [DOC]; `setOnSoundshare*` [CLI] |
| Fallback de codec | Troca automática | `setOnVideoEncoderFallbackCallback`, `...DecoderFallback...` [CLI]. Encoder em GPU separado: `gpu_encoder_helper` [CLI] |
| Sincronia A/V | Transparente | `setAvSyncEnabled` [CLI] |
| Clipes | "Clip" de uma transmissão | `setClipRecordUser`, `startReplay` [CLI] |
| Plano de fundo/efeitos de câmera | Desfoque/fundo virtual | `libmediapipe.so` [CLI]; função exata [INF] |

### 2.4 Protocolo (detalhe útil para o Telacast)
- **Modos de criptografia [DOC]:** `aead_aes256_gcm_rtpsize` (preferido, depende de hardware), `aead_xchacha20_poly1305_rtpsize` (obrigatório). Demais (`xsalsa20_poly1305*`, `aead_aes256_gcm`) descontinuados desde 18/11/2024. Nonce de 32 bits incremental anexado ao fim do payload.
- **IP Discovery [DOC]:** pacote UDP de 74 bytes (tipo 2B, length 2B = 70, SSRC 4B, endereço 64B, porta 2B), big-endian.
- **RTP [DOC]:** byte 0x80, PT 0x78, seq u16, timestamp u32, SSRC u32. Ao pausar, enviar 5 frames de silêncio Opus (`F8 FF FE`).
- **Speaking [DOC]:** bitmask (microfone, soundshare, prioridade), deve ser enviado antes do áudio.
- **DAVE [DOC]:** `max_dave_protocol_version` no Identify; MLS (ops 21-31: transição, epoch, key package, proposals, commit/welcome); AES128-GCM por frame com tag truncada de 8 bytes, ratchet por remetente, marcador `0xFAFA`. Cliente expõe `prepareSecureFramesTransition`, `getMLSKeyPackage`, `processMLSProposals` [CLI].

## 3) Proposta técnica para o Telacast

Premissas: servidor próprio em Rust (axum + tokio), cliente egui. O Telacast pode usar protocolo próprio; compatibilidade com Discord não é necessária.

### 3.1 Áudio (núcleo)
- **Captura/saída:** `cpal` (ALSA/PulseAudio/PipeWire no Linux). Mesmo clock de 48 kHz, quadros de 20 ms (960 amostras).
- **Codec:** crate `opus` (binding de libopus, exige libopus instalada) ou `audiopus`. Alternativa 100% Rust: `opus-rs`/`magnum-opus` são imaturos [INF]; ficar com o binding C.
- **Jitter buffer + mixagem:** buffer adaptativo próprio por remetente (5 a 10 quadros), PLC do Opus para perda, mixagem soma+limitador no callback do cpal. Volume por usuário = ganho multiplicativo local.
- **Processamento:** VAD simples por energia (RMS com histerese) no começo; `webrtc-audio-processing` (binding C++ do APM do WebRTC: AEC, NS, AGC) para eco/ruído; `nnnoiseless` (RNNoise em Rust puro) para supressão de ruído estilo Krisp. Prós: licenças livres, leves. [INF]
- **Push-to-talk:** atalho global no Linux é difícil (X11 `XGrabKey`/XInput2 via x11rb, que já existe no projeto; Wayland não permite). Começar com tecla dentro da janela.

### 3.2 Transporte: três opções
| Opção | Prós | Contras | Esforço |
|---|---|---|---|
| A. **Áudio Opus em WebSocket (TCP)** no servidor atual | Zero NAT/firewall, reaproveita auth e relay existentes (como a tela) | Head-of-line blocking, latência pior sob perda; ok em LAN/internet estável | **S/M** |
| B. **UDP próprio** (RTP-like: seq, timestamp, SSRC + AEAD `chacha20poly1305` com chave por sessão) como o Discord | Latência baixa, controle total, simples de entender; SFU trivial (reenviar pacotes) | NAT/UDP bloqueado exige fallback (A); precisa IP discovery, keepalive, resume | **M/L** |
| C. **WebRTC (`webrtc-rs`)** | ICE/STUN/TURN, DTLS-SRTP, RTCP/NACK/bitrate adaptativo prontos; serve também ao vídeo | Dependência grande, API verbosa, crate menos ativo; SFU precisa ser escrito sobre ele; mais RAM e compilação pesada | **L** |

**Recomendação [INF]:** começar por **A** (P0) com o mesmo formato de quadro do B (cabeçalho seq/ts/ssrc, payload Opus), de forma que depois se troque o transporte para **B (UDP)** mantendo A como fallback. `webrtc-rs` só se surgir necessidade real de NAT traversal entre clientes sem servidor público (o servidor já é público, então UDP direto cliente-servidor basta).

### 3.3 Servidor
- Estado de sala: `voice_state` (usuário, canal, mute, deaf) transmitido pelos eventos do WS atual; novo tipo de canal `voice` (e depois `stage`) em `telacast-proto`.
- SFU: para cada pacote recebido de um usuário, reenviar para os demais da sala (sem decodificar). Cada receptor recebe N fluxos e mixa localmente. Custo O(N²) em banda de saída; ok até ~25 pessoas com Opus 32-64 kbps. [INF]
- Eventos "falando": derivar no servidor pela chegada de pacotes não silenciosos ou flag no cabeçalho.

### 3.4 Vídeo/tela com áudio
- Já existe H.264 NVENC + relay WS. Para **áudio do sistema**: capturar o monitor do PipeWire/PulseAudio (`cpal` com dispositivo "monitor", ou `libpulse-binding`/`pipewire-rs`), codificar em Opus, enviar como segundo fluxo no mesmo relay com flag "soundshare". Sincronia por timestamp comum (relógio monotônico do cliente). Esforço **M**.
- **Câmera:** `nokhwa` ou `v4l` (V4L2) + o mesmo encoder; sem bitrate adaptativo no início. Esforço **M**.
- **Qualidade/fps:** expor presets (720p30, 1080p60, fonte) mapeando para parâmetros do NVENC já existente. Esforço **S**. Limites por plano/boost são política do servidor Telacast (config), não técnica.
- **Fallback sem NVENC:** `openh264` já está no app (software). [fato do Cargo.toml]

### 3.5 E2EE
DAVE/MLS é complexo (crate `openmls`). Não recomendado agora: o servidor é do próprio dono. Alternativa [INF]: AEAD por quadro com chave de sala derivada no servidor (TLS já protege o transporte). P3.

## 4) O que o Telacast já tem vs falta

**Já tem:** captura X11, H.264 NVENC (GPU) com fallback openh264, relay WS no servidor, até 144 fps em 1080p (teste), transmissão em canal de texto, auth e eventos WS.

**Falta:**
- Canais de voz e de palco (modelo, UI, estado de presença em voz).
- Todo o áudio: captura, saída, Opus, jitter buffer, mixagem.
- Mudo/ensurdecer, VAD, push-to-talk, indicador de fala, volume por usuário.
- Supressão de ruído, eco, AGC.
- Áudio do sistema na transmissão e espectadores (seleção de janela vs tela, lista de quem assiste).
- Câmera.
- UDP/SFU, reconexão com resume, medição de ping.
- Seletor de qualidade, limites por plano/boost, bitrate por canal.
- E2EE.

## 5) Prioridade

| Prio | Item | Esforço | Motivo |
|---|---|---|---|
| P0 | Canal de voz: modelo, entrar/sair, lista de participantes, voice_state no servidor | M | Base de tudo; sem isso nada de áudio existe |
| P0 | Áudio Opus via WS (A): cpal + opus + jitter buffer + mixagem | M | Entrega a funcionalidade central com a infraestrutura atual |
| P0 | Mudo, ensurdecer, indicador de fala (VAD por energia) | S | Controles mínimos de usabilidade |
| P1 | Volume por usuário, seleção de dispositivos, push-to-talk (na janela) | S | Esperado por qualquer usuário de voz |
| P1 | Áudio do sistema na transmissão de tela existente | M | Completa o recurso que já existe (hoje sem áudio) |
| P1 | Presets de qualidade/fps/resolução na transmissão | S | O encoder já suporta; só UI e parâmetros |
| P1 | Medição de ping e reconexão automática | S | Estabilidade percebida |
| P2 | Transporte UDP com fallback WS (B) | L | Reduz latência e perda; não bloqueia o MVP |
| P2 | Supressão de ruído (`nnnoiseless`) e eco/AGC (`webrtc-audio-processing`) | M | Qualidade; eco só importa sem fone |
| P2 | Câmera (V4L2 + H.264) | M | Útil, mas pesado em banda e UI de mosaico |
| P2 | Canal de palco (oradores/plateia, mão levantada) | M | Depende da voz estável e de permissões |
| P3 | Limites por plano/boost, bitrate por canal | S | Só faz sentido com monetização/planos |
| P3 | Atalho global de PTT (X11/portal) | M | Limitado no Wayland |
| P3 | webrtc-rs / E2EE (MLS) / soundboard / fundo virtual | L | Custo alto, ganho baixo no cenário atual |

## 6) Fontes consultadas

- Documentação oficial Voice Connections (redirecionada de discord.com/developers/docs/topics/voice-connections): https://docs.discord.com/developers/topics/voice-connections (versões do gateway, fluxo, modos de criptografia, IP Discovery, RTP, Speaking, Resume, DAVE).
- Suporte Discord, Go Live e Screen Share (via resultado de busca, página direta deu 403): https://support.discord.com/hc/hi-in/articles/360040816151-Go-Live-and-Screen-Share (50 pessoas, Nitro até 4K/60).
- Suporte Discord, Audio Bitrate FAQ (apenas localizada, sem números): https://support.discord.com/hc/en-us/articles/11635925354775-Audio-Bitrate-FAQ
- Terceiros, não confirmados: howtogeek.com (bitrate/boost), voxbooster.com, wisechecker.com, eklipse.gg (qualidades de stream), neowin.net (Krisp).
- Cliente instalado, leitura leve (só nomes de funções expostas): `/snap/discord/current/usr/share/discord/resources/standalone_modules/discord_voice/index.js`, listagem de arquivos do diretório. O `.node` não foi aberto.
- Projeto: `/home/guilherme/telacast/app/Cargo.toml` e `server/Cargo.toml` (dependências atuais).

**Lacunas:** limites exatos de Stage Channel, regiões atuais, codecs de vídeo negociados, algoritmo de VAD/Krisp e tabela oficial Nitro/boost não foram confirmados em fonte oficial.
