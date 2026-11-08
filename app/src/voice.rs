//! Motor de áudio do chat de voz (sem egui, sem código do app).
//!
//! Tudo roda em threads próprias; a UI só chama os métodos de [`Engine`], que são baratos e não bloqueiam.
//!
//! ```text
//! microfone (cpal) ─► fila SPSC ─► [thread do codificador]
//!     downmix mono       lock-free    reamostra p/ 48 kHz → ganho → medidor → VAD → Opus → send(seq, pacote)
//!
//! push_packet() ─► canal ─► [thread do decodificador/mixer]
//!     jitter buffer por usuário → Opus (PLC na perda) → mistura com volume → limitador → reamostra p/ a saída
//!     ─► fila SPSC lock-free ─► callback de saída (cpal): só copia amostras, duplicando nos canais
//! ```
//!
//! Os callbacks do cpal não alocam e não pegam mutex: só filas `rtrb`, atômicos e conversão de formato.
//! As peças de lógica (reamostrador, VAD, jitter buffer, mixer, pipeline de envio) são estruturas puras,
//! testáveis sem hardware; a parte do cpal é fina.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering::Relaxed};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SampleRate, SizedSample, Stream, StreamConfig};
use nnnoiseless::DenoiseState;
use opus::{Application, Bandwidth, Bitrate, Channels, Decoder, Encoder, Signal};
use rtrb::{Consumer, Producer, RingBuffer};

use crate::voice_dsp::{Agc2, Eco, EcoStats, Escala, Esticador, JitterChegada, PassaAlta, PreAgc};

// ───────────────────────────── constantes ─────────────────────────────

/// Taxa interna de tudo (Opus fullband).
pub const SAMPLE_RATE: u32 = 48_000;
/// Amostras por quadro de 20 ms a 48 kHz.
pub const FRAME: usize = 960;
/// Tamanho máximo de um pacote Opus (igual a `proto::VOICE_MAX_PACKET`).
pub const MAX_PACKET: usize = 400;
/// Bitrate do codificador (VBR com restrição, banda total, sinal "voz"). 48 kbps = 120 bytes médios por quadro de 20 ms;
/// o Opus nunca passa de `MAX_PACKET` (o buffer de saída tem esse tamanho e a libopus trunca o orçamento a ele), e o
/// teto do VBR restrito fica perto de 1,5× a média: a conta abaixo garante folga de mais de 2× antes do limite.
const BITRATE: i32 = 48_000;
const _: () = assert!(BITRATE as usize / 8 / 50 * 3 <= MAX_PACKET, "bitrate alto demais para VOICE_MAX_PACKET");
/// Janela de "falando": recebeu pacote há menos que isto.
const SPEAKING_WINDOW: Duration = Duration::from_millis(250);
/// Usuário sem pacotes por tanto tempo é descartado (decodificador e buffer liberados).
const USER_IDLE: Duration = Duration::from_secs(5);
/// Máximo de usuários simultâneos que o motor decodifica (limite de memória).
const MAX_USERS: usize = 64;
/// Pacotes aguardando a thread do mixer (limite contra rajada).
const MAX_PENDING: usize = 512;
/// Quanto áudio já decodificado ficar à frente do dispositivo (ms).
const PLAYBACK_AHEAD_MS: usize = 40;
/// Sobra mínima (ms) à frente do tamanho do callback do dispositivo na fila de reprodução.
const PLAYBACK_MARGEM_MS: usize = 20;
/// Idem no modo de baixa latência.
const PLAYBACK_MARGEM_BAIXA_MS: usize = 10;
/// Teto da margem extra que cresce a cada subfluxo (ms).
const PLAYBACK_EXTRA_MAX_MS: usize = 80;
/// Fila do monitor (quadros de 20 ms): começa a tocar com `MON_PRIME`; acima de `MON_ALTO` aperta o tempo
/// em ~0,3% para voltar ao alvo; abaixo de `MON_BAIXO` estica; acima de `MON_MAX` descarta o mais antigo.
const MON_PRIME: usize = 4;
const MON_BAIXO: usize = 2;
const MON_ALTO: usize = 7;
const MON_MAX: usize = 10;
const MON_RING_QUADROS: usize = 16;
/// Amostras a mais/menos lidas por quadro do monitor ao compensar deriva (≈ 0,3%).
const MON_AJUSTE: usize = 3;
/// Fila da referência do cancelador de eco (o que o app toca, 48 kHz mono): 1 s.
const REF_RING: usize = 48_000;
/// Referência guardada a mais que isto (amostras) = o capturador ficou para trás: descarta o excesso mais antigo.
const REF_MAX: usize = FRAME * 4;
/// Atraso fixo (amostras, 10 ms) da referência: dá folga para o mixer (que acorda a cada 2–5 ms) nunca deixar a fila curta.
const REF_FOLGA: usize = 480;
/// Fade de entrada/saída nas bordas de falas (5 ms).
const FADE_AMOSTRAS: usize = 240;
/// Capacidade das filas entre os callbacks do cpal e as threads (amostras mono; ≥ 0,5 s a 192 kHz).
const RING_SAMPLES: usize = 96_000;
/// Pré-rolagem: quadros guardados antes de o VAD abrir, para não cortar o começo da fala.
const PREROLL_FRAMES: usize = 5; // 100 ms
/// Quadro do RNNoise (10 ms a 48 kHz); o quadro Opus de 20 ms leva dois.
const DEN_FRAME: usize = DenoiseState::FRAME_SIZE;

// ───────────────────────────── utilidades ─────────────────────────────

/// Nível RMS do quadro em dBFS (silêncio digital ≈ -120).
pub fn rms_db(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return -120.0;
    }
    let soma: f32 = frame.iter().map(|x| x * x).sum();
    10.0 * (soma / frame.len() as f32 + 1e-12).log10()
}

/// Limitador suave: abaixo do joelho (0,7) não mexe; acima, curva em tanh que nunca passa de 1,0.
pub fn soft_limit(x: f32) -> f32 {
    const JOELHO: f32 = 0.7;
    if !x.is_finite() {
        return 0.0;
    }
    let a = x.abs();
    if a <= JOELHO {
        x
    } else {
        let y = JOELHO + (1.0 - JOELHO) * ((a - JOELHO) / (1.0 - JOELHO)).tanh();
        y.min(1.0).copysign(x)
    }
}

