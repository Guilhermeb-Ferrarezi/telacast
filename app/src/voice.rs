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
/// Capacidade das filas entre os callbacks do cpal e as threads (amostras mono; ≥ 0,5 s a 192 kHz).
const RING_SAMPLES: usize = 96_000;
/// Pré-rolagem: quadros guardados antes de o VAD abrir, para não cortar o começo da fala.
const PREROLL_FRAMES: usize = 2;

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

/// Detector de voz por energia, com histerese, *hangover* e piso de ruído adaptativo.
///
/// Abre quando o nível do quadro passa do limiar de abertura; fica aberto enquanto o nível não cair
/// abaixo do limiar de fechamento (5 dB menor) por mais que o *hangover* (10 quadros = 200 ms).
/// O piso de ruído sobe devagar e desce rápido enquanto fechado; os limiares nunca ficam
/// a menos de 9 dB (abre) e 4 dB (fecha) acima dele.
pub struct Vad {
    open_db: f32,
    piso: f32,
    aberto: bool,
    resto: u32,
}

impl Vad {
    pub const HISTERESE_DB: f32 = 5.0;
    pub const HANGOVER_QUADROS: u32 = 10;
    const MARGEM_ABRE: f32 = 9.0;
    const MARGEM_FECHA: f32 = 4.0;
    pub const PADRAO_DB: f32 = -42.0;

    pub fn new(open_db: f32) -> Vad {
        Vad { open_db: open_db.clamp(-80.0, -10.0), piso: -60.0, aberto: false, resto: 0 }
    }

    pub fn set_open_db(&mut self, db: f32) {
        if db.is_finite() {
            self.open_db = db.clamp(-80.0, -10.0);
        }
    }

    pub fn is_open(&self) -> bool {
        self.aberto
    }

    pub fn reset(&mut self) {
        self.aberto = false;
        self.resto = 0;
    }

    pub fn noise_floor_db(&self) -> f32 {
        self.piso
    }

    pub fn process(&mut self, quadro: &[f32]) -> bool {
        self.process_db(rms_db(quadro))
    }

