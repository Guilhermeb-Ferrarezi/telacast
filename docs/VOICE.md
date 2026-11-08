# Voz no Telacast (Fase 2)

Chat de voz no estilo do Discord: canais de voz (`kind = 1`), entrar/sair, lista de participantes, mudo e surdo,
indicador de "falando". O áudio é Opus (mono, 48 kHz, 20 ms) e vai **pelo WebSocket do gateway** em mensagens
binárias, com um formato que um UDP futuro poderia reusar. Spec visual: `docs/ui-spec/08-voz-transmissao.md`.

## Protocolo (já em `proto/src/lib.rs`)

- `Channel.kind == 1`: canal de voz. `C2S::CreateChannel { kind: 1 }` cria (só o dono, como os de texto).
- `C2S::VoiceJoin { channel_id }`, `C2S::VoiceLeave`, `C2S::VoiceSet { muted, deafened }`.
- `S2C::VoiceUpdate(VoiceState)` (entrou ou mudou), `S2C::VoiceLeft { user_id, channel_id }`.
- `Snapshot.voice: Vec<VoiceState>`: estado completo dos seus servidores (volta a cada snapshot).
- Quadros de áudio (mensagem **binária**, ver `proto::VOICE_FRAME`):
  - cliente → servidor: `[1][seq u32 BE][opus]`
  - servidor → clientes do canal (menos o remetente): `[1][remetente i64 BE][seq u32 BE][opus]`
  - pacote Opus ≤ `VOICE_MAX_PACKET` (400 bytes). Quem está em silêncio (VAD) **não manda nada**; quem ouve
    considera "falando" quem mandou pacote nos últimos ~250 ms.

## Regras do servidor

- Estado de voz **em memória** (`user -> canal`), nada no Postgres. Um canal por vez: entrar noutro move.
- Só quem é membro do servidor do canal (`kind == 1`) entra. Canal inexistente ou de texto: erro `NotFound`/`InvalidInput`.
- Cair a conexão (todas as conexões do usuário), `Logout` ou sessão revogada tira da voz e avisa com `VoiceLeft`.
- `VoiceUpdate`/`VoiceLeft` vão a todos os membros do servidor do canal (quem está online).
- Quadro binário de quem **não** está em voz, de canal sem outros, ou maior que o limite: descartado em silêncio.
  Limite por usuário: ~100 pacotes/s com folga de rajada (20 ms = 50/s); acima disso descarta.
- Quem está **surdo** não recebe quadros; quem está **mudo** não tem os quadros repassados.
- Quadros binários nunca podem derrubar a conexão nem consumir memória sem limite.

## Motor de áudio do cliente (`app/src/voice.rs`, sem egui)

Roda em threads próprias; **não** depende de a janela repintar (o egui só desenha quando há evento).

```rust
pub struct Config { pub input: Option<String>, pub output: Option<String> }   // None = padrão
pub fn list_devices() -> Devices;                 // { inputs: Vec<String>, outputs: Vec<String> }
pub struct Engine { .. }
impl Engine {
    /// `send`: chamada pela thread do codificador com (seq, pacote Opus). Não bloqueia.
    pub fn start(cfg: Config, send: Box<dyn FnMut(u32, &[u8]) + Send>) -> Result<Engine, String>;
    pub fn set_muted(&self, muted: bool);          // para de capturar/enviar
    pub fn set_deafened(&self, deafened: bool);    // para de tocar (e a UI também liga o mudo)
    pub fn set_user_volume(&self, user: i64, volume: f32);   // 0.0..=2.0, padrão 1.0
    pub fn push_packet(&self, user: i64, seq: u32, packet: &[u8]);   // chamado da thread da rede
    pub fn remove_user(&self, user: i64);
    pub fn speaking(&self, user: i64) -> bool;     // recebeu pacote nos últimos ~250 ms
    pub fn mic_level(&self) -> f32;                // 0.0..=1.0, para o medidor
    pub fn self_speaking(&self) -> bool;           // VAD aberto agora
    pub fn error(&self) -> Option<String>;         // falha do dispositivo depois de iniciado
    pub fn set_agc(&self, on: bool);               // pré-AGC + AGC2 (padrão ligado)
    pub fn set_echo_cancel(&self, on: bool);       // AEC3 (padrão desligado)
    pub fn voice_stats(&self) -> VoiceStats;       // painel de estatísticas (envio, recepção por usuário e total)
}
```

