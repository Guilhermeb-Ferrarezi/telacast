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
use opus::{Application, Bitrate, Channels, Decoder, Encoder, Signal};
use rtrb::{Consumer, Producer, RingBuffer};

// ───────────────────────────── constantes ─────────────────────────────

/// Taxa interna de tudo (Opus fullband).
pub const SAMPLE_RATE: u32 = 48_000;
/// Amostras por quadro de 20 ms a 48 kHz.
pub const FRAME: usize = 960;
/// Tamanho máximo de um pacote Opus (igual a `proto::VOICE_MAX_PACKET`).
pub const MAX_PACKET: usize = 400;
/// Bitrate do codificador.
const BITRATE: i32 = 32_000;
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

/// Controle automático de ganho: leva a fala ao nível-alvo (-22 dBFS RMS) com ganho entre 0 e +24 dB
/// (nunca atenua). Só adapta quando há voz (congela no silêncio e no hangover, para não subir o ruído):
/// ataque de ~50 ms para baixar, liberação de ~1 s para subir. O ganho é interpolado dentro do quadro
/// (sem degrau) e termina num limitador suave.
pub struct Agc {
    ligado: bool,
    ganho_db: f32,
}

impl Agc {
    pub const ALVO_DB: f32 = -22.0;
    pub const MAX_DB: f32 = 24.0;
    const JOELHO: f32 = 0.85;

    pub fn new() -> Agc {
        Agc { ligado: true, ganho_db: 0.0 }
    }

    pub fn set_enabled(&mut self, on: bool) {
        self.ligado = on;
    }

    pub fn is_enabled(&self) -> bool {
        self.ligado
    }

    pub fn gain_db(&self) -> f32 {
        self.ganho_db
    }

    pub fn reset(&mut self) {
        self.ganho_db = 0.0;
    }

    /// Adapta o ganho e o aplica a `quadro` (20 ms). `nivel_db` é o RMS do quadro antes do AGC; `fala` diz
    /// se há voz neste quadro.
    pub fn process(&mut self, quadro: &mut [f32], nivel_db: f32, fala: bool) {
        // 20 ms por quadro: coeficientes de 1 polo para τ = 50 ms (ataque) e 1 s (liberação)
        const ATAQUE: f32 = 0.3297; // 1 - exp(-20/50)
        const LIBERACAO: f32 = 0.0198; // 1 - exp(-20/1000)
        let g0 = self.ganho_db;
        let desejado = if !self.ligado {
            0.0
        } else if fala && nivel_db.is_finite() {
            (Self::ALVO_DB - nivel_db).clamp(0.0, Self::MAX_DB)
        } else {
            g0 // congelado
        };
        let k = if desejado < g0 || !self.ligado { ATAQUE } else { LIBERACAO };
        let g1 = (g0 + (desejado - g0) * k).clamp(0.0, Self::MAX_DB);
        self.ganho_db = g1;
        let (l0, l1) = (10f32.powf(g0 / 20.0), 10f32.powf(g1 / 20.0));
        let n = quadro.len().max(1) as f32;
        for (i, x) in quadro.iter_mut().enumerate() {
            let l = l0 + (l1 - l0) * ((i + 1) as f32 / n);
            *x = soft_limit_knee(*x * l, Self::JOELHO);
        }
    }
}