/// Como [`soft_limit`] com joelho configurável (0 < joelho < 1).
pub fn soft_limit_knee(x: f32, joelho: f32) -> f32 {
    if !x.is_finite() {
        return 0.0;
    }
    let a = x.abs();
    if a <= joelho {
        x
    } else {
        let y = joelho + (1.0 - joelho) * ((a - joelho) / (1.0 - joelho)).tanh();
        y.min(1.0).copysign(x)
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

fn f32_from_bits(a: &AtomicU32) -> f32 {
    f32::from_bits(a.load(Relaxed))
}

// ───────────────────────────── reamostrador ─────────────────────────────

const RS_METADE: usize = 16; // meia largura do filtro (amostras de entrada)
const RS_TAPS: usize = RS_METADE * 2;
const RS_FASES: usize = 256; // fases da tabela (com interpolação linear entre elas)

/// Reamostrador em fluxo (sinc com janela de Blackman, tabela de fases). Aceita pedaços de qualquer tamanho.
/// Com taxas iguais só copia. Abaixando a taxa, corta antes do novo Nyquist.
pub struct Resampler {
    passa: bool,
    passo: f64, // amostras de entrada por amostra de saída
    pos: f64,   // posição (fracionária) da próxima saída, em índices de `buf`
    buf: Vec<f32>,
    tabela: Vec<f32>, // (RS_FASES + 1) linhas de RS_TAPS
}

impl Resampler {
    pub fn new(taxa_entrada: u32, taxa_saida: u32) -> Resampler {
        let passa = taxa_entrada == taxa_saida || taxa_entrada == 0 || taxa_saida == 0;
        let mut r = Resampler {
            passa,
            passo: if passa { 1.0 } else { taxa_entrada as f64 / taxa_saida as f64 },
            pos: RS_METADE as f64,
            buf: Vec::new(),
            tabela: Vec::new(),
        };
        if !passa {
            r.buf.resize(RS_METADE, 0.0);
            let fc = (taxa_saida as f64 / taxa_entrada as f64).min(1.0) * 0.95;
            r.tabela = Self::monta_tabela(fc);
        }
        r
    }

    fn monta_tabela(fc: f64) -> Vec<f32> {
        use std::f64::consts::PI;
        let mut t = Vec::with_capacity((RS_FASES + 1) * RS_TAPS);
        for p in 0..=RS_FASES {
            let frac = p as f64 / RS_FASES as f64;
            let mut linha = [0f64; RS_TAPS];
            let mut soma = 0.0;
            for (m, v) in linha.iter_mut().enumerate() {
                let tt = (m as f64 + 1.0 - RS_METADE as f64) - frac;
                let x = fc * tt;
                let sinc = if x.abs() < 1e-9 { 1.0 } else { (PI * x).sin() / (PI * x) };
                let u = tt / RS_METADE as f64;
                let janela = if u.abs() >= 1.0 {
                    0.0
                } else {
                    0.42 + 0.5 * (PI * u).cos() + 0.08 * (2.0 * PI * u).cos()
                };
                *v = fc * sinc * janela;
                soma += *v;
            }
            for v in linha {
                t.push((v / soma) as f32); // ganho DC exatamente 1 em toda fase
            }
        }
        t
    }

    /// Consome `entrada` e acrescenta as amostras reamostradas em `saida`.
    pub fn process(&mut self, entrada: &[f32], saida: &mut Vec<f32>) {
        if self.passa {
            saida.extend_from_slice(entrada);
            return;
        }
        self.buf.extend_from_slice(entrada);
        loop {
            let i = self.pos.floor() as usize;
            if i + RS_METADE >= self.buf.len() {
                break;
            }
            let fase = (self.pos - i as f64) * RS_FASES as f64;
            let p = (fase as usize).min(RS_FASES - 1);
            let fr = (fase - p as f64) as f32;
            let a = &self.tabela[p * RS_TAPS..(p + 1) * RS_TAPS];
            let b = &self.tabela[(p + 1) * RS_TAPS..(p + 2) * RS_TAPS];
            let x = &self.buf[i + 1 - RS_METADE..i + 1 + RS_METADE];
            let (mut s0, mut s1) = (0f32, 0f32);
            for m in 0..RS_TAPS {
                s0 += x[m] * a[m];
                s1 += x[m] * b[m];
            }
            saida.push(s0 + (s1 - s0) * fr);
            self.pos += self.passo;
        }
        let descarta = (self.pos.floor() as usize + 1).saturating_sub(RS_METADE).min(self.buf.len());
        self.buf.drain(..descarta);
        self.pos -= descarta as f64;
    }
}

// ───────────────────────────── VAD ─────────────────────────────

/// Limiares do VAD num dado momento (ver [`Vad::thresholds`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Limiares {
    /// Nível mínimo (dBFS) para abrir / para manter aberto.
    pub abre_db: f32,
    pub fecha_db: f32,
    /// Probabilidade de voz do RNNoise para abrir / manter (0 quando o RNNoise está desligado).
    pub prob_abre: f32,
    pub prob_fecha: f32,
}

/// Detector de voz com histerese, *hangover* e piso de ruído adaptativo.
///
/// - **Com RNNoise** (`prob` presente): a decisão principal é a probabilidade de voz (abre com
///   `prob >= prob_abre`, mantém com `prob >= prob_fecha`), com um piso de energia bem baixo
///   (mínimo absoluto ≈ -61 dBFS na sensibilidade padrão, ou piso de ruído + ~8 dB).
/// - **Sem RNNoise**: só energia, com piso de ruído + ~8 dB (mínimo ≈ -53 dBFS) e histerese de 5 dB.
/// - *Hangover* de 18 quadros (360 ms) depois da última evidência de voz.
/// - O piso de ruído sobe devagar e desce rápido enquanto o VAD está fechado.
/// - `sensibilidade` 0.0 (menos sensível) a 1.0 (mais sensível), padrão 0.7: todos os limiares se movem de
///   forma monotônica (mais sensibilidade = limiares menores).
pub struct Vad {
    sens: f32,
    abs_db: Option<f32>,
    piso: f32,
    aberto: bool,
    resto: u32,
    evidencia: bool,
}

impl Vad {
    pub const HANGOVER_QUADROS: u32 = 18;
    pub const SENSIBILIDADE_PADRAO: f32 = 0.7;

    pub fn new(sensibilidade: f32) -> Vad {
        Vad {
            sens: if sensibilidade.is_finite() { sensibilidade.clamp(0.0, 1.0) } else { Self::SENSIBILIDADE_PADRAO },
            abs_db: None,
            piso: -60.0,
            aberto: false,
            resto: 0,
            evidencia: false,
        }
    }

    pub fn set_sensitivity(&mut self, s: f32) {
        if s.is_finite() {
            self.sens = s.clamp(0.0, 1.0);
        }
    }

    pub fn sensitivity(&self) -> f32 {
        self.sens
    }

    /// Força o nível mínimo absoluto de abertura (dBFS, -80..=-10); `None` volta ao calculado pela sensibilidade.
    pub fn set_open_db(&mut self, db: Option<f32>) {
        self.abs_db = db.filter(|d| d.is_finite()).map(|d| d.clamp(-80.0, -10.0));
    }

    pub fn is_open(&self) -> bool {
        self.aberto
    }

    /// O último quadro tinha voz de fato (acima do limiar de manter): usado para congelar o AGC no silêncio
    /// e durante o *hangover*.
    pub fn evidence(&self) -> bool {
        self.evidencia
    }

    pub fn reset(&mut self) {
        self.aberto = false;
        self.resto = 0;
        self.evidencia = false;
    }

    pub fn noise_floor_db(&self) -> f32 {
        self.piso
    }

    /// Limiares atuais para o modo com ou sem RNNoise.
    pub fn thresholds(&self, com_prob: bool) -> Limiares {
        let s = self.sens;
        if com_prob {
            let abs = self.abs_db.unwrap_or_else(|| lerp(-50.0, -66.0, s));
            let m = lerp(12.0, 6.0, s);
            Limiares {
                abre_db: abs.max(self.piso + m),
                fecha_db: (abs - 4.0).max(self.piso + m - 4.0),
                prob_abre: lerp(0.60, 0.25, s),
                prob_fecha: lerp(0.30, 0.10, s),
            }
        } else {
            let abs = self.abs_db.unwrap_or_else(|| lerp(-40.0, -58.0, s));
            let m = lerp(12.0, 7.0, s);
            Limiares {
                abre_db: abs.max(self.piso + m),
                fecha_db: (abs - 5.0).max(self.piso + m - 5.0),
                prob_abre: 0.0,
                prob_fecha: 0.0,
            }
        }
    }

    pub fn process(&mut self, quadro: &[f32]) -> bool {
        self.process_db_prob(rms_db(quadro), None)
    }

    /// Só por energia (dBFS), sem probabilidade de voz.
    pub fn process_db(&mut self, db: f32) -> bool {
        self.process_db_prob(db, None)
    }

    /// Energia (dBFS) e, se houver, a probabilidade de voz do RNNoise (0.0..=1.0).
    pub fn process_db_prob(&mut self, db: f32, prob: Option<f32>) -> bool {
        let db = if db.is_finite() { db } else { -120.0 };
        let l = self.thresholds(prob.is_some());
        let ok_abre = prob.map_or(true, |p| p >= l.prob_abre);
        let apoio = db >= l.fecha_db && prob.map_or(true, |p| p >= l.prob_fecha);
        if !self.aberto {
            if db >= l.abre_db && ok_abre {
                self.aberto = true;
                self.resto = Self::HANGOVER_QUADROS;
                self.evidencia = true;
            } else {
                self.evidencia = false;
                let k = if db < self.piso { 0.25 } else { 0.005 };
                self.piso = (self.piso + (db - self.piso) * k).clamp(-90.0, -35.0);
            }
        } else if apoio {
            self.resto = Self::HANGOVER_QUADROS;
            self.evidencia = true;
        } else {
            self.evidencia = false;
            if self.resto > 0 {
                self.resto -= 1;
            } else {
                self.aberto = false;
            }
        }
        self.aberto
    }
}

/// Medidor para a UI: dBFS → 0.0..=1.0 (-60 dB = 0, 0 dB = 1), subida imediata e descida suave.
pub struct LevelMeter {
    nivel: f32,
}

impl LevelMeter {
    pub fn new() -> LevelMeter {
        LevelMeter { nivel: 0.0 }
    }
    pub fn update(&mut self, db: f32) -> f32 {
        let alvo = ((db + 60.0) / 60.0).clamp(0.0, 1.0);
        self.nivel = if alvo > self.nivel { alvo } else { self.nivel * 0.8 + alvo * 0.2 };
        self.nivel
    }
    pub fn reset(&mut self) {
        self.nivel = 0.0;
    }
}

impl Default for LevelMeter {
    fn default() -> Self {
        Self::new()
    }
}

// ───────────────────────────── jitter buffer ─────────────────────────────

/// Atraso mínimo e máximo do jitter buffer, em quadros de 20 ms (40 e 240 ms). O alvo é adaptativo: o mínimo vale
/// numa rede calma, e rajadas do TCP (o WebSocket segura os pacotes e solta tudo de uma vez) sobem o alvo até o máximo.
pub const JB_MIN: usize = 2;
pub const JB_MAX: usize = 12;
const JB_INICIAL: usize = 3;
/// Alvo inicial do jitter buffer no modo de baixa latência (o mínimo, 40 ms).
const JB_INICIAL_BAIXA: usize = 2;
const JB_MAX_PACOTES: usize = 48;
/// Diferença de seq acima disso (para mais ou para menos) = o remetente recomeçou: ressincroniza.
const JB_RESYNC: i32 = 200;
/// Tantos descartes por atraso seguidos = o remetente recomeçou: ressincroniza.
const JB_ATRASADOS_RESYNC: u32 = 8;
/// Buraco maior que isso é saltado de uma vez em vez de gerar um PLC por quadro perdido.
const JB_BURACO_MAX: i32 = 4;
/// Excesso de profundidade (acima do alvo) a partir do qual o alvo sobe (rajada) ou, no máximo, descarta o mais antigo.
const JB_EXCESSO: usize = 4;
/// Quadros de PLC antes de considerar silêncio quando o buffer esvazia.
const JB_PLC_QUADROS: u32 = 3;
/// Quadros sem problema para diminuir o alvo em 1 (3 s).
const JB_CALMA: u32 = 150;
/// Esvaziar e voltar em menos que isto (quadros) conta como subfluxo (aumenta o alvo), não como pausa de fala.
const JB_SUBFLUXO_JANELA: u32 = 12;
/// Quanto pode "voltar" o primeiro pacote ainda não tocado (reordenação na chegada).
const JB_RETROCESSO_INICIAL: i32 = 8;

/// Resultado de pedir um quadro de 20 ms ao jitter buffer.
#[derive(Debug, PartialEq, Eq)]
pub enum Playout {
    /// Pacote Opus a decodificar.
    Frame(Vec<u8>),
    /// Pacote perdido ou atrasado: o decodificador deve fazer PLC.
    Lost,
    /// Nada a tocar (usuário calado ou ainda enchendo o buffer).
    Silence,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PushOutcome {
    Accepted,
    Duplicate,
    Late,
    Resync,
}

/// Jitter buffer de um usuário. Orientado a "ticks": cada [`pop`](JitterBuffer::pop) é um quadro de 20 ms
/// de reprodução, então não depende de relógio e é determinístico nos testes.
///
/// - Ordena por `seq` com aritmética modular de `u32` (funciona na virada do contador).
/// - Descarta duplicados e pacotes que chegaram depois do seu quadro já ter passado.
/// - Começa a tocar quando há `alvo` quadros guardados; o alvo (2 a 12 quadros = 40 a 240 ms) sobe de 2 em 2 quando há
///   subfluxo ou pacote atrasado, sobe sozinho para absorver uma rajada (em vez de cortar áudio) e desce 1 depois de 3 s
///   sem problemas.
/// - Perda no meio da fala vira `Lost` (PLC); buffer esvaziado vira alguns quadros de PLC e depois `Silence`
///   (o `seq` continua contíguo depois de uma pausa de VAD, então a fala retomada encaixa sozinha).
pub struct JitterBuffer {
    pacotes: Vec<(u32, Vec<u8>)>,
    proximo: u32,
    iniciado: bool,
    tocando: bool,
    virgem: bool, // nada tocado desde o último reset: aceita pacote um pouco anterior
    alvo: usize,
    espera: u32,
    vazios: u32,
    ocioso: u32,
    atrasados_seguidos: u32,
    calma: u32,
    descartes: u32,
    /// Para as estatísticas: esvaziamentos no meio da fala (subfluxos), PLCs por buraco na sequência.
    cortes: u32,
    buracos: u32,
    ultimo_buraco: bool,
    /// Quadros de PLC por buffer vazio desde o início (só os testes leem).
    plc_vazio: u32,
}

impl Default for JitterBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl JitterBuffer {
    pub fn new() -> JitterBuffer {
        Self::com_alvo(JB_INICIAL)
    }

    /// Jitter buffer com o atraso alvo inicial dado (em quadros, limitado a `JB_MIN..=JB_MAX`).
    pub fn com_alvo(inicial: usize) -> JitterBuffer {
        JitterBuffer {
            pacotes: Vec::with_capacity(JB_MAX_PACOTES + 1),
            proximo: 0,
            iniciado: false,
            tocando: false,
            virgem: true,
            alvo: inicial.clamp(JB_MIN, JB_MAX),
            espera: 0,
            vazios: 0,
            ocioso: u32::MAX / 2,
            atrasados_seguidos: 0,
            calma: 0,
            descartes: 0,
            cortes: 0,
            buracos: 0,
            ultimo_buraco: false,
            plc_vazio: 0,
        }
    }

    /// Quadros de PLC por buffer vazio (só para os testes).
    #[cfg(test)]
    fn plc_vazio(&self) -> u32 {
        self.plc_vazio
    }

    /// Esvaziamentos (cortes audíveis) desde a última chamada.
    pub fn take_cuts(&mut self) -> u32 {
        std::mem::take(&mut self.cortes)
    }

    /// Quadros perdidos por buraco na sequência (rede) desde a última chamada.
    #[allow(dead_code)] // diagnóstico (usado nos testes)
    pub fn take_holes(&mut self) -> u32 {
        std::mem::take(&mut self.buracos)
    }

    /// O último `Playout::Lost` foi um buraco na sequência (perda de rede), não um esvaziamento.
    pub fn last_was_hole(&self) -> bool {
        self.ultimo_buraco
    }

    /// Pacotes descartados desde a última chamada (atrasados, duplicados, excesso de latência).
    pub fn take_discarded(&mut self) -> u32 {
        std::mem::take(&mut self.descartes)
    }

    /// Atraso alvo atual, em quadros de 20 ms.
    pub fn target(&self) -> usize {
        self.alvo
    }

    /// Quadros de áudio à frente do ponto de reprodução (até o pacote mais novo).
    pub fn depth(&self) -> usize {
        self.pacotes
            .iter()
            .map(|(s, _)| s.wrapping_sub(self.proximo) as i32)
            .max()
            .map_or(0, |d| (d.max(-1) + 1) as usize)
    }

    pub fn is_playing(&self) -> bool {
        self.tocando
    }

    fn recomeca(&mut self, seq: u32, dados: Vec<u8>) {
        self.pacotes.clear();
        self.proximo = seq;
        self.iniciado = true;
        self.tocando = false;
        self.virgem = true;
        self.espera = 0;
        self.vazios = 0;
        self.atrasados_seguidos = 0;
        self.pacotes.push((seq, dados));
    }

    fn sobe_alvo(&mut self) {
        self.alvo = (self.alvo + 2).min(JB_MAX);
        self.calma = 0;
    }

    /// Entrega um pacote recebido da rede.
    pub fn push(&mut self, seq: u32, dados: Vec<u8>) -> PushOutcome {
        if !self.iniciado {
            self.recomeca(seq, dados);
            return PushOutcome::Accepted;
        }
        let dif = seq.wrapping_sub(self.proximo) as i32;
        if !(-JB_RESYNC..=JB_RESYNC).contains(&dif) {
            self.recomeca(seq, dados);
            return PushOutcome::Resync;
        }
        if dif < 0 {
            if self.virgem && dif >= -JB_RETROCESSO_INICIAL {
                // chegou fora de ordem antes de qualquer reprodução: recua o ponto de partida
                self.proximo = seq;
            } else {
                self.atrasados_seguidos += 1;
                self.descartes += 1;
                if self.atrasados_seguidos >= JB_ATRASADOS_RESYNC {
                    self.recomeca(seq, dados);
                    return PushOutcome::Resync;
                }
                if self.tocando {
                    self.sobe_alvo();
                }
                return PushOutcome::Late;
            }
        }
        self.atrasados_seguidos = 0;
        if self.pacotes.iter().any(|(s, _)| *s == seq) {
            self.descartes += 1;
            return PushOutcome::Duplicate;
        }
        self.pacotes.push((seq, dados));
        while self.pacotes.len() > JB_MAX_PACOTES {
            self.pula_um();
        }
        PushOutcome::Accepted
    }

    /// Descarta o quadro do ponto de reprodução (existindo ou não) e avança.
    fn pula_um(&mut self) {
        let p = self.proximo;
        if let Some(i) = self.pacotes.iter().position(|(s, _)| *s == p) {
            self.pacotes.swap_remove(i);
            self.descartes += 1;
        } else if let Some(i) = self.pacotes.iter().position(|(s, _)| (s.wrapping_sub(p) as i32) < 0) {
            self.pacotes.swap_remove(i);
        }
        self.proximo = p.wrapping_add(1);
    }

    /// Próximo quadro de 20 ms de reprodução.
    pub fn pop(&mut self) -> Playout {
        if !self.iniciado {
            return Playout::Silence;
        }
        if !self.tocando {
            let d = self.pacotes.len();
            if d == 0 {
                self.ocioso = self.ocioso.saturating_add(1);
                return Playout::Silence;
            }
            self.espera += 1;
            if d >= self.alvo || self.espera as usize > JB_MAX {
                self.tocando = true;
                self.espera = 0;
                if self.ocioso <= JB_SUBFLUXO_JANELA {
                    self.sobe_alvo(); // voltou logo depois de esvaziar: foi subfluxo, não pausa de fala
                    self.cortes += 1;
                }
                self.ocioso = 0;
            } else {
                self.ocioso = self.ocioso.saturating_add(1);
                return Playout::Silence;
            }
        }

        // Rajada (o WebSocket/TCP segurou os pacotes e soltou tudo de uma vez): em vez de cortar áudio, sobe o alvo até
        // o máximo e só descarta o que passar disso. O excesso que sobra é gasto aos poucos pelo comprimidor de tempo.
        if self.pacotes.len() > self.alvo + JB_EXCESSO && self.alvo < JB_MAX {
            self.alvo = JB_MAX.min(self.pacotes.len() - JB_EXCESSO);
            self.calma = 0;
        }
        while self.pacotes.len() > self.alvo + JB_EXCESSO {
            self.pula_um();
        }
        self.ultimo_buraco = false;

        let p = self.proximo;
        if let Some(i) = self.pacotes.iter().position(|(s, _)| *s == p) {
            let (_, dados) = self.pacotes.swap_remove(i);
            self.proximo = p.wrapping_add(1);
            self.virgem = false;
            if self.vazios > 0 {
                self.vazios = 0;
                self.sobe_alvo(); // o pacote chegou logo depois de esvaziar: subfluxo
                self.cortes += 1;
            }
            self.calma += 1;
            if self.calma >= JB_CALMA && self.alvo > JB_MIN {
                self.alvo -= 1;
                self.calma = 0;
            }
            return Playout::Frame(dados);
        }

        if self.pacotes.is_empty() {
            self.vazios += 1;
            if self.vazios > JB_PLC_QUADROS {
                self.tocando = false;
                self.vazios = 0;
                self.espera = 0;
                self.ocioso = 1;
                return Playout::Silence;
            }
            self.plc_vazio += 1;
            return Playout::Lost;
        }

        // há pacotes mais novos mas o do ponto de reprodução não veio: perdido
        self.vazios = 0;
        self.calma = 0;
        self.ultimo_buraco = true;
        self.buracos += 1;
        let menor = self
            .pacotes
            .iter()
            .map(|(s, _)| s.wrapping_sub(p) as i32)
            .filter(|d| *d > 0)
            .min()
            .unwrap_or(1);
        if menor > JB_BURACO_MAX {
            self.proximo = p.wrapping_add((menor - 1) as u32);
        }
        self.proximo = self.proximo.wrapping_add(1);
        Playout::Lost
    }
}

// ───────────────────────────── mixer ─────────────────────────────

/// Soma quadros de vários usuários com volume individual e aplica o limitador suave.
pub struct Mixer {
    acc: Vec<f32>,
}

impl Mixer {
    pub fn new(len: usize) -> Mixer {
        Mixer { acc: vec![0.0; len] }
    }
    pub fn begin(&mut self) {
        self.acc.iter_mut().for_each(|x| *x = 0.0);
    }
    /// Soma `src * volume` (volume 0.0..=2.0).
    pub fn add(&mut self, src: &[f32], volume: f32) {
        let v = if volume.is_finite() { volume.clamp(0.0, 2.0) } else { 1.0 };
        for (a, s) in self.acc.iter_mut().zip(src) {
            *a += s * v;
        }
    }
    /// Aplica o limitador e devolve o quadro misturado.
    pub fn finish(&mut self) -> &[f32] {
        for a in self.acc.iter_mut() {
            *a = soft_limit(*a);
        }
        &self.acc
    }
}

// ───────────────────────────── recepção por usuário ─────────────────────────────

/// Suaviza as bordas de uma fala (usuário remoto ou monitor): fade-in de 5 ms no primeiro quadro depois do
/// silêncio e, quando o áudio acaba, um fade-out de 5 ms a partir do último valor (em vez de um degrau para zero).
pub struct Declick {
    ativo: bool,
    ultimo: f32,
}

impl Declick {
    pub fn new() -> Declick {
        Declick { ativo: false, ultimo: 0.0 }
    }

    pub fn reset(&mut self) {
        self.ativo = false;
        self.ultimo = 0.0;
    }

    pub fn is_active(&self) -> bool {
        self.ativo
    }

    /// Há áudio neste quadro.
    pub fn audio(&mut self, quadro: &mut [f32]) {
        if !self.ativo {
            self.ativo = true;
            let n = FADE_AMOSTRAS.min(quadro.len());
            for (i, x) in quadro[..n].iter_mut().enumerate() {
                let t = (i + 1) as f32 / n as f32;
                *x *= 0.5 - 0.5 * (std::f32::consts::PI * t).cos();
            }
        }
        if let Some(u) = quadro.last() {
            self.ultimo = if u.is_finite() { *u } else { 0.0 };
        }
    }

    /// Não há áudio neste quadro. Devolve `true` se escreveu o fade-out em `quadro` (precisa ser somado);
    /// `false` se está em silêncio (`quadro` zerado).
    pub fn silencio(&mut self, quadro: &mut [f32]) -> bool {
        quadro.fill(0.0);
        if !self.ativo {
            return false;
        }
        self.ativo = false;
        let n = FADE_AMOSTRAS.min(quadro.len());
        for (i, x) in quadro[..n].iter_mut().enumerate() {
            let t = (i + 1) as f32 / n as f32;
            *x = self.ultimo * (0.5 + 0.5 * (std::f32::consts::PI * t).cos());
        }
        self.ultimo = 0.0;
        true
    }
}

impl Default for Declick {
    fn default() -> Self {
        Self::new()
    }
}

/// Estatísticas de um usuário remoto, para o painel de voz (ver [`Engine::voice_stats`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RxUserStats {
    pub user: i64,
    /// Perda de rede em % (média móvel de ~5 s de fala: quadros com buraco na sequência / quadros tocados).
    pub loss_pct: f32,
    /// Jitter de chegada (RFC 3550), em ms.
    pub jitter_ms: f32,
    /// Alvo do jitter buffer e áudio guardado agora (buffer + decodificado à frente), em ms.
    pub target_ms: f32,
    pub buffered_ms: f32,
    /// Esvaziamentos no meio da fala nos últimos 60 s, convertidos em cortes por minuto.
    pub cuts_per_min: f32,
    /// Atraso estimado até o alto-falante (sem a rede): buffer atual + fila de saída + 2 blocos do dispositivo, em ms.
    pub latency_ms: f32,
    /// O receptor está comprimindo (+1) ou esticando (-1) o tempo agora, 0 = reprodução normal.
    pub warp: i8,
    /// Recebeu pacote nos últimos ~250 ms.
    pub speaking: bool,
}

/// Quadros de fala para a média móvel da perda (~5 s).
const PERDA_JANELA: f32 = 250.0;
/// Janela dos "cortes por minuto".
const CORTES_JANELA: Duration = Duration::from_secs(60);
/// Desvio (em quadros de 20 ms) do buffer em relação ao alvo que liga/desliga o comprimidor e o esticador.
const WARP_LIGA_COMPRIME: f32 = 2.0;
const WARP_DESLIGA_COMPRIME: f32 = 1.0;
const WARP_LIGA_ESTICA: f32 = 2.0;
const WARP_DESLIGA_ESTICA: f32 = 1.0;

/// Um usuário remoto: jitter buffer + decodificador Opus + esticador de tempo + suavização das bordas.
///
/// O buffer de jitter entrega pacotes; o decodificador vira PCM de 20 ms e o PCM passa por um [`Esticador`], que mantém
/// o atraso perto do alvo do jitter buffer sem cortar nem repetir quadro: acima do alvo (rajada do TCP, relógio do
/// remetente mais rápido) a fala toca até 20% mais rápido; abaixo (rede atrasando) até 20% mais devagar, em vez de
/// esvaziar e fazer PLC. Sem desvio o PCM passa direto, idêntico (a regra só liga com ≥ 2 quadros de diferença, com
/// histerese, e fala em silêncio não é processada).
pub struct UserStream {
    jb: JitterBuffer,
    dec: Decoder,
    declick: Declick,
    last_rx: Instant,
    esc: Esticador,
    warp: Escala,
    tmp: Vec<f32>,
    t0: Instant,
    jit: JitterChegada,
    perda: f32,
    cortes: VecDeque<Instant>,
    /// Comprimir/esticar o tempo ligado (padrão). Desligado só nos testes de comparação.
    warp_ligado: bool,
}

impl UserStream {
    pub fn new() -> Result<UserStream, String> {
        Self::new_com_jitter(JB_INICIAL)
    }

    pub fn new_com_jitter(alvo_inicial: usize) -> Result<UserStream, String> {
        let dec = Decoder::new(SAMPLE_RATE, Channels::Mono).map_err(|e| format!("decodificador Opus: {e}"))?;
        Ok(UserStream {
            jb: JitterBuffer::com_alvo(alvo_inicial),
            dec,
            declick: Declick::new(),
            last_rx: Instant::now(),
            esc: Esticador::new(),
            warp: Escala::Normal,
            tmp: vec![0.0; FRAME],
            t0: Instant::now(),
            jit: JitterChegada::new(),
            perda: 0.0,
            cortes: VecDeque::new(),
            warp_ligado: true,
        })
    }

    pub fn push(&mut self, seq: u32, dados: Vec<u8>) -> PushOutcome {
        self.last_rx = Instant::now();
        self.jit.update(self.t0.elapsed().as_secs_f64() * 1000.0, seq);
        self.jb.push(seq, dados)
    }

    pub fn jitter(&self) -> &JitterBuffer {
        &self.jb
    }

    /// Pacotes descartados pelo jitter buffer desde a última chamada.
    pub fn take_discarded(&mut self) -> u32 {
        self.jb.take_discarded()
    }

    /// Quadros de 20 ms guardados (jitter buffer + PCM decodificado à frente).
    fn guardado(&self) -> f32 {
        self.jb.depth() as f32 + self.esc.disponivel() as f32 / FRAME as f32
    }

    /// Escolhe comprimir/esticar com histerese a partir do desvio do buffer em relação ao alvo. Devolve a fração de
    /// velocidade (0,05 a 0,20, proporcional ao desvio).
    fn decide(&mut self) -> f32 {
        if !self.jb.is_playing() || !self.warp_ligado {
            self.warp = Escala::Normal;
            return 0.0;
        }
        let dev = self.guardado() - self.jb.target() as f32;
        self.warp = match self.warp {
            Escala::Comprime if dev >= WARP_DESLIGA_COMPRIME => Escala::Comprime,
            Escala::Estica if -dev >= WARP_DESLIGA_ESTICA => Escala::Estica,
            _ if dev >= WARP_LIGA_COMPRIME => Escala::Comprime,
            _ if -dev >= WARP_LIGA_ESTICA => Escala::Estica,
            _ => Escala::Normal,
        };
        (dev.abs() * 0.05).clamp(0.05, Esticador::TAXA_MAX)
    }

    /// Pede o próximo quadro ao jitter buffer e decodifica para `self.tmp`. `None` = silêncio (nada a tocar).
    fn proximo_pcm(&mut self) -> Option<bool> {
        let (dados, perdido) = match self.jb.pop() {
            Playout::Silence => return None,
            Playout::Frame(d) => (d, false),
            Playout::Lost => (Vec::new(), true),
        };
        if perdido {
            let buraco = self.jb.last_was_hole();
            self.perda += (buraco as u8 as f32 - self.perda) / PERDA_JANELA;
        } else {
            self.perda -= self.perda / PERDA_JANELA;
        }
        match self.dec.decode_float(&dados, &mut self.tmp, false) {
            Ok(n) => {
                if n < FRAME {
                    self.tmp[n..].fill(0.0);
                }
                for x in self.tmp.iter_mut() {
                    if !x.is_finite() {
                        *x = 0.0;
                    }
                }
                Some(true)
            }
            Err(_) => Some(false),
        }
    }

    /// Gera os próximos 20 ms em `out` (`FRAME` amostras). `false` = silêncio (`out` zerado).
    pub fn render(&mut self, out: &mut [f32]) -> bool {
        debug_assert!(out.len() >= FRAME);
        let out = &mut out[..FRAME];
        let taxa = self.decide();
        let mut fim = false;
        for _ in 0..4 {
            if self.esc.disponivel() >= Esticador::precisa(self.warp) {
                break;
            }
            match self.proximo_pcm() {
                None => {
                    fim = true;
                    break;
                }
                Some(true) => {
                    let pcm = std::mem::take(&mut self.tmp);
                    self.esc.push(&pcm);
                    self.tmp = pcm;
                }
                Some(false) => {}
            }
        }
        let cortes = self.jb.take_cuts();
        for _ in 0..cortes {
            self.cortes.push_back(Instant::now());
        }
        if self.esc.pull(out, self.warp, taxa).is_some() {
            self.declick.audio(out);
            return true;
        }
        // menos de um quadro guardado e o jitter buffer não tem mais nada (fim da fala): esvazia o resto, com fade
        let resto = self.esc.disponivel();
        if fim && resto > 0 {
            out.fill(0.0);
            self.esc.drain_all(out);
            let u = out[resto - 1];
            let n = FADE_AMOSTRAS.min(FRAME - resto);
            for (i, x) in out[resto..resto + n].iter_mut().enumerate() {
                *x = u * (0.5 + 0.5 * (std::f32::consts::PI * (i + 1) as f32 / n as f32).cos());
            }
            self.declick.audio(out);
            return true;
        }
        if fim {
            self.esc.clear();
            self.warp = Escala::Normal;
        }
        self.declick.silencio(out)
    }

    /// Estatísticas atuais; `cb_saida_ms` e `fila_saida_ms` vêm do mixer.
    pub fn stats(&mut self, user: i64, fila_saida_ms: f32, cb_saida_ms: f32) -> RxUserStats {
        let agora = Instant::now();
        while self.cortes.front().is_some_and(|t| agora.duration_since(*t) > CORTES_JANELA) {
            self.cortes.pop_front();
        }
        let buffered_ms = if self.jb.is_playing() || self.esc.disponivel() > 0 { self.guardado() * 20.0 } else { 0.0 };
        let observado = self.t0.elapsed().min(CORTES_JANELA).as_secs_f32().max(10.0);
        RxUserStats {
            user,
            loss_pct: self.perda * 100.0,
            jitter_ms: self.jit.ms(),
            target_ms: self.jb.target() as f32 * 20.0,
            buffered_ms,
            cuts_per_min: self.cortes.len() as f32 * 60.0 / observado,
            latency_ms: buffered_ms + fila_saida_ms + 2.0 * cb_saida_ms,
            warp: match self.warp {
                Escala::Comprime => 1,
                Escala::Estica => -1,
                Escala::Normal => 0,
            },
            speaking: self.last_rx.elapsed() < SPEAKING_WINDOW,
        }
    }

    #[cfg(test)]
    fn sem_warp(&mut self) {
        self.warp_ligado = false;
    }

    /// Amostras removidas / repetidas pelo esticador desde o início.
    #[allow(dead_code)] // diagnóstico (usado nos testes)
    pub fn warped(&self) -> (u64, u64) {
        (self.esc.removidas, self.esc.repetidas)
    }
}

// ───────────────────────────── envio (captura → pacotes) ─────────────────────────────

/// Do quadro de 20 ms capturado ao pacote Opus: ganho, medidor, VAD, codificação e pré-rolagem.
/// Só gera pacotes quando o VAD está aberto (e os 2 quadros anteriores, para não cortar o início da fala).
pub struct SendPipeline {
    enc: Encoder,
    pub vad: Vad,
    /// Passa-alta de 80 Hz, a primeira coisa na captura (sempre ligada).
    hpf: PassaAlta,
    /// Ganho de entrada automático antes do RNNoise (a opção "AGC" liga/desliga este e o AGC2).
    pub pre_agc: PreAgc,
    /// AGC2 (igual ao do Discord) depois do RNNoise; o VAD decide sobre o nível *antes* dele.
    pub agc2: Agc2,
    /// Nível (dBFS) do último quadro depois do RNNoise e antes do AGC2.
    nivel_db: f32,
    /// Opção "AGC" do usuário. O pré-AGC só roda junto com o RNNoise (é para ele que serve).
    agc_on: bool,
    /// Cancelador de eco (AEC3), só enquanto a opção está ligada (cria e solta a memória dele).
    eco: Option<Eco>,
    /// O que o app tocou no intervalo do quadro atual (mesma base de tempo do microfone), para o cancelador de eco.
    pub reference: Vec<f32>,
    meter: LevelMeter,
    /// Ganho linear de entrada (1.0 = sem mudança).
    pub gain: f32,
    /// Só para testes com hardware: trata o VAD como sempre aberto.
    pub force_open: bool,
    preroll: VecDeque<Vec<u8>>,
    /// Pré-rolagem em PCM (já processado) para o monitor, com reaproveitamento dos vetores.
    preroll_pcm: VecDeque<Vec<f32>>,
    pcm_pool: Vec<Vec<f32>>,
    rajada: Vec<f32>,
    seq: u32,
    buf: Vec<f32>,
    out: [u8; MAX_PACKET],
    level: f32,
    speaking: bool,
    /// Supressão de ruído (RNNoise) ligada.
    den_on: bool,
    den_ativo: bool, // o denoiser rodou no quadro anterior (para o crossfade ao alternar)
    den: Box<DenoiseState<'static>>,
    prob: f32,
    processou: bool,
    den_buf: Vec<f32>,
    den_in: [f32; DEN_FRAME],
    den_out: [f32; DEN_FRAME],
}

impl SendPipeline {
    pub fn new() -> Result<SendPipeline, String> {
        let mut enc = Encoder::new(SAMPLE_RATE, Channels::Mono, Application::Voip)
            .map_err(|e| format!("codificador Opus: {e}"))?;
        // Ajustes finos são "melhor esforço": se algum falhar, o padrão do Opus continua servindo.
        let _ = enc.set_bitrate(Bitrate::Bits(BITRATE));
        let _ = enc.set_vbr(true);
        let _ = enc.set_vbr_constraint(true);
        let _ = enc.set_signal(Signal::Voice);
        let _ = enc.set_max_bandwidth(Bandwidth::Fullband);
        let _ = enc.set_bandwidth(Bandwidth::Fullband);
        let _ = enc.set_inband_fec(false);
        let _ = enc.set_dtx(false);
        let _ = enc.set_complexity(8);
        // seq inicial "aleatório": se o motor for recriado, o receptor ressincroniza na hora
        let semente = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() ^ (d.as_secs() as u32).wrapping_mul(2_654_435_761))
            .unwrap_or(1);
        Ok(SendPipeline {
            enc,
            vad: Vad::new(Vad::SENSIBILIDADE_PADRAO),
            hpf: PassaAlta::new(),
            pre_agc: PreAgc::new(),
            agc2: Agc2::new(),
            nivel_db: -120.0,
            agc_on: true,
            eco: None,
            reference: vec![0.0; FRAME],
            meter: LevelMeter::new(),
            gain: 1.0,
            force_open: false,
            preroll: VecDeque::with_capacity(PREROLL_FRAMES + 1),
            preroll_pcm: VecDeque::with_capacity(PREROLL_FRAMES + 1),
            pcm_pool: Vec::new(),
            rajada: Vec::with_capacity(FRAME * (PREROLL_FRAMES + 1)),
            seq: semente,
            buf: vec![0.0; FRAME],
            out: [0; MAX_PACKET],
            level: 0.0,
            speaking: false,
            den_on: true,
            den_ativo: true, // começa ligado: sem crossfade com o sinal cru no primeiro quadro
            prob: 0.0,
            processou: false,
            den: DenoiseState::new(),
            den_buf: vec![0.0; FRAME],
            den_in: [0.0; DEN_FRAME],
            den_out: [0.0; DEN_FRAME],
        })
    }

    /// Liga/desliga a supressão de ruído. Ao ligar o estado do RNNoise é reiniciado; a troca é
    /// suavizada com um crossfade de um quadro, então não estala.
    pub fn set_noise_suppression(&mut self, ligado: bool) {
        if ligado && !self.den_on {
            self.den = DenoiseState::new();
        }
        self.den_on = ligado;
        self.pre_agc.ligado = self.agc_on && ligado;
        if !self.processou {
            self.den_ativo = ligado; // nada tocado ainda: não há o que suavizar
        }
    }

    pub fn noise_suppression(&self) -> bool {
        self.den_on
    }

    /// Áudio já processado (ganho + supressão) do último quadro, o que foi para o VAD e o Opus.
    pub fn last_frame(&self) -> &[f32] {
        &self.buf
    }

    /// Roda o RNNoise (2 × 480) sobre `self.buf` e deixa o resultado em `self.den_buf`; devolve a
    /// probabilidade de voz (média das duas metades).
    fn denoise(&mut self) -> f32 {
        let mut prob = 0.0f32;
        for h in 0..2 {
            let ini = h * DEN_FRAME;
            for (d, s) in self.den_in.iter_mut().zip(&self.buf[ini..ini + DEN_FRAME]) {
                *d = s * 32768.0;
            }
            let p = self.den.process_frame(&mut self.den_out, &self.den_in);
            prob += if p.is_finite() { p } else { 0.0 };
            for (d, s) in self.den_buf[ini..ini + DEN_FRAME].iter_mut().zip(&self.den_out) {
                *d = (s / 32768.0).clamp(-1.0, 1.0);
            }
        }
        prob / 2.0
    }

    /// Nível para o medidor (0.0..=1.0) do último quadro.
    pub fn level(&self) -> f32 {
        self.level
    }

    /// VAD aberto no último quadro.
    pub fn speaking(&self) -> bool {
        self.speaking
    }

    /// Liga/desliga o AGC: pré-AGC antes do RNNoise (só vale com a supressão de ruído ligada: sem RNNoise, ganho antes
    /// do VAD de energia só amplificaria o ruído) e AGC2 depois.
    pub fn set_agc(&mut self, on: bool) {
        self.agc_on = on;
        self.pre_agc.ligado = on && self.den_on;
        self.agc2.ligado = on;
    }

    /// Liga/desliga o cancelador de eco (AEC3). Ao ligar cria o estado; ao desligar solta a memória.
    pub fn set_echo_cancel(&mut self, on: bool) {
        if on && self.eco.is_none() {
            self.eco = Some(Eco::new());
        } else if !on {
            self.eco = None;
        }
    }

    pub fn echo_cancel(&self) -> bool {
        self.eco.is_some()
    }

    /// ERLE e atraso do eco estimados pelo cancelador (`None` com ele desligado).
    pub fn echo_stats(&self) -> Option<EcoStats> {
        self.eco.as_ref().map(|e| e.stats())
    }

    /// Nível (dBFS) depois do RNNoise e antes do AGC2, do último quadro.
    pub fn level_db(&self) -> f32 {
        self.nivel_db
    }

    /// Esquece o estado (ao ficar mudo): fecha o VAD, zera o medidor e a pré-rolagem.
    pub fn reset(&mut self) {
        self.hpf.reset();
        self.pre_agc.reset();
        self.agc2.reset();
        self.vad.reset();
        self.meter.reset();
        self.preroll.clear();
        while let Some(f) = self.preroll_pcm.pop_front() {
            self.pcm_pool.push(f);
        }
        self.level = 0.0;
        self.speaking = false;
        if self.den_on {
            self.den = DenoiseState::new();
        }
        self.den_ativo = self.den_on;
        self.prob = 0.0;
    }

    /// Probabilidade de voz do RNNoise no último quadro (0 com a supressão desligada).
    pub fn voice_prob(&self) -> f32 {
        self.prob
    }

    /// Processa um quadro de `FRAME` amostras mono a 48 kHz; chama `send(seq, pacote)` se for o caso.
    pub fn process(&mut self, quadro: &[f32], send: &mut dyn FnMut(u32, &[u8])) {
        self.process_mon(quadro, send, &mut |_| {});
    }

    /// Como [`process`](Self::process), e além disso entrega ao monitor o PCM processado (o mesmo áudio que
    /// vai para o Opus) com o mesmo portão: ao abrir, primeiro a pré-rolagem de 100 ms e depois cada quadro
    /// enquanto o VAD estiver aberto (inclusive o *hangover*). Não passa por Opus.
    pub fn process_mon(
        &mut self,
        quadro: &[f32],
        send: &mut dyn FnMut(u32, &[u8]),
        monitor: &mut dyn FnMut(&[f32]),
    ) {
        if quadro.len() != FRAME {
            return;
        }
        let g = if self.gain.is_finite() { self.gain.clamp(0.0, 8.0) } else { 1.0 };
        // passa-alta de 80 Hz antes de tudo, depois o ganho manual e o pré-AGC (voz baixa/longe chega ao RNNoise num
        // nível normal; senão ele a corta como se fosse ruído)
        self.buf.copy_from_slice(quadro);
        self.hpf.process(&mut self.buf);
        if let Some(eco) = self.eco.as_mut() {
            eco.process(&mut self.buf, &self.reference);
        }
        for d in self.buf.iter_mut() {
            *d = (*d * g).clamp(-1.0, 1.0);
        }
        self.pre_agc.process(&mut self.buf);
        // Supressão de ruído (depois do ganho, antes do VAD e do Opus). Ao alternar, crossfade linear
        // de um quadro entre o sinal cru e o limpo.
        let mut prob = None;
        if self.den_on || self.den_ativo {
            let p = self.denoise();
            let n = FRAME as f32;
            match (self.den_on, self.den_ativo) {
                (true, true) => self.buf.copy_from_slice(&self.den_buf),
                (true, false) => {
                    for (i, (b, d)) in self.buf.iter_mut().zip(&self.den_buf).enumerate() {
                        let t = (i + 1) as f32 / n;
                        *b = *b * (1.0 - t) + d * t;
                    }
                }
                (false, _) => {
                    for (i, (b, d)) in self.buf.iter_mut().zip(&self.den_buf).enumerate() {
                        let t = (i + 1) as f32 / n;
                        *b = d * (1.0 - t) + *b * t;
                    }
                }
            }
            if self.den_on {
                prob = Some(p);
            }
            self.prob = if self.den_on { p } else { 0.0 };
        }
        self.den_ativo = self.den_on;
        self.processou = true;
        // O VAD decide sobre o nível pós-RNNoise e *antes* do AGC2: assim os limiares não dependem do ganho que o
        // AGC2 acumulou. O AGC2 usa a probabilidade de voz do RNNoise (sem RNNoise, 1 quando o VAD vê voz).
        let db = rms_db(&self.buf);
        self.nivel_db = db;
        let estava = self.vad.is_open();
        let aberto = self.vad.process_db_prob(db, prob) || self.force_open;
        // só vale como "fala" o que o VAD também enxerga (energia mínima e probabilidade): o RNNoise às vezes dá
        // probabilidade alta para chiado de sala quase digital, e o AGC2 aprenderia esse nível como se fosse voz
        let p_fala = if self.vad.evidence() { prob.unwrap_or(1.0) } else { 0.0 };
        self.agc2.process(&mut self.buf, p_fala);
        self.level = self.meter.update(rms_db(&self.buf));
        self.speaking = aberto;

        if aberto {
            // Entrega tudo numa chamada só (pré-rolagem + quadro atual): a fila do monitor publica de uma vez
            // e o consumidor nunca enxerga a rajada pela metade.
            self.rajada.clear();
            if !estava {
                while let Some(f) = self.preroll_pcm.pop_front() {
                    self.rajada.extend_from_slice(&f);
                    self.pcm_pool.push(f);
                }
            }
            self.rajada.extend_from_slice(&self.buf);
            monitor(&self.rajada);
        } else {
            let mut f = self.pcm_pool.pop().unwrap_or_else(|| vec![0.0; FRAME]);
            f.copy_from_slice(&self.buf);
            if self.preroll_pcm.len() >= PREROLL_FRAMES {
                if let Some(velho) = self.preroll_pcm.pop_front() {
                    self.pcm_pool.push(velho);
                }
            }
            self.preroll_pcm.push_back(f);
        }

        // O codificador roda em todo quadro (estado sempre aquecido, sem estalo no começo da fala).
        let n = match self.enc.encode_float(&self.buf, &mut self.out) {
            Ok(n) if n > 0 => n,
            _ => return,
        };
        if aberto {
            if !estava {
                while let Some(p) = self.preroll.pop_front() {
                    let s = self.seq;
                    self.seq = self.seq.wrapping_add(1);
                    send(s, &p);
                }
            }
            let s = self.seq;
            self.seq = self.seq.wrapping_add(1);
            send(s, &self.out[..n]);
        } else {
            if self.preroll.len() >= PREROLL_FRAMES {
                self.preroll.pop_front();
            }
            self.preroll.push_back(self.out[..n].to_vec());
        }
    }
}