Cadeia de envio (20 ms por quadro, `SendPipeline`; o DSP puro está em `app/src/voice_dsp.rs`, portado de
`android/.../voz/Dsp.kt` e `Agc2.kt` com os mesmos parâmetros):

```text
captura (cpal, qualquer taxa/canais → 48 kHz mono)
  → passa-alta de 80 Hz (biquad Butterworth, sempre ligada)
  → ganho manual
  → pré-AGC            (só com RNNoise ligado; traz a voz baixa/longe para perto de -30 dBFS, até +30 dB)
  → RNNoise            (probabilidade de voz p)
  → VAD                (decide sobre o nível pós-RNNoise, antes do AGC2)
  → AGC2 do Discord    (usa p do RNNoise; limiar de fala 0,9; ganho inicial 0 dB)
  → Opus               (voip, 48 kbps VBR restrito, banda total, sinal "voz"; FEC e DTX desligados)
  → send(seq, pacote)
```

A opção "AGC" (`Engine::set_agc`, padrão ligada) liga e desliga o pré-AGC e o AGC2 juntos; desligada, os dois voltam a 0 dB
com rampa dentro do quadro (sem degrau). O pré-AGC só roda junto com o RNNoise: ele existe para o RNNoise não cortar fala
fraca, e sem RNNoise um ganho antes do VAD de energia só amplificaria o ruído.

- **Passa-alta** em 80 Hz (2ª ordem): -12,3 dB em 40 Hz, -3 dB em 80 Hz, 1 kHz intacto. É o `high_pass_filter` que o
  Discord liga no WebRTC.
- **AGC2** é o `GainController2` adaptativo digital do WebRTC m146 como o Discord o configura (ficha em
  `docs/discord/11-agc2-nativo.md`, seção 5): estimador de nível de fala (só confirma rajadas de ≥ 12 quadros de 10 ms),
  margem pico/média de 12 a 25 dB, piso de ruído por mínimo de energia em 5 s, leva o pico da fala a -5 dBFS, sobe no
  máximo 6 dB/s e só depois de 120 ms de fala, até +50 dB, nunca acima do que deixaria o ruído de saída passando de
  -50 dBFS, e limitador suave (transparente até -0,75 dBFS). Diferenças: ganho inicial 0 dB (o do Discord é +15 dB e
  estoura nos primeiros segundos) e limiar de fala 0,9 (o 0,95 deles é calibrado para o RNN VAD do WebRTC).
- **Opus**: 48 kbps (média de ~115 bytes por quadro de 20 ms na medição com fala e ruído forte, máximo de 135), banda total
  (`OPUS_BANDWIDTH_FULLBAND`) e `OPUS_SIGNAL_VOICE`. O teto de `VOICE_MAX_PACKET` (400) é garantido por construção: o buffer
  de saída tem 400 bytes e a libopus nunca passa dele; além disso há um `const assert` de que `bitrate/8/50 × 3 ≤ 400`
  (o VBR restrito fica perto de 1,5× a média). Compatível com o Android: ele decodifica qualquer Opus mono de 48 kHz e 20 ms.

Recepção: um *jitter buffer* por usuário (ordena por `seq`, tolera perda e atraso) → decodifica (PLC na perda) → esticador de
tempo → mistura com o volume de cada um → saída (cpal, 48 kHz mono duplicado nos canais do dispositivo). Sem alocação na
thread de áudio dos callbacks do cpal.

### Jitter buffer adaptativo e esticar/comprimir o tempo