impl Default for Agc {
    fn default() -> Self {
        Self::new()
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

/// Atraso mínimo e máximo do jitter buffer, em quadros de 20 ms (40 e 100 ms).
pub const JB_MIN: usize = 2;
pub const JB_MAX: usize = 5;
const JB_INICIAL: usize = 3;
const JB_MAX_PACOTES: usize = 32;
/// Diferença de seq acima disso (para mais ou para menos) = o remetente recomeçou: ressincroniza.
const JB_RESYNC: i32 = 200;
/// Tantos descartes por atraso seguidos = o remetente recomeçou: ressincroniza.
const JB_ATRASADOS_RESYNC: u32 = 8;
/// Buraco maior que isso é saltado de uma vez em vez de gerar um PLC por quadro perdido.
const JB_BURACO_MAX: i32 = 4;
/// Excesso de profundidade (acima do alvo) a partir do qual descarta o mais antigo para recuperar latência.
const JB_EXCESSO: usize = 4;
/// Quadros de PLC antes de considerar silêncio quando o buffer esvazia.
const JB_PLC_QUADROS: u32 = 2;
/// Quadros sem problema para diminuir o alvo em 1 (5 s).
const JB_CALMA: u32 = 250;
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
/// - Começa a tocar quando há `alvo` quadros guardados; o alvo (2 a 5 quadros) sobe quando há subfluxo ou
///   pacote atrasado e desce depois de 5 s sem problemas.
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
}

impl Default for JitterBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl JitterBuffer {
    pub fn new() -> JitterBuffer {
        JitterBuffer {
            pacotes: Vec::with_capacity(JB_MAX_PACOTES + 1),
            proximo: 0,
            iniciado: false,
            tocando: false,
            virgem: true,
            alvo: JB_INICIAL,
            espera: 0,
            vazios: 0,
            ocioso: u32::MAX / 2,
            atrasados_seguidos: 0,
            calma: 0,
            descartes: 0,
        }
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
        self.alvo = (self.alvo + 1).min(JB_MAX);
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
                }
                self.ocioso = 0;
            } else {
                self.ocioso = self.ocioso.saturating_add(1);
                return Playout::Silence;
            }
        }

        // recupera latência acumulada (rajada, relógio do remetente mais rápido)
        while self.pacotes.len() > self.alvo + JB_EXCESSO {
            self.pula_um();
        }

        let p = self.proximo;
        if let Some(i) = self.pacotes.iter().position(|(s, _)| *s == p) {
            let (_, dados) = self.pacotes.swap_remove(i);
            self.proximo = p.wrapping_add(1);
            self.virgem = false;
            if self.vazios > 0 {
                self.vazios = 0;
                self.sobe_alvo(); // o pacote chegou logo depois de esvaziar: subfluxo
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
            return Playout::Lost;
        }

        // há pacotes mais novos mas o do ponto de reprodução não veio: perdido
        self.vazios = 0;
        self.calma = 0;
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

/// Um usuário remoto: jitter buffer + decodificador Opus + suavização das bordas.
pub struct UserStream {
    jb: JitterBuffer,
    dec: Decoder,
    declick: Declick,
    last_rx: Instant,
}

impl UserStream {
    pub fn new() -> Result<UserStream, String> {
        let dec = Decoder::new(SAMPLE_RATE, Channels::Mono).map_err(|e| format!("decodificador Opus: {e}"))?;
        Ok(UserStream { jb: JitterBuffer::new(), dec, declick: Declick::new(), last_rx: Instant::now() })
    }

    pub fn push(&mut self, seq: u32, dados: Vec<u8>) -> PushOutcome {
        self.last_rx = Instant::now();
        self.jb.push(seq, dados)
    }

    pub fn jitter(&self) -> &JitterBuffer {
        &self.jb
    }

    /// Pacotes descartados pelo jitter buffer desde a última chamada.
    pub fn take_discarded(&mut self) -> u32 {
        self.jb.take_discarded()
    }

    /// Gera os próximos 20 ms em `out` (`FRAME` amostras). `false` = silêncio (`out` zerado).
    pub fn render(&mut self, out: &mut [f32]) -> bool {
        debug_assert!(out.len() >= FRAME);
        let out = &mut out[..FRAME];
        let dados = match self.jb.pop() {
            Playout::Silence => return self.declick.silencio(out),
            Playout::Frame(d) => d,
            Playout::Lost => Vec::new(),
        };
        match self.dec.decode_float(&dados, out, false) {
            Ok(n) => {
                if n < FRAME {
                    out[n..].fill(0.0);
                }
                for x in out.iter_mut() {
                    if !x.is_finite() {
                        *x = 0.0;
                    }
                }
                self.declick.audio(out);
                true
            }
            Err(_) => self.declick.silencio(out),
        }
    }
}

// ───────────────────────────── envio (captura → pacotes) ─────────────────────────────

/// Do quadro de 20 ms capturado ao pacote Opus: ganho, medidor, VAD, codificação e pré-rolagem.
/// Só gera pacotes quando o VAD está aberto (e os 2 quadros anteriores, para não cortar o início da fala).
pub struct SendPipeline {
    enc: Encoder,
    pub vad: Vad,
    /// Controle automático de ganho (depois do RNNoise; o VAD decide sobre o nível *antes* dele).
    pub agc: Agc,
    meter: LevelMeter,
    /// Ganho linear de entrada (1.0 = sem mudança).
    pub gain: f32,
    /// Só para testes com hardware: trata o VAD como sempre aberto.
    pub force_open: bool,
    preroll: VecDeque<Vec<u8>>,
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
            agc: Agc::new(),
            meter: LevelMeter::new(),
            gain: 1.0,
            force_open: false,
            preroll: VecDeque::with_capacity(PREROLL_FRAMES + 1),
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

    /// Esquece o estado (ao ficar mudo): fecha o VAD, zera o medidor e a pré-rolagem.
    pub fn reset(&mut self) {
        self.vad.reset();
        self.meter.reset();
        self.preroll.clear();
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
        if quadro.len() != FRAME {
            return;
        }
        let g = if self.gain.is_finite() { self.gain.clamp(0.0, 8.0) } else { 1.0 };
        for (d, s) in self.buf.iter_mut().zip(quadro) {
            *d = (s * g).clamp(-1.0, 1.0);
        }
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
        // O VAD decide sobre o nível pós-RNNoise e *antes* do AGC: assim os limiares não dependem do ganho
        // que o AGC acumulou. O AGC só adapta com evidência de voz neste quadro.
        let db = rms_db(&self.buf);
        let estava = self.vad.is_open();
        let aberto = self.vad.process_db_prob(db, prob) || self.force_open;
        self.agc.process(&mut self.buf, db, self.vad.evidence());
        self.level = self.meter.update(rms_db(&self.buf));
        self.speaking = aberto;

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

/// "Ouvir meu microfone": decodifica localmente o mesmo pacote que vai para a rede e entrega o PCM à thread
/// do mixer (o usuário ouve exatamente o que os outros ouvem, com o mesmo VAD/hangover/pré-rolagem).
struct Monitor {
    dec: Decoder,
    prod: Producer<f32>,
    buf: Vec<f32>,
    sh: Arc<Shared>,
}

impl Monitor {
    fn new(prod: Producer<f32>, sh: Arc<Shared>) -> Option<Monitor> {
        let dec = Decoder::new(SAMPLE_RATE, Channels::Mono).ok()?;
        Some(Monitor { dec, prod, buf: vec![0.0; FRAME], sh })
    }

    fn feed(&mut self, pacote: &[u8]) {
        if let Ok(n) = self.dec.decode_float(pacote, &mut self.buf, false) {
            let n = n.min(FRAME);
            let (_, resto) = self.prod.push_partial_slice(&self.buf[..n]);
            if !resto.is_empty() {
                self.sh.c_monitor_perdidos.fetch_add(1, Relaxed);
            }
        }
    }
}

/// Thread do codificador: lê a fila de captura, reamostra para 48 kHz e alimenta o [`SendPipeline`].
fn loop_captura(
    sh: Arc<Shared>,
    mut cons: Consumer<f32>,
    taxa: u32,
    mut send: Box<dyn FnMut(u32, &[u8]) + Send>,
    mon_prod: Producer<f32>,
) {
    let mut monitor = Monitor::new(mon_prod, sh.clone());
    let mut monitor_antes = false;
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
    while !sh.parar.load(Relaxed) {
        let n = cons.slots();
        if n == 0 {
            thread::sleep(Duration::from_millis(4));
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
            continue;
        }
        mudo_antes = false;
        rs.process(&cru, &mut a48);
        pipe.gain = f32_from_bits(&sh.ganho);
        pipe.force_open = sh.forcar_vad.load(Relaxed);
        let limite = f32_from_bits(&sh.vad_db);
        pipe.vad.set_open_db(if limite.is_finite() { Some(limite) } else { None });
        pipe.vad.set_sensitivity(f32_from_bits(&sh.vad_sens));
        pipe.agc.set_enabled(sh.agc.load(Relaxed));
        let ruido = sh.ruido.load(Relaxed);
        if ruido != ruido_antes {
            ruido_antes = ruido;
            pipe.set_noise_suppression(ruido);
        }
        // Monitor: só enquanto ligado e sem surdez; ao ligar começa com o decodificador limpo.
        let mon_ligado = sh.monitor.load(Relaxed) && !sh.surdo.load(Relaxed);
        if mon_ligado && !monitor_antes {
            if let Some(m) = monitor.as_mut() {
                let _ = m.dec.reset_state();
            }
        }
        monitor_antes = mon_ligado;
        let mut enviar = |seq: u32, pacote: &[u8]| {
            sh.c_enviados.fetch_add(1, Relaxed);
            send(seq, pacote);
            if mon_ligado {
                if let Some(m) = monitor.as_mut() {
                    m.feed(pacote);
                }
            }
        };
        let mut usados = 0;
        while a48.len() - usados >= FRAME {
            pipe.process(&a48[usados..usados + FRAME], &mut enviar);
            usados += FRAME;
        }
        a48.drain(..usados);
        sh.agc_db.store(pipe.agc.gain_db().to_bits(), Relaxed);
        sh.nivel.store(pipe.level().to_bits(), Relaxed);
        sh.falando.store(pipe.speaking(), Relaxed);
    }
}

/// Lado da reprodução do monitor: fila de quadros do codificador local com alvo de ~80 ms, compensação
/// de deriva entre o relógio do microfone e o da saída (±0,3% de esticamento em vez de cortes) e
/// fade nas bordas. Quando falta dado no meio da fala, faz fade-out em vez de cair a zero.
struct MonitorOut {
    cons: Consumer<f32>,
    tocando: bool,
    declick: Declick,
    cru: Vec<f32>,
}

impl MonitorOut {
    fn new(cons: Consumer<f32>) -> MonitorOut {
        MonitorOut { cons, tocando: false, declick: Declick::new(), cru: vec![0.0; FRAME + MON_AJUSTE] }
    }

    /// Monitor desligado (ou surdo): joga fora o que sobrou e volta ao estado inicial.
    fn drenar(&mut self) {
        let n = self.cons.slots();
        if n > 0 {
            if let Ok(c) = self.cons.read_chunk(n) {
                c.commit_all();
            }
        }
        self.tocando = false;
        self.declick.reset();
    }

    fn descarta_quadro(&mut self, sh: &Shared) {
        if let Ok(c) = self.cons.read_chunk(FRAME) {
            c.commit_all();
            sh.c_monitor_perdidos.fetch_add(1, Relaxed);
        }
    }

    /// Próximo quadro do monitor em `out` (`FRAME` amostras). `true` = há áudio para somar ao mixer.
    fn proximo(&mut self, sh: &Shared, out: &mut [f32]) -> bool {
        while self.cons.slots() > MON_MAX * FRAME {
            self.descarta_quadro(sh);
        }
        let disp = self.cons.slots();
        if !self.tocando {
            if disp < MON_PRIME * FRAME {
                return self.declick.silencio(out);
            }
            self.tocando = true;
        }
        if disp < FRAME {
            // faltou dado no meio da fala: fade-out e volta a juntar quadros
            self.tocando = false;
            sh.c_monitor_perdidos.fetch_add(1, Relaxed);
            return self.declick.silencio(out);
        }
        let n = if disp > MON_ALTO * FRAME {
            FRAME + MON_AJUSTE
        } else if disp < MON_BAIXO * FRAME {
            FRAME - MON_AJUSTE
        } else {
            FRAME
        };
        let Ok(c) = self.cons.read_chunk(n) else {
            return self.declick.silencio(out);
        };
        let (a, b) = c.as_slices();
        self.cru[..a.len()].copy_from_slice(a);
        self.cru[a.len()..a.len() + b.len()].copy_from_slice(b);
        c.commit_all();
        if n == FRAME {
            out[..FRAME].copy_from_slice(&self.cru[..FRAME]);
        } else {
            // estica/aperta n amostras em FRAME por interpolação linear (deriva de clock)
            let passo = (n - 1) as f32 / (FRAME - 1) as f32;
            for (i, o) in out[..FRAME].iter_mut().enumerate() {
                let pos = i as f32 * passo;
                let k = (pos as usize).min(n - 2);
                let f = pos - k as f32;
                *o = self.cru[k] + (self.cru[k + 1] - self.cru[k]) * f;
            }
        }
        for x in out[..FRAME].iter_mut() {
            if !x.is_finite() {
                *x = 0.0;
            }
        }
        self.declick.audio(&mut out[..FRAME]);
        true
    }
}

/// Thread do decodificador/mixer: mantém a fila de reprodução cheia (~40 ms à frente do dispositivo).
fn loop_reproducao(
    sh: Arc<Shared>,
    rx: Receiver<Msg>,
    mut prod: Producer<f32>,
    taxa: u32,
    mon_cons: Consumer<f32>,
) {
    let mut usuarios: HashMap<i64, UserStream> = HashMap::new();
    let mut volumes: HashMap<i64, f32> = HashMap::new();
    let mut mixer = Mixer::new(FRAME);
    let mut rs = Resampler::new(SAMPLE_RATE, taxa);
    let mut tmp = vec![0f32; FRAME];
    let mut saida: Vec<f32> = Vec::with_capacity(FRAME * 8);
    let capacidade = prod.buffer().capacity();
    let mut ultima_limpeza = Instant::now();
    let mut mon = MonitorOut::new(mon_cons);
    let mut tmp_mon = vec![0f32; FRAME];
    let mut extra_ms = 0usize;
    let mut subfluxos_vistos = 0u32;

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
                        match UserStream::new() {
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
            }
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

        // Quanto áudio manter à frente do dispositivo: no mínimo 40 ms e sempre mais que um callback inteiro
        // (o PipeWire/ALSA costuma pedir 2048 quadros = 43 ms de uma vez) mais uma margem. Cada subfluxo
        // observado aumenta a margem em 10 ms (até 80 ms).
        let subf = sh.c_subfluxos.load(Relaxed);
        if subf != subfluxos_vistos {
            subfluxos_vistos = subf;
            extra_ms = (extra_ms + 10).min(PLAYBACK_EXTRA_MAX_MS);
        }
        let cb = sh.cb_saida.load(Relaxed) as usize;
        let alvo = (taxa as usize * PLAYBACK_AHEAD_MS / 1000)
            .max(cb + taxa as usize * (PLAYBACK_MARGEM_MS + extra_ms) / 1000)
            .min(capacidade / 2)
            .max(1);

        // enche a fila de reprodução
        let mut voltas = 0;
        while capacidade.saturating_sub(prod.slots()) < alvo && voltas < 24 {
            let tem_mon = monitor && mon.proximo(&sh, &mut tmp_mon);
            if usuarios.is_empty() && !tem_mon && !sh.manter_saida.load(Relaxed) {
                break;
            }
            voltas += 1;
            mixer.begin();
            for (id, u) in usuarios.iter_mut() {
                if u.render(&mut tmp) {
                    mixer.add(&tmp, volumes.get(id).copied().unwrap_or(1.0));
                }
                let d = u.take_discarded();
                if d > 0 {
                    sh.c_descartados.fetch_add(d, Relaxed);
                }
            }
            if tem_mon {
                mixer.add(&tmp_mon, 1.0);
            }
            let quadro = mixer.finish();
            if taxa == SAMPLE_RATE {
                let _ = prod.push_partial_slice(quadro);
            } else {
                saida.clear();
                rs.process(quadro, &mut saida);
                let _ = prod.push_partial_slice(&saida);
            }
        }
        sh.saida_ativa.store(
            !usuarios.is_empty() || mon.tocando || mon.declick.is_active() || sh.manter_saida.load(Relaxed),
            Relaxed,
        );
        thread::sleep(Duration::from_millis(5));
    }
}

// ───────────────────────────── dispositivos (cpal) ─────────────────────────────

/// Escolha do dispositivo de áudio. `None` = o padrão do sistema.
#[derive(Clone, Debug, Default)]
pub struct Config {
    pub input: Option<String>,
    pub output: Option<String>,
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

fn abre_saida(host: &cpal::Host, cfg: &Config, cons: Consumer<f32>, sh: Arc<Shared>) -> Result<(Stream, u32), String> {
    let dev = acha_saida(host, &cfg.output).ok_or_else(|| "Nenhuma saída de áudio disponível.".to_string())?;
    let faixas = dev.supported_output_configs().ok().map(|i| i.collect::<Vec<_>>());
    let sc = escolhe_config(faixas, dev.default_output_config(), true)?;
    let fmt = sc.sample_format();
    let conf: StreamConfig = sc.config();
    let taxa = conf.sample_rate.0;
    let stream = despacha_formato!(fmt, constroi_saida, &dev, &conf, cons, sh)?;
    Ok((stream, taxa))
}

fn abre_entrada(host: &cpal::Host, cfg: &Config, prod: Producer<f32>, sh: Arc<Shared>) -> Result<(Stream, u32), String> {
    let dev = acha_entrada(host, &cfg.input).ok_or_else(|| "nenhum microfone encontrado".to_string())?;
    let faixas = dev.supported_input_configs().ok().map(|i| i.collect::<Vec<_>>());
    let sc = escolhe_config(faixas, dev.default_input_config(), false)?;
    let fmt = sc.sample_format();
    let conf: StreamConfig = sc.config();
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
    pub fn start(cfg: Config, send: Box<dyn FnMut(u32, &[u8]) + Send>) -> Result<Engine, String> {
        let sh = Arc::new(Shared::new());
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
        let mut threads = Vec::new();
        let mut falha = |e: std::io::Error| format!("Não foi possível iniciar a thread de áudio: {e}");

        if let Some((cons, taxa)) = captura {
            let s = sh.clone();
            threads.push(
                thread::Builder::new()
                    .name("telacast-audio-codificador".into())
                    .spawn(move || loop_captura(s, cons, taxa, send, mon_prod))
                    .map_err(&mut falha)?,
            );
        }
        let s = sh.clone();
        let (prod, taxa) = reproducao;
        threads.push(
            thread::Builder::new()
                .name("telacast-audio-mixer".into())
                .spawn(move || loop_reproducao(s, rx, prod, taxa, mon_cons))
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

    /// Controle automático de ganho do microfone (alvo -22 dBFS, até +24 dB, nunca atenua). Padrão: ligado.
    pub fn set_agc(&self, on: bool) {
        self.sh.agc.store(on, Relaxed);
    }

    pub fn agc(&self) -> bool {
        self.sh.agc.load(Relaxed)
    }

    /// Ganho que o AGC está aplicando agora, em dB (0..=24), para depuração.
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
        assert!(media < 160.0, "32 kbps ≈ 80 bytes por quadro, veio {media}");
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
        assert_eq!(jb.target(), 4, "subfluxo aumenta o atraso");
        // muita calma (fala contínua e estável) diminui devagar
        seq += 1;
        for _ in 0..(JB_CALMA as usize * 3) {
            jb.push(seq, pkt(0));
            seq += 1;
            jb.pop();
        }
        assert!(jb.target() < 4 && jb.target() >= JB_MIN);
    }

    #[test]
    fn jitter_limites_do_alvo() {
        let mut jb = JitterBuffer::new();
        for _ in 0..20 {
            jb.sobe_alvo();
        }
        assert_eq!(jb.target(), JB_MAX);
        assert_eq!(JB_MIN * 20, 40);
        assert_eq!(JB_MAX * 20, 100);
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
        assert_eq!(o, x);
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
    }

    fn banco() -> Banco {
        let sh = Arc::new(Shared::new());
        let (cap, cap_cons) = RingBuffer::<f32>::new(RING_SAMPLES);
        let (play_prod, play) = RingBuffer::<f32>::new(RING_SAMPLES);
        let (tx, pacotes) = mpsc::channel::<(u32, Vec<u8>)>();
        let send = Box::new(move |s: u32, d: &[u8]| {
            let _ = tx.send((s, d.to_vec()));
        });
        let eng = Engine::liga_threads(sh, Some((cap_cons, 48000)), (play_prod, 48000), send).unwrap();
        eng.set_noise_suppression(false);
        Banco { eng, cap, play, pacotes }
    }

    impl Banco {
        /// Alimenta o "microfone" em tempo real (10 ms por vez) e consome a saída como um dispositivo faria.
        fn roda(&mut self, x: &[f32], saida: &mut Vec<f32>) {
            for b in x.chunks(480) {
                let _ = self.cap.push_partial_slice(b);
                thread::sleep(Duration::from_millis(10));
                for _ in 0..480 {
                    saida.push(self.play.pop().unwrap_or(0.0));
                }
            }
        }
    }

    #[test]
    fn monitor_devolve_exatamente_o_que_foi_enviado() {
        let mut b = banco();
        assert!(!b.eng.monitoring() && b.eng.noise_suppression() == false);
        b.eng.set_monitor(true);
        assert!(b.eng.monitoring());
        let mut saida = Vec::new();
        // 0,3 s de silêncio: VAD fechado, nada enviado e nada tocado
        b.roda(&vec![0.0; 14400], &mut saida);
        assert!(b.pacotes.try_recv().is_err());
        assert!(saida.iter().all(|v| v.abs() < 1e-9), "monitor com VAD fechado deve ficar mudo");
        // 1 s de tom
        let tom = seno(600.0, 48000.0, 0.3, 48000, 0);
        b.roda(&tom, &mut saida);
        b.roda(&vec![0.0; 9600], &mut saida);
        let enviados: Vec<Vec<u8>> = b.pacotes.try_iter().map(|(_, d)| d).collect();
        assert!(enviados.len() > 40, "enviou {}", enviados.len());
        // o mesmo que um receptor decodificaria dos pacotes enviados
        let mut dec = Decoder::new(SAMPLE_RATE, Channels::Mono).unwrap();
        let mut esperado = Vec::new();
        let mut q = vec![0f32; FRAME];
        for d in &enviados {
            let n = dec.decode_float(d, &mut q, false).unwrap();
            esperado.extend_from_slice(&q[..n]);
        }
        // alinha pelo primeiro som audível de cada lado
        let primeiro = |x: &[f32]| x.iter().position(|v| v.abs() > 1e-4).expect("há som");
        let k = primeiro(&saida) - primeiro(&esperado);
        let comparar = FRAME * 25;
        let maior = (0..comparar).map(|i| (saida[k + i] - esperado[i]).abs()).fold(0f32, f32::max);
        // tolerância de ~3 LSB de 16 bits: reset_state() do decodificador não zera 100% do estado interno
        assert!(maior < 1e-4, "o monitor tocou algo diferente do enviado (dif máx {maior}, início {k})");
        // Atraso do monitor: o tom começa no microfone em 14400 (0,3 s); no fluxo enviado o início do tom
        // está depois da pré-rolagem de PREROLL_FRAMES quadros. Medido: ~37 ms com o monitor sozinho; com outros falando soma até 40 ms da fila de saída.
        let inicio_tom = esperado.iter().position(|v| v.abs() > 0.05).expect("tom nos pacotes");
        let atraso_ms = ((k + inicio_tom) as f32 - 14400.0) / 48.0;
        eprintln!("atraso do monitor (microfone → saída, sem o buffer do dispositivo): {atraso_ms:.0} ms");
        assert!(atraso_ms < 160.0, "atraso do monitor {atraso_ms} ms");
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

    #[test]
    fn agc_sobe_fala_fraca_para_perto_do_alvo() {
        let mut x = fala_sintetica(FRAME * 300);
        escala_db(&mut x, -40.0);
        let mut p = pipeline_energia();
        let o = processa(&mut p, &x);
        let fim = rms_db(&o[FRAME * 200..]);
        assert!((-27.0..=-19.0).contains(&fim), "saída em {fim} dBFS (alvo -22)");
        assert!(p.agc.gain_db() > 12.0 && p.agc.gain_db() <= Agc::MAX_DB, "ganho {}", p.agc.gain_db());
        assert!(o.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
    }

    #[test]
    fn agc_nao_amplifica_sinal_alto_nem_atenua() {
        let mut x = fala_sintetica(FRAME * 150);
        escala_db(&mut x, -15.0);
        let mut p = pipeline_energia();
        let o = processa(&mut p, &x);
        assert!(p.agc.gain_db() < 0.01, "ganho {}", p.agc.gain_db());
        assert!((rms_db(&o[FRAME * 50..]) - rms_db(&x[FRAME * 50..])).abs() < 1.0);
    }

    #[test]
    fn agc_congela_no_silencio_e_no_ruido() {
        // ruído de -65 dBFS por 10 s: sem voz o ganho nunca sobe
        let mut r = ruido_rosa(0.1, FRAME * 500, 5);
        escala_db(&mut r, -65.0);
        let mut p = pipeline_energia();
        processa(&mut p, &r);
        assert_eq!(p.agc.gain_db(), 0.0, "ruído não pode subir o ganho");
        // fala e depois 5 s de silêncio: o ganho fica onde estava
        let mut x = fala_sintetica(FRAME * 250);
        escala_db(&mut x, -40.0);
        let mut p = pipeline_energia();
        processa(&mut p, &x);
        let g = p.agc.gain_db();
        assert!(g > 10.0);
        processa(&mut p, &vec![0.0; FRAME * 250]);
        assert!((p.agc.gain_db() - g).abs() < 0.01, "ganho mexeu no silêncio: {g} → {}", p.agc.gain_db());
    }

    #[test]
    fn agc_sem_pumping() {
        // tom estável a -40 dBFS: depois de convergir o ganho praticamente não mexe
        let x = seno(300.0, 48000.0, 0.0141, FRAME * 400, 0);
        let mut p = pipeline_energia();
        let mut ganhos = Vec::new();
        for q in x.chunks_exact(FRAME) {
            p.process(q, &mut |_, _| {});
            ganhos.push(p.agc.gain_db());
        }
        assert!((ganhos[399] - 18.0).abs() < 0.6, "convergiu para {}", ganhos[399]);
        let cauda = &ganhos[300..];
        let (mn, mx) = cauda.iter().fold((99f32, -99f32), |(a, b), g| (a.min(*g), b.max(*g)));
        assert!(mx - mn < 0.1, "oscila {mn}..{mx} em regime");
        // fala com sílabas: sobe no máximo ~0,5 dB por quadro (24 dB/s) e a faixa em regime é pequena
        let mut f = fala_sintetica(FRAME * 400);
        escala_db(&mut f, -40.0);
        let mut p = pipeline_energia();
        let mut g = Vec::new();
        for q in f.chunks_exact(FRAME) {
            p.process(q, &mut |_, _| {});
            g.push(p.agc.gain_db());
        }
        let maior_subida = g.windows(2).map(|w| w[1] - w[0]).fold(0f32, f32::max);
        assert!(maior_subida <= 0.5, "subida de {maior_subida} dB num quadro");
        let cauda = &g[200..];
        let (mn, mx) = cauda.iter().fold((99f32, -99f32), |(a, b), v| (a.min(*v), b.max(*v)));
        assert!(mx - mn < 6.0, "faixa do ganho em regime {mn}..{mx}");
    }

    #[test]
    fn agc_desligado_volta_a_zero_sem_degrau() {
        let mut x = seno(300.0, 48000.0, 0.0141, FRAME * 200, 0);
        let mut p = pipeline_energia();
        processa(&mut p, &x);
        assert!(p.agc.gain_db() > 10.0);
        p.agc.set_enabled(false);
        x = seno(300.0, 48000.0, 0.0141, FRAME * 30, 0);
        let o = processa(&mut p, &x);
        assert!(p.agc.gain_db() < 0.5);
        assert!(passo_max(&o) < 0.1, "degrau ao desligar o AGC: {}", passo_max(&o));
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

    /// Alimenta o monitor com um seno contínuo a `razao` quadros por tick de saída (deriva de relógio).
    fn simula_monitor(razao: f32, ticks: usize) -> (Vec<f32>, u32, usize) {
        let sh = Arc::new(Shared::new());
        let (mut prod, cons) = RingBuffer::<f32>::new(FRAME * MON_RING_QUADROS);
        let mut mon = MonitorOut::new(cons);
        let mut fase = 0usize;
        let mut acum = 0.0f32;
        let mut saida = Vec::new();
        let mut out = vec![0f32; FRAME];
        let mut max_prof = 0;
        // burst inicial de pré-rolagem (6 quadros)
        let empurra = |prod: &mut Producer<f32>, fase: &mut usize| {
            let q = seno(400.0, 48000.0, 0.4, FRAME, *fase);
            *fase += FRAME;
            let _ = prod.push_partial_slice(&q);
        };
        for _ in 0..6 {
            empurra(&mut prod, &mut fase);
        }
        for _ in 0..ticks {
            acum += razao;
            while acum >= 1.0 {
                acum -= 1.0;
                empurra(&mut prod, &mut fase);
            }
            let tem = mon.proximo(&sh, &mut out);
            if tem {
                saida.extend_from_slice(&out);
            } else {
                saida.extend(std::iter::repeat(0.0).take(FRAME));
            }
            max_prof = max_prof.max(mon.cons.slots() / FRAME);
        }
        (saida, sh.c_monitor_perdidos.load(Relaxed), max_prof)
    }

    #[test]
    fn monitor_compensa_deriva_sem_cortes_nem_degraus() {
        // deriva real de relógios é ~0,01%; o compensador aguenta até ~0,3%
        for razao in [1.0f32, 1.0001, 0.9999, 1.001, 0.999, 1.003, 0.997] {
            let (saida, perdidos, prof) = simula_monitor(razao, 3000); // 60 s simulados
            assert_eq!(perdidos, 0, "razão {razao}: perdeu quadros do monitor");
            assert!(prof <= MON_MAX, "razão {razao}: fila chegou a {prof} quadros");
            // contínuo: o seno de 0,4 a 400 Hz tem passo natural 0,021; esticar ±0,3% não cria degraus
            let miolo = &saida[FRAME * 10..];
            assert!(passo_max(miolo) < 0.04, "razão {razao}: passo {}", passo_max(miolo));
            assert!(rms(miolo) > 0.25, "razão {razao}: ficou sem som");
        }
    }

    #[test]
    fn monitor_deriva_absurda_degrada_com_graca() {
        // 1% de diferença está acima do que o compensador cobre: descarta quadros, mas a fila fica limitada
        let (saida, perdidos, prof) = simula_monitor(1.01, 3000);
        assert!(perdidos > 0 && prof <= MON_MAX + 1);
        assert!(saida.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
    }

    #[test]
    fn monitor_sem_dados_faz_fade_out_e_volta() {
        let sh = Arc::new(Shared::new());
        let (mut prod, cons) = RingBuffer::<f32>::new(FRAME * MON_RING_QUADROS);
        let mut mon = MonitorOut::new(cons);
        let mut out = vec![0f32; FRAME];
        let mut fase = 0;
        let mut todos = Vec::new();
        for _ in 0..MON_PRIME {
            let q = seno(400.0, 48000.0, 0.4, FRAME, fase);
            fase += FRAME;
            let _ = prod.push_partial_slice(&q);
        }
        for _ in 0..4 {
            assert!(mon.proximo(&sh, &mut out));
            todos.extend_from_slice(&out);
        }
        // acabou: o primeiro quadro sem dado ainda devolve o fade-out, depois silêncio (e conta uma perda)
        assert!(mon.proximo(&sh, &mut out));
        todos.extend_from_slice(&out);
        assert!(!mon.proximo(&sh, &mut out));
        assert_eq!(sh.c_monitor_perdidos.load(Relaxed), 1);
        assert!(passo_max(&todos[FRAME..]) < 0.04, "passo no corte: {}", passo_max(&todos[FRAME..]));
        // volta: junta de novo MON_PRIME quadros antes de tocar
        for _ in 0..MON_PRIME - 1 {
            let q = seno(400.0, 48000.0, 0.4, FRAME, fase);
            fase += FRAME;
            let _ = prod.push_partial_slice(&q);
            assert!(!mon.proximo(&sh, &mut out));
        }
        let q = seno(400.0, 48000.0, 0.4, FRAME, fase);
        let _ = prod.push_partial_slice(&q);
        assert!(mon.proximo(&sh, &mut out));
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

    // ── dispositivo real (manual) ──

    /// Só roda com `cargo test -- --ignored`. Abre o microfone real e uma saída **que não produz som**
    /// (rode com `PIPEWIRE_NODE=<sink nulo>` e `VOICE_LAB_OUT=pipewire`), força o VAD aberto e o monitor
    /// ligado por 10 s e imprime os contadores. Nada audível: a saída é um sink virtual descartável.
    #[test]
    #[ignore]
    fn dispositivo_real_contadores_10s() {
        let saida = std::env::var("VOICE_LAB_OUT").ok();
        let eng = Engine::start(Config { input: None, output: saida }, Box::new(|_, _| {})).expect("abrir");
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
