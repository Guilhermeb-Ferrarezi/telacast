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
}
```

Cadeia: captura (cpal, qualquer taxa/canais → 48 kHz mono) → ganho → VAD por energia com histerese e
*hangover* de ~200 ms → Opus (voip, ~32 kbps, FEC e DTX desligados no começo) → `send`. Recepção: um *jitter buffer*
por usuário (ordena por `seq`, tolera perda e atraso, 40–100 ms) → decodifica (PLC na perda) → mistura com o volume
de cada um → saída (cpal, 48 kHz mono duplicado nos canais do dispositivo). Sem alocação na thread de áudio.

## Divisão do trabalho

| Parte | Onde | Quem |
|---|---|---|
| Protocolo (feito) | `proto/` | eu |
| Servidor: estado de voz, relé de quadros, limites, testes e2e | `server/` | agente A |
| Motor de áudio + testes sem hardware | `app/src/voice.rs` (desenvolvido num crate de laboratório) | agente B |
| Interface: lista de canais, painel "Voz conectada", cartão com mudo/surdo, visão do canal, ligação ao gateway | `app/src/main.rs`, `theme.rs`, `gateway.rs` | eu |

## Estado (2026-11-07)

Feito: protocolo, servidor, motor (`app/src/voice.rs`, dependências `cpal 0.16`, `opus 0.4` com libopus embutida, `rtrb`), interface e ligação ao gateway (o motor é criado quando o servidor confirma a entrada e destruído ao sair/cair a conexão; quadros binários vão direto da thread da rede ao motor). Testado: entrada real no PipeWire, medidor de microfone, anel de fala de um remoto simulado por pacotes. Não testado: duas pessoas reais conversando, troca de dispositivo com o áudio aberto por muito tempo.