O alvo do jitter buffer vai de 2 a 12 quadros (**40 a 240 ms**, antes era 40 a 100 ms fixo) e é o mesmo do Android:
sobe de 2 em 2 quadros quando o buffer esvazia e o pacote chega logo depois (subfluxo) ou quando chega pacote atrasado,
desce 1 depois de 3 s sem problemas, e **uma rajada** (o WebSocket/TCP segura os pacotes e solta tudo de uma vez) sobe o alvo
até o excesso caber em vez de cortar áudio; só o que passar de 12 + 4 quadros é descartado.

Em cima disso o receptor **estica ou comprime o tempo** do PCM já decodificado para manter o atraso perto do alvo, em vez de
repetir quadro (PLC) ou pular (`Esticador`, SOLA com busca de correlação, um corte por quadro de 20 ms):

- comprimir (buffer ≥ 2 quadros acima do alvo): remove **um período de pitch** (100 a 576 amostras = 480 a 83 Hz, achado pela
  correlação normalizada) com *crossfade* de 6 ms; esticar (≥ 2 quadros abaixo do alvo): repete um período. Velocidade média
  limitada a ±20% por um "crédito" (o corte só acontece quando o crédito cobre o período achado) e proporcional ao desvio
  (5% por quadro de desvio). Histerese: liga com 2 quadros de desvio, desliga abaixo de 1.
- sem desvio o PCM passa **idêntico** (nenhuma amostra tocada). Em silêncio (janelas abaixo de -55 dBFS) o comprimidor pula um
  quadro inteiro, que ninguém ouve, e o esticador não faz nada (não há o que esticar).
- fim da fala: o resto do PCM decodificado sai com fade-out de 5 ms.

Testes (`cargo test -p telacast voice`): rajada de 10 pacotes e falta de 3 pacotes tocam sem descontinuidade (degrau máximo
entre amostras igual ao do sinal original, 0,013), a duração tocada + removida - repetida bate com a enviada (a sobra é a cauda de
3 quadros de PLC + fade-out), esticar evita os esvaziamentos de uma rede que entrega a 90% da cadência (3 contra 8 quadros de PLC).

### Estatísticas da voz

Configurações de voz → coluna "Estatísticas da voz" à direita (a janela abre em duas colunas quando há motor de áudio; atualiza
~2 vezes por segundo; `Engine::voice_stats()`):

- **Envio**: nível (dBFS depois do RNNoise, antes do AGC2), probabilidade de voz do RNNoise, ganho do pré-AGC e do AGC2 (dB) e,
  com o cancelamento de eco ligado, ERLE e atraso do eco.
- **Recepção, por usuário e total**: perda (% em média móvel de ~5 s de fala: buracos na sequência), jitter de chegada
  (RFC 3550, ignora pausas de fala), buffer atual/alvo em ms (com `»` comprimindo e `«` esticando o tempo), cortes por minuto
  (esvaziamentos no meio da fala nos últimos 60 s) e atraso estimado até o alto-falante (buffer + fila de saída + 2 blocos do
  dispositivo; **sem a rede**). Um acento âmbar nos valores fora do normal (perda ≥ 2%, jitter ≥ 30 ms, ≥ 3 cortes/min, atraso ≥ 250 ms).
- O total é a média de perda e jitter, a soma dos cortes por minuto e o maior buffer e atraso.

### Eco (caixa de som no microfone)

Investigado em 2026-11-07. Duas saídas, as duas funcionam sem mexer no servidor.