    /// Mesma coisa a partir do nível já calculado (dBFS).
    pub fn process_db(&mut self, db: f32) -> bool {
        let db = if db.is_finite() { db } else { -120.0 };
        let abre = self.open_db.max(self.piso + Self::MARGEM_ABRE);
        let fecha = (self.open_db - Self::HISTERESE_DB).max(self.piso + Self::MARGEM_FECHA);
        if !self.aberto {
            if db >= abre {
                self.aberto = true;
                self.resto = Self::HANGOVER_QUADROS;
            } else {
                let k = if db < self.piso { 0.25 } else { 0.005 };
                self.piso = (self.piso + (db - self.piso) * k).clamp(-90.0, -35.0);
            }
        } else if db >= fecha {
            self.resto = Self::HANGOVER_QUADROS;
        } else if self.resto > 0 {
            self.resto -= 1;
        } else {
            self.aberto = false;
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
        }
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

/// Um usuário remoto: jitter buffer + decodificador Opus.
pub struct UserStream {
    jb: JitterBuffer,
    dec: Decoder,
    last_rx: Instant,
}

impl UserStream {
    pub fn new() -> Result<UserStream, String> {
        let dec = Decoder::new(SAMPLE_RATE, Channels::Mono).map_err(|e| format!("decodificador Opus: {e}"))?;
        Ok(UserStream { jb: JitterBuffer::new(), dec, last_rx: Instant::now() })
    }

    pub fn push(&mut self, seq: u32, dados: Vec<u8>) -> PushOutcome {
        self.last_rx = Instant::now();
        self.jb.push(seq, dados)
    }

    pub fn jitter(&self) -> &JitterBuffer {
        &self.jb
    }

    /// Gera os próximos 20 ms em `out` (`FRAME` amostras). `false` = silêncio (`out` zerado).
    pub fn render(&mut self, out: &mut [f32]) -> bool {
        debug_assert!(out.len() >= FRAME);
        let out = &mut out[..FRAME];
        let (dados, _) = match self.jb.pop() {
            Playout::Silence => {
                out.fill(0.0);
                return false;
            }
            Playout::Frame(d) => (d, ()),
            Playout::Lost => (Vec::new(), ()),
        };
        match self.dec.decode_float(&dados, out, false) {
            Ok(n) => {
                if n < FRAME {
                    out[n..].fill(0.0);
                }
                true
            }
            Err(_) => {
                out.fill(0.0);
                false
            }
        }
    }
}

// ───────────────────────────── envio (captura → pacotes) ─────────────────────────────

/// Do quadro de 20 ms capturado ao pacote Opus: ganho, medidor, VAD, codificação e pré-rolagem.
/// Só gera pacotes quando o VAD está aberto (e os 2 quadros anteriores, para não cortar o início da fala).
pub struct SendPipeline {
    enc: Encoder,
    pub vad: Vad,
    meter: LevelMeter,
    /// Ganho linear de entrada (1.0 = sem mudança).
    pub gain: f32,
    preroll: VecDeque<Vec<u8>>,
    seq: u32,
    buf: Vec<f32>,
    out: [u8; MAX_PACKET],
    level: f32,
    speaking: bool,
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
            vad: Vad::new(Vad::PADRAO_DB),
            meter: LevelMeter::new(),
            gain: 1.0,
            preroll: VecDeque::with_capacity(PREROLL_FRAMES + 1),
            seq: semente,
            buf: vec![0.0; FRAME],
            out: [0; MAX_PACKET],
            level: 0.0,
            speaking: false,
        })
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
        let db = rms_db(&self.buf);
        self.level = self.meter.update(db);
        let estava = self.vad.is_open();
        let aberto = self.vad.process_db(db);
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
    vad_db: AtomicU32,
    nivel: AtomicU32,
    falando: AtomicBool,
    pendentes: AtomicUsize,
    subfluxos: AtomicU32,
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
            vad_db: AtomicU32::new(Vad::PADRAO_DB.to_bits()),
            nivel: AtomicU32::new(0),
            falando: AtomicBool::new(false),
            pendentes: AtomicUsize::new(0),
            subfluxos: AtomicU32::new(0),
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

/// Thread do codificador: lê a fila de captura, reamostra para 48 kHz e alimenta o [`SendPipeline`].
fn loop_captura(
    sh: Arc<Shared>,
    mut cons: Consumer<f32>,
    taxa: u32,
    mut send: Box<dyn FnMut(u32, &[u8]) + Send>,
) {
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
        pipe.vad.set_open_db(f32_from_bits(&sh.vad_db));
        let mut usados = 0;
        while a48.len() - usados >= FRAME {
            pipe.process(&a48[usados..usados + FRAME], &mut *send);
            usados += FRAME;
        }
        a48.drain(..usados);
        sh.nivel.store(pipe.level().to_bits(), Relaxed);
        sh.falando.store(pipe.speaking(), Relaxed);
    }
}