// ───────────────────────────── estado compartilhado ─────────────────────────────

struct Shared {
    parar: AtomicBool,
    mudo: AtomicBool,
    surdo: AtomicBool,
    ganho: AtomicU32,
    vad_db: AtomicU32, // NaN = sem sobreposição
    vad_sens: AtomicU32,
    agc: AtomicBool,
    agc_db: AtomicU32,
    /// Para o painel de estatísticas: ganho do pré-AGC (dB), nível pós-RNNoise (dBFS) e probabilidade de voz.
    pre_db: AtomicU32,
    nivel_db: AtomicU32,
    prob: AtomicU32,
    /// Estatísticas por usuário remoto, publicadas pelo mixer a cada ~250 ms.
    rx_stats: Mutex<Vec<RxUserStats>>,
    /// Cancelamento de eco ligado (padrão desligado) e o que o cancelador mede (NaN = sem valor).
    eco: AtomicBool,
    eco_erle: AtomicU32,
    eco_atraso: AtomicU32,
    nivel: AtomicU32,
    falando: AtomicBool,
    pendentes: AtomicUsize,
    ruido: AtomicBool,
    monitor: AtomicBool,
    /// Contadores de diagnóstico (ver [`Stats`]).
    c_subfluxos: AtomicU32,
    c_sobrefluxos: AtomicU32,
    c_enviados: AtomicU32,
    c_descartados: AtomicU32,
    c_monitor_perdidos: AtomicU32,
    cb_entrada: AtomicU32,
    cb_saida: AtomicU32,
    /// A saída está sendo alimentada (há usuários ou monitor): só então falta de dados conta como subfluxo.
    saida_ativa: AtomicBool,
    /// Modo de baixa latência (fixo depois de iniciado).
    baixa_lat: AtomicBool,
    taxa_entrada: AtomicU32,
    taxa_saida: AtomicU32,
    /// Estimativas publicadas pelo mixer: antecedência da fila de saída e fila do monitor, em ms (f32).
    est_alvo_ms: AtomicU32,
    est_mon_ms: AtomicU32,
    /// Só para testes com hardware: mantém o mixer produzindo quadros mesmo sem ninguém falando.
    manter_saida: AtomicBool,
    /// Só para testes com hardware: força o VAD aberto.
    forcar_vad: AtomicBool,
    erro: Mutex<Option<String>>,
    ouvidos: Mutex<HashMap<i64, Instant>>,
}

impl Shared {
    fn new() -> Shared {
        Shared {
            parar: AtomicBool::new(false),
            mudo: AtomicBool::new(false),
            surdo: AtomicBool::new(false),
            ganho: AtomicU32::new(1.0f32.to_bits()),
            vad_db: AtomicU32::new(f32::NAN.to_bits()),
            vad_sens: AtomicU32::new(Vad::SENSIBILIDADE_PADRAO.to_bits()),
            agc: AtomicBool::new(true),
            agc_db: AtomicU32::new(0),
            pre_db: AtomicU32::new(0),
            nivel_db: AtomicU32::new((-120.0f32).to_bits()),
            prob: AtomicU32::new(0),
            rx_stats: Mutex::new(Vec::new()),
            eco: AtomicBool::new(false),
            eco_erle: AtomicU32::new(f32::NAN.to_bits()),
            eco_atraso: AtomicU32::new(f32::NAN.to_bits()),
            nivel: AtomicU32::new(0),
            falando: AtomicBool::new(false),
            pendentes: AtomicUsize::new(0),
            ruido: AtomicBool::new(true),
            monitor: AtomicBool::new(false),
            c_subfluxos: AtomicU32::new(0),
            c_sobrefluxos: AtomicU32::new(0),
            c_enviados: AtomicU32::new(0),
            c_descartados: AtomicU32::new(0),
            c_monitor_perdidos: AtomicU32::new(0),
            cb_entrada: AtomicU32::new(0),
            cb_saida: AtomicU32::new(0),
            saida_ativa: AtomicBool::new(false),
            baixa_lat: AtomicBool::new(false),
            taxa_entrada: AtomicU32::new(SAMPLE_RATE),
            taxa_saida: AtomicU32::new(SAMPLE_RATE),
            est_alvo_ms: AtomicU32::new(0),
            est_mon_ms: AtomicU32::new(0),
            manter_saida: AtomicBool::new(false),
            forcar_vad: AtomicBool::new(false),
            erro: Mutex::new(None),
            ouvidos: Mutex::new(HashMap::new()),
        }
    }

    fn set_error(&self, msg: String) {
        if let Ok(mut e) = self.erro.lock() {
            if e.is_none() {
                *e = Some(msg);
            }
        }
    }
}

enum Msg {
    Packet { user: i64, seq: u32, dados: Vec<u8> },
    Remove(i64),
    Volume(i64, f32),
}

enum Cmd {
    Entrada(bool), // true = captura ativa
}

// ───────────────────────────── threads de processamento ─────────────────────────────

/// Descarta até `n` amostras da fila da referência (as mais antigas).
fn descarta_ref(cons: &mut Consumer<f32>, n: usize) {
    let k = cons.slots().min(n);
    if k > 0 {
        if let Ok(c) = cons.read_chunk(k) {
            c.commit_all();
        }
    }
}

/// Lê `FRAME` amostras da referência (o que o app tocou) para casar com o quadro de microfone atual. Se faltar, completa com
/// zeros e anota a dívida: quando as amostras que faltavam chegarem, são descartadas, então a linha do tempo continua
/// casada com a captura (só esses poucos ms da referência viram silêncio). Se sobrar demais, descarta as mais antigas.
fn le_referencia(cons: &mut Consumer<f32>, dst: &mut [f32], divida: &mut usize) {
    if *divida > 0 {
        let d = cons.slots().min(*divida);
        descarta_ref(cons, d);
        *divida -= d;
    }
    let n = cons.slots();
    if n > REF_MAX && *divida == 0 {
        descarta_ref(cons, n - FRAME * 2);
    }
    let k = cons.slots().min(dst.len());
    if k > 0 {
        if let Ok(c) = cons.read_chunk(k) {
            let (a, b) = c.as_slices();
            dst[..a.len()].copy_from_slice(a);
            dst[a.len()..a.len() + b.len()].copy_from_slice(b);
            c.commit_all();
        }
    }
    if k < dst.len() {
        dst[k..].fill(0.0);
        *divida += dst.len() - k;
    }
}

/// Thread do codificador: lê a fila de captura, reamostra para 48 kHz e alimenta o [`SendPipeline`].
fn loop_captura(
    sh: Arc<Shared>,
    mut cons: Consumer<f32>,
    taxa: u32,
    mut send: Box<dyn FnMut(u32, &[u8]) + Send>,
    mut mon_prod: Producer<f32>,
    mut ref_cons: Consumer<f32>,
) {
    sh.taxa_entrada.store(taxa, Relaxed);
    let espera = Duration::from_millis(if sh.baixa_lat.load(Relaxed) { 1 } else { 4 });
    let mut pipe = match SendPipeline::new() {
        Ok(p) => p,
        Err(e) => {
            sh.set_error(format!("Não foi possível iniciar o codificador de voz: {e}"));
            return;
        }
    };
    let mut rs = Resampler::new(taxa, SAMPLE_RATE);
    let mut cru: Vec<f32> = Vec::with_capacity(8192);
    let mut a48: Vec<f32> = Vec::with_capacity(8192);
    let mut mudo_antes = false;
    let mut ruido_antes = true;
    let mut eco_antes = false;
    let mut divida_ref = 0usize; // amostras de referência que faltaram (zeros usados): descarta o mesmo tanto quando chegar
    let mut quadros = 0u32;
    while !sh.parar.load(Relaxed) {
        let n = cons.slots();
        if n == 0 {
            thread::sleep(espera);
            continue;
        }
        cru.clear();
        if let Ok(chunk) = cons.read_chunk(n.min(8192)) {
            let (a, b) = chunk.as_slices();
            cru.extend_from_slice(a);
            cru.extend_from_slice(b);
            chunk.commit_all();
        }
        if sh.mudo.load(Relaxed) {
            if !mudo_antes {
                mudo_antes = true;
                pipe.reset();
                rs = Resampler::new(taxa, SAMPLE_RATE);
                a48.clear();
                sh.nivel.store(0, Relaxed);
                sh.falando.store(false, Relaxed);
            }
            descarta_ref(&mut ref_cons, usize::MAX);
            continue;
        }
        mudo_antes = false;
        rs.process(&cru, &mut a48);
        pipe.gain = f32_from_bits(&sh.ganho);
        pipe.force_open = sh.forcar_vad.load(Relaxed);
        let limite = f32_from_bits(&sh.vad_db);
        pipe.vad.set_open_db(if limite.is_finite() { Some(limite) } else { None });
        pipe.vad.set_sensitivity(f32_from_bits(&sh.vad_sens));
        pipe.set_agc(sh.agc.load(Relaxed));
        let ruido = sh.ruido.load(Relaxed);
        if ruido != ruido_antes {
            ruido_antes = ruido;
            pipe.set_noise_suppression(ruido);
        }
        let eco = sh.eco.load(Relaxed);
        if eco != eco_antes {
            eco_antes = eco;
            pipe.set_echo_cancel(eco);
            descarta_ref(&mut ref_cons, usize::MAX);
            divida_ref = 0;
            sh.eco_erle.store(f32::NAN.to_bits(), Relaxed);
            sh.eco_atraso.store(f32::NAN.to_bits(), Relaxed);
        }
        // Monitor: o PCM já processado (ganho + RNNoise + AGC), só com o portão do VAD aberto, vai direto para
        // a fila do mixer, sem Opus e sem passar pelo `send`. Mudo/surdo desligam.
        let mon_ligado = sh.monitor.load(Relaxed) && !sh.surdo.load(Relaxed);
        let mut enviar = |seq: u32, pacote: &[u8]| {
            sh.c_enviados.fetch_add(1, Relaxed);
            send(seq, pacote);
        };
        let mut monitorar = |pcm: &[f32]| {
            if mon_ligado {
                let (_, resto) = mon_prod.push_partial_slice(pcm);
                if !resto.is_empty() {
                    sh.c_monitor_perdidos.fetch_add(1, Relaxed);
                }
            }
        };
        let mut usados = 0;
        while a48.len() - usados >= FRAME {
            if eco_antes {
                le_referencia(&mut ref_cons, &mut pipe.reference, &mut divida_ref);
                quadros = quadros.wrapping_add(1);
            }
            pipe.process_mon(&a48[usados..usados + FRAME], &mut enviar, &mut monitorar);
            usados += FRAME;
        }
        a48.drain(..usados);
        if eco_antes && quadros % 25 == 0 {
            if let Some(st) = pipe.echo_stats() {
                sh.eco_erle.store(st.erle_db.unwrap_or(f32::NAN).to_bits(), Relaxed);
                sh.eco_atraso.store(st.delay_ms.map_or(f32::NAN, |v| v as f32).to_bits(), Relaxed);
            }
        }
        sh.agc_db.store(pipe.agc2.gain_db().to_bits(), Relaxed);
        sh.pre_db.store(pipe.pre_agc.gain_db().to_bits(), Relaxed);
        sh.nivel_db.store(pipe.level_db().to_bits(), Relaxed);
        sh.prob.store(pipe.voice_prob().to_bits(), Relaxed);
        sh.nivel.store(pipe.level().to_bits(), Relaxed);
        sh.falando.store(pipe.speaking(), Relaxed);
    }
}

/// Tamanhos da fila do monitor (em amostras a 48 kHz). No modo normal valem 4 quadros de 20 ms para começar e
/// 2/7/10 para os limites; no de baixa latência escalam com o bloco do mixer e o callback de entrada (o PCM
/// processado chega em quadros de 20 ms, então a fila oscila em dente de serra de 960 amostras).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MonParams {
    /// Quantidade guardada antes de começar a tocar.
    prime: usize,
    /// Controle de deriva pelo **mínimo** da fila numa janela de ~250 ms (é o que mede o risco de esvaziar):
    /// abaixo de `baixo` estica o tempo (~0,4%) para a fila voltar a encher; acima de `alto` aperta.
    baixo: usize,
    alto: usize,
    /// Fila instantânea acima disto: descarta o excedente (conta como quadro perdido).
    max: usize,
    /// Tamanho médio da fila em regime (só para a estimativa de latência).
    media: usize,
}

impl MonParams {
    /// Acrescenta `extra` amostras a todos os alvos (aprendizado por subfluxo).
    fn mais(self, extra: usize) -> MonParams {
        // `baixo` não anda junto: ele mede o `m` observado, que só sobe depois que a fila reenche
        MonParams {
            prime: self.prime + extra,
            baixo: self.baixo,
            alto: self.alto + extra,
            max: self.max + extra,
            media: self.media + extra,
        }
    }
}

fn mon_params(baixa: bool, cb_entrada_48k: usize, bloco: usize, lead48: usize) -> MonParams {
    if !baixa {
        return MonParams {
            prime: MON_PRIME * FRAME,
            baixo: MON_BAIXO * FRAME,
            alto: MON_ALTO * FRAME,
            max: MON_MAX * FRAME,
            media: (MON_PRIME * FRAME).saturating_sub(lead48),
        };
    }
    // O PCM chega em quadros de 20 ms e é lido em blocos. Ao começar, o mixer ainda puxa `lead48` amostras
    // para encher a antecedência da fila de saída, então o menor tamanho da fila visto numa leitura (`m`)
    // fica entre `prime - lead48 - FRAME - 2*bloco` (pior encaixe) e `... - bloco`. Para tolerar uma chegada
    // atrasada em um bloco sem esvaziar, `m` precisa ficar acima de 2 blocos; a folga extra é meio callback
    // de entrada. `baixo` (onde começa a esticar) fica um pouco abaixo do `m` do pior caso e `alto` dois
    // blocos acima dele.
    let folga = cb_entrada_48k.max(bloco) / 2;
    let m_alvo = 2 * bloco + folga;
    let prime = FRAME + 2 * bloco + m_alvo + lead48;
    let baixo = 2 * bloco + folga / 2;
    let alto = m_alvo + 2 * bloco;
    MonParams { prime, baixo, alto, max: alto + 2 * FRAME + bloco, media: m_alvo + (FRAME - bloco) / 2 }
}

/// Lado da reprodução do monitor: fila do PCM processado, compensação de deriva entre o relógio do microfone
/// e o da saída (±0,3~0,4% de esticamento em vez de cortes) e fade nas bordas. Quando falta dado no meio da
/// fala, faz fade-out em vez de cair a zero.
struct MonitorOut {
    cons: Consumer<f32>,
    tocando: bool,
    declick: Declick,
    cru: Vec<f32>,
    /// Mínimo da fila na janela atual, leituras na janela e decisão vigente (-1 estica, 0 nada, 1 aperta).
    janela_min: usize,
    janela_n: u32,
    ajuste: i8,
    /// Margem aprendida (amostras): +10 ms a cada falta de dado com o portão aberto, até 80 ms.
    extra: usize,
}

impl MonitorOut {
    fn new(cons: Consumer<f32>) -> MonitorOut {
        MonitorOut {
            cons,
            tocando: false,
            declick: Declick::new(),
            cru: vec![0.0; FRAME + MON_AJUSTE],
            janela_min: usize::MAX,
            janela_n: 0,
            ajuste: 0,
            extra: 0,
        }
    }

    fn descarta(&mut self, n: usize) {
        if n > 0 {
            if let Ok(c) = self.cons.read_chunk(n.min(self.cons.slots())) {
                c.commit_all();
            }
        }
    }

    /// Monitor desligado (ou surdo): joga fora o que sobrou e volta ao estado inicial.
    fn drenar(&mut self) {
        let n = self.cons.slots();
        self.descarta(n);
        self.tocando = false;
        self.declick.reset();
    }

    /// Próximo bloco do monitor em `out` (`out.len()` amostras). `true` = há áudio para somar ao mixer.
    fn proximo(&mut self, sh: &Shared, out: &mut [f32], p: &MonParams) -> bool {
        let p = &p.mais(self.extra);
        let b = out.len();
        let aj = (b * MON_AJUSTE).div_ceil(FRAME).max(1);
        if !self.tocando {
            let disp = self.cons.slots();
            if disp < p.prime {
                return self.declick.silencio(out);
            }
            // começo de fala: a pré-rolagem excedente (a mais antiga, quase silêncio) sai sem contar como perda
            self.descarta(disp - p.prime);
            self.tocando = true;
            self.janela_min = usize::MAX;
            self.janela_n = 0;
            self.ajuste = 0;
        }
        let disp = self.cons.slots();
        if disp > p.max {
            self.descarta(disp - p.alto);
            sh.c_monitor_perdidos.fetch_add(1, Relaxed);
        }
        let disp = self.cons.slots();
        if disp < b {
            // faltou dado: fade-out e volta a juntar. Só é perda se o portão ainda está aberto (senão é o
            // fim normal da fala).
            self.tocando = false;
            if sh.falando.load(Relaxed) {
                sh.c_monitor_perdidos.fetch_add(1, Relaxed);
                self.extra = (self.extra + SAMPLE_RATE as usize / 100).min(SAMPLE_RATE as usize * 80 / 1000);
            }
            return self.declick.silencio(out);
        }
        self.janela_min = self.janela_min.min(disp);
        self.janela_n += 1;
        if self.janela_n as usize >= (12_000 / b).max(4) {
            self.ajuste = if self.janela_min < p.baixo {
                -1
            } else if self.janela_min > p.alto {
                1
            } else {
                0
            };
            self.janela_min = usize::MAX;
            self.janela_n = 0;
        }
        let n = match self.ajuste {
            1 if b + aj <= disp => b + aj,
            -1 if b > aj => b - aj,
            _ => b,
        };
        if self.cru.len() < b + aj {
            self.cru.resize(b + aj, 0.0);
        }
        let Ok(c) = self.cons.read_chunk(n) else {
            return self.declick.silencio(out);
        };
        let (a, bb) = c.as_slices();
        self.cru[..a.len()].copy_from_slice(a);
        self.cru[a.len()..a.len() + bb.len()].copy_from_slice(bb);
        c.commit_all();
        if n == b {
            out.copy_from_slice(&self.cru[..b]);
        } else {
            // estica/aperta n amostras em b por interpolação linear (deriva de clock)
            let passo = (n - 1) as f32 / (b - 1) as f32;
            for (i, o) in out.iter_mut().enumerate() {
                let pos = i as f32 * passo;
                let k = (pos as usize).min(n - 2);
                let f = pos - k as f32;
                *o = self.cru[k] + (self.cru[k + 1] - self.cru[k]) * f;
            }
        }
        for x in out.iter_mut() {
            if !x.is_finite() {
                *x = 0.0;
            }
        }
        self.declick.audio(out);
        true
    }
}

/// Thread do decodificador/mixer: mantém a fila de reprodução cheia (~40 ms à frente do dispositivo no modo
/// normal; no de baixa latência, só o callback do dispositivo mais uma pequena margem).
fn loop_reproducao(
    sh: Arc<Shared>,
    rx: Receiver<Msg>,
    mut prod: Producer<f32>,
    taxa: u32,
    mon_cons: Consumer<f32>,
    mut ref_prod: Producer<f32>,
) {
    let baixa = sh.baixa_lat.load(Relaxed);
    // Bloco de mixagem: 20 ms no modo normal, 5 ms no de baixa latência (as vozes continuam sendo
    // decodificadas em quadros de 20 ms e ficam numa fila intermediária).
    let bloco = if baixa { FRAME / 4 } else { FRAME };
    let (ahead_min_ms, margem_ms, espera) = if baixa {
        (0, PLAYBACK_MARGEM_BAIXA_MS, Duration::from_millis(2))
    } else {
        (PLAYBACK_AHEAD_MS, PLAYBACK_MARGEM_MS, Duration::from_millis(5))
    };
    sh.taxa_saida.store(taxa, Relaxed);
    let mut usuarios: HashMap<i64, UserStream> = HashMap::new();
    let mut volumes: HashMap<i64, f32> = HashMap::new();
    let mut usu_buf: VecDeque<f32> = VecDeque::with_capacity(FRAME * 2);
    let mut m_usu = Mixer::new(FRAME);
    let mut mixer = Mixer::new(bloco);
    let mut rs = Resampler::new(SAMPLE_RATE, taxa);
    let mut tmp = vec![0f32; FRAME];
    let mut bloco_usu = vec![0f32; bloco];
    let mut saida: Vec<f32> = Vec::with_capacity(FRAME * 8);
    let capacidade = prod.buffer().capacity();
    let mut ultima_limpeza = Instant::now();
    let mut mon = MonitorOut::new(mon_cons);
    let mut tmp_mon = vec![0f32; bloco];
    let mut extra_ms = 0usize;
    let mut subfluxos_vistos = 0u32;
    let mut ultima_estat = Instant::now();
    let mut eco_antes = false;
    let mut ref_inicio = Instant::now();
    let mut ref_escrito: u64 = 0;

    while !sh.parar.load(Relaxed) {
        // mensagens da rede/UI
        loop {
            match rx.try_recv() {
                Ok(Msg::Packet { user, seq, dados }) => {
                    sh.pendentes.fetch_sub(1, Relaxed);
                    if sh.surdo.load(Relaxed) {
                        continue;
                    }
                    if !usuarios.contains_key(&user) {
                        if usuarios.len() >= MAX_USERS {
                            continue;
                        }
                        match UserStream::new_com_jitter(if baixa { JB_INICIAL_BAIXA } else { JB_INICIAL }) {
                            Ok(u) => {
                                usuarios.insert(user, u);
                            }
                            Err(e) => {
                                sh.set_error(format!("Falha no decodificador de voz: {e}"));
                                continue;
                            }
                        }
                    }
                    if let Some(u) = usuarios.get_mut(&user) {
                        u.push(seq, dados);
                        let d = u.take_discarded();
                        if d > 0 {
                            sh.c_descartados.fetch_add(d, Relaxed);
                        }
                    }
                }
                Ok(Msg::Remove(user)) => {
                    usuarios.remove(&user);
                }
                Ok(Msg::Volume(user, v)) => {
                    volumes.insert(user, v);
                }
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => return,
            }
        }

        let surdo = sh.surdo.load(Relaxed);
        let monitor = sh.monitor.load(Relaxed) && !surdo;
        if !monitor {
            // monitor desligado (ou surdo): descarta o que sobrou na fila para não tocar áudio velho
            mon.drenar();
        }
        if surdo {
            if !usuarios.is_empty() {
                usuarios.clear();
                if let Ok(mut g) = sh.rx_stats.lock() {
                    g.clear();
                }
            }
            usu_buf.clear();
            thread::sleep(Duration::from_millis(10));
            continue;
        }

        // usuários sem pacote há muito tempo
        if ultima_limpeza.elapsed() > Duration::from_millis(500) {
            ultima_limpeza = Instant::now();
            let mortos: Vec<i64> =
                usuarios.iter().filter(|(_, u)| u.last_rx.elapsed() > USER_IDLE).map(|(k, _)| *k).collect();
            for k in mortos {
                usuarios.remove(&k);
                if let Ok(mut m) = sh.ouvidos.lock() {
                    m.remove(&k);
                }
            }
            if let Ok(mut m) = sh.ouvidos.lock() {
                m.retain(|_, t| t.elapsed() < USER_IDLE);
            }
        }

        // Quanto áudio manter à frente do dispositivo: sempre mais que um callback inteiro mais uma margem
        // (no modo normal, no mínimo 40 ms; o PipeWire costuma pedir 2048 quadros = 43 ms de uma vez).
        // Cada subfluxo observado aumenta a margem em 10 ms (até 80 ms).
        let subf = sh.c_subfluxos.load(Relaxed);
        if subf != subfluxos_vistos {
            subfluxos_vistos = subf;
            extra_ms = (extra_ms + 10).min(PLAYBACK_EXTRA_MAX_MS);
        }
        let cb = sh.cb_saida.load(Relaxed) as usize;
        let alvo = (taxa as usize * ahead_min_ms / 1000)
            .max(cb + taxa as usize * (margem_ms + extra_ms) / 1000)
            .min(capacidade / 2)
            .max(1);
        let cb_in_48k = {
            let t_in = sh.taxa_entrada.load(Relaxed).max(1) as usize;
            sh.cb_entrada.load(Relaxed) as usize * SAMPLE_RATE as usize / t_in
        };
        let lead48 = alvo * SAMPLE_RATE as usize / taxa as usize;
        let mp = mon_params(baixa, cb_in_48k, bloco, lead48);
        sh.est_alvo_ms.store((alvo as f32 * 1000.0 / taxa as f32).to_bits(), Relaxed);
        if ultima_estat.elapsed() >= Duration::from_millis(250) {
            ultima_estat = Instant::now();
            let fila_ms = alvo as f32 * 1000.0 / taxa as f32;
            let cb_ms = sh.cb_saida.load(Relaxed) as f32 * 1000.0 / taxa as f32;
            let mut v: Vec<RxUserStats> = usuarios.iter_mut().map(|(id, u)| u.stats(*id, fila_ms, cb_ms)).collect();
            v.sort_by_key(|r| r.user);
            if let Ok(mut g) = sh.rx_stats.lock() {
                *g = v;
            }
        }
        sh.est_mon_ms.store(((mp.media + mon.extra) as f32 * 1000.0 / SAMPLE_RATE as f32).to_bits(), Relaxed);

        // Referência do cancelador de eco: a linha do tempo do que o app toca, contínua e na taxa do relógio (blocos de
        // áudio quando há voz, zeros por tempo quando não há), 10 ms atrasada para nunca ficar curta.
        let eco = sh.eco.load(Relaxed);
        if eco != eco_antes {
            eco_antes = eco;
            ref_inicio = Instant::now();
            ref_escrito = 0;
        }
        if eco {
            let devido = (ref_inicio.elapsed().as_secs_f64() * SAMPLE_RATE as f64) as u64 + REF_FOLGA as u64;
            let mut falta = devido.saturating_sub(ref_escrito) as usize;
            let zeros = [0f32; 480];
            while falta > 0 && ref_prod.slots() > 0 {
                let k = falta.min(zeros.len());
                let (_, resto) = ref_prod.push_partial_slice(&zeros[..k]);
                let escrito = k - resto.len();
                ref_escrito += escrito as u64;
                falta -= k;
                if escrito < k {
                    break;
                }
            }
        }

        // enche a fila de reprodução
        let mut voltas = 0;
        while capacidade.saturating_sub(prod.slots()) < alvo && voltas < 96 {
            let tem_mon = monitor && mon.proximo(&sh, &mut tmp_mon, &mp);
            let tem_usu = !usuarios.is_empty() || !usu_buf.is_empty();
            if !tem_usu && !tem_mon && !sh.manter_saida.load(Relaxed) {
                break;
            }
            voltas += 1;
            mixer.begin();
            if tem_usu {
                while usu_buf.len() < bloco && !usuarios.is_empty() {
                    m_usu.begin();
                    for (id, u) in usuarios.iter_mut() {
                        if u.render(&mut tmp) {
                            m_usu.add(&tmp, volumes.get(id).copied().unwrap_or(1.0));
                        }
                        let d = u.take_discarded();
                        if d > 0 {
                            sh.c_descartados.fetch_add(d, Relaxed);
                        }
                    }
                    usu_buf.extend(m_usu.finish().iter().copied());
                }
                for x in bloco_usu.iter_mut() {
                    *x = usu_buf.pop_front().unwrap_or(0.0);
                }
                mixer.add(&bloco_usu, 1.0);
            }
            if tem_mon {
                mixer.add(&tmp_mon, 1.0);
            }
            let quadro = mixer.finish();
            if eco {
                let (_, resto) = ref_prod.push_partial_slice(quadro);
                ref_escrito += (quadro.len() - resto.len()) as u64;
            }
            if taxa == SAMPLE_RATE {
                let _ = prod.push_partial_slice(quadro);
            } else {
                saida.clear();
                rs.process(quadro, &mut saida);
                let _ = prod.push_partial_slice(&saida);
            }
        }
        sh.saida_ativa.store(
            !usuarios.is_empty()
                || !usu_buf.is_empty()
                || mon.tocando
                || mon.declick.is_active()
                || sh.manter_saida.load(Relaxed),
            Relaxed,
        );
        thread::sleep(espera);
    }
}