**1. Cancelador de eco embutido (feito, desligado por padrão, "experimental").** Configurações de voz → "Cancelamento de eco".
É o AEC3 do WebRTC portado para Rust puro (`sonora` 0.3, só o cancelador; sem C/C++, compila igual no Windows). Fica logo depois da
passa-alta: `captura → passa-alta → AEC3 → ganho → pré-AGC → RNNoise → VAD → AGC2 → Opus`. A **referência é o que o app toca**: a
mixagem das vozes (e do "ouvir meu microfone") vai para uma fila de 1 s (`REF_RING`), que o mixer completa com zeros no ritmo do relógio
quando ninguém fala (linha do tempo contínua) e a thread de captura lê 1:1 com os quadros do microfone. O AEC3 acha sozinho o atraso
do caminho (alto-falante → sala → microfone, 50 a 500 ms; mostrado no painel de estatísticas), por isso não precisa medir a latência do
dispositivo. Se a referência faltar (mixer atrasado), completa com zeros e descarta o mesmo tanto depois (`le_referencia`), então a
linha do tempo nunca desloca. Ligar cria o estado (memória), desligar solta.

| Medida | Valor |
|---|---|
| Qualidade (eco sintético, atraso de 60, 120 e 200 ms, cauda de 60 ms, saturação leve, fala dos dois lados) | ERLE 39 dB com só eco; fala do perto preservada em -0,7 dB no *double-talk* (laboratório `aec-lab`, release) |
| Qualidade no pipeline do app (teste `eco_pipeline_cancela_o_eco_e_preserva_a_fala_do_perto`) | ERLE 40,7 dB; fala do perto -0,4 dB do original; atraso estimado 76 ms (real 80) |
| CPU | 78 µs por bloco de 10 ms no laboratório (**~0,8% de um núcleo**); +96 µs por quadro de 20 ms no pipeline do app (~0,5%) |
| Memória | laboratório: +1,1 MB ao criar, +4,6 MB depois de rodar (RSS); no app (Xvfb, 60 s, chamada com um remoto): PSS 103,8 MB com o eco ligado contra 102,3 a 103,0 MB sem (A/B), ou seja **~1 MB** |
| Binário | Linux +706 KB (44,26 → 44,97 MB; strip +0,75 MB); Windows (zigbuild) +0,83 MB no total das mudanças desta rodada (35,73 → 36,57 MB) |
| Build | laboratório do zero: 12,6 s (deps + `sonora`); no app, incremental, 30 s; 105 linhas novas no `Cargo.lock` (`sonora*`, `derive_more`, `unicode-xid`) |

O que **só dá para confirmar no PC real** (com caixa de som e microfone de verdade): que o atraso real fica dentro do que o AEC3
alcança (a referência é produzida *antes* de o som sair do dispositivo, porque a fila de saída guarda 40 ms ou mais; no modo
"Baixa latência" a folga é bem menor), o efeito de
saturação do alto-falante e de ganho do microfone (o AEC3 vem antes do ganho manual e do AGC, de propósito) e o corte de início de
fala em *double-talk* com voz real. Por isso fica desligado até alguém testar.

Alternativas olhadas e **não** usadas: `webrtc-audio-processing` (tonarino, o mesmo AEC3 em C++: exige meson/ninja e toolchain C++, e
cross-compilar para o Windows com zigbuild é arriscado; não compilei), `aec-rs`/speexdsp (C via `aec-rs-sys`; o filtro MDF do Speex
não tem estimador de atraso, então teria de medir a latência do dispositivo; não compilei) e `aec3` 0.4 (outra porta em Rust puro, só
o bloco, sem a camada de bandas; não testei).

**2. Módulo `echo-cancel` do PipeWire (sistema inteiro, sem custo no app).** O PipeWire 1.6.8 desta máquina traz
`libpipewire-module-echo-cancel` e o plugin `aec/libspa-aec-webrtc`. Com `monitor.mode = true` o módulo usa o *monitor* do
alto-falante padrão como referência, então não precisa redirecionar a saída do app: basta usar o microfone novo. Arquivo
`~/.config/pipewire/pipewire.conf.d/60-eco.conf`, depois `systemctl --user restart pipewire pipewire-pulse wireplumber`:

```
context.modules = [
  { name = libpipewire-module-echo-cancel
    args = {
      library.name = aec/libspa-aec-webrtc
      monitor.mode = true
      source.props = { node.name = "tc-eco-mic" node.description = "Microfone sem eco" }
      aec.args = { webrtc.gain_control = false webrtc.noise_suppression = false webrtc.high_pass_filter = false }
    }
  }
]
```