/// Thread do decodificador/mixer: mantém a fila de reprodução cheia (~40 ms à frente do dispositivo).
fn loop_reproducao(sh: Arc<Shared>, rx: Receiver<Msg>, mut prod: Producer<f32>, taxa: u32) {
    let mut usuarios: HashMap<i64, UserStream> = HashMap::new();
    let mut volumes: HashMap<i64, f32> = HashMap::new();
    let mut mixer = Mixer::new(FRAME);
    let mut rs = Resampler::new(SAMPLE_RATE, taxa);
    let mut tmp = vec![0f32; FRAME];
    let mut saida: Vec<f32> = Vec::with_capacity(FRAME * 8);
    let capacidade = prod.buffer().capacity();
    let alvo = (taxa as usize * PLAYBACK_AHEAD_MS / 1000).max(1);
    let mut ultima_limpeza = Instant::now();

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

        if sh.surdo.load(Relaxed) {
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

        // enche a fila de reprodução
        let mut voltas = 0;
        while !usuarios.is_empty() && capacidade.saturating_sub(prod.slots()) < alvo && voltas < 16 {
            voltas += 1;
            mixer.begin();
            for (id, u) in usuarios.iter_mut() {
                if u.render(&mut tmp) {
                    mixer.add(&tmp, volumes.get(id).copied().unwrap_or(1.0));
                }
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
            if sh.mudo.load(Relaxed) {
                return;
            }
            for quadro in dados.chunks_exact(canais) {
                let mut s = 0.0f32;
                for x in quadro {
                    s += f32::from_sample(*x);
                }
                let _ = prod.push(s * inv);
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
                    0.0
                } else {
                    match cons.pop() {
                        Ok(v) => v,
                        Err(_) => {
                            faltou = true;
                            0.0
                        }
                    }
                };
                let v = T::from_sample(s);
                for o in quadro {
                    *o = v;
                }
            }
            if faltou {
                sh.subfluxos.fetch_add(1, Relaxed);
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
        let mut threads = Vec::new();
        let mut falha = |e: std::io::Error| format!("Não foi possível iniciar a thread de áudio: {e}");

        if let Some((cons, taxa)) = captura {
            let s = sh.clone();
            threads.push(
                thread::Builder::new()
                    .name("telacast-audio-codificador".into())
                    .spawn(move || loop_captura(s, cons, taxa, send))
                    .map_err(&mut falha)?,
            );
        }
        let s = sh.clone();
        let (prod, taxa) = reproducao;
        threads.push(
            thread::Builder::new()
                .name("telacast-audio-mixer".into())
                .spawn(move || loop_reproducao(s, rx, prod, taxa))
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

    /// Ganho do microfone (linear, 0.0..=4.0; 1.0 = sem mudança). Extra em relação ao VOICE.md.
    pub fn set_input_gain(&self, gain: f32) {
        let g = if gain.is_finite() { gain.clamp(0.0, 4.0) } else { 1.0 };
        self.sh.ganho.store(g.to_bits(), Relaxed);
    }

    /// Limiar de abertura do VAD em dBFS (-80..=-10, padrão -42). Extra em relação ao VOICE.md.
    pub fn set_vad_threshold_db(&self, db: f32) {
        if db.is_finite() {
            self.sh.vad_db.store(db.clamp(-80.0, -10.0).to_bits(), Relaxed);
        }
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

    /// Quantas vezes a saída ficou sem áudio pronto (diagnóstico).
    pub fn underruns(&self) -> u32 {
        self.sh.subfluxos.load(Relaxed)
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
        let mut v = Vad::new(-42.0);
        for _ in 0..20 {
            assert!(!v.process_db(-70.0));
        }
        assert!(v.process_db(-20.0), "fala forte abre");
        // cai para silêncio: continua aberto durante o hangover (10 quadros), depois fecha
        for i in 0..Vad::HANGOVER_QUADROS {
            assert!(v.process_db(-70.0), "quadro {i} do hangover");
        }
        assert!(!v.process_db(-70.0));
    }

    #[test]
    fn vad_histerese_nao_pisca() {
        let mut v = Vad::new(-42.0);
        // entre o limiar de fechar (-47) e o de abrir (-42): fechado não abre...
        for _ in 0..30 {
            assert!(!v.process_db(-45.0));
        }
        // ...mas aberto não fecha
        assert!(v.process_db(-30.0));
        for _ in 0..100 {
            assert!(v.process_db(-45.0), "-45 dB mantém aberto (acima de -47)");
        }
        for _ in 0..Vad::HANGOVER_QUADROS + 1 {
            v.process_db(-60.0);
        }
        assert!(!v.is_open());
    }

    #[test]
    fn vad_piso_de_ruido_adapta() {
        // sem adaptação, um estouro de -45 dB abriria o VAD de limiar -60
        assert!(Vad::new(-60.0).process_db(-45.0));
        // com ruído constante de -53 dB (fechado), o piso sobe e o mesmo estouro deixa de abrir
        let mut v = Vad::new(-60.0);
        for _ in 0..3000 {
            assert!(!v.process_db(-53.0));
        }
        assert!(v.noise_floor_db() > -55.0, "piso {}", v.noise_floor_db());
        assert!(!v.process_db(-45.0));
        assert!(v.process_db(-30.0), "fala de verdade ainda abre");
        // quadros reais de ruído e de fala por RMS
        let mut v = Vad::new(Vad::PADRAO_DB);
        assert!(!v.process(&vec![0.0005; FRAME]));
        assert!(v.process(&seno(200.0, 48000.0, 0.2, FRAME, 0)));
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

    // ── dispositivo real (manual) ──

    /// Só roda com `cargo test -- --ignored`. Abre entrada e saída reais, **sem produzir som**
    /// (nada é decodificado nem tocado: a saída só recebe silêncio), e lê o nível do microfone por 1 s.
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
        let mut max = 0f32;
        for _ in 0..20 {
            thread::sleep(Duration::from_millis(50));
            max = max.max(eng.mic_level());
        }
        eprintln!("nível máx do microfone em 1 s: {max}; pacotes enviados: {}", enviados.load(Relaxed));
        eprintln!("erro: {:?}; subfluxos: {}", eng.error(), eng.underruns());
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