// ───────────────────────────── dispositivos (cpal) ─────────────────────────────

/// Escolha do dispositivo de áudio. `None` = o padrão do sistema.
#[derive(Clone, Debug, Default)]
pub struct Config {
    pub input: Option<String>,
    pub output: Option<String>,
    /// Modo de baixa latência: pede blocos pequenos ao dispositivo (ver [`Engine::start`]).
    pub low_latency: bool,
}

/// Nomes dos dispositivos disponíveis (para as configurações).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Devices {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

/// Lista entradas e saídas. Nunca falha: sem áudio devolve listas vazias.
pub fn list_devices() -> Devices {
    let host = cpal::default_host();
    let mut d = Devices::default();
    if let Ok(it) = host.input_devices() {
        for dev in it {
            if let Ok(n) = dev.name() {
                if !d.inputs.contains(&n) {
                    d.inputs.push(n);
                }
            }
        }
    }
    if let Ok(it) = host.output_devices() {
        for dev in it {
            if let Ok(n) = dev.name() {
                if !d.outputs.contains(&n) {
                    d.outputs.push(n);
                }
            }
        }
    }
    d
}

fn formato_suportado(f: SampleFormat) -> Option<u8> {
    Some(match f {
        SampleFormat::F32 => 0,
        SampleFormat::I16 => 1,
        SampleFormat::I32 => 2,
        SampleFormat::U16 => 3,
        SampleFormat::I8 => 4,
        SampleFormat::U8 => 5,
        SampleFormat::U32 => 6,
        SampleFormat::F64 => 7,
        _ => return None,
    })
}

/// Prefere 48 kHz com poucos canais e formato f32/i16; senão cai no padrão do dispositivo.
fn escolhe_config(
    faixas: Option<Vec<cpal::SupportedStreamConfigRange>>,
    padrao: Result<cpal::SupportedStreamConfig, cpal::DefaultStreamConfigError>,
    saida: bool,
) -> Result<cpal::SupportedStreamConfig, String> {
    if let Some(faixas) = faixas {
        let melhor = faixas
            .into_iter()
            .filter(|f| {
                f.min_sample_rate().0 <= SAMPLE_RATE
                    && f.max_sample_rate().0 >= SAMPLE_RATE
                    && formato_suportado(f.sample_format()).is_some()
            })
            .min_by_key(|f| {
                let canais = f.channels() as usize;
                let rank_canais = if saida { (canais != 2) as usize * 100 + canais } else { canais };
                (formato_suportado(f.sample_format()).unwrap_or(99), rank_canais)
            });
        if let Some(f) = melhor {
            return Ok(f.with_sample_rate(SampleRate(SAMPLE_RATE)));
        }
    }
    let c = padrao.map_err(|e| e.to_string())?;
    if formato_suportado(c.sample_format()).is_none() {
        return Err(format!("formato de amostra {:?} não suportado", c.sample_format()));
    }
    Ok(c)
}

fn msg_stream_erro(lado: &'static str, e: cpal::StreamError) -> String {
    match e {
        cpal::StreamError::DeviceNotAvailable => {
            format!("O dispositivo de {lado} de áudio foi desconectado ou ficou indisponível.")
        }
        other => format!("Erro no dispositivo de {lado} de áudio: {other}"),
    }
}

macro_rules! despacha_formato {
    ($fmt:expr, $f:ident, $($arg:expr),*) => {
        match $fmt {
            SampleFormat::F32 => $f::<f32>($($arg),*),
            SampleFormat::I16 => $f::<i16>($($arg),*),
            SampleFormat::I32 => $f::<i32>($($arg),*),
            SampleFormat::U16 => $f::<u16>($($arg),*),
            SampleFormat::I8 => $f::<i8>($($arg),*),
            SampleFormat::U8 => $f::<u8>($($arg),*),
            SampleFormat::U32 => $f::<u32>($($arg),*),
            SampleFormat::F64 => $f::<f64>($($arg),*),
            outro => Err(format!("formato de amostra {outro:?} não suportado")),
        }
    };
}

/// Amostra de saída com "segurar e decair": sem dado (ou surdo) repete o último valor decaindo (τ ≈ 4 ms)
/// em vez de cair a zero de uma vez, o que evita o estalo de um degrau. Também barra NaN/infinito e satura
/// em ±1 antes da conversão para o formato do dispositivo.
struct Suave {
    ultimo: f32,
}

impl Suave {
    fn new() -> Suave {
        Suave { ultimo: 0.0 }
    }
    fn dado(&mut self, v: f32) -> f32 {
        self.ultimo = if v.is_finite() { v.clamp(-1.0, 1.0) } else { 0.0 };
        self.ultimo
    }
    fn vazio(&mut self) -> f32 {
        self.ultimo *= 0.995;
        self.ultimo
    }
}

/// Callback de entrada: mistura os canais em mono e empurra na fila. Sem alocação, sem mutex.
fn constroi_entrada<T>(
    dev: &cpal::Device,
    cfg: &StreamConfig,
    mut prod: Producer<f32>,
    sh: Arc<Shared>,
) -> Result<Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let canais = cfg.channels.max(1) as usize;
    let inv = 1.0 / canais as f32;
    let sh_erro = sh.clone();
    dev.build_input_stream(
        cfg,
        move |dados: &[T], _| {
            sh.cb_entrada.store((dados.len() / canais) as u32, Relaxed);
            if sh.mudo.load(Relaxed) {
                return;
            }
            let mut perdeu = false;
            for quadro in dados.chunks_exact(canais) {
                let mut s = 0.0f32;
                for x in quadro {
                    s += f32::from_sample(*x);
                }
                perdeu |= prod.push(s * inv).is_err();
            }
            if perdeu {
                sh.c_sobrefluxos.fetch_add(1, Relaxed);
            }
        },
        move |e| sh_erro.set_error(msg_stream_erro("entrada", e)),
        None,
    )
    .map_err(|e| e.to_string())
}

/// Callback de saída: lê mono da fila e duplica nos canais; fila vazia = silêncio. Sem alocação, sem mutex.
fn constroi_saida<T>(
    dev: &cpal::Device,
    cfg: &StreamConfig,
    mut cons: Consumer<f32>,
    sh: Arc<Shared>,
) -> Result<Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let canais = cfg.channels.max(1) as usize;
    let sh_erro = sh.clone();
    let mut suave = Suave::new();
    dev.build_output_stream(
        cfg,
        move |dados: &mut [T], _| {
            let surdo = sh.surdo.load(Relaxed);
            if surdo {
                while cons.pop().is_ok() {}
            }
            let mut faltou = false;
            for quadro in dados.chunks_mut(canais) {
                let s = if surdo {
                    suave.vazio()
                } else {
                    match cons.pop() {
                        Ok(v) => suave.dado(v),
                        Err(_) => {
                            faltou = true;
                            suave.vazio()
                        }
                    }
                };
                let v = T::from_sample(s);
                for o in quadro {
                    *o = v;
                }
            }
            sh.cb_saida.store((dados.len() / canais) as u32, Relaxed);
            if faltou && sh.saida_ativa.load(Relaxed) {
                sh.c_subfluxos.fetch_add(1, Relaxed);
            }
        },
        move |e| sh_erro.set_error(msg_stream_erro("saída", e)),
        None,
    )
    .map_err(|e| e.to_string())
}

struct InfoDispositivos {
    taxa_entrada: Option<u32>,
    taxa_saida: u32,
}

fn acha_entrada(host: &cpal::Host, nome: &Option<String>) -> Option<cpal::Device> {
    if let Some(n) = nome {
        if let Ok(mut it) = host.input_devices() {
            if let Some(d) = it.find(|d| d.name().map(|x| &x == n).unwrap_or(false)) {
                return Some(d);
            }
        }
    }
    host.default_input_device()
}

fn acha_saida(host: &cpal::Host, nome: &Option<String>) -> Option<cpal::Device> {
    if let Some(n) = nome {
        if let Ok(mut it) = host.output_devices() {
            if let Some(d) = it.find(|d| d.name().map(|x| &x == n).unwrap_or(false)) {
                return Some(d);
            }
        }
    }
    host.default_output_device()
}

/// Buffer pedido ao ALSA no modo de baixa latência: 1024 quadros de buffer = 4 períodos de 256 quadros
/// (5,3 ms por callback). Medido no PipeWire: `Fixed(1024)` entrega callbacks de 256, `Fixed(512)` de 128.
const BUFFER_BAIXA_QUADROS: u32 = 1024;

fn tamanho_buffer_baixa() -> u32 {
    #[cfg(test)]
    if let Some(n) = std::env::var("VOICE_LAB_FIXED").ok().and_then(|v| v.parse::<u32>().ok()) {
        return n;
    }
    BUFFER_BAIXA_QUADROS
}

/// Escolhe o `BufferSize` do modo de baixa latência: tenta `Fixed` (respeitando o intervalo que o dispositivo
/// informa) abrindo um fluxo de prova com filas descartáveis; se não abrir, volta ao padrão e, como o ALSA
/// do cpal passa pelo plugin do PipeWire, pede o quantum por `PIPEWIRE_LATENCY` (da próxima conexão em diante).
fn escolhe_buffer(
    dev: &cpal::Device,
    sc: &cpal::SupportedStreamConfig,
    conf: &StreamConfig,
    saida: bool,
    sh: &Arc<Shared>,
) -> cpal::BufferSize {
    let fmt = sc.sample_format();
    let pedido = match sc.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } => Some(tamanho_buffer_baixa().clamp(*min, *max)),
        cpal::SupportedBufferSize::Unknown => None,
    };
    if let Some(n) = pedido {
        let mut prova = conf.clone();
        prova.buffer_size = cpal::BufferSize::Fixed(n);
        let ok = if saida {
            let (_, cons) = RingBuffer::<f32>::new(8);
            despacha_formato!(fmt, constroi_saida, dev, &prova, cons, sh.clone()).is_ok()
        } else {
            let (prod, _) = RingBuffer::<f32>::new(8);
            despacha_formato!(fmt, constroi_entrada, dev, &prova, prod, sh.clone()).is_ok()
        };
        if ok {
            return cpal::BufferSize::Fixed(n);
        }
    }
    pede_quantum_por_ambiente();
    cpal::BufferSize::Default
}

/// `PIPEWIRE_LATENCY=256/48000` é uma variável **do processo**: o libpipewire só a lê quando o cliente se
/// conecta, então precisa estar definida antes de abrir o stream. Só mexemos nela se o usuário não tiver
/// definido a sua e só no plano B (`Fixed` recusado).
fn pede_quantum_por_ambiente() {
    if std::env::var_os("PIPEWIRE_LATENCY").is_none() {
        std::env::set_var("PIPEWIRE_LATENCY", "256/48000");
    }
}

fn abre_saida(host: &cpal::Host, cfg: &Config, cons: Consumer<f32>, sh: Arc<Shared>) -> Result<(Stream, u32), String> {
    let dev = acha_saida(host, &cfg.output).ok_or_else(|| "Nenhuma saída de áudio disponível.".to_string())?;
    let faixas = dev.supported_output_configs().ok().map(|i| i.collect::<Vec<_>>());
    let sc = escolhe_config(faixas, dev.default_output_config(), true)?;
    let fmt = sc.sample_format();
    let mut conf: StreamConfig = sc.config();
    if cfg.low_latency {
        conf.buffer_size = escolhe_buffer(&dev, &sc, &conf, true, &sh);
    }
    let taxa = conf.sample_rate.0;
    let stream = despacha_formato!(fmt, constroi_saida, &dev, &conf, cons, sh)?;
    Ok((stream, taxa))
}

fn abre_entrada(host: &cpal::Host, cfg: &Config, prod: Producer<f32>, sh: Arc<Shared>) -> Result<(Stream, u32), String> {
    let dev = acha_entrada(host, &cfg.input).ok_or_else(|| "nenhum microfone encontrado".to_string())?;
    let faixas = dev.supported_input_configs().ok().map(|i| i.collect::<Vec<_>>());
    let sc = escolhe_config(faixas, dev.default_input_config(), false)?;
    let fmt = sc.sample_format();
    let mut conf: StreamConfig = sc.config();
    if cfg.low_latency {
        conf.buffer_size = escolhe_buffer(&dev, &sc, &conf, false, &sh);
    }
    let taxa = conf.sample_rate.0;
    let stream = despacha_formato!(fmt, constroi_entrada, &dev, &conf, prod, sh)?;
    Ok((stream, taxa))
}

/// Thread que é dona dos `Stream`s do cpal (eles não podem trocar de thread). Abre, avisa o resultado
/// por `pronto`, e fica esperando comandos até o motor parar; ao sair, fecha os streams.
fn thread_dispositivos(
    cfg: Config,
    sh: Arc<Shared>,
    cap_prod: Producer<f32>,
    play_cons: Consumer<f32>,
    pronto: Sender<Result<InfoDispositivos, String>>,
    cmds: Receiver<Cmd>,
) {
    let host = cpal::default_host();
    let (saida, taxa_saida) = match abre_saida(&host, &cfg, play_cons, sh.clone()) {
        Ok(v) => v,
        Err(e) => {
            let _ = pronto.send(Err(format!("Não foi possível abrir a saída de áudio: {e}")));
            return;
        }
    };
    let entrada = match abre_entrada(&host, &cfg, cap_prod, sh.clone()) {
        Ok(v) => Some(v),
        Err(e) => {
            sh.set_error(format!("Sem microfone ({e}). Você consegue ouvir os outros, mas não falar."));
            None
        }
    };
    if let Err(e) = saida.play() {
        let _ = pronto.send(Err(format!("Não foi possível iniciar a saída de áudio: {e}")));
        return;
    }
    let mut taxa_entrada = None;
    if let Some((s, t)) = &entrada {
        match s.play() {
            Ok(()) => taxa_entrada = Some(*t),
            Err(e) => sh.set_error(format!("Não foi possível iniciar o microfone: {e}")),
        }
    }
    let _ = pronto.send(Ok(InfoDispositivos { taxa_entrada, taxa_saida }));

    while !sh.parar.load(Relaxed) {
        match cmds.recv_timeout(Duration::from_millis(100)) {
            Ok(Cmd::Entrada(ativa)) => {
                if let Some((s, _)) = &entrada {
                    // Pausar o fluxo apaga o indicador de microfone do sistema; se o backend não deixar,
                    // o callback já descarta as amostras enquanto mudo.
                    if ativa {
                        let _ = s.play();
                    } else {
                        let _ = s.pause();
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    drop(entrada);
    drop(saida);
}

// ───────────────────────────── motor ─────────────────────────────

/// Contadores de diagnóstico do motor (monotônicos desde o início).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    /// Callbacks de saída que ficaram sem dados enquanto havia algo para tocar (usuários ou monitor).
    pub underruns: u32,
    /// Callbacks de entrada que perderam amostras porque a fila de captura estava cheia.
    pub overruns: u32,
    /// Pacotes Opus entregues ao `send`.
    pub packets_sent: u32,
    /// Pacotes recebidos descartados pelo jitter buffer (atrasados, duplicados ou excesso de latência).
    pub jitter_dropped: u32,
    /// Quadros do monitor perdidos (fila cheia, falta de dados no meio da fala).
    pub monitor_lost: u32,
    /// Tamanho do último callback do dispositivo, em quadros (por canal): entrada e saída.
    pub input_callback_frames: u32,
    pub output_callback_frames: u32,
}

/// Estatísticas do que o microfone está enviando.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TxStats {
    /// Nível depois do RNNoise e antes do AGC2, em dBFS.
    pub level_db: f32,
    /// Ganho aplicado agora pelo pré-AGC (antes do RNNoise) e pelo AGC2, em dB.
    pub pre_gain_db: f32,
    pub agc2_gain_db: f32,
    /// Probabilidade de voz do RNNoise (0 com a supressão desligada).
    pub voice_prob: f32,
    /// VAD aberto: está mandando pacotes.
    pub speaking: bool,
    /// Cancelador de eco ligado: ERLE e atraso do caminho do eco (`None` com ele desligado).
    pub echo: Option<EcoStats>,
}

/// Totais da recepção (todos os usuários remotos).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RxTotals {
    pub users: usize,
    /// Médias de perda e jitter; soma dos cortes por minuto; maiores buffer e latência.
    pub loss_pct: f32,
    pub jitter_ms: f32,
    pub cuts_per_min: f32,
    pub buffered_ms: f32,
    pub latency_ms: f32,
}

impl RxTotals {
    pub fn from_users(rx: &[RxUserStats]) -> RxTotals {
        let n = rx.len();
        if n == 0 {
            return RxTotals::default();
        }
        let m = n as f32;
        RxTotals {
            users: n,
            loss_pct: rx.iter().map(|r| r.loss_pct).sum::<f32>() / m,
            jitter_ms: rx.iter().map(|r| r.jitter_ms).sum::<f32>() / m,
            cuts_per_min: rx.iter().map(|r| r.cuts_per_min).sum(),
            buffered_ms: rx.iter().map(|r| r.buffered_ms).fold(0.0, f32::max),
            latency_ms: rx.iter().map(|r| r.latency_ms).fold(0.0, f32::max),
        }
    }
}

/// Tudo que o painel de estatísticas de voz mostra, de uma vez só.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VoiceStats {
    pub tx: TxStats,
    pub rx: Vec<RxUserStats>,
    pub total: RxTotals,
}

/// O motor de voz. Criar com [`Engine::start`]; soltar (`Drop`) encerra threads e fecha os dispositivos.
pub struct Engine {
    sh: Arc<Shared>,
    tx: Sender<Msg>,
    cmd: Option<Sender<Cmd>>,
    threads: Vec<JoinHandle<()>>,
}

impl Engine {
    /// Abre os dispositivos e liga as threads.
    ///
    /// `send` é chamada pela thread do codificador com `(seq, pacote Opus)` e **não deve bloquear**
    /// (o normal é só enfileirar para o gateway). Erro (em português) se não abrir a saída; se só a entrada
    /// falhar, o motor inicia sem microfone e [`Engine::error`] explica.
    ///
    /// Com `cfg.low_latency` o motor pede blocos pequenos ao dispositivo: `BufferSize::Fixed(1024)` no ALSA do
    /// cpal (buffer de 4 períodos) vira callbacks de 256 quadros (5,3 ms) no PipeWire, em vez dos 2048 (43 ms)
    /// do padrão; a entrada e a saída são testadas com um fluxo de prova antes. Se o `Fixed` for recusado, cai
    /// no padrão e define `PIPEWIRE_LATENCY=256/48000` no **ambiente do processo** (só se o usuário não tiver
    /// definido a sua; vale para conexões ao PipeWire feitas depois, sozinha reduz os blocos de 2048 para 1280).
    /// As antecedências das filas (saída, monitor, jitter) escalam com o tamanho real do callback.
    pub fn start(cfg: Config, send: Box<dyn FnMut(u32, &[u8]) + Send>) -> Result<Engine, String> {
        let sh = Arc::new(Shared::new());
        sh.baixa_lat.store(cfg.low_latency, Relaxed);
        let (cap_prod, cap_cons) = RingBuffer::<f32>::new(RING_SAMPLES);
        let (play_prod, play_cons) = RingBuffer::<f32>::new(RING_SAMPLES);
        let (pronto_tx, pronto_rx) = mpsc::channel();
        let (cmd_tx, cmd_rx) = mpsc::channel();

        let sh_dev = sh.clone();
        let dev = thread::Builder::new()
            .name("telacast-audio-dispositivos".into())
            .spawn(move || thread_dispositivos(cfg, sh_dev, cap_prod, play_cons, pronto_tx, cmd_rx))
            .map_err(|e| format!("Não foi possível iniciar a thread de áudio: {e}"))?;

        let info = match pronto_rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(i)) => i,
            Ok(Err(e)) => {
                sh.parar.store(true, Relaxed);
                let _ = dev.join();
                return Err(e);
            }
            Err(_) => {
                sh.parar.store(true, Relaxed);
                return Err("Tempo esgotado ao abrir os dispositivos de áudio.".into());
            }
        };

        let mut eng = Engine::liga_threads(
            sh,
            info.taxa_entrada.map(|t| (cap_cons, t)),
            (play_prod, info.taxa_saida),
            send,
        )?;
        eng.cmd = Some(cmd_tx);
        eng.threads.push(dev);
        Ok(eng)
    }

    /// Núcleo sem cpal: liga as threads de codificação e mixagem sobre filas já criadas (usado por `start`
    /// e pelos testes sem hardware).
    fn liga_threads(
        sh: Arc<Shared>,
        captura: Option<(Consumer<f32>, u32)>,
        reproducao: (Producer<f32>, u32),
        send: Box<dyn FnMut(u32, &[u8]) + Send>,
    ) -> Result<Engine, String> {
        let (tx, rx) = mpsc::channel();
        let (mon_prod, mon_cons) = RingBuffer::<f32>::new(FRAME * MON_RING_QUADROS);
        let (ref_prod, ref_cons) = RingBuffer::<f32>::new(REF_RING);
        let mut threads = Vec::new();
        let mut falha = |e: std::io::Error| format!("Não foi possível iniciar a thread de áudio: {e}");

        if let Some((cons, taxa)) = captura {
            let s = sh.clone();
            threads.push(
                thread::Builder::new()
                    .name("telacast-audio-codificador".into())
                    .spawn(move || loop_captura(s, cons, taxa, send, mon_prod, ref_cons))
                    .map_err(&mut falha)?,
            );
        }
        let s = sh.clone();
        let (prod, taxa) = reproducao;
        threads.push(
            thread::Builder::new()
                .name("telacast-audio-mixer".into())
                .spawn(move || loop_reproducao(s, rx, prod, taxa, mon_cons, ref_prod))
                .map_err(&mut falha)?,
        );
        Ok(Engine { sh, tx, cmd: None, threads })
    }

    /// Para de capturar e enviar (o microfone é pausado quando o sistema deixa).
    pub fn set_muted(&self, muted: bool) {
        self.sh.mudo.store(muted, Relaxed);
        if let Some(c) = &self.cmd {
            let _ = c.send(Cmd::Entrada(!muted));
        }
    }

    /// Para de tocar e esvazia os buffers de todos.
    pub fn set_deafened(&self, deafened: bool) {
        self.sh.surdo.store(deafened, Relaxed);
        if deafened {
            if let Ok(mut m) = self.sh.ouvidos.lock() {
                m.clear();
            }
        }
    }

    /// Volume de um usuário, 0.0..=2.0 (padrão 1.0).
    pub fn set_user_volume(&self, user: i64, volume: f32) {
        let v = if volume.is_finite() { volume.clamp(0.0, 2.0) } else { 1.0 };
        let _ = self.tx.send(Msg::Volume(user, v));
    }

    /// Supressão de ruído (RNNoise) no microfone. Padrão: ligada. Pode alternar com a chamada em curso.
    pub fn set_noise_suppression(&self, on: bool) {
        self.sh.ruido.store(on, Relaxed);
    }

    pub fn noise_suppression(&self) -> bool {
        self.sh.ruido.load(Relaxed)
    }

    /// Cancelamento de eco (AEC3 do WebRTC) no microfone, usando o que o app toca como referência. Padrão: desligado
    /// (experimental). Ao ligar custa ~5,7 MB e ~0,8% de um núcleo; sem fone, tira o que sai da caixa de som do microfone.
    pub fn set_echo_cancel(&self, on: bool) {
        self.sh.eco.store(on, Relaxed);
    }

    pub fn echo_cancel(&self) -> bool {
        self.sh.eco.load(Relaxed)
    }

    /// "Ouvir meu microfone": toca localmente exatamente o que está sendo enviado (já com ganho e supressão,
    /// só com o VAD aberto). Padrão: desligado. Mudo não toca; surdo não toca nada.
    pub fn set_monitor(&self, on: bool) {
        self.sh.monitor.store(on, Relaxed);
    }

    pub fn monitoring(&self) -> bool {
        self.sh.monitor.load(Relaxed)
    }

    /// Ganho do microfone (linear, 0.0..=4.0; 1.0 = sem mudança). Extra em relação ao VOICE.md.
    pub fn set_input_gain(&self, gain: f32) {
        let g = if gain.is_finite() { gain.clamp(0.0, 4.0) } else { 1.0 };
        self.sh.ganho.store(g.to_bits(), Relaxed);
    }

    /// Força o nível mínimo absoluto de abertura do VAD em dBFS (-80..=-10), valendo para os dois modos.
    /// Por padrão ele é calculado pela sensibilidade (≈ -61 dBFS com RNNoise, ≈ -53 sem). `NaN` desfaz.
    pub fn set_vad_threshold_db(&self, db: f32) {
        let v = if db.is_finite() { db.clamp(-80.0, -10.0) } else { f32::NAN };
        self.sh.vad_db.store(v.to_bits(), Relaxed);
    }

    /// Sensibilidade do VAD: 0.0 = menos sensível, 1.0 = mais sensível (padrão 0.7). Move todos os limiares
    /// (probabilidade de voz, energia mínima e margem sobre o piso de ruído) de forma monotônica.
    pub fn set_vad_sensitivity(&self, sensitivity: f32) {
        if sensitivity.is_finite() {
            self.sh.vad_sens.store(sensitivity.clamp(0.0, 1.0).to_bits(), Relaxed);
        }
    }

    pub fn vad_sensitivity(&self) -> f32 {
        f32_from_bits(&self.sh.vad_sens)
    }

    /// AGC do microfone: pré-AGC antes do RNNoise (traz a voz baixa para perto de -30 dBFS, até +30 dB) e AGC2 igual ao
    /// do Discord depois (pico da fala em -5 dBFS, até +50 dB, 6 dB/s, nunca amplifica ruído). Padrão: ligado.
    pub fn set_agc(&self, on: bool) {
        self.sh.agc.store(on, Relaxed);
    }

    pub fn agc(&self) -> bool {
        self.sh.agc.load(Relaxed)
    }

    /// Estatísticas de envio e de recepção (por usuário e total), para o painel de voz. Barato: copia um vetor pequeno.
    pub fn voice_stats(&self) -> VoiceStats {
        let sh = &self.sh;
        let rx = sh.rx_stats.lock().map(|g| g.clone()).unwrap_or_default();
        let falando = self.self_speaking();
        VoiceStats {
            tx: TxStats {
                level_db: f32_from_bits(&sh.nivel_db),
                pre_gain_db: f32_from_bits(&sh.pre_db),
                agc2_gain_db: f32_from_bits(&sh.agc_db),
                voice_prob: f32_from_bits(&sh.prob),
                speaking: falando,
                echo: sh.eco.load(Relaxed).then(|| {
                    let (e, a) = (f32_from_bits(&sh.eco_erle), f32_from_bits(&sh.eco_atraso));
                    EcoStats { erle_db: e.is_finite().then_some(e), delay_ms: a.is_finite().then_some(a as i32) }
                }),
            },
            total: RxTotals::from_users(&rx),
            rx,
        }
    }

    /// Ganho que o AGC2 está aplicando agora, em dB (0..=50), para depuração.
    pub fn agc_gain_db(&self) -> f32 {
        f32_from_bits(&self.sh.agc_db)
    }

    /// Entrega um pacote recebido do servidor. Pode ser chamado de qualquer thread; não bloqueia.
    pub fn push_packet(&self, user: i64, seq: u32, packet: &[u8]) {
        if packet.is_empty() || packet.len() > MAX_PACKET || self.sh.surdo.load(Relaxed) {
            return;
        }
        if self.sh.pendentes.load(Relaxed) >= MAX_PENDING {
            return;
        }
        if let Ok(mut m) = self.sh.ouvidos.lock() {
            if m.len() >= 2 * MAX_USERS && !m.contains_key(&user) {
                return;
            }
            m.insert(user, Instant::now());
        }
        self.sh.pendentes.fetch_add(1, Relaxed);
        if self.tx.send(Msg::Packet { user, seq, dados: packet.to_vec() }).is_err() {
            self.sh.pendentes.fetch_sub(1, Relaxed);
        }
    }

    /// Esquece o usuário (saiu do canal): libera decodificador e buffer.
    pub fn remove_user(&self, user: i64) {
        if let Ok(mut m) = self.sh.ouvidos.lock() {
            m.remove(&user);
        }
        let _ = self.tx.send(Msg::Remove(user));
    }

    /// Recebeu pacote deste usuário nos últimos ~250 ms.
    pub fn speaking(&self, user: i64) -> bool {
        self.sh
            .ouvidos
            .lock()
            .ok()
            .and_then(|m| m.get(&user).map(|t| t.elapsed() < SPEAKING_WINDOW))
            .unwrap_or(false)
    }

    /// Nível do microfone, 0.0..=1.0 (0 quando mudo).
    pub fn mic_level(&self) -> f32 {
        if self.sh.mudo.load(Relaxed) {
            return 0.0;
        }
        f32_from_bits(&self.sh.nivel).clamp(0.0, 1.0)
    }

    /// O VAD está aberto agora (você está falando).
    pub fn self_speaking(&self) -> bool {
        !self.sh.mudo.load(Relaxed) && self.sh.falando.load(Relaxed)
    }

    /// Falha do dispositivo (ou ausência de microfone), em português. Fica até o motor ser recriado.
    pub fn error(&self) -> Option<String> {
        self.sh.erro.lock().ok().and_then(|e| e.clone())
    }

    /// Estimativa de latência em ms, `(monitor, voz_remota)`:
    /// - monitor = bloco de captura + quadro de 20 ms + fila do monitor + antecedência da fila de saída
    ///   + 2 blocos do dispositivo de saída;
    /// - voz remota (sem contar a rede) = antecedência da fila de saída + 2 blocos de saída + atraso do
    ///   jitter buffer (3 quadros, ou 2 no modo de baixa latência).
    ///
    /// Os blocos vêm do último callback observado (por isso começa em 0 até o dispositivo falar). É estimativa:
    /// não mede o buffer interno do hardware.
    pub fn latency_estimate_ms(&self) -> (f32, f32) {
        let sh = &self.sh;
        let ms = |quadros: u32, taxa: u32| quadros as f32 * 1000.0 / taxa.max(1) as f32;
        let cb_in = ms(sh.cb_entrada.load(Relaxed), sh.taxa_entrada.load(Relaxed));
        let cb_out = ms(sh.cb_saida.load(Relaxed), sh.taxa_saida.load(Relaxed));
        let alvo = f32_from_bits(&sh.est_alvo_ms);
        let fila_mon = f32_from_bits(&sh.est_mon_ms);
        // atraso do jitter buffer: o real (maior entre os usuários ouvidos agora) ou, sem ninguém, o alvo inicial
        let real = sh.rx_stats.lock().map(|g| g.iter().map(|r| r.buffered_ms).fold(0.0, f32::max)).unwrap_or(0.0);
        let jitter = if real > 0.0 { real } else { (if sh.baixa_lat.load(Relaxed) { JB_INICIAL_BAIXA } else { JB_INICIAL }) as f32 * 20.0 };
        let monitor = cb_in + 20.0 + fila_mon + alvo + 2.0 * cb_out;
        let remota = alvo + 2.0 * cb_out + jitter;
        (monitor, remota)
    }

    /// Contadores de diagnóstico desde que o motor iniciou.
    pub fn stats(&self) -> Stats {
        let sh = &self.sh;
        Stats {
            underruns: sh.c_subfluxos.load(Relaxed),
            overruns: sh.c_sobrefluxos.load(Relaxed),
            packets_sent: sh.c_enviados.load(Relaxed),
            jitter_dropped: sh.c_descartados.load(Relaxed),
            monitor_lost: sh.c_monitor_perdidos.load(Relaxed),
            input_callback_frames: sh.cb_entrada.load(Relaxed),
            output_callback_frames: sh.cb_saida.load(Relaxed),
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.sh.parar.store(true, Relaxed);
        // espera cada thread por um tempo limitado; se uma estiver presa num driver, solta e segue
        let limite = Instant::now() + Duration::from_secs(3);
        for t in self.threads.drain(..) {
            while !t.is_finished() && Instant::now() < limite {
                thread::sleep(Duration::from_millis(5));
            }
            if t.is_finished() {
                let _ = t.join();
            }
        }
    }
}