Depois escolha "Microfone sem eco" como entrada padrão do sistema (`wpctl status` mostra o id; `wpctl set-default <id>`) e deixe o
Telacast em "Padrão do sistema". Com `pipewire-pulse` o equivalente é `pactl load-module module-echo-cancel aec_method=webrtc
source_name=tc-eco-mic` (não tem `pactl` instalado aqui). **Validado**: carreguei exatamente esse módulo, com esses argumentos, num
PipeWire de teste isolado (socket privado, sem tocar no áudio do usuário) e ele subiu ("Using plugin AEC webrtc", nós
`capture.tc-eco`/`tc-eco-mic` criados); **não** validei o efeito no áudio real. Nota: `pw-cli load-module` vive só enquanto o `pw-cli`
está aberto, por isso o arquivo de configuração. O app **não detecta** o módulo (o cpal/ALSA só enxerga "default", "pipewire" e
"pulse", não os nós do PipeWire); o que o app oferece é o cancelador embutido acima, e as duas coisas juntas não ajudam (eco
cancelado duas vezes pode comer a voz): use uma delas.

**Recomendação:** com fone, nada. Com caixa de som, ligar o cancelamento embutido (1 clique, ~1 MB, ~0,5% de um núcleo) e, se não
bastar ou se também outros programas (Discord, navegador) precisarem, o módulo do PipeWire. Depois de uma chamada de teste com caixa
de som real, trocar o padrão para ligado se o resultado for bom.


## Divisão do trabalho

| Parte | Onde | Quem |
|---|---|---|
| Protocolo (feito) | `proto/` | eu |
| Servidor: estado de voz, relé de quadros, limites, testes e2e | `server/` | agente A |
| Motor de áudio + testes sem hardware | `app/src/voice.rs` (desenvolvido num crate de laboratório) | agente B |
| Interface: lista de canais, painel "Voz conectada", cartão com mudo/surdo, visão do canal, ligação ao gateway | `app/src/main.rs`, `theme.rs`, `gateway.rs` | eu |

## Estado (2026-11-07)

Feito: protocolo, servidor, motor (`app/src/voice.rs`, dependências `cpal 0.16`, `opus 0.4` com libopus embutida, `rtrb`), interface e ligação ao gateway (o motor é criado quando o servidor confirma a entrada e destruído ao sair/cair a conexão; quadros binários vão direto da thread da rede ao motor). Testado: entrada real no PipeWire, medidor de microfone, anel de fala de um remoto simulado por pacotes. Não testado: duas pessoas reais conversando, troca de dispositivo com o áudio aberto por muito tempo.

Qualidade da voz do desktop (2026-11-07, depois do estado acima): passa-alta, pré-AGC, AGC2 do Discord, Opus 48 kbps em banda total,
jitter buffer de 40 a 240 ms com esticar/comprimir o tempo, painel de estatísticas e cancelamento de eco opcional. Testes:
`cargo test -p telacast voice` (93 testes, 4 ignorados que precisam de dispositivo ou medem CPU; `custo_cpu_dsp_novo` imprime o
custo de cada peça com `--release -- --ignored --nocapture`: passa-alta 2,5 µs, pré-AGC 1,4 µs, AGC2 1,4 µs, AEC3 69 µs, esticador
93 µs por quadro de 20 ms só enquanto comprime; envio completo 246 µs sem eco e 343 µs com eco, 1,2% e 1,7% de um núcleo).
Interface conferida no Xvfb `:99` (duas colunas, estatísticas de um remoto simulado com jitter, 3% de perda e rajadas).
**Só dá para confirmar no PC real:** o ganho do AGC2 com o microfone de verdade e com fala de gente (os testes usam voz
sintética), a naturalidade do esticar/comprimir com fala real, e o efeito do cancelamento de eco com caixa de som.