// ───────────────────────────── testes ─────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    fn seno(freq: f32, taxa: f32, amp: f32, n: usize, inicio: usize) -> Vec<f32> {
        (0..n).map(|i| amp * (2.0 * PI * freq * (i + inicio) as f32 / taxa).sin()).collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    /// Fração da potência do sinal que está na frequência `freq` (Goertzel com janela de Hann).
    fn fracao_tom(x: &[f32], freq: f32, taxa: f32) -> f32 {
        let n = x.len();
        let w = 2.0 * PI as f64 * freq as f64 / taxa as f64;
        let (mut re, mut im, mut soma_j) = (0f64, 0f64, 0f64);
        for (i, v) in x.iter().enumerate() {
            let jan = 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos();
            soma_j += jan;
            re += *v as f64 * jan * (w * i as f64).cos();
            im += *v as f64 * jan * (w * i as f64).sin();
        }
        let amp = 2.0 * (re * re + im * im).sqrt() / soma_j;
        let potencia_tom = amp * amp / 2.0;
        let total = x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / n as f64;
        (potencia_tom / total.max(1e-12)) as f32
    }

    fn pkt(n: u8) -> Vec<u8> {
        vec![n; 4]
    }

    fn frame_num(p: Playout) -> Option<u8> {
        match p {
            Playout::Frame(d) => Some(d[0]),
            _ => None,
        }
    }

    // ── Opus ──

    #[test]
    fn opus_ida_e_volta_de_um_seno() {
        let mut p = SendPipeline::new().unwrap();
        p.set_noise_suppression(false);
        let mut u = UserStream::new().unwrap();
        let mut pacotes = Vec::new();
        let tom = seno(440.0, 48000.0, 0.5, FRAME * 30, 0);
        for q in tom.chunks(FRAME) {
            p.process(q, &mut |s, d| pacotes.push((s, d.to_vec())));
        }
        assert!(pacotes.len() >= 28, "VAD deveria abrir com um tom de -9 dBFS: {}", pacotes.len());
        assert!(pacotes.iter().all(|(_, d)| d.len() <= MAX_PACKET));
        let media = pacotes.iter().map(|(_, d)| d.len()).sum::<usize>() as f32 / pacotes.len() as f32;
        assert!(media < 190.0, "48 kbps ≈ 120 bytes por quadro, veio {media}");
        let mut saida = Vec::new();
        let mut q = vec![0f32; FRAME];
        for i in 0..pacotes.len() + 4 {
            if let Some((s, d)) = pacotes.get(i) {
                u.push(*s, d.clone());
            }
            u.render(&mut q);
            saida.extend_from_slice(&q);
        }
        // pula o começo (buffer + aquecimento) e mede o miolo
        let miolo: Vec<f32> = saida.iter().copied().skip(FRAME * 8).take(FRAME * 12).collect();
        let r = rms(&miolo);
        assert!((r - 0.5 / 2f32.sqrt()).abs() < 0.06, "rms {r}");
        assert!(fracao_tom(&miolo, 440.0, 48000.0) > 0.9);
    }

    #[test]
    fn opus_perda_usa_plc_sem_explodir() {
        let mut p = SendPipeline::new().unwrap();
        p.set_noise_suppression(false);
        let mut u = UserStream::new().unwrap();
        let mut pacotes = Vec::new();
        for q in seno(300.0, 48000.0, 0.4, FRAME * 40, 0).chunks(FRAME) {
            p.process(q, &mut |s, d| pacotes.push((s, d.to_vec())));
        }
        let mut saida = Vec::new();
        let mut q = vec![0f32; FRAME];
        for (i, (s, d)) in pacotes.iter().enumerate() {
            if i % 7 != 3 {
                u.push(*s, d.clone()); // perde 1 em cada 7
            }
            u.render(&mut q);
            saida.extend_from_slice(&q);
        }
        assert!(saida.iter().all(|v| v.is_finite() && v.abs() <= 1.5));
        let miolo = &saida[FRAME * 10..FRAME * 30];
        assert!(rms(miolo) > 0.15, "a perda não pode virar silêncio: {}", rms(miolo));
    }

    // ── VAD ──

    #[test]
    fn vad_abre_segura_e_fecha_com_hangover() {
        let mut v = Vad::new(0.7);
        for _ in 0..20 {
            assert!(!v.process_db(-80.0));
        }
        assert!(v.process_db(-20.0), "fala forte abre");
        // cai para silêncio: continua aberto durante o hangover (18 quadros = 360 ms), depois fecha
        for i in 0..Vad::HANGOVER_QUADROS {
            assert!(v.process_db(-80.0), "quadro {i} do hangover");
        }
        assert!(!v.process_db(-80.0));
    }

    #[test]
    fn vad_histerese_nao_pisca() {
        let mut v = Vad::new(0.7);
        for _ in 0..40 {
            v.process_db(-85.0); // o piso de ruído desce até o mínimo
        }
        let l = v.thresholds(false);
        assert!(l.fecha_db < l.abre_db);
        let meio = (l.abre_db + l.fecha_db) / 2.0;
        // entre o limiar de fechar e o de abrir: fechado não abre...
        for _ in 0..30 {
            assert!(!v.process_db(meio));
        }
        // ...mas aberto não fecha
        assert!(v.process_db(l.abre_db + 10.0));
        for _ in 0..100 {
            assert!(v.process_db(meio), "no meio da histerese mantém aberto");
        }
        for _ in 0..Vad::HANGOVER_QUADROS + 1 {
            v.process_db(-85.0);
        }
        assert!(!v.is_open());
    }

    #[test]
    fn vad_piso_de_ruido_adapta() {
        // ruído constante de -56 dB: um estouro de -49 dB abre um VAD novo...
        assert!(Vad::new(0.7).process_db(-49.0));
        // ...mas depois de o piso se adaptar ao ruído fechado, deixa de abrir
        let mut v = Vad::new(0.7);
        for _ in 0..3000 {
            assert!(!v.process_db(-56.0));
        }
        assert!(v.noise_floor_db() > -58.0, "piso {}", v.noise_floor_db());
        assert!(!v.process_db(-49.0));
        assert!(v.process_db(-30.0), "fala de verdade ainda abre");
        // quadros reais de ruído e de fala por RMS
        let mut v = Vad::new(0.7);
        assert!(!v.process(&vec![0.0005; FRAME]));
        assert!(v.process(&seno(200.0, 48000.0, 0.2, FRAME, 0)));
    }

    #[test]
    fn vad_sensibilidade_e_monotonica() {
        let mut anterior: Option<(Limiares, Limiares)> = None;
        for k in 0..=20 {
            let v = Vad::new(k as f32 / 20.0);
            let (a, b) = (v.thresholds(true), v.thresholds(false));
            if let Some((pa, pb)) = anterior {
                assert!(a.abre_db <= pa.abre_db && a.fecha_db <= pa.fecha_db, "energia c/ prob em {k}");
                assert!(a.prob_abre <= pa.prob_abre && a.prob_fecha <= pa.prob_fecha, "prob em {k}");
                assert!(b.abre_db <= pb.abre_db && b.fecha_db <= pb.fecha_db, "energia pura em {k}");
            }
            assert!(a.fecha_db <= a.abre_db && b.fecha_db <= b.abre_db);
            anterior = Some((a, b));
        }
        // padrão (0.7): valores pedidos
        let v = Vad::new(0.7);
        let (a, b) = (v.thresholds(true), v.thresholds(false));
        assert!((a.prob_abre - 0.355).abs() < 0.01 && (a.prob_fecha - 0.16).abs() < 0.01);
        assert!((-64.0..=-60.0).contains(&a.abre_db.max(-90.0)) || a.abre_db > -60.0);
        assert!(b.abre_db < -50.0);
        // a sensibilidade mais alta abre com sinal mais fraco
        let (mut baixa, mut alta) = (Vad::new(0.0), Vad::new(1.0));
        assert!(!baixa.process_db_prob(-50.0, Some(0.3)) && alta.process_db_prob(-50.0, Some(0.3)));
        // set_open_db sobrepõe o mínimo absoluto
        let mut v = Vad::new(0.7);
        v.set_open_db(Some(-30.0));
        assert!(!v.process_db(-40.0));
        v.set_open_db(None);
        assert!(v.process_db(-40.0));
    }

    #[test]
    fn medidor_sobe_rapido_e_desce_devagar() {
        let mut m = LevelMeter::new();
        assert!(m.update(-60.0) < 0.01);
        let alto = m.update(-6.0);
        assert!(alto > 0.85);
        let depois = m.update(-80.0);
        assert!(depois < alto && depois > 0.5, "descida suave: {depois}");
        assert!(m.update(10.0) <= 1.0);
    }

    // ── jitter buffer ──

    /// Enche e consome `n` ticks, devolvendo o que saiu.
    fn tocar(jb: &mut JitterBuffer, n: usize) -> Vec<Playout> {
        (0..n).map(|_| jb.pop()).collect()
    }

    #[test]
    fn jitter_em_ordem() {
        let mut jb = JitterBuffer::new();
        for s in 0..6u32 {
            assert_eq!(jb.push(s, pkt(s as u8)), PushOutcome::Accepted);
        }
        let v: Vec<_> = tocar(&mut jb, 6).into_iter().map(frame_num).collect();
        assert_eq!(v, (0..6).map(Some).collect::<Vec<_>>());
    }

    #[test]
    fn jitter_reordena() {
        let mut jb = JitterBuffer::new();
        for s in [0u32, 2, 1, 4, 3, 5] {
            jb.push(s, pkt(s as u8));
        }
        let v: Vec<_> = tocar(&mut jb, 6).into_iter().map(frame_num).collect();
        assert_eq!(v, (0..6).map(Some).collect::<Vec<_>>());
    }

    #[test]
    fn jitter_primeiro_pacote_fora_de_ordem() {
        let mut jb = JitterBuffer::new();
        for s in [5u32, 4, 6, 3, 7] {
            jb.push(s, pkt(s as u8));
        }
        let v: Vec<_> = tocar(&mut jb, 5).into_iter().map(frame_num).collect();
        assert_eq!(v, (3..8).map(Some).collect::<Vec<_>>());
    }

    #[test]
    fn jitter_perda_vira_lost() {
        let mut jb = JitterBuffer::new();
        for s in [0u32, 1, 3, 4, 5, 6] {
            jb.push(s, pkt(s as u8));
        }
        let v = tocar(&mut jb, 7);
        assert_eq!(frame_num(jb_clone(&v[0])), Some(0));
        assert_eq!(v[2], Playout::Lost, "seq 2 perdido");
        assert_eq!(frame_num(jb_clone(&v[3])), Some(3));
        assert_eq!(frame_num(jb_clone(&v[6])), Some(6));
    }

    fn jb_clone(p: &Playout) -> Playout {
        match p {
            Playout::Frame(d) => Playout::Frame(d.clone()),
            Playout::Lost => Playout::Lost,
            Playout::Silence => Playout::Silence,
        }
    }

    #[test]
    fn jitter_buraco_grande_e_saltado() {
        let mut jb = JitterBuffer::new();
        for s in [0u32, 1, 2, 40, 41, 42] {
            jb.push(s, pkt(s as u8));
        }
        let v = tocar(&mut jb, 8);
        let perdidos = v.iter().filter(|p| **p == Playout::Lost).count();
        assert!(perdidos <= 2, "não gera 37 PLCs: {perdidos}");
        assert!(v.iter().any(|p| matches!(p, Playout::Frame(d) if d[0] == 42)));
    }

    #[test]
    fn jitter_duplicado_e_atrasado() {
        let mut jb = JitterBuffer::new();
        for s in 0..4u32 {
            jb.push(s, pkt(s as u8));
        }
        assert_eq!(jb.push(2, pkt(2)), PushOutcome::Duplicate);
        let v: Vec<_> = tocar(&mut jb, 4).into_iter().map(frame_num).collect();
        assert_eq!(v, vec![Some(0), Some(1), Some(2), Some(3)]);
        // seq 1 já passou
        assert_eq!(jb.push(1, pkt(1)), PushOutcome::Late);
        assert_eq!(jb.push(3, pkt(3)), PushOutcome::Late);
    }

    #[test]
    fn jitter_virada_do_contador_u32() {
        let mut jb = JitterBuffer::new();
        let base = u32::MAX - 2;
        for k in [0u32, 2, 1, 3, 5, 4] {
            let s = base.wrapping_add(k);
            assert_eq!(jb.push(s, pkt(k as u8)), PushOutcome::Accepted, "seq {s}");
        }
        let v: Vec<_> = tocar(&mut jb, 6).into_iter().map(frame_num).collect();
        assert_eq!(v, (0..6).map(Some).collect::<Vec<_>>());
        // atrasado do outro lado da virada
        assert_eq!(jb.push(u32::MAX, pkt(9)), PushOutcome::Late);
        // e o seguinte continua normal
        assert_eq!(jb.push(base.wrapping_add(6), pkt(6)), PushOutcome::Accepted);
    }

    #[test]
    fn jitter_silencio_depois_de_esvaziar_e_retoma_contiguo() {
        let mut jb = JitterBuffer::new();
        for s in 0..4u32 {
            jb.push(s, pkt(s as u8));
        }
        let v = tocar(&mut jb, 4 + 1 + JB_PLC_QUADROS as usize + 3);
        assert!(v[..4].iter().all(|p| matches!(p, Playout::Frame(_))));
        assert_eq!(v[4], Playout::Lost, "PLC suaviza o fim");
        assert_eq!(v.last(), Some(&Playout::Silence));
        let alvo = jb.target();
        // pausa longa (VAD fechado) e a fala volta com seq contíguo
        tocar(&mut jb, 40);
        for s in 4..9u32 {
            jb.push(s, pkt(s as u8));
        }
        let v: Vec<_> = tocar(&mut jb, 5).into_iter().filter_map(frame_num).collect();
        assert_eq!(v, vec![4, 5, 6, 7, 8]);
        assert_eq!(jb.target(), alvo, "pausa de fala não é subfluxo");
    }

    #[test]
    fn jitter_alvo_sobe_no_subfluxo_e_desce_na_calma() {
        let mut jb = JitterBuffer::new();
        assert_eq!(jb.target(), 3);
        // fala com buffer raso: chega 1 por tick, esvazia e o pacote seguinte chega 1 tick atrasado
        let mut seq = 0u32;
        for s in 0..3 {
            jb.push(seq + s, pkt(0));
        }
        seq += 3;
        tocar(&mut jb, 3);
        let _ = jb.pop(); // vazio: PLC
        jb.push(seq, pkt(0)); // chegou no tick seguinte
        let p = jb.pop();
        assert!(matches!(p, Playout::Frame(_)));
        assert_eq!(jb.target(), 5, "subfluxo aumenta o atraso em 2 quadros (40 ms)");
        // muita calma (fala contínua e estável) diminui devagar
        seq += 1;
        for _ in 0..(JB_CALMA as usize * 3) {
            jb.push(seq, pkt(0));
            seq += 1;
            jb.pop();
        }
        assert!(jb.target() < 5 && jb.target() >= JB_MIN);
    }

    #[test]
    fn jitter_limites_do_alvo() {
        let mut jb = JitterBuffer::new();
        for _ in 0..20 {
            jb.sobe_alvo();
        }
        assert_eq!(jb.target(), JB_MAX);
        assert_eq!(JB_MIN * 20, 40);
        assert_eq!(JB_MAX * 20, 240);
    }

    #[test]
    fn jitter_recomeco_do_remetente() {
        let mut jb = JitterBuffer::new();
        for s in 1000..1004u32 {
            jb.push(s, pkt(1));
        }
        tocar(&mut jb, 4);
        // motor do remetente recriado: seq muito diferente
        assert_eq!(jb.push(5_000_000, pkt(9)), PushOutcome::Resync);
        for s in 5_000_001..5_000_004u32 {
            assert_eq!(jb.push(s, pkt(9)), PushOutcome::Accepted);
        }
        let v: Vec<_> = tocar(&mut jb, 4).into_iter().filter_map(frame_num).collect();
        assert_eq!(v.len(), 4);
        // recomeço "perto" (seq voltou para um valor baixo): vários atrasados seguidos ressincronizam
        let mut resync = false;
        for s in 0..JB_ATRASADOS_RESYNC + 1 {
            if jb.push(5_000_000 - 50 + s, pkt(2)) == PushOutcome::Resync {
                resync = true;
            }
        }
        assert!(resync);
    }

    #[test]
    fn jitter_rajada_nao_acumula_latencia() {
        let mut jb = JitterBuffer::new();
        for s in 0..30u32 {
            jb.push(s, pkt(s as u8));
        }
        let p = jb.pop();
        assert!(frame_num(p).is_some());
        assert!(jb.depth() <= jb.target() + JB_EXCESSO, "profundidade {}", jb.depth());
    }

    #[test]
    fn jitter_limite_de_memoria() {
        let mut jb = JitterBuffer::new();
        for s in 0..200u32 {
            jb.push(s, pkt(0));
        }
        assert!(jb.pacotes.len() <= JB_MAX_PACOTES);
    }

    // ── mixer ──

    #[test]
    fn mixer_volume_e_soma() {
        let mut m = Mixer::new(4);
        m.begin();
        m.add(&[0.2, 0.2, 0.2, 0.2], 1.0);
        m.add(&[0.1, -0.1, 0.0, 0.1], 0.5);
        let o = m.finish();
        assert!((o[0] - 0.25).abs() < 1e-6);
        assert!((o[1] - 0.15).abs() < 1e-6);
        // volume 0 cala, volume acima de 2 é limitado a 2
        m.begin();
        m.add(&[0.1; 4], 0.0);
        assert!(m.finish().iter().all(|x| *x == 0.0));
        m.begin();
        m.add(&[0.1; 4], 50.0);
        assert!((m.finish()[0] - 0.2).abs() < 1e-6);
    }

    #[test]
    fn limitador_nunca_estoura_e_e_monotono() {
        assert_eq!(soft_limit(0.5), 0.5);
        assert_eq!(soft_limit(-0.7), -0.7);
        let mut anterior = soft_limit(-100.0);
        let mut x = -100.0f32;
        while x < 100.0 {
            let y = soft_limit(x);
            assert!(y.abs() <= 1.0, "{x} → {y}");
            assert!(y >= anterior - 1e-6, "monotônico em {x}");
            anterior = y;
            x += 0.013;
        }
        assert_eq!(soft_limit(f32::NAN), 0.0);
        assert_eq!(soft_limit(f32::INFINITY), 0.0);
        assert!(soft_limit(3.0) > 0.95);
        // contínuo no joelho
        assert!((soft_limit(0.7001) - soft_limit(0.6999)).abs() < 1e-3);
    }

    #[test]
    fn mixer_com_limitador_em_pico() {
        let mut m = Mixer::new(FRAME);
        m.begin();
        for _ in 0..5 {
            m.add(&seno(500.0, 48000.0, 0.9, FRAME, 0), 2.0);
        }
        assert!(m.finish().iter().all(|x| x.abs() <= 1.0));
    }

    // ── reamostrador ──

    fn reamostra(x: &[f32], de: u32, para: u32, pedaco: usize) -> Vec<f32> {
        let mut r = Resampler::new(de, para);
        let mut o = Vec::new();
        for c in x.chunks(pedaco) {
            r.process(c, &mut o);
        }
        o
    }

    #[test]
    fn reamostrador_mesma_taxa_copia() {
        let x = seno(1000.0, 48000.0, 0.5, 1000, 0);
        assert_eq!(reamostra(&x, 48000, 48000, 333), x);
    }

    #[test]
    fn reamostrador_sobe_44100_para_48000() {
        let x = seno(1000.0, 44100.0, 0.5, 44100, 0);
        let y = reamostra(&x, 44100, 48000, 441);
        assert!((y.len() as i64 - 48000).abs() < 40, "{}", y.len());
        let miolo = &y[2000..40000];
        assert!((rms(miolo) - 0.5 / 2f32.sqrt()).abs() < 0.01, "ganho {}", rms(miolo));
        assert!(fracao_tom(miolo, 1000.0, 48000.0) > 0.99);
    }

    #[test]
    fn reamostrador_independe_do_tamanho_do_pedaco() {
        let x = seno(700.0, 44100.0, 0.5, 20000, 0);
        let a = reamostra(&x, 44100, 48000, 20000);
        let b = reamostra(&x, 44100, 48000, 97);
        let n = a.len().min(b.len());
        assert!((a.len() as i64 - b.len() as i64).abs() <= 1);
        for i in 0..n {
            assert!((a[i] - b[i]).abs() < 1e-4, "difere em {i}");
        }
    }

    #[test]
    fn reamostrador_desce_e_corta_acima_de_nyquist() {
        // 48k → 16k: 1 kHz passa, 10 kHz (acima de 8 kHz) some
        let passa = reamostra(&seno(1000.0, 48000.0, 0.5, 48000, 0), 48000, 16000, 480);
        let corta = reamostra(&seno(10000.0, 48000.0, 0.5, 48000, 0), 48000, 16000, 480);
        assert!((passa.len() as i64 - 16000).abs() < 20);
        assert!(rms(&passa[1000..15000]) > 0.33);
        assert!(rms(&corta[1000..15000]) < 0.02, "alias {}", rms(&corta[1000..15000]));
        // 48k → 44.1k mantém o tom
        let y = reamostra(&seno(2000.0, 48000.0, 0.5, 48000, 0), 48000, 44100, 960);
        assert!(fracao_tom(&y[2000..40000], 2000.0, 44100.0) > 0.99);
    }

    #[test]
    fn reamostrador_continuo_sem_emendas() {
        // sem descontinuidade entre pedaços: a diferença entre amostras vizinhas fica pequena
        let x = seno(300.0, 44100.0, 0.5, 8000, 0);
        let y = reamostra(&x, 44100, 48000, 128);
        let salto = y.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0f32, f32::max);
        assert!(salto < 0.05, "salto {salto}");
    }

    // ── pipeline de envio ──

    #[test]
    fn envio_so_manda_com_vad_aberto_e_seq_contiguo() {
        let mut p = SendPipeline::new().unwrap();
        p.set_noise_suppression(false);
        let mut pacotes: Vec<(u32, Vec<u8>)> = Vec::new();
        let mut env = |pacotes: &mut Vec<(u32, Vec<u8>)>, q: &[f32]| {
            p.process(q, &mut |s, d| pacotes.push((s, d.to_vec())));
        };
        let silencio = vec![0.0f32; FRAME];
        for _ in 0..40 {
            env(&mut pacotes, &silencio);
        }
        assert!(pacotes.is_empty(), "silêncio não envia nada");
        for q in seno(250.0, 48000.0, 0.3, FRAME * 25, 0).chunks(FRAME) {
            env(&mut pacotes, q);
        }
        let durante_fala = pacotes.len();
        assert!(durante_fala >= 25 + PREROLL_FRAMES, "inclui a pré-rolagem: {durante_fala}");
        for _ in 0..60 {
            env(&mut pacotes, &silencio);
        }
        let total = pacotes.len();
        // hangover de 10 quadros (+1 do quadro de fechamento) e para
        assert!(total - durante_fala >= Vad::HANGOVER_QUADROS as usize, "hangover: {}", total - durante_fala);
        assert!(total - durante_fala <= Vad::HANGOVER_QUADROS as usize + 2);
        for w in pacotes.windows(2) {
            assert_eq!(w[1].0, w[0].0.wrapping_add(1), "seq contíguo");
        }
    }

    #[test]
    fn envio_ganho_zero_cala_e_nivel_reflete() {
        let mut p = SendPipeline::new().unwrap();
        p.set_noise_suppression(false);
        p.gain = 0.0;
        let mut n = 0;
        for q in seno(250.0, 48000.0, 0.3, FRAME * 10, 0).chunks(FRAME) {
            p.process(q, &mut |_, _| n += 1);
        }
        assert_eq!(n, 0);
        assert!(p.level() < 0.01 && !p.speaking());
        p.gain = 1.0;
        p.process(&seno(250.0, 48000.0, 0.3, FRAME, 0), &mut |_, _| n += 1);
        assert!(p.level() > 0.5 && p.speaking());
    }

    // ── caminho completo em memória: codificador → rede embaralhada → jitter → decoder → mixer ──

    #[test]
    fn integracao_em_memoria_dois_usuarios() {
        // dois falantes (440 Hz e 880 Hz) mandam 1 s de fala
        let gera = |freq: f32| {
            let mut p = SendPipeline::new().unwrap();
            p.set_noise_suppression(false);
            let mut pacotes = Vec::new();
            for q in seno(freq, 48000.0, 0.4, FRAME * 50, 0).chunks(FRAME) {
                p.process(q, &mut |s, d| pacotes.push((s, d.to_vec())));
            }
            pacotes
        };
        let (mut a, mut b) = (gera(440.0), gera(880.0));
        let (na, nb) = (a.len(), b.len());
        // rede: troca vizinhos (reordenação), duplica um pacote e perde outro
        for i in (1..a.len() - 1).step_by(9) {
            a.swap(i, i + 1);
        }
        let dup = a[10].clone();
        a.insert(11, dup);
        b.remove(20);

        let mut ua = UserStream::new().unwrap();
        let mut ub = UserStream::new().unwrap();
        let mut mixer = Mixer::new(FRAME);
        let (mut qa, mut qb) = (vec![0f32; FRAME], vec![0f32; FRAME]);
        let mut saida = Vec::new();
        let (mut ia, mut ib) = (0, 0);
        // cada tick: chegam ~1 pacote de cada, toca 20 ms
        for _ in 0..na.max(nb) + 6 {
            if ia < a.len() {
                ua.push(a[ia].0, a[ia].1.clone());
                ia += 1;
            }
            if ib < b.len() {
                ub.push(b[ib].0, b[ib].1.clone());
                ib += 1;
            }
            mixer.begin();
            if ua.render(&mut qa) {
                mixer.add(&qa, 1.0);
            }
            if ub.render(&mut qb) {
                mixer.add(&qb, 0.5);
            }
            saida.extend_from_slice(mixer.finish());
        }
        assert!(saida.iter().all(|x| x.abs() <= 1.0));
        let miolo = &saida[FRAME * 15..FRAME * 35];
        let (ea, eb) = (fracao_tom(miolo, 440.0, 48000.0), fracao_tom(miolo, 880.0, 48000.0));
        assert!(ea > 0.6 && eb > 0.05 && ea + eb > 0.9, "440 Hz {ea}, 880 Hz {eb}");
        assert!(ea > eb * 2.0, "o de volume 0.5 deve estar mais baixo (≈4× em potência)");
        // o jitter buffer entregou em ordem: nada de saída enorme ou travada em silêncio
        assert!(rms(miolo) > 0.2);
    }

    // ── motor completo sem hardware: filas em memória no lugar do cpal ──

    #[test]
    fn motor_em_memoria_captura_a_44100_e_toca_a_44100() {
        let sh = Arc::new(Shared::new());
        let (mut cap_prod, cap_cons) = RingBuffer::<f32>::new(RING_SAMPLES);
        let (play_prod, mut play_cons) = RingBuffer::<f32>::new(RING_SAMPLES);
        let (pk_tx, pk_rx) = mpsc::channel::<(u32, Vec<u8>)>();
        let send = Box::new(move |s: u32, d: &[u8]| {
            let _ = pk_tx.send((s, d.to_vec()));
        });
        let eng = Engine::liga_threads(sh, Some((cap_cons, 44100)), (play_prod, 44100), send).unwrap();
        eng.set_noise_suppression(false); // tom puro seria tratado como ruído
        eng.set_user_volume(7, 1.0);

        // 1,2 s de microfone a 44,1 kHz em tempo real (10 ms por vez), loopback dos pacotes como usuário 7
        let tom = seno(1000.0, 44100.0, 0.4, 44100 * 6 / 5, 0);
        let mut tocado: Vec<f32> = Vec::new();
        let mut recebidos = 0;
        let mut viu_falando = false;
        let mut viu_nivel = false;
        for bloco in tom.chunks(441) {
            let _ = cap_prod.push_partial_slice(bloco);
            thread::sleep(Duration::from_millis(10));
            while let Ok((s, d)) = pk_rx.try_recv() {
                recebidos += 1;
                eng.push_packet(7, s, &d);
            }
            // o "dispositivo" consome em tempo real: 10 ms (441 amostras) por bloco; fila vazia = silêncio
            for _ in 0..441 {
                tocado.push(play_cons.pop().unwrap_or(0.0));
            }
            viu_falando |= eng.speaking(7);
            viu_nivel |= eng.self_speaking() && eng.mic_level() > 0.3;
        }
        assert!(recebidos > 40, "pacotes enviados: {recebidos}");
        assert!(viu_falando && viu_nivel);
        assert!(eng.error().is_none());
        // a saída tem o tom de 1 kHz a 44,1 kHz
        assert!(tocado.len() > 44100 / 2, "tocou {} amostras", tocado.len());
        let ini = tocado.len() / 3;
        let cauda = &tocado[ini..ini + 4000];
        let pico = cauda.iter().fold(0f32, |m, x| m.max(x.abs()));
        assert!(pico > 0.2 && pico <= 1.0, "pico {pico}");
        assert!(fracao_tom(cauda, 1000.0, 44100.0) > 0.9);

        // silêncio do microfone: o VAD fecha e para de mandar
        let silencio = vec![0f32; 441];
        for _ in 0..60 {
            let _ = cap_prod.push_partial_slice(&silencio);
            thread::sleep(Duration::from_millis(10));
        }
        while pk_rx.try_recv().is_ok() {}
        for _ in 0..20 {
            let _ = cap_prod.push_partial_slice(&silencio);
            thread::sleep(Duration::from_millis(10));
        }
        assert!(pk_rx.try_recv().is_err(), "silêncio não envia");
        assert!(!eng.self_speaking());
        thread::sleep(Duration::from_millis(300));
        assert!(!eng.speaking(7), "sem pacotes há 250 ms");

        // mudo: nem com tom manda; surdo: não toca nada
        eng.set_muted(true);
        for bloco in tom.chunks(441).take(30) {
            let _ = cap_prod.push_partial_slice(bloco);
            thread::sleep(Duration::from_millis(10));
        }
        assert!(pk_rx.try_recv().is_err());
        assert_eq!(eng.mic_level(), 0.0);
        eng.set_deafened(true);
        eng.push_packet(7, 123, &[1, 2, 3]);
        assert!(!eng.speaking(7));
        drop(eng); // não trava
    }

    #[test]
    fn motor_descarta_usuario_e_pacotes_invalidos() {
        let sh = Arc::new(Shared::new());
        let (play_prod, _play_cons) = RingBuffer::<f32>::new(RING_SAMPLES);
        let eng = Engine::liga_threads(sh, None, (play_prod, 48000), Box::new(|_, _| {})).unwrap();
        eng.push_packet(1, 0, &[]);
        eng.push_packet(1, 0, &vec![0u8; MAX_PACKET + 1]);
        assert!(!eng.speaking(1));
        eng.push_packet(1, 0, &[0xff, 0xfe, 0xfd]); // lixo que não é Opus: não pode derrubar nada
        assert!(eng.speaking(1));
        eng.remove_user(1);
        assert!(!eng.speaking(1));
        eng.set_user_volume(1, f32::NAN);
        for i in 0..2000u32 {
            eng.push_packet(2, i, &[0x08, 1, 2, 3]); // rajada acima do limite: só descarta
        }
        thread::sleep(Duration::from_millis(100));
    }

    // ── supressão de ruído ──

    /// Ruído branco determinístico (LCG), amplitude de pico `amp`.
    fn ruido_branco(amp: f32, n: usize, semente: u32) -> Vec<f32> {
        let mut x = semente;
        (0..n)
            .map(|_| {
                x = x.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                ((x >> 8) as f32 / (1u32 << 24) as f32 * 2.0 - 1.0) * amp
            })
            .collect()
    }

    /// "Fala" sintética: pitch de 120 Hz com harmônicos sob envelope de formantes, modulada a 4 Hz.
    fn fala_sintetica(n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let t = i as f32 / 48000.0;
                let mut v = 0.0;
                for h in 1..40 {
                    let f = 120.0 * h as f32;
                    let env = (-((f - 700.0) / 400.0).powi(2)).exp() + 0.6 * (-((f - 1800.0) / 600.0).powi(2)).exp();
                    v += env / h as f32 * (2.0 * PI * f * t).sin();
                }
                let sil = 0.5 + 0.5 * (2.0 * PI * 4.0 * t).sin().abs();
                v * 0.25 * sil
            })
            .collect()
    }

    /// Passa o sinal pelo pipeline e devolve o áudio processado e a probabilidade/estado do último quadro.
    fn processa(p: &mut SendPipeline, x: &[f32]) -> Vec<f32> {
        let mut o = Vec::new();
        for q in x.chunks_exact(FRAME) {
            p.process(q, &mut |_, _| {});
            o.extend_from_slice(p.last_frame());
        }
        o
    }

    /// Ruído "rosa" simples (passa-baixa de 1 polo sobre ruído branco), mais parecido com ruído de ambiente.
    fn ruido_rosa(amp: f32, n: usize, semente: u32) -> Vec<f32> {
        let mut y = 0.0f32;
        ruido_branco(amp, n, semente)
            .into_iter()
            .map(|v| {
                y = 0.95 * y + v;
                y
            })
            .collect()
    }

    fn passo_max(x: &[f32]) -> f32 {
        x.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max)
    }

    #[test]
    fn denoiser_reduz_ruido_e_preserva_voz() {
        let n = FRAME * 300;
        // ruído branco (pico 0.03) e rosa (rms ~0.18): bem menos energia depois de estabilizar (~4 s)
        for (nome, x) in [("branco", ruido_branco(0.03, n, 2)), ("rosa", ruido_rosa(0.1, n, 5))] {
            let mut p = SendPipeline::new().unwrap();
            let o = processa(&mut p, &x);
            let (ent, sai) = (rms(&x[FRAME * 200..]), rms(&o[FRAME * 200..]));
            assert!(sai < ent * 0.3, "{nome}: {ent} → {sai} (queda menor que 10 dB)");
            assert!(o.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
        }
        // sinais "de voz" (tom puro e fala sintética harmônica) passam quase intactos
        for (nome, x) in [("tom", seno(1000.0, 48000.0, 0.3, n, 0)), ("fala", fala_sintetica(n))] {
            let mut p = SendPipeline::new().unwrap();
            let o = processa(&mut p, &x);
            let (ent, sai) = (rms(&x[FRAME * 50..]), rms(&o[FRAME * 50..]));
            assert!(sai > ent * 0.7, "{nome} não pode ser engolido: {ent} → {sai}");
        }
    }

    #[test]
    fn denoiser_desligado_nao_mexe_no_sinal() {
        let x = seno(500.0, 48000.0, 0.3, FRAME * 10, 0);
        let mut p = SendPipeline::new().unwrap();
        p.set_noise_suppression(false);
        assert!(!p.noise_suppression());
        let o = processa(&mut p, &x);
        // sem RNNoise, com ganho unitário e AGC sem o que amplificar, só a passa-alta de 80 Hz mexe no sinal
        let mut esperado = x.clone();
        PassaAlta::new().process(&mut esperado);
        assert_eq!(o, esperado);
    }

    #[test]
    fn denoiser_alternar_nao_gera_nan_nem_salto() {
        let n = FRAME * 120;
        let voz = fala_sintetica(n);
        let amb = ruido_rosa(0.02, n, 9);
        let x: Vec<f32> = voz.iter().zip(&amb).map(|(a, b)| a + b).collect();
        // referência: o maior passo entre amostras vizinhas com cada modo fixo
        let mut base = 0f32;
        for ligado in [true, false] {
            let mut p = SendPipeline::new().unwrap();
            p.set_noise_suppression(ligado);
            base = base.max(passo_max(&processa(&mut p, &x)[FRAME * 5..]));
        }
        // alternando a cada 3 quadros
        let mut p = SendPipeline::new().unwrap();
        let mut o = Vec::new();
        for (k, q) in x.chunks_exact(FRAME).enumerate() {
            p.set_noise_suppression((k / 3) % 2 == 0);
            p.process(q, &mut |_, _| {});
            o.extend_from_slice(p.last_frame());
        }
        assert!(o.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
        let alt = passo_max(&o[FRAME * 5..]);
        assert!(alt <= base * 2.0 + 0.02, "salto ao alternar: {alt} (referência {base})");
    }

    #[test]
    fn vad_com_probabilidade_de_voz() {
        let mut v = Vad::new(0.7);
        // energia alta mas probabilidade baixa (ruído forte): não abre
        for _ in 0..10 {
            assert!(!v.process_db_prob(-20.0, Some(0.1)));
        }
        assert!(v.process_db_prob(-20.0, Some(0.9)), "voz de verdade abre");
        // histerese: entre prob_fecha (0,16) e prob_abre (0,355) mantém aberto
        for _ in 0..30 {
            assert!(v.process_db_prob(-20.0, Some(0.25)));
        }
        // probabilidade abaixo de prob_fecha deixa o hangover correr e fecha
        for _ in 0..Vad::HANGOVER_QUADROS {
            assert!(v.process_db_prob(-20.0, Some(0.05)));
        }
        assert!(!v.process_db_prob(-20.0, Some(0.05)));
        // sem probabilidade (denoiser desligado) é só energia, como antes
        assert!(v.process_db_prob(-20.0, None));
    }

    #[test]
    fn denoiser_segura_ruido_forte_que_o_vad_por_energia_abriria() {
        // ventilador: zumbido de 60 Hz com harmônicos + ruído rosa, rms ≈ -23 dBFS
        let zumbido = |i: usize| -> f32 {
            let t = i as f32 / 48000.0;
            (0..8).map(|h| 0.05 / (h + 1) as f32 * (2.0 * PI * 60.0 * (h + 1) as f32 * t).sin()).sum()
        };
        let ruido: Vec<f32> =
            ruido_rosa(0.03, FRAME * 300, 4).iter().enumerate().map(|(i, r)| r + zumbido(i)).collect();
        // pacotes enviados nos últimos 100 quadros (depois de o RNNoise se adaptar)
        let conta = |ligado: bool| {
            let mut p = SendPipeline::new().unwrap();
            p.set_noise_suppression(ligado);
            let mut n = 0;
            for (k, q) in ruido.chunks_exact(FRAME).enumerate() {
                p.process(q, &mut |_, _| n += (k >= 200) as usize);
            }
            n
        };
        assert!(conta(false) >= 99, "sem supressão o ruído alto abre o VAD");
        assert!(conta(true) < 5, "com supressão o VAD quase não abre: {}", conta(true));
        // e a fala sintética continua passando
        let fala = fala_sintetica(FRAME * 80);
        let mut p = SendPipeline::new().unwrap();
        let mut n = 0;
        for q in fala.chunks_exact(FRAME) {
            p.process(q, &mut |_, _| n += 1);
        }
        assert!(n > 40, "fala bloqueada pelo VAD+RNNoise: {n}");
    }

    // ── monitor ("ouvir meu microfone") ──

    struct Banco {
        eng: Engine,
        cap: Producer<f32>,
        play: Consumer<f32>,
        pacotes: Receiver<(u32, Vec<u8>)>,
        /// Amostras por passo de simulação (10 ms no modo normal, 5 ms no de baixa latência).
        passo: usize,
    }

    fn banco() -> Banco {
        banco_cfg(false, false)
    }

    /// `baixa`: modo de baixa latência (simula callbacks de 256 quadros); `send_vazio`: como o motor de
    /// teste das configurações, cujo `send` não faz nada.
    fn banco_cfg(baixa: bool, send_vazio: bool) -> Banco {
        let sh = Arc::new(Shared::new());
        sh.baixa_lat.store(baixa, Relaxed);
        if baixa {
            sh.cb_saida.store(256, Relaxed);
            sh.cb_entrada.store(256, Relaxed);
        }
        let (cap, cap_cons) = RingBuffer::<f32>::new(RING_SAMPLES);
        let (play_prod, play) = RingBuffer::<f32>::new(RING_SAMPLES);
        let (tx, pacotes) = mpsc::channel::<(u32, Vec<u8>)>();
        let send: Box<dyn FnMut(u32, &[u8]) + Send> = if send_vazio {
            Box::new(|_, _| {})
        } else {
            Box::new(move |s: u32, d: &[u8]| {
                let _ = tx.send((s, d.to_vec()));
            })
        };
        let eng = Engine::liga_threads(sh, Some((cap_cons, 48000)), (play_prod, 48000), send).unwrap();
        eng.set_noise_suppression(false);
        Banco { eng, cap, play, pacotes, passo: if baixa { 240 } else { 480 } }
    }

    impl Banco {
        /// Alimenta o "microfone" em tempo real e consome a saída como um dispositivo faria.
        fn roda(&mut self, x: &[f32], saida: &mut Vec<f32>) {
            for b in x.chunks(self.passo) {
                let _ = self.cap.push_partial_slice(b);
                thread::sleep(Duration::from_micros(self.passo as u64 * 1000 / 48));
                for _ in 0..self.passo {
                    saida.push(self.play.pop().unwrap_or(0.0));
                }
            }
        }
    }

    /// Silêncio (0,3 s), 1 s de tom, silêncio; devolve a saída do monitor e o instante (em amostras) do começo do tom.
    fn roda_monitor_com_tom(b: &mut Banco, tom: &[f32]) -> Vec<f32> {
        let mut saida = Vec::new();
        b.roda(&vec![0.0; 14400], &mut saida);
        assert!(b.pacotes.try_recv().is_err());
        assert!(saida.iter().all(|v| v.abs() < 1e-9), "monitor com VAD fechado deve ficar mudo");
        b.roda(tom, &mut saida);
        b.roda(&vec![0.0; 14400], &mut saida);
        saida
    }

    #[test]
    fn monitor_devolve_o_pcm_processado() {
        let mut b = banco();
        assert!(!b.eng.monitoring() && !b.eng.noise_suppression());
        b.eng.set_monitor(true);
        assert!(b.eng.monitoring());
        let tom = seno(600.0, 48000.0, 0.3, 48000, 0);
        let saida = roda_monitor_com_tom(&mut b, &tom);
        // o envio continua normal (Opus), mas o monitor não depende dele
        assert!(b.pacotes.try_iter().count() > 40);
        // com ganho unitário (AGC sem o que amplificar) e sem RNNoise o PCM processado é o próprio tom
        let primeiro = |x: &[f32]| x.iter().position(|v| v.abs() > 1e-4).expect("há som");
        let k = primeiro(&saida) - primeiro(&tom);
        let mut esperado = tom.clone();
        PassaAlta::new().process(&mut esperado); // a passa-alta de 80 Hz é a única coisa que mexe no tom
        let maior = (0..FRAME * 25).map(|i| (saida[k + i] - esperado[i]).abs()).fold(0f32, f32::max);
        assert!(maior < 1e-6, "o monitor tocou algo diferente do PCM processado (dif máx {maior})");
        let atraso_ms = (k as f32 - 14400.0) / 48.0;
        eprintln!("atraso do monitor (modo normal, microfone → fila de saída): {atraso_ms:.0} ms");
        assert!(atraso_ms < 120.0, "atraso do monitor {atraso_ms} ms");
        assert_eq!(b.eng.stats().monitor_lost, 0, "{:?}", b.eng.stats());
    }

    #[test]
    fn monitor_funciona_com_send_vazio_e_em_baixa_latencia() {
        // igual ao motor de teste das configurações (send vazio) e no modo de baixa latência
        for baixa in [false, true] {
            let mut b = banco_cfg(baixa, true);
            b.eng.set_monitor(true);
            let tom = seno(600.0, 48000.0, 0.3, 48000, 0);
            let saida = roda_monitor_com_tom(&mut b, &tom);
            let primeiro = |x: &[f32]| x.iter().position(|v| v.abs() > 1e-4).expect("há som");
            let k = primeiro(&saida) - primeiro(&tom);
            if !baixa {
                let mut esperado = tom.clone();
                PassaAlta::new().process(&mut esperado);
                let maior = (0..FRAME * 25).map(|i| (saida[k + i] - esperado[i]).abs()).fold(0f32, f32::max);
                assert!(maior < 1e-6, "conteúdo diferente (dif {maior})");
            } else {
                // 5 ms de granularidade num teste com threads e build de depuração: pode haver uma falta de
                // dado ocasional (que o motor aprende), então só confere o tom no meio e a continuidade
                let meio = &saida[14400 + 20000..14400 + 28000];
                assert!(fracao_tom(meio, 600.0, 48000.0) > 0.8 && rms(meio) > 0.15, "tom no meio: rms {} fração {} stats {:?} est {:?}", rms(meio), fracao_tom(meio, 600.0, 48000.0), b.eng.stats(), b.eng.latency_estimate_ms());
            }
            let atraso_ms = (k as f32 - 14400.0) / 48.0;
            eprintln!("atraso do monitor (baixa latência = {baixa}): {atraso_ms:.0} ms");
            assert!(atraso_ms < if baixa { 100.0 } else { 120.0 }, "baixa={baixa}: atraso {atraso_ms} ms");
            if baixa {
                assert!(atraso_ms < 80.0, "meta do modo de baixa latência: {atraso_ms} ms");
            }
            assert!(b.eng.stats().monitor_lost <= if baixa { 6 } else { 0 }, "baixa={baixa}: {:?}", b.eng.stats());
            // o fim da fala não pode deixar degrau: passo máximo de um seno de 0,3 a 600 Hz é 0,024
            assert!(passo_max(&saida) < 0.05, "baixa={baixa}: degrau {}", passo_max(&saida));
        }
    }

    #[test]
    fn latencia_estimada_cai_no_modo_baixo() {
        let normal = banco_cfg(false, true);
        normal.eng.sh.cb_entrada.store(2048, Relaxed);
        normal.eng.sh.cb_saida.store(2048, Relaxed);
        let baixo = banco_cfg(true, true);
        thread::sleep(Duration::from_millis(60)); // o mixer publica antecedência e fila do monitor
        let (mn, rn) = normal.eng.latency_estimate_ms();
        let (mb, rb) = baixo.eng.latency_estimate_ms();
        eprintln!("latência estimada (monitor, voz): normal ({mn:.0}, {rn:.0}) ms; baixa ({mb:.0}, {rb:.0}) ms");
        assert!(mb < mn * 0.6 && rb < rn * 0.6, "baixa {mb}/{rb} vs normal {mn}/{rn}");
        assert!(mb < 100.0, "meta ≤ ~100 ms no monitor: {mb}");
    }

    #[test]
    fn voz_remota_em_baixa_latencia_toca_o_tom() {
        let mut b = banco_cfg(true, false);
        let mut p = pipeline_energia();
        let mut pacotes = Vec::new();
        for q in seno(500.0, 48000.0, 0.4, FRAME * 60, 0).chunks_exact(FRAME) {
            p.process(q, &mut |s, d| pacotes.push((s, d.to_vec())));
        }
        let mut saida = Vec::new();
        // um pacote por 20 ms de simulação (4 passos de 5 ms), consumindo a saída em tempo real
        for (s, d) in &pacotes {
            b.eng.push_packet(9, *s, d);
            b.roda(&vec![0.0; 960], &mut saida);
        }
        let ini = saida.len() / 3;
        let janela = &saida[ini..ini + 8000];
        assert!(rms(janela) > 0.2, "rms {}", rms(janela));
        assert!(fracao_tom(janela, 500.0, 48000.0) > 0.9);
        assert!(saida.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
    }

    #[test]
    fn monitor_respeita_mudo_e_surdo() {
        let tom = seno(600.0, 48000.0, 0.3, 24000, 0);
        let nada = |s: &[f32]| s.iter().all(|v| v.abs() < 1e-9);

        // mudo: não envia nem toca
        let mut b = banco();
        b.eng.set_monitor(true);
        b.eng.set_muted(true);
        let mut saida = Vec::new();
        b.roda(&tom, &mut saida);
        assert!(b.pacotes.try_recv().is_err() && nada(&saida));
        // desmutar volta a tocar
        b.eng.set_muted(false);
        let mut saida = Vec::new();
        b.roda(&tom, &mut saida);
        assert!(!nada(&saida), "depois de desmutar o monitor volta");
        // desligar o monitor cala o retorno mas continua enviando
        b.eng.set_monitor(false);
        let mut lixo = Vec::new();
        b.roda(&[0.0; 4800], &mut lixo); // deixa a fila esvaziar
        while b.pacotes.try_recv().is_ok() {}
        let mut saida = Vec::new();
        b.roda(&tom, &mut saida);
        assert!(nada(&saida), "monitor desligado não toca");
        assert!(b.pacotes.try_recv().is_ok(), "mas o envio segue");

        // surdo: não toca nada (nem o monitor)
        let mut b = banco();
        b.eng.set_monitor(true);
        b.eng.set_deafened(true);
        let mut lixo = Vec::new();
        b.roda(&[0.0; 4800], &mut lixo);
        let mut saida = Vec::new();
        b.roda(&tom, &mut saida);
        assert!(nada(&saida), "surdo não ouve o monitor");
        // sem eco infinito: o monitor não vira pacote recebido nem 'speaking'
        assert!(!b.eng.speaking(0));
    }

    // ── sensibilidade do VAD com fala fraca ──

    fn escala_db(x: &mut [f32], alvo_db: f32) {
        let g = 10f32.powf((alvo_db - rms_db(x)) / 20.0);
        x.iter_mut().for_each(|v| *v *= g);
    }

    /// Quais quadros de entrada saíram em pacote (a pré-rolagem é atribuída aos quadros anteriores).
    fn quadros_enviados(p: &mut SendPipeline, x: &[f32]) -> Vec<bool> {
        let total = x.len() / FRAME;
        let mut enviados = vec![false; total];
        for (k, q) in x.chunks_exact(FRAME).enumerate() {
            let mut m = 0;
            p.process(q, &mut |_, _| m += 1);
            for j in 0..m.min(k + 1) {
                enviados[k - j] = true;
            }
        }
        enviados
    }

    /// Fala sintética com envelope em dB por quadro, sobre ruído rosa de -60 dBFS.
    fn cena(env_db: &[f32]) -> Vec<f32> {
        let n = env_db.len() * FRAME;
        let mut fala = fala_sintetica(n);
        escala_db(&mut fala, 0.0);
        let mut ruido = ruido_rosa(0.1, n, 77);
        escala_db(&mut ruido, -60.0);
        (0..n)
            .map(|i| {
                let d = env_db[i / FRAME];
                let g = if d <= -100.0 { 0.0 } else { 10f32.powf(d / 20.0) };
                fala[i] * g + ruido[i]
            })
            .collect()
    }

    fn pipeline_energia() -> SendPipeline {
        let mut p = SendPipeline::new().unwrap();
        p.set_noise_suppression(false);
        p
    }

    #[test]
    fn vad_nao_corta_comeco_fraco_nem_final_decaindo() {
        // 1 s de ruído; fala que começa fraca (80 ms a -52 dBFS), fica a -40, decai até sumir; 1,2 s de ruído
        let mut env = vec![-200.0f32; 50];
        env.extend([-52.0; 4]);
        env.extend([-40.0; 30]);
        env.extend((0..20).map(|i| -40.0 - i as f32)); // decai 20 dB
        let fim_fala = env.len();
        env.extend(vec![-200.0; 60]);
        let x = cena(&env);
        let enviados = quadros_enviados(&mut pipeline_energia(), &x);
        assert!(enviados[..44].iter().all(|e| !e), "ruído sozinho não pode abrir");
        let (ini, fim) = (50, fim_fala - 6); // últimos quadros decaem para dentro do ruído
        for k in ini..fim {
            assert!(enviados[k], "quadro {k} de fala cortado (começo fraco / final)");
        }
        let ultimo = enviados.iter().rposition(|e| *e).unwrap();
        assert!(ultimo <= fim_fala + Vad::HANGOVER_QUADROS as usize + 2, "gate demorou a fechar: {ultimo}");
    }

    #[test]
    fn vad_segura_silabas_fracas_sobre_ruido() {
        // fala de -47 dBFS RMS com sílabas a 4 Hz sobre ruído rosa de -60 dBFS: nada cortado no meio
        let mut env = vec![-200.0f32; 50];
        env.extend(vec![-47.0; 100]);
        env.extend(vec![-200.0; 60]);
        let x = cena(&env);
        let enviados = quadros_enviados(&mut pipeline_energia(), &x);
        assert!(enviados[..44].iter().all(|e| !e));
        for k in 50..150 {
            assert!(enviados[k], "quadro {k} cortado dentro de uma fala fraca");
        }
        assert!(enviados[150..].iter().filter(|e| **e).count() <= Vad::HANGOVER_QUADROS as usize + 2);
        // e um pouco mais fraco ainda (-50 dBFS): continua passando
        let mut env = vec![-200.0f32; 50];
        env.extend(vec![-50.0; 100]);
        env.extend(vec![-200.0; 40]);
        let enviados = quadros_enviados(&mut pipeline_energia(), &cena(&env));
        assert!((52..150).all(|k| enviados[k]), "fala de -50 dBFS cortada");
    }

    #[test]
    fn vad_com_prob_segura_consoantes_e_fim_de_frase() {
        // (nível dBFS, probabilidade): ruído, começo fraco, vogal, consoante fraca, fim de frase decaindo, ruído
        let mut quadros: Vec<(f32, f32)> = vec![(-75.0, 0.03); 30];
        let ini = quadros.len();
        quadros.extend([(-58.0, 0.25), (-55.0, 0.30), (-52.0, 0.45)]); // começo fraco: abre no 3º
        quadros.extend(vec![(-35.0, 0.95); 10]);
        quadros.extend(vec![(-57.0, 0.20); 6]); // consoante fraca: continua acima de prob_fecha
        quadros.extend(vec![(-38.0, 0.9); 10]);
        quadros.extend((0..8).map(|i| (-45.0 - i as f32 * 2.0, 0.4 - i as f32 * 0.04))); // decaindo
        let fim_fala = quadros.len();
        quadros.extend(vec![(-80.0, 0.02); 40]);
        let mut v = Vad::new(0.7);
        let abertos: Vec<bool> = quadros.iter().map(|(db, p)| v.process_db_prob(*db, Some(*p))).collect();
        assert!(abertos[..ini].iter().all(|a| !a), "ruído não abre");
        assert!(abertos[ini + 2], "abre já no começo fraco (3º quadro)");
        assert!(abertos[ini + 2..fim_fala].iter().all(|a| *a), "nada cortado até o fim da fala");
        let fecha = abertos.iter().rposition(|a| *a).unwrap();
        assert!(fecha >= fim_fala && fecha <= fim_fala + Vad::HANGOVER_QUADROS as usize, "fechou em {fecha}");
        // com sensibilidade mínima o mesmo começo fraco ainda não teria aberto no 3º quadro
        let mut duro = Vad::new(0.0);
        let r: Vec<bool> = quadros.iter().map(|(db, p)| duro.process_db_prob(*db, Some(*p))).collect();
        assert!(!r[ini + 2], "menos sensível abre mais tarde");
    }

    // ── AGC ──

    #[test]
    fn vad_com_rnnoise_deixa_passar_fala_fraca_e_barra_ruido() {
        // pipeline padrão (RNNoise ligado): fala sintética de -55 a -45 dBFS sobre ruído rosa de -60 dBFS
        for nivel in [-55.0f32, -50.0, -45.0] {
            let mut env = vec![-200.0f32; 60];
            env.extend(vec![nivel; 100]);
            env.extend(vec![-200.0; 40]);
            let x = cena(&env);
            let enviados = quadros_enviados(&mut SendPipeline::new().unwrap(), &x);
            // depois do aquecimento do RNNoise, ruído sozinho não abre
            assert!(enviados[10..54].iter().all(|e| !e), "{nivel} dBFS: ruído abriu o VAD");
            assert!((60..160).all(|k| enviados[k]), "{nivel} dBFS: fala cortada");
        }
    }

    /// Fala com pausas, como numa conversa: blocos de 2 s com 1,4 s de fala (RMS `nivel_db`) e 0,6 s de ruído de sala
    /// (RMS `ruido_db`). O AGC2 estima o piso de ruído nas pausas, então um sinal sem pausa nunca sobe de ganho.
    fn fala_com_pausas(nivel_db: f32, ruido_db: f32, blocos: usize) -> Vec<f32> {
        let mut x = Vec::new();
        for b in 0..blocos {
            let mut f = fala_sintetica(FRAME * 70);
            escala_db(&mut f, nivel_db);
            x.extend(f);
            let mut r = ruido_branco(0.1, FRAME * 30, 11 + b as u32);
            escala_db(&mut r, ruido_db);
            x.extend(r);
        }
        x
    }

    #[test]
    fn agc_sobe_fala_fraca_com_agc2() {
        let x = fala_com_pausas(-45.0, -80.0, 15); // 30 s
        let mut p = pipeline_energia();
        let o = processa(&mut p, &x);
        let miolo = |v: &[f32]| rms_db(&v[FRAME * 1500 - FRAME * 600..FRAME * 1500 - FRAME * 100]);
        assert_eq!(p.pre_agc.gain_db(), 0.0, "sem RNNoise o pré-AGC fica desligado");
        assert!(p.agc2.gain_db() > 8.0 && p.agc2.gain_db() <= Agc2::MAX_GANHO as f32, "AGC2 {}", p.agc2.gain_db());
        assert!(miolo(&o) > miolo(&x) + 6.0, "saída {} dBFS contra entrada {}", miolo(&o), miolo(&x));
        assert!(o.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
    }

    #[test]
    fn pre_agc_sobe_antes_do_rnnoise_e_a_voz_baixa_nao_e_cortada() {
        let x = fala_com_pausas(-52.0, -80.0, 8);
        let mut com = SendPipeline::new().unwrap();
        let o = processa(&mut com, &x);
        let g = com.pre_agc.gain_db();
        assert!(g > 5.0 && g <= PreAgc::MAX_DB, "pré-AGC {g}");
        let mut sem = SendPipeline::new().unwrap();
        sem.set_agc(false);
        let o2 = processa(&mut sem, &x);
        assert_eq!(sem.pre_agc.gain_db(), 0.0);
        // com o pré-AGC o RNNoise recebe a voz num nível normal: sai mais voz que sem ele
        let fala = |v: &[f32]| rms_db(&v[FRAME * 70..FRAME * 70 + FRAME * 60]);
        assert!(fala(&o) > fala(&o2) + 3.0, "com {} dBFS, sem {} dBFS", fala(&o), fala(&o2));
    }

    #[test]
    fn agc_nao_amplifica_sinal_alto_nem_atenua() {
        let mut x = fala_sintetica(FRAME * 150);
        escala_db(&mut x, -15.0);
        for ns in [false, true] {
            let mut p = SendPipeline::new().unwrap();
            p.set_noise_suppression(ns);
            let o = processa(&mut p, &x);
            assert!(p.pre_agc.gain_db() < 0.01 && p.agc2.gain_db() < 0.01, "ns={ns}: ganhos {} {}", p.pre_agc.gain_db(), p.agc2.gain_db());
            if !ns {
                // só a passa-alta de 80 Hz mexe: menos de 1,5 dB
                assert!((rms_db(&o[FRAME * 50..]) - rms_db(&x[FRAME * 50..])).abs() < 1.5);
            }
        }
    }

    #[test]
    fn agc_congela_no_silencio_e_no_ruido() {
        // ruído de -65 dBFS por 10 s: sem voz os ganhos nunca sobem
        let mut r = ruido_rosa(0.1, FRAME * 500, 5);
        escala_db(&mut r, -65.0);
        let mut p = SendPipeline::new().unwrap();
        processa(&mut p, &r);
        assert!(p.pre_agc.gain_db() < 0.5, "ruído subiu o pré-AGC: {}", p.pre_agc.gain_db());
        assert_eq!(p.agc2.gain_db(), 0.0, "ruído não pode subir o AGC2");
        // fala e depois 5 s de silêncio: o AGC2 fica onde estava
        let x = fala_com_pausas(-45.0, -80.0, 15);
        let mut p = pipeline_energia();
        processa(&mut p, &x);
        let g = p.agc2.gain_db();
        assert!(g > 4.0, "AGC2 {g}");
        processa(&mut p, &vec![0.0; FRAME * 250]);
        assert!((p.agc2.gain_db() - g).abs() < 0.3, "ganho mexeu no silêncio: {g} → {}", p.agc2.gain_db());
    }

    #[test]
    fn agc_sem_pumping() {
        // fala com pausas: o AGC2 sobe no máximo 6 dB/s (0,12 dB por quadro de 20 ms; 0,72 dB uma vez no fim da espera)
        // e em regime o ganho praticamente não mexe
        let f = fala_com_pausas(-45.0, -80.0, 25);
        let mut p = pipeline_energia();
        let mut g = Vec::new();
        for q in f.chunks_exact(FRAME) {
            p.process(q, &mut |_, _| {});
            g.push(p.agc2.gain_db());
        }
        let maior_subida = g.windows(2).map(|w| w[1] - w[0]).fold(0f32, f32::max);
        assert!(maior_subida <= 0.12 + 0.72 + 0.01, "subida de {maior_subida} dB num quadro");
        let cauda = &g[g.len() - 500..];
        let (mn, mx) = cauda.iter().fold((99f32, -99f32), |(a, b), v| (a.min(*v), b.max(*v)));
        assert!(mx - mn < 3.0, "faixa do ganho em regime {mn}..{mx}");
    }

    #[test]
    fn agc_desligado_volta_a_zero_sem_degrau() {
        let x = fala_com_pausas(-52.0, -80.0, 15);
        let mut p = SendPipeline::new().unwrap();
        processa(&mut p, &x);
        let (pre, g2) = (p.pre_agc.gain_db(), p.agc2.gain_db());
        assert!(pre > 5.0 && g2 > 3.0, "{pre} {g2}");
        p.set_agc(false);
        let x2 = fala_com_pausas(-52.0, -80.0, 1);
        let o = processa(&mut p, &x2);
        assert_eq!(p.pre_agc.gain_db(), 0.0);
        assert!(p.agc2.gain_db() < g2 - 3.0, "AGC2 desce a 6 dB/s: {g2} → {}", p.agc2.gain_db());
        assert!(passo_max(&o) < 0.1, "degrau ao desligar o AGC: {}", passo_max(&o));
    }

    // ── jitter adaptativo (40–240 ms) e esticar/comprimir ──

    #[test]
    fn jitter_rajada_de_dez_pacotes_sobe_o_alvo_e_nao_corta_audio() {
        let mut jb = JitterBuffer::com_alvo(5);
        for i in 0..10u32 {
            jb.push(i, pkt(i as u8));
        }
        let r: Vec<_> = (0..10).map(|_| frame_num(jb.pop())).collect();
        assert_eq!(r, (0..10).map(Some).collect::<Vec<_>>());
        assert!(jb.target() > 5, "o alvo subiu para absorver a rajada: {}", jb.target());
        assert_eq!(jb.take_discarded(), 0);
    }

    #[test]
    fn jitter_rajada_grande_sobe_ao_maximo_e_so_entao_descarta() {
        let mut jb = JitterBuffer::com_alvo(2);
        for i in 0..20u32 {
            jb.push(i, pkt(i as u8));
        }
        let primeiro = frame_num(jb.pop());
        assert!(primeiro.is_some());
        assert_eq!(jb.target(), JB_MAX);
        assert!(jb.depth() <= jb.target() + JB_EXCESSO, "profundidade {}", jb.depth());
        assert!(jb.take_discarded() > 0);
    }

    #[test]
    fn jitter_conta_cortes_e_buracos() {
        let mut jb = JitterBuffer::com_alvo(2);
        jb.push(0, pkt(0));
        jb.push(1, pkt(1));
        jb.push(3, pkt(3)); // buraco no 2
        jb.push(4, pkt(4));
        tocar(&mut jb, 5);
        assert_eq!(jb.take_holes(), 1);
        assert_eq!(jb.take_cuts(), 0);
        // esvazia e o pacote chega no tick seguinte: corte
        let _ = jb.pop(); // PLC por esvaziamento (não é buraco)
        assert!(!jb.last_was_hole());
        jb.push(5, pkt(5));
        assert!(matches!(jb.pop(), Playout::Frame(_)));
        assert_eq!(jb.take_cuts(), 1);
        assert_eq!(jb.take_holes(), 0);
    }

    fn codifica(x: &[f32]) -> Vec<(u32, Vec<u8>)> {
        let mut enc = Encoder::new(SAMPLE_RATE, Channels::Mono, Application::Voip).unwrap();
        enc.set_bitrate(Bitrate::Bits(BITRATE)).unwrap();
        enc.set_signal(Signal::Voice).unwrap();
        let mut buf = [0u8; MAX_PACKET];
        x.chunks_exact(FRAME)
            .enumerate()
            .map(|(i, q)| {
                let n = enc.encode_float(q, &mut buf).unwrap();
                (i as u32, buf[..n].to_vec())
            })
            .collect()
    }

    /// Resultado de tocar um fluxo de pacotes por um `UserStream`.
    struct Tocado {
        pcm: Vec<f32>,
        quadros_com_audio: usize,
        u: UserStream,
    }

    /// `chegadas[t]` = índices dos pacotes que chegam antes do tick `t`; roda `ticks` ticks de 20 ms.
    fn toca(pacotes: &[(u32, Vec<u8>)], chegadas: &[Vec<usize>], ticks: usize, alvo: usize, warp: bool) -> Tocado {
        let mut u = UserStream::new_com_jitter(alvo).unwrap();
        if !warp {
            u.sem_warp();
        }
        let mut pcm = Vec::new();
        let mut q = vec![0f32; FRAME];
        let mut com_audio = 0;
        for t in 0..ticks {
            for &i in chegadas.get(t).map(|v| v.as_slice()).unwrap_or(&[]) {
                u.push(pacotes[i].0, pacotes[i].1.clone());
            }
            if u.render(&mut q) {
                com_audio += 1;
            }
            pcm.extend_from_slice(&q);
        }
        Tocado { pcm, quadros_com_audio: com_audio, u }
    }

    /// Um pacote por tick, com a rajada pedida: `rajada` pacotes de uma vez no tick 0, depois um por tick.
    fn chegadas_com_rajada(n: usize, rajada: usize) -> Vec<Vec<usize>> {
        let mut c: Vec<Vec<usize>> = vec![Vec::new(); n + 40];
        for i in 0..rajada.min(n) {
            c[0].push(i);
        }
        for i in rajada..n {
            c[i - rajada + 1].push(i);
        }
        c
    }

    /// Maior degrau entre amostras vizinhas, ignorando as bordas (fade-in do começo e fade-out do fim).
    fn degrau_do_miolo(pcm: &[f32]) -> f32 {
        let ini = pcm.iter().position(|v| v.abs() > 1e-6).unwrap_or(0) + FRAME * 2;
        let fim = pcm.iter().rposition(|v| v.abs() > 1e-6).unwrap_or(0).saturating_sub(FRAME * 2);
        passo_max(&pcm[ini..fim.max(ini + 2)])
    }

    fn tom_de_fala(n_quadros: usize) -> Vec<f32> {
        // tom de 220 Hz com 3 harmônicos e variação lenta de amplitude (como uma vogal sustentada)
        (0..n_quadros * FRAME)
            .map(|i| {
                let t = i as f32 / 48000.0;
                let env = 0.8 + 0.2 * (2.0 * PI * 3.0 * t).sin();
                env * 0.25 * ((2.0 * PI * 220.0 * t).sin() + 0.5 * (2.0 * PI * 440.0 * t).sin() + 0.25 * (2.0 * PI * 660.0 * t).sin()) / 1.5
            })
            .collect()
    }

    #[test]
    fn usuario_sem_desvio_nao_mexe_no_tempo_e_e_transparente() {
        let x = tom_de_fala(60);
        let pacotes = codifica(&x);
        let ch = chegadas_com_rajada(60, 1);
        let r = toca(&pacotes, &ch, 55, 3, true); // enquanto o fluxo corre (no fim o buffer seca e o esticador pode entrar)
        assert_eq!(r.u.warped(), (0, 0), "rede calma: o tempo não pode ser mexido");
        assert!(r.quadros_com_audio >= 50);
        let natural = passo_max(&x);
        assert!(degrau_do_miolo(&r.pcm) < natural * 2.0 + 0.01, "degrau {} contra {natural}", degrau_do_miolo(&r.pcm));
    }

    #[test]
    fn usuario_rajada_de_dez_pacotes_e_absorvida_sem_descontinuidade() {
        let x = tom_de_fala(80);
        let pacotes = codifica(&x);
        let ch = chegadas_com_rajada(80, 10);
        let natural = passo_max(&x);
        let r = toca(&pacotes, &ch, 130, 3, true);
        let (rem, rep) = r.u.warped();
        let tocado = r.quadros_com_audio * FRAME;
        eprintln!(
            "rajada de 10: tocou {tocado} amostras de {} enviadas; comprimiu {rem}, esticou {rep}; degrau {:.4} (natural {natural:.4}); alvo final {} quadros",
            x.len(),
            degrau_do_miolo(&r.pcm),
            r.u.jitter().target()
        );
        assert!(rem > 0, "o excesso da rajada deveria ser gasto comprimindo o tempo");
        assert!(degrau_do_miolo(&r.pcm) < 0.05, "descontinuidade: degrau {}", degrau_do_miolo(&r.pcm));
        assert!(degrau_do_miolo(&r.pcm) < natural * 3.0 + 0.02);
        // nada some: o que entrou saiu, descontando só o que o comprimidor removeu (um período de pitch por vez)
        assert_eq!(r.u.jitter().depth(), 0);
        let soma = tocado as u64 + rem - rep;
        // a sobra é a cauda: JB_PLC_QUADROS de PLC depois do último pacote, o fade-out e o quadro de resto
        let cauda = ((JB_PLC_QUADROS as usize + 3) * FRAME) as i64;
        assert!((soma as i64 - x.len() as i64).abs() <= cauda, "tocado {tocado} + removido {rem} - repetido {rep} contra {} enviado", x.len());
        assert!((tocado as f32) >= x.len() as f32 * 0.85, "a duração tocada ficou longe da enviada: {tocado} de {}", x.len());
        // sem o comprimidor, a mesma rajada toca igual, só que com 40–60 ms a mais de atraso até o fim
        let sem = toca(&pacotes, &ch, 130, 3, false);
        assert_eq!(sem.u.warped(), (0, 0));
        assert!(degrau_do_miolo(&sem.pcm) < 0.05);
    }

    #[test]
    fn usuario_falta_de_tres_pacotes_nao_gera_descontinuidade() {
        let x = tom_de_fala(80);
        let pacotes = codifica(&x);
        let mut ch = chegadas_com_rajada(80, 1);
        for t in ch.iter_mut() {
            t.retain(|i| !(30..33).contains(i)); // perde 3 seguidos
        }
        let natural = passo_max(&x);
        let r = toca(&pacotes, &ch, 120, 3, true);
        let tocado = r.quadros_com_audio * FRAME;
        eprintln!("falta de 3: tocou {tocado} de {} enviadas; warped {:?}; degrau {:.4} (natural {natural:.4})", x.len(), r.u.warped(), degrau_do_miolo(&r.pcm));
        assert!(degrau_do_miolo(&r.pcm) < 0.05, "descontinuidade: degrau {}", degrau_do_miolo(&r.pcm));
        assert!(rms(&r.pcm[FRAME * 29..FRAME * 40]) > 0.05, "o buraco virou silêncio");
        // os 3 quadros perdidos viram 3 de PLC (mesma duração); a sobra é a cauda (3 de PLC + fade-out + resto)
        let (rem, rep) = r.u.warped();
        let liquido = tocado as i64 + rem as i64 - rep as i64 - x.len() as i64;
        assert!((0..=((JB_PLC_QUADROS as usize + 3) * FRAME) as i64).contains(&liquido), "duração tocada {tocado} contra {} enviada (líquido {liquido})", x.len());
        let mut u = r.u;
        let st = u.stats(1, 0.0, 0.0);
        assert!(st.loss_pct > 0.5, "a perda deveria aparecer nas estatísticas: {}", st.loss_pct);
    }

    #[test]
    fn usuario_rede_atrasando_estica_o_tempo_em_vez_de_esvaziar() {
        // alvo de 6 quadros; os pacotes passam a chegar a 90% da cadência (relógio do remetente mais lento / rede
        // segurando): sem esticar o buffer esvazia e faz PLC; esticando, a fala desacelera e não corta
        let n = 120;
        let x = tom_de_fala(n);
        let pacotes = codifica(&x);
        let mut ch: Vec<Vec<usize>> = vec![Vec::new(); 160];
        let mut t_chegada = 0.0f32;
        for i in 0..n {
            t_chegada += if i < 8 { 0.0 } else { 1.0 / 0.9 };
            ch[(t_chegada as usize).min(159)].push(i);
        }
        let com = toca(&pacotes, &ch, 160, 6, true);
        let sem = toca(&pacotes, &ch, 160, 6, false);
        let (_, rep) = com.u.warped();
        eprintln!(
            "chegada a 90%: com esticar repetiu {rep} amostras, degrau {:.4}; quadros de PLC por buffer vazio: com={} sem={}",
            degrau_do_miolo(&com.pcm),
            com.u.jitter().plc_vazio(),
            sem.u.jitter().plc_vazio()
        );
        assert!(rep > 0, "deveria esticar");
        assert!(degrau_do_miolo(&com.pcm) < 0.05, "degrau {}", degrau_do_miolo(&com.pcm));
        assert!(com.u.jitter().plc_vazio() < sem.u.jitter().plc_vazio(), "esticar deveria evitar esvaziamentos");
    }

    #[test]
    fn usuario_estatisticas_basicas() {
        let x = tom_de_fala(60);
        let pacotes = codifica(&x);
        let ch = chegadas_com_rajada(60, 10);
        let mut r = toca(&pacotes, &ch, 40, 3, true);
        let st = r.u.stats(7, 40.0, 10.0);
        assert_eq!(st.user, 7);
        assert!(st.target_ms >= 40.0 && st.target_ms <= 240.0);
        assert!(st.buffered_ms >= 0.0 && st.latency_ms >= st.buffered_ms + 40.0 + 20.0 - 0.1, "{st:?}");
        assert_eq!(st.loss_pct, 0.0);
        let tot = RxTotals::from_users(&[st, RxUserStats { user: 8, loss_pct: 4.0, jitter_ms: 6.0, cuts_per_min: 3.0, latency_ms: 500.0, ..Default::default() }]);
        assert_eq!(tot.users, 2);
        assert!((tot.loss_pct - 2.0).abs() < 1e-4 && tot.cuts_per_min >= 3.0 && tot.latency_ms == 500.0);
        assert_eq!(RxTotals::from_users(&[]), RxTotals::default());
    }

    #[test]
    fn opus_48_kbps_banda_total_cabe_no_limite_do_servidor() {
        let mut p = SendPipeline::new().unwrap();
        p.set_noise_suppression(false);
        // pior caso: fala sintética com ruído forte e transientes (pacotes grandes no VBR)
        let mut x = fala_sintetica(FRAME * 200);
        let r = ruido_branco(0.5, FRAME * 200, 3);
        for (a, b) in x.iter_mut().zip(&r) {
            *a = (*a * 2.0 + b).clamp(-1.0, 1.0);
        }
        let mut tamanhos = Vec::new();
        for q in x.chunks_exact(FRAME) {
            p.process(q, &mut |_, d| tamanhos.push(d.len()));
        }
        assert!(tamanhos.len() > 150);
        let (max, media) = (*tamanhos.iter().max().unwrap(), tamanhos.iter().sum::<usize>() as f32 / tamanhos.len() as f32);
        eprintln!("Opus 48 kbps: {} pacotes, média {media:.0} bytes, máximo {max} bytes (limite {MAX_PACKET})", tamanhos.len());
        assert!(max <= MAX_PACKET);
        assert!(media < 190.0, "média {media}");
        let mut enc = Encoder::new(SAMPLE_RATE, Channels::Mono, Application::Voip).unwrap();
        enc.set_bitrate(Bitrate::Bits(BITRATE)).unwrap();
        assert_eq!(enc.get_bitrate().unwrap(), Bitrate::Bits(48_000));
    }

    // ── cancelamento de eco ──

    /// Eco sintético: `far` atrasado `atraso_ms` e passado por uma resposta curta (cauda de 5 ms) com ganho `ganho_db`.
    fn eco_de(far: &[f32], atraso_ms: usize, ganho_db: f32) -> Vec<f32> {
        let d = atraso_ms * 48;
        let h: Vec<f32> = {
            let r = ruido_branco(1.0, 240, 77);
            let n = (r.iter().map(|v| v * v).sum::<f32>()).sqrt();
            r.iter().enumerate().map(|(k, v)| v / n * (-(k as f32) / 60.0).exp() * 10f32.powf(ganho_db / 20.0) * 1.6).collect()
        };
        (0..far.len())
            .map(|i| h.iter().enumerate().filter(|(k, _)| i >= d + k).map(|(k, hk)| hk * far[i - d - k]).sum())
            .collect()
    }

    #[test]
    fn eco_pipeline_cancela_o_eco_e_preserva_a_fala_do_perto() {
        let seg = 14usize;
        let n = FRAME * 50 * seg;
        // duas vozes de verdade diferentes: tons fundamentais com vibrato em 130 e 210 Hz (harmônicos iguais seriam
        // "eco" para o filtro adaptativo, e não é isso que acontece entre duas pessoas)
        let voz = |f0: f32, db: f32| {
            let mut fase = 0.0f32;
            let mut v: Vec<f32> = (0..n)
                .map(|i| {
                    let t = i as f32 / 48000.0;
                    fase += 2.0 * PI * f0 * (1.0 + 0.12 * (2.0 * PI * 0.7 * t).sin()) / 48000.0;
                    let mut x = 0.0;
                    for h in 1..25 {
                        let fh = f0 * h as f32;
                        let env = (-((fh - 700.0) / 400.0).powi(2)).exp() + 0.6 * (-((fh - 1800.0) / 600.0).powi(2)).exp();
                        x += env / h as f32 * (fase * h as f32).sin();
                    }
                    x * (0.35 + 0.65 * (2.0 * PI * 3.7 * t).sin().abs())
                })
                .collect();
            escala_db(&mut v, db);
            v
        };
        let far = voz(130.0, -25.0);
        let mut perto = voz(210.0, -28.0);
        for (i, v) in perto.iter_mut().enumerate() {
            if i < 10 * 50 * FRAME || i >= 13 * 50 * FRAME {
                *v = 0.0; // fala do perto só de 10 s a 13 s
            }
        }
        let eco = eco_de(&far, 80, -10.0);
        let mic: Vec<f32> = (0..n).map(|i| eco[i] + perto[i]).collect();
        let roda = |ligado: bool| {
            let mut p = SendPipeline::new().unwrap();
            p.set_noise_suppression(false);
            p.set_agc(false);
            p.set_echo_cancel(ligado);
            let mut out = Vec::with_capacity(n);
            for (q, r) in mic.chunks_exact(FRAME).zip(far.chunks_exact(FRAME)) {
                p.reference.copy_from_slice(r);
                p.process(q, &mut |_, _| {});
                out.extend_from_slice(p.last_frame());
            }
            (out, p.echo_stats())
        };
        let (sem, st_sem) = roda(false);
        let (com, st) = roda(true);
        assert!(st_sem.is_none());
        let en = |x: &[f32]| x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>();
        let seg_s = |s: usize, e: usize| (s * 48000, e * 48000);
        let (a, b) = seg_s(6, 10);
        let erle = 10.0 * (en(&sem[a..b]) / en(&com[a..b])).log10();
        let (a, b) = seg_s(10, 13);
        let perto_db = 10.0 * (en(&com[a..b]) / en(&perto[a..b])).log10();
        eprintln!("eco: ERLE {erle:.1} dB (6–10 s), fala do perto na saída {perto_db:+.1} dB do original; stats {st:?}");
        assert!(erle > 20.0, "ERLE {erle:.1} dB");
        assert!((-6.0..=3.0).contains(&perto_db), "a fala do perto saiu {perto_db:+.1} dB do original");
        let st = st.unwrap();
        assert!(st.delay_ms.is_some_and(|d| (40..=200).contains(&d)), "atraso estimado {:?}", st.delay_ms);
        assert!(com.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
    }

    #[test]
    fn eco_desligado_nao_mexe_no_sinal_e_ligar_desligar_e_barato() {
        let x = seno(500.0, 48000.0, 0.3, FRAME * 10, 0);
        let mut p = SendPipeline::new().unwrap();
        p.set_noise_suppression(false);
        p.set_agc(false);
        assert!(!p.echo_cancel() && p.echo_stats().is_none());
        let sem = processa(&mut p, &x);
        let mut p2 = SendPipeline::new().unwrap();
        p2.set_noise_suppression(false);
        p2.set_agc(false);
        p2.set_echo_cancel(true);
        p2.set_echo_cancel(false);
        assert_eq!(processa(&mut p2, &x), sem, "ligar e desligar tem de deixar tudo como estava (e soltar a memória)");
        // ligado com referência vazia (ninguém falando): não pode engolir a voz do usuário
        let mut p3 = SendPipeline::new().unwrap();
        p3.set_noise_suppression(false);
        p3.set_agc(false);
        p3.set_echo_cancel(true);
        let o = processa(&mut p3, &x);
        let (ent, sai) = (rms(&x[FRAME * 5..]), rms(&o[FRAME * 5..]));
        assert!(sai > ent * 0.8, "sem referência o microfone passa: {ent} → {sai}");
    }

    #[test]
    fn eco_referencia_do_motor_fica_na_linha_do_tempo_da_captura() {
        // le_referencia: completa com zeros e depois descarta o que faltou, sem deslocar a referência
        let (mut prod, mut cons) = RingBuffer::<f32>::new(REF_RING);
        let mut dst = vec![0f32; FRAME];
        let mut divida = 0usize;
        prod.push_partial_slice(&vec![1.0; 700]);
        le_referencia(&mut cons, &mut dst, &mut divida);
        assert_eq!((dst[699], dst[700], divida), (1.0, 0.0, FRAME - 700));
        // as amostras atrasadas chegam: as primeiras `divida` são descartadas, o resto segue
        prod.push_partial_slice(&vec![2.0; 260 + FRAME]);
        le_referencia(&mut cons, &mut dst, &mut divida);
        assert_eq!(divida, 0);
        assert!(dst.iter().all(|v| *v == 2.0), "a referência seguinte começa exatamente onde a captura está");
        // fila acumulada demais: descarta as mais antigas e segue
        prod.push_partial_slice(&vec![3.0; FRAME * 6]);
        le_referencia(&mut cons, &mut dst, &mut divida);
        assert!(cons.slots() <= FRAME * 2 && dst.iter().all(|v| *v == 3.0));
    }

    // ── estalos: bordas, deriva do monitor e saída ──

    #[test]
    fn declick_fade_in_e_fade_out_suaves() {
        let mut d = Declick::new();
        let mut q = vec![0.5f32; FRAME];
        d.audio(&mut q);
        assert!(q[0].abs() < 0.01 && (q[FADE_AMOSTRAS] - 0.5).abs() < 1e-6 && (q[FRAME - 1] - 0.5).abs() < 1e-6);
        assert!(passo_max(&q) < 0.01, "fade-in suave");
        let mut q2 = vec![0.5f32; FRAME];
        d.audio(&mut q2); // já ativo: sem novo fade
        assert_eq!(q2, vec![0.5; FRAME]);
        let mut cauda = vec![1.0f32; FRAME];
        assert!(d.silencio(&mut cauda), "devolve o fade-out");
        assert!((cauda[0] - 0.5).abs() < 0.01, "começa onde o áudio parou: {}", cauda[0]);
        assert!(cauda[FADE_AMOSTRAS..].iter().all(|v| *v == 0.0) && cauda[FADE_AMOSTRAS - 1].abs() < 1e-3);
        assert!(cauda.windows(2).all(|w| w[1] <= w[0] + 1e-7), "monotônico");
        assert!(!d.silencio(&mut cauda) && cauda.iter().all(|v| *v == 0.0), "depois disso, silêncio");
    }

    #[test]
    fn recepcao_sem_degrau_nas_bordas_da_fala() {
        // duas falas separadas por uma pausa de VAD (seq contíguo): nada de salto no PLC → silêncio → retomada
        let mut p = pipeline_energia();
        let mut pacotes = Vec::new();
        let tom = seno(300.0, 48000.0, 0.4, FRAME * 30, 0);
        let pausa = vec![0.0f32; FRAME * 40];
        for q in tom.chunks_exact(FRAME).chain(pausa.chunks_exact(FRAME)).chain(tom.chunks_exact(FRAME)) {
            p.process(q, &mut |s, d| pacotes.push((s, d.to_vec())));
        }
        let mut u = UserStream::new().unwrap();
        let mut saida = Vec::new();
        let mut q = vec![0f32; FRAME];
        let mut i = 0;
        for _ in 0..pacotes.len() + 60 {
            if let Some((s, d)) = pacotes.get(i) {
                u.push(*s, d.clone());
                i += 1;
            }
            u.render(&mut q);
            saida.extend_from_slice(&q);
        }
        // um seno de 0,4 a 300 Hz tem passo natural de 0,016; tolera ruído do Opus mas não degraus
        let passo = passo_max(&saida);
        assert!(passo < 0.08, "degrau de {passo} nas bordas");
    }

    #[test]
    fn saida_segura_e_decai_em_vez_de_degrau() {
        let mut s = Suave::new();
        assert_eq!(s.dado(0.5), 0.5);
        let mut x = vec![0.5f32];
        for _ in 0..1500 {
            x.push(s.vazio());
        }
        assert!(passo_max(&x) < 0.0026, "passo {}", passo_max(&x));
        assert!(x.last().unwrap().abs() < 0.001);
        assert_eq!(s.dado(f32::NAN), 0.0);
        assert_eq!(s.dado(7.0), 1.0);
        assert_eq!(s.dado(f32::NEG_INFINITY), 0.0);
    }

    /// Alimenta o monitor com um seno contínuo (quadros de 20 ms) a `razao` quadros por 20 ms de saída
    /// (deriva de relógio), consumindo blocos de `bloco` amostras. Devolve (saída, perdidos, fila máxima, params).
    fn simula_monitor(razao: f32, ticks: usize, baixa: bool) -> (Vec<f32>, u32, usize, MonParams) {
        let bloco = if baixa { FRAME / 4 } else { FRAME };
        let mp = mon_params(baixa, 256, bloco, 736);
        let sh = Arc::new(Shared::new());
        sh.falando.store(true, Relaxed);
        let (mut prod, cons) = RingBuffer::<f32>::new(FRAME * MON_RING_QUADROS);
        let mut mon = MonitorOut::new(cons);
        let mut fase = 0usize;
        let mut acum = 0.0f32;
        let mut saida = Vec::new();
        let mut out = vec![0f32; bloco];
        let mut max_prof = 0;
        let empurra = |prod: &mut Producer<f32>, fase: &mut usize| {
            let q = seno(400.0, 48000.0, 0.4, FRAME, *fase);
            *fase += FRAME;
            let _ = prod.push_partial_slice(&q);
        };
        // rajada inicial de pré-rolagem (6 quadros)
        for _ in 0..6 {
            empurra(&mut prod, &mut fase);
        }
        for _ in 0..ticks * FRAME / bloco {
            acum += razao * bloco as f32 / FRAME as f32;
            while acum >= 1.0 {
                acum -= 1.0;
                empurra(&mut prod, &mut fase);
            }
            if mon.proximo(&sh, &mut out, &mp) {
                saida.extend_from_slice(&out);
            } else {
                saida.extend(std::iter::repeat(0.0).take(bloco));
            }
            max_prof = max_prof.max(mon.cons.slots());
        }
        (saida, sh.c_monitor_perdidos.load(Relaxed), max_prof, mp)
    }

    #[test]
    fn monitor_compensa_deriva_sem_cortes_nem_degraus() {
        // deriva real de relógios é ~0,01%; o compensador aguenta até ~0,3%
        for baixa in [false, true] {
            for razao in [1.0f32, 1.0001, 0.9999, 1.001, 0.999, 1.003, 0.997] {
                let (saida, perdidos, prof, mp) = simula_monitor(razao, 3000, baixa); // 60 s simulados
                assert_eq!(perdidos, 0, "baixa={baixa} razão {razao}: perdeu quadros do monitor");
                assert!(prof <= mp.max + FRAME, "baixa={baixa} razão {razao}: fila chegou a {prof} amostras");
                // contínuo: o seno de 0,4 a 400 Hz tem passo natural 0,021; esticar ±0,4% não cria degraus
                let miolo = &saida[FRAME * 10..];
                assert!(passo_max(miolo) < 0.04, "baixa={baixa} razão {razao}: passo {}", passo_max(miolo));
                assert!(rms(miolo) > 0.25, "baixa={baixa} razão {razao}: ficou sem som");
            }
        }
    }

    #[test]
    fn monitor_deriva_absurda_degrada_com_graca() {
        // 1% de diferença está acima do que o compensador cobre: descarta quadros, mas a fila fica limitada
        for baixa in [false, true] {
            let (saida, perdidos, prof, mp) = simula_monitor(1.01, 3000, baixa);
            assert!(perdidos > 0 && prof <= mp.max + FRAME, "baixa={baixa}: {perdidos} {prof}");
            assert!(saida.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
        }
    }

    #[test]
    fn monitor_sem_dados_faz_fade_out_e_volta() {
        for baixa in [false, true] {
            let bloco = if baixa { FRAME / 4 } else { FRAME };
            let mp = mon_params(baixa, 256, bloco, 736);
            let sh = Arc::new(Shared::new());
            sh.falando.store(true, Relaxed);
            let (mut prod, cons) = RingBuffer::<f32>::new(FRAME * MON_RING_QUADROS);
            let mut mon = MonitorOut::new(cons);
            let mut out = vec![0f32; bloco];
            let mut fase = 0;
            let mut todos = Vec::new();
            let mut empurra = |n: usize, prod: &mut Producer<f32>| {
                let q = seno(400.0, 48000.0, 0.4, n, fase);
                fase += n;
                let _ = prod.push_partial_slice(&q);
            };
            empurra(mp.prime, &mut prod);
            let blocos = mp.prime / bloco;
            for _ in 0..blocos {
                assert!(mon.proximo(&sh, &mut out, &mp), "baixa={baixa}");
                todos.extend_from_slice(&out);
            }
            // acabou: o primeiro bloco sem dado ainda devolve o fade-out, depois silêncio (e conta uma perda)
            while mon.cons.slots() >= bloco {
                assert!(mon.proximo(&sh, &mut out, &mp));
                todos.extend_from_slice(&out);
            }
            assert!(mon.proximo(&sh, &mut out, &mp), "fade-out");
            todos.extend_from_slice(&out);
            assert!(!mon.proximo(&sh, &mut out, &mp));
            assert_eq!(sh.c_monitor_perdidos.load(Relaxed), 1, "baixa={baixa}");
            assert!(passo_max(&todos[bloco..]) < 0.04, "baixa={baixa}: passo {}", passo_max(&todos[bloco..]));
            // fim normal de fala (portão fechado) não conta como perda
            sh.falando.store(false, Relaxed);
            let pe = mp.mais(mon.extra); // o motor aprendeu +10 ms com a falta de dado acima
            empurra(pe.prime, &mut prod);
            while mon.cons.slots() >= bloco {
                assert!(mon.proximo(&sh, &mut out, &mp));
            }
            assert!(mon.proximo(&sh, &mut out, &mp) && !mon.proximo(&sh, &mut out, &mp));
            assert_eq!(sh.c_monitor_perdidos.load(Relaxed), 1, "baixa={baixa}: fim normal contado como perda");
            // volta: só toca de novo depois de juntar `prime` amostras
            empurra(pe.prime - bloco, &mut prod);
            assert!(!mon.proximo(&sh, &mut out, &mp));
            empurra(bloco, &mut prod);
            assert!(mon.proximo(&sh, &mut out, &mp));
        }
    }

    #[test]
    fn estatisticas_contam_envio_e_descartes() {
        let mut b = banco();
        assert_eq!(b.eng.stats(), Stats::default());
        let mut saida = Vec::new();
        b.roda(&seno(500.0, 48000.0, 0.3, 24000, 0), &mut saida);
        let st = b.eng.stats();
        assert!(st.packets_sent > 20, "{st:?}");
        // pacote duplicado e atrasado → descartes do jitter buffer
        let dado = vec![0x08u8, 1, 2, 3];
        b.eng.push_packet(3, 100, &dado);
        b.eng.push_packet(3, 100, &dado);
        b.eng.push_packet(3, 101, &dado);
        thread::sleep(Duration::from_millis(60));
        assert!(b.eng.stats().jitter_dropped >= 1, "{:?}", b.eng.stats());
    }

    // ── custo de CPU (manual: cargo test --release -- --ignored custo_cpu --nocapture) ──

    #[test]
    #[ignore]
    fn custo_cpu_por_quadro() {
        let n = 1000;
        let x = fala_sintetica(FRAME * n);
        let ruido = ruido_rosa(0.05, FRAME * n, 3);
        let mix: Vec<f32> = x.iter().zip(&ruido).map(|(a, b)| a + b).collect();
        let tempo = |ligado: bool| {
            let mut p = SendPipeline::new().unwrap();
            p.set_noise_suppression(ligado);
            let mut bytes = 0usize;
            let t = Instant::now();
            for q in mix.chunks_exact(FRAME) {
                p.process(q, &mut |_, d| bytes += d.len());
            }
            let dt = t.elapsed();
            (dt.as_secs_f64() * 1e6 / n as f64, bytes)
        };
        let (sem, _) = tempo(false);
        let (com, _) = tempo(true);
        let mut d = DenoiseState::new();
        let (mut i, mut o) = ([100f32; DEN_FRAME], [0f32; DEN_FRAME]);
        let t = Instant::now();
        for _ in 0..n * 2 {
            std::hint::black_box(d.process_frame(&mut o, &i));
            i[0] += 1.0;
        }
        let so_den = t.elapsed().as_secs_f64() * 1e6 / n as f64;
        eprintln!(
            "por quadro de 20 ms: pipeline sem RNNoise {sem:.0} µs, com RNNoise {com:.0} µs, só RNNoise (2×480) {so_den:.0} µs; \
             uso de 1 núcleo: {:.2}% (orçamento 20000 µs)",
            com / 200.0
        );
    }

    /// Custo de cada peça nova do DSP por quadro de 20 ms (rode com `--release -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn custo_cpu_dsp_novo() {
        let n = 2000;
        let mut x = fala_sintetica(FRAME * n);
        escala_db(&mut x, -35.0);
        let medir = |nome: &str, mut f: Box<dyn FnMut(&mut [f32])>| {
            let mut y = x.clone();
            let t = Instant::now();
            for q in y.chunks_exact_mut(FRAME) {
                f(q);
            }
            let us = t.elapsed().as_secs_f64() * 1e6 / n as f64;
            eprintln!("{nome:28} {us:7.1} µs por quadro de 20 ms ({:.3}% de um núcleo)", us / 200.0);
        };
        let mut hp = PassaAlta::new();
        medir("passa-alta 80 Hz", Box::new(move |q| hp.process(q)));
        let mut pre = PreAgc::new();
        medir("pré-AGC", Box::new(move |q| {
            pre.process(q);
        }));
        let mut a2 = Agc2::new();
        medir("AGC2", Box::new(move |q| a2.process(q, 1.0)));
        let mut eco = Eco::new();
        let ref_ = vec![0.01f32; FRAME];
        medir("cancelador de eco (AEC3)", Box::new(move |q| eco.process(q, &ref_)));
        // esticador em modo comprimir (pior caso: busca de período todo quadro)
        let mut e = Esticador::new();
        let mut out = vec![0f32; FRAME];
        let t = Instant::now();
        let mut pulls = 0;
        for q in x.chunks_exact(FRAME) {
            e.push(q);
            while e.disponivel() >= Esticador::precisa(Escala::Comprime) {
                e.pull(&mut out, Escala::Comprime, 0.2);
                pulls += 1;
            }
        }
        let us = t.elapsed().as_secs_f64() * 1e6 / pulls as f64;
        eprintln!("{:28} {us:7.1} µs por quadro de 20 ms ({:.3}% de um núcleo, por usuário comprimindo)", "esticador (comprimindo)", us / 200.0);
        // pipeline completo de envio com tudo ligado
        let mut p = SendPipeline::new().unwrap();
        p.set_echo_cancel(true);
        let t = Instant::now();
        for q in x.chunks_exact(FRAME) {
            p.process(q, &mut |_, _| {});
        }
        let us = t.elapsed().as_secs_f64() * 1e6 / n as f64;
        eprintln!("{:28} {us:7.1} µs por quadro de 20 ms ({:.2}% de um núcleo)", "envio completo (c/ eco)", us / 200.0);
        let mut p = SendPipeline::new().unwrap();
        let t = Instant::now();
        for q in x.chunks_exact(FRAME) {
            p.process(q, &mut |_, _| {});
        }
        let us = t.elapsed().as_secs_f64() * 1e6 / n as f64;
        eprintln!("{:28} {us:7.1} µs por quadro de 20 ms ({:.2}% de um núcleo)", "envio completo (s/ eco)", us / 200.0);
    }

    // ── dispositivo real (manual) ──

    /// Só roda com `cargo test -- --ignored`. Abre o microfone real e uma saída **que não produz som**
    /// (rode com `PIPEWIRE_NODE=<sink nulo>` e `VOICE_LAB_OUT=pipewire`), força o VAD aberto e o monitor
    /// ligado por 10 s e imprime os contadores. Nada audível: a saída é um sink virtual descartável.
    #[test]
    #[ignore]
    fn dispositivo_real_contadores_10s() {
        let saida = std::env::var("VOICE_LAB_OUT").ok();
        let eng = Engine::start(
            Config { input: None, output: saida, low_latency: std::env::var("VOICE_LAB_LOW").map(|v| v == "1").unwrap_or(false) },
            Box::new(|_, _| {}),
        ).expect("abrir");
        let monitor = std::env::var("VOICE_LAB_MONITOR").map(|v| v != "0").unwrap_or(true);
        eng.sh.manter_saida.store(true, Relaxed);
        eng.sh.forcar_vad.store(monitor, Relaxed);
        eng.set_monitor(monitor);
        thread::sleep(Duration::from_secs(1));
        let antes = eng.stats();
        let t0 = Instant::now();
        let mut max = 0f32;
        while t0.elapsed() < Duration::from_secs(10) {
            thread::sleep(Duration::from_millis(100));
            max = max.max(eng.mic_level());
        }
        let depois = eng.stats();
        eprintln!("CONTADORES (10 s, monitor {monitor}): antes {antes:?}");
        eprintln!("CONTADORES (10 s, monitor {monitor}): depois {depois:?}");
        eprintln!(
            "CONTADORES delta: underruns {} overruns {} enviados {} perdidos_monitor {}; nível máx {max}; erro {:?}",
            depois.underruns - antes.underruns,
            depois.overruns - antes.overruns,
            depois.packets_sent - antes.packets_sent,
            depois.monitor_lost - antes.monitor_lost,
            eng.error()
        );
        eprintln!("CONTADORES latência estimada (monitor, voz remota): {:?} ms", eng.latency_estimate_ms());
        drop(eng);
    }

    /// Só roda com `cargo test -- --ignored`. Abre entrada e saída reais **sem produzir som** (a saída só
    /// recebe silêncio) e confere mudo/desmudo.
    #[test]
    #[ignore]
    fn dispositivo_real_abre_e_le_nivel() {
        let d = list_devices();
        eprintln!("entradas: {:?}\nsaídas: {:?}", d.inputs, d.outputs);
        let enviados = Arc::new(AtomicUsize::new(0));
        let e2 = enviados.clone();
        let eng = Engine::start(
            Config::default(),
            Box::new(move |_, _| {
                e2.fetch_add(1, Relaxed);
            }),
        )
        .expect("abrir dispositivos");
        eng.set_noise_suppression(false);
        let mut max = 0f32;
        for _ in 0..20 {
            thread::sleep(Duration::from_millis(50));
            max = max.max(eng.mic_level());
        }
        eprintln!("nível máx do microfone em 1 s: {max}; pacotes: {}; stats {:?}", enviados.load(Relaxed), eng.stats());
        eng.set_muted(true);
        thread::sleep(Duration::from_millis(200));
        assert_eq!(eng.mic_level(), 0.0);
        eng.set_muted(false);
        let mut depois = 0f32;
        for _ in 0..20 {
            thread::sleep(Duration::from_millis(50));
            depois = depois.max(eng.mic_level());
        }
        eprintln!("nível máx depois de desmutar: {depois}");
        assert!(depois > 0.0, "a captura deve voltar depois de desmutar");
        drop(eng);
    }
}
