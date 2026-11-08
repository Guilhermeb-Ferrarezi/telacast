//! DSP da voz que não depende de dispositivo nem de Opus (puro, testável): passa-alta de 80 Hz, ganho de entrada
//! automático antes do RNNoise ([`PreAgc`]), AGC2 igual ao do Discord ([`Agc2`]) e o esticador/comprimidor de tempo
//! do receptor ([`Esticador`]). Portado de `android/.../voz/Dsp.kt` e `Agc2.kt` (mesmos parâmetros), com a análise do
//! AGC2 nativo em `docs/discord/11-agc2-nativo.md`. Amostras `f32` em -1.0..1.0, quadros de 20 ms (960) a 48 kHz.

use crate::voice::{rms_db, soft_limit_knee, FRAME};

// ───────────────────────────── passa-alta ─────────────────────────────

/// Passa-alta de 2ª ordem (Butterworth, Q = 0,7071) em 80 Hz a 48 kHz: tira ronco de mesa, vento e o "pop" de baixa
/// frequência antes de o RNNoise, o VAD e o Opus verem o sinal. Atenua 40 Hz em 12,3 dB e deixa 1 kHz intacto
/// (a fala útil começa em ~100 Hz). Estado em `f64` (poucos coeficientes, custo desprezível).
pub struct PassaAlta {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}

impl PassaAlta {
    pub const FREQ_HZ: f64 = 80.0;

    pub fn new() -> PassaAlta {
        Self::com_frequencia(Self::FREQ_HZ, crate::voice::SAMPLE_RATE as f64)
    }

    pub fn com_frequencia(f0: f64, taxa: f64) -> PassaAlta {
        let w0 = 2.0 * std::f64::consts::PI * f0 / taxa;
        let alfa = w0.sin() / (2.0 * std::f64::consts::FRAC_1_SQRT_2);
        let c = w0.cos();
        let a0 = 1.0 + alfa;
        PassaAlta {
            b0: (1.0 + c) / 2.0 / a0,
            b1: -(1.0 + c) / a0,
            b2: (1.0 + c) / 2.0 / a0,
            a1: -2.0 * c / a0,
            a2: (1.0 - alfa) / a0,
            z1: 0.0,
            z2: 0.0,
        }
    }

    pub fn reset(&mut self) {
        self.z1 = 0.0;
        self.z2 = 0.0;
    }

    /// Filtra `q` no lugar (forma transposta II).
    pub fn process(&mut self, q: &mut [f32]) {
        for x in q.iter_mut() {
            let e = if x.is_finite() { *x as f64 } else { 0.0 };
            let y = self.b0 * e + self.z1;
            self.z1 = self.b1 * e - self.a1 * y + self.z2;
            self.z2 = self.b2 * e - self.a2 * y;
            *x = y as f32;
        }
    }
}

impl Default for PassaAlta {
    fn default() -> Self {
        Self::new()
    }
}

// ───────────────────────────── pré-AGC ─────────────────────────────

/// Ganho de entrada automático ANTES do supressor de ruído. O RNNoise trata fala baixa (ou longe do microfone) como
/// ruído e a corta, e o VAD que depende dele fecha: por isso a voz precisa chegar ao RNNoise num nível normal. Aqui não
/// há alvo de volume final (isso é do [`Agc2`], depois): só se traz o sinal para perto de [`PreAgc::ALVO_DB`].
///
/// - "evidência de fala" = nível acima do piso de ruído estimado + [`PreAgc::MARGEM_DB`] (e acima de
///   [`PreAgc::MINIMO_DB`]); o piso é o mínimo dos últimos 1,5 s, então ruído constante de fundo não conta como fala;
/// - com evidência o ganho sobe depressa (~100 ms) até `ALVO - nível` (no máximo +30 dB); sem evidência desce devagar
///   (~4 dB/s), para o ruído do silêncio não ser amplificado por muito tempo;
/// - o ganho muda de forma contínua dentro do quadro e termina num limitador suave.
pub struct PreAgc {
    pub ligado: bool,
    ganho_db: f32,
    piso_db: f32,
    janela: [f32; Self::JANELA],
    preenchidos: usize,
    pos: usize,
}

impl PreAgc {
    pub const JANELA: usize = 75;
    pub const ALVO_DB: f32 = -30.0;
    pub const MAX_DB: f32 = 30.0;
    pub const MINIMO_DB: f32 = -68.0;
    pub const MARGEM_DB: f32 = 6.0;
    pub const SUBIDA: f32 = 0.18;
    pub const DESCIDA: f32 = 0.3;
    pub const QUEDA_DB_POR_QUADRO: f32 = 0.08;
    const JOELHO: f32 = 0.85;

    pub fn new() -> PreAgc {
        PreAgc { ligado: true, ganho_db: 0.0, piso_db: -70.0, janela: [0.0; Self::JANELA], preenchidos: 0, pos: 0 }
    }

    pub fn gain_db(&self) -> f32 {
        self.ganho_db
    }

    #[allow(dead_code)] // diagnóstico (usado nos testes)
    pub fn floor_db(&self) -> f32 {
        self.piso_db
    }

    pub fn reset(&mut self) {
        self.ganho_db = 0.0;
        self.piso_db = -70.0;
        self.preenchidos = 0;
        self.pos = 0;
    }

    /// Aplica o ganho a `q` (20 ms, no lugar). Devolve o nível (dBFS) medido ANTES do ganho.
    pub fn process(&mut self, q: &mut [f32]) -> f32 {
        let db = rms_db(q);
        let g0 = self.ganho_db;
        if !self.ligado {
            // desligado: o ganho volta a 0 dB com rampa dentro deste quadro (sem degrau)
            self.ganho_db = 0.0;
        } else {
            self.janela[self.pos] = db;
            self.pos = (self.pos + 1) % Self::JANELA;
            self.preenchidos = (self.preenchidos + 1).min(Self::JANELA);
            let mn = self.janela[..self.preenchidos].iter().copied().fold(f32::MAX, f32::min);
            self.piso_db = mn.clamp(-90.0, -35.0);
            let evidencia = db > Self::MINIMO_DB && db > self.piso_db + Self::MARGEM_DB;
            self.ganho_db = if evidencia {
                let desejado = (Self::ALVO_DB - db).clamp(0.0, Self::MAX_DB);
                let k = if desejado > g0 { Self::SUBIDA } else { Self::DESCIDA };
                (g0 + (desejado - g0) * k).clamp(0.0, Self::MAX_DB)
            } else {
                (g0 - Self::QUEDA_DB_POR_QUADRO).max(0.0)
            };
        }
        let g1 = self.ganho_db;
        if g0 == 0.0 && g1 == 0.0 {
            return db;
        }
        let (l0, l1) = (10f32.powf(g0 / 20.0), 10f32.powf(g1 / 20.0));
        let n = q.len().max(1) as f32;
        for (i, x) in q.iter_mut().enumerate() {
            let l = l0 + (l1 - l0) * ((i + 1) as f32 / n);
            *x = soft_limit_knee(*x * l, Self::JOELHO);
        }
        db
    }
}

impl Default for PreAgc {
    fn default() -> Self {
        Self::new()
    }
}

// ───────────────────────────── AGC2 ─────────────────────────────

/// Controle de ganho adaptativo digital igual ao AGC2 do WebRTC que o Discord usa (m146), lido do `discord_voice.node`
/// (`docs/discord/11-agc2-nativo.md`, seção 5). Trabalha em quadros de 10 ms (480 amostras) com a probabilidade de
/// fala `p` de um VAD (aqui a do RNNoise; sem RNNoise, 1 quando o VAD de energia vê voz):
///
/// - estimador de nível de fala (só confirma rajadas de ≥ 12 quadros com `p` alto, média ponderada por `p`);
/// - margem entre o pico e o nível médio da fala (12 a 25 dB, começa em 20);
/// - estimador de ruído (mínimo de energia em janelas de 5 s);
/// - ganho-alvo: leva o pico da fala a -5 dBFS (headroom), no máximo +50 dB, e nunca acima do que deixaria o ruído de
///   saída passando de -50 dBFS; sobe no máximo 6 dB/s (e só depois de 12 quadros de fala), desce a 6 dB/s;
/// - limitador suave no fim (transparente até -0,75 dBFS, nunca passa de 0 dBFS).
///
/// Diferenças para o Discord: o ganho começa em 0 dB (ele começa em +15, o que estoura nos primeiros segundos) e o
/// limiar de fala é [`Agc2::LIMIAR_P`] (0,95 no WebRTC, calibrado para o RNN VAD deles; o do RNNoise é menos "cravado").
pub struct Agc2 {
    pub ligado: bool,
    ganho_db: f32,
    // estimador de nível de fala
    fala: f64,
    num: f64,
    den: f64,
    confianca: i32,
    t_num: f64,
    t_den: f64,
    t_conf: i32,
    rajada: u32,
    // protetor de saturação (margem)
    margem: f64,
    max_pico: f64,
    quadros_no_periodo: u32,
    fila: [f64; 4],
    n_fila: usize,
    // ruído
    ruido: f64,
    energia_min: f64,
    quadros_janela: u32,
    // ganho
    contador: u32,
}

impl Agc2 {
    /// Amostras por quadro de 10 ms.
    pub const SUB: usize = 480;
    pub const HEADROOM: f64 = 5.0;
    pub const MAX_GANHO: f64 = 50.0;
    pub const MAX_RUIDO: f64 = -50.0;
    pub const PASSO_DB: f64 = 0.06;
    pub const ESPERA_QUADROS: u32 = 12;
    pub const MIN_RAJADA: u32 = 12;
    pub const INICIAL_FALA_DB: f64 = -40.0;
    pub const LIMIAR_P: f32 = 0.9;
    pub const DB_MIN: f64 = -90.309;
    /// -0,75 dBFS
    pub const LIMITE: f32 = 0.917;
    const PISO_ENERGIA: f64 = 4.0 * Self::SUB as f64;

    pub fn new() -> Agc2 {
        Agc2 {
            ligado: true,
            ganho_db: 0.0,
            fala: Self::INICIAL_FALA_DB,
            num: 0.0,
            den: 0.0,
            confianca: 400,
            t_num: 0.0,
            t_den: 0.0,
            t_conf: 400,
            rajada: 0,
            margem: 20.0,
            max_pico: Self::DB_MIN,
            quadros_no_periodo: 0,
            fila: [0.0; 4],
            n_fila: 0,
            ruido: -84.29,
            energia_min: f64::MAX,
            quadros_janela: 0,
            contador: Self::ESPERA_QUADROS,
        }
    }

    pub fn gain_db(&self) -> f32 {
        self.ganho_db
    }
    #[allow(dead_code)] // diagnósticos (usados nos testes)
    pub fn speech_level_db(&self) -> f32 {
        self.fala as f32
    }
    #[allow(dead_code)]
    pub fn margin_db(&self) -> f32 {
        self.margem as f32
    }
    #[allow(dead_code)]
    pub fn noise_db(&self) -> f32 {
        self.ruido as f32
    }

    pub fn reset(&mut self) {
        let ligado = self.ligado;
        *self = Agc2::new();
        self.ligado = ligado;
    }

    /// dBFS de um valor em escala de 16 bits (±32768).
    pub fn dbfs(x: f64) -> f64 {
        if x > 1.0 {
            20.0 * x.log10() - 90.309
        } else {
            -90.309
        }
    }

    /// Aplica o ganho e o limitador a `q` (no lugar, múltiplos de 10 ms). `p` é a probabilidade de fala (0..1).
    pub fn process(&mut self, q: &mut [f32], p: f32) {
        let p = if p.is_finite() { p } else { 0.0 };
        let mut i = 0;
        while i + Self::SUB <= q.len() {
            self.quadro(&mut q[i..i + Self::SUB], p);
            i += Self::SUB;
        }
    }

    fn quadro(&mut self, q: &mut [f32], p: f32) {
        let mut soma = 0.0f64;
        let mut pico = 0.0f64;
        for &v in q.iter() {
            let x = v as f64 * 32768.0;
            soma += x * x;
            pico = pico.max(x.abs());
        }
        let n = Self::SUB as f64;
        let rms_db = Self::dbfs((soma / n).sqrt());
        let pico_db = Self::dbfs(pico);
        let eh_fala = p >= Self::LIMIAR_P;

        // ruído: mínimo de energia por janela de 500 quadros (piso = RMS 2 em escala de 16 bits)
        self.energia_min = self.energia_min.min(soma.max(Self::PISO_ENERGIA));
        self.quadros_janela += 1;
        if self.quadros_janela >= 500 {
            let prel = 10.0 * (self.energia_min / n).log10() - 90.309;
            self.ruido = if self.ruido < prel { 0.5 * (self.ruido + prel) } else { prel };
            self.energia_min = f64::MAX;
            self.quadros_janela = 0;
        }
        // nível de fala (só confirma rajadas de 12+ quadros)
        let pd = p as f64;
        if eh_fala {
            if self.rajada == 0 {
                self.t_num = self.num;
                self.t_den = self.den;
                self.t_conf = self.confianca;
            }
            self.rajada += 1;
            let vaza = if self.t_conf > 0 { 1.0 } else { 0.9975 };
            self.t_conf = (self.t_conf - 10).max(0);
            self.t_num = self.t_num * vaza + rms_db * pd;
            self.t_den = self.t_den * vaza + pd;
            if self.rajada >= Self::MIN_RAJADA {
                self.num = self.t_num;
                self.den = self.t_den;
                self.confianca = self.t_conf;
                if self.den > 0.0 {
                    self.fala = (self.num / self.den).clamp(-90.0, 30.0);
                }
            }
        } else {
            self.rajada = 0;
        }
        // margem (protetor de saturação), só com fala confirmada
        if eh_fala && self.rajada >= Self::MIN_RAJADA {
            self.max_pico = self.max_pico.max(pico_db);
            self.quadros_no_periodo += 1;
            if self.quadros_no_periodo > 40 {
                if self.n_fila == 4 {
                    self.fila.copy_within(1..4, 0);
                    self.n_fila = 3;
                }
                self.fila[self.n_fila] = self.max_pico;
                self.n_fila += 1;
                self.max_pico = Self::DB_MIN;
                self.quadros_no_periodo = 0;
            }
            let referencia = if self.n_fila == 0 { self.max_pico } else { self.fila[0] };
            let delta = referencia - self.fala;
            self.margem = if delta > self.margem {
                0.998849 * self.margem + 0.001151 * delta
            } else {
                0.99977 * self.margem + 0.00023 * delta
            };
            self.margem = self.margem.clamp(12.0, 25.0);
        }
        // ganho-alvo
        let entrada = self.fala + self.margem;
        let mut alvo = if entrada < -(Self::HEADROOM + Self::MAX_GANHO) {
            Self::MAX_GANHO
        } else if entrada < -Self::HEADROOM {
            -Self::HEADROOM - entrada
        } else {
            0.0
        };
        alvo = alvo.min((Self::MAX_RUIDO - self.ruido).max(0.0));
        if !self.ligado {
            alvo = 0.0;
        }
        // passo: só sobe depois de 12 quadros de fala seguidos; nunca mais que 6 dB/s
        let antes = self.contador;
        if !eh_fala {
            self.contador = Self::ESPERA_QUADROS;
        } else if self.contador > 0 {
            self.contador -= 1;
        }
        let max_sobe = if self.contador == 0 {
            if antes > 0 {
                Self::ESPERA_QUADROS as f64 * Self::PASSO_DB
            } else {
                Self::PASSO_DB
            }
        } else {
            0.0
        };
        let g_atual = self.ganho_db as f64;
        let mut d = alvo - g_atual;
        if max_sobe == 0.0 {
            d = d.min(0.0);
        }
        d = d.clamp(-Self::PASSO_DB, max_sobe);
        let g0 = self.ganho_db;
        self.ganho_db = ((g_atual + d) as f32).clamp(0.0, Self::MAX_GANHO as f32);
        Self::aplicar(q, g0, self.ganho_db);
    }

    fn aplicar(q: &mut [f32], g0: f32, g1: f32) {
        let (l0, l1) = (10f32.powf(g0 / 20.0), 10f32.powf(g1 / 20.0));
        if l0 == 1.0 && l1 == 1.0 {
            return;
        }
        let n = q.len() as f32;
        for (k, v) in q.iter_mut().enumerate() {
            let l = l0 + (l1 - l0) * ((k + 1) as f32 / n);
            *v = soft_limit_knee(*v * l, Self::LIMITE).clamp(-1.0, 1.0);
        }
    }
}

impl Default for Agc2 {
    fn default() -> Self {
        Self::new()
    }
}

// ───────────────────────────── esticar / comprimir o tempo ─────────────────────────────

/// O que fazer com o tempo do próximo quadro de saída.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Escala {
    /// Reproduz do jeito que veio.
    #[default]
    Normal,
    /// Toca mais rápido (consome mais entrada que saída) para gastar o atraso acumulado no buffer.
    Comprime,
    /// Toca mais devagar (consome menos entrada que saída) para o buffer não esvaziar.
    Estica,
}

/// Esticador/comprimidor de tempo para fala (SOLA com busca de correlação, um corte por quadro de 20 ms).
///
/// Cada quadro de saída tem [`FRAME`] amostras. Para comprimir, o quadro copia a entrada até `FRAME - L`, e nas últimas `L`
/// amostras faz um *crossfade* entre a continuação natural e o trecho `P` amostras adiante, com `P` o período que
/// maximiza a correlação normalizada (um período de pitch, 100 a 576 amostras = 83 a 480 Hz): o próximo quadro começa
/// em `FRAME + P`, ou seja, **remove um período** sem descontinuidade. Para esticar é o espelho: o *crossfade* usa o
/// trecho `P` amostras ATRÁS e o próximo quadro começa em `FRAME - P` (**repete um período**).
///
/// Um "crédito" limita a velocidade média a [`Esticador::TAXA_MAX`] (20%): o corte só acontece quando o crédito cobre o
/// período achado. Em silêncio (as duas janelas abaixo de -55 dBFS) o comprimidor pula um quadro inteiro, que não se
/// ouve, em vez de procurar pitch. Sem corte, o quadro sai idêntico à entrada (não acrescenta latência nem som).
pub struct Esticador {
    buf: Vec<f32>,
    credito: f32,
    /// Amostras removidas / repetidas desde o início (para as estatísticas e os testes).
    pub removidas: u64,
    pub repetidas: u64,
}

impl Esticador {
    /// Janela de correlação = duração do *crossfade* (6 ms).
    pub const L: usize = 288;
    pub const PMIN: usize = 100;
    pub const PMAX: usize = 576;
    /// Velocidade média máxima acima/abaixo do normal.
    pub const TAXA_MAX: f32 = 0.20;
    const SILENCIO_DB: f32 = -55.0;

    pub fn new() -> Esticador {
        Esticador { buf: Vec::with_capacity(FRAME * 4), credito: 0.0, removidas: 0, repetidas: 0 }
    }

    /// Acrescenta áudio decodificado.
    pub fn push(&mut self, x: &[f32]) {
        self.buf.extend_from_slice(x);
    }

    /// Amostras guardadas à frente.
    pub fn disponivel(&self) -> usize {
        self.buf.len()
    }

    pub fn clear(&mut self) {
        self.buf.clear();
        self.credito = 0.0;
    }

    /// Copia o que sobrou (menos que um quadro) para o começo de `out` e esvazia. Devolve quantas amostras.
    pub fn drain_all(&mut self, out: &mut [f32]) -> usize {
        let n = self.buf.len().min(out.len());
        out[..n].copy_from_slice(&self.buf[..n]);
        self.clear();
        n
    }

    /// Quantas amostras precisam estar disponíveis para o modo pedido.
    pub fn precisa(modo: Escala) -> usize {
        match modo {
            Escala::Comprime => 2 * FRAME,
            _ => FRAME,
        }
    }

    /// Gera `FRAME` amostras em `out`. `taxa` (0..=0.2) é a fração de velocidade desejada. Devolve o que de fato
    /// foi feito (`Normal` se o crédito ainda não cobre um período, se é silêncio ao esticar ou se faltam dados);
    /// `None` se há menos que um quadro guardado (nada é consumido).
    pub fn pull(&mut self, out: &mut [f32], modo: Escala, taxa: f32) -> Option<Escala> {
        let n = FRAME;
        if self.buf.len() < n || out.len() < n {
            return None;
        }
        let modo = if modo == Escala::Comprime && self.buf.len() < Self::precisa(modo) { Escala::Normal } else { modo };
        let taxa = if taxa.is_finite() { taxa.clamp(0.0, Self::TAXA_MAX) } else { 0.0 };
        let l = Self::L;
        let m = n - l;
        let mut feito = Escala::Normal;
        let mut consumido = n;
        out[..m].copy_from_slice(&self.buf[..m]);
        match modo {
            Escala::Normal => {
                self.credito = 0.0;
                out[m..n].copy_from_slice(&self.buf[m..n]);
            }
            Escala::Comprime => {
                self.credito = (self.credito + taxa * n as f32).min(Self::PMAX as f32);
                let a = &self.buf[m..m + l];
                let silencio = rms_db(a) < Self::SILENCIO_DB && rms_db(&self.buf[m + n..m + n + l]) < Self::SILENCIO_DB;
                let p = if silencio {
                    Some(n)
                } else {
                    let p = Self::acha_periodo(a, |p| &self.buf[m + p..m + p + l], Self::PMIN, Self::PMAX);
                    (p as f32 <= self.credito).then_some(p)
                };
                match p {
                    Some(p) => {
                        for i in 0..l {
                            let w = Self::peso(i, l);
                            out[m + i] = self.buf[m + i] * (1.0 - w) + self.buf[m + p + i] * w;
                        }
                        consumido = n + p;
                        feito = Escala::Comprime;
                        if !silencio {
                            self.credito -= p as f32;
                        }
                        self.removidas += p as u64;
                    }
                    None => out[m..n].copy_from_slice(&self.buf[m..n]),
                }
            }
            Escala::Estica => {
                self.credito = (self.credito + taxa * n as f32).min(Self::PMAX as f32);
                let a = &self.buf[m..m + l];
                let silencio = rms_db(a) < Self::SILENCIO_DB;
                let p = if silencio { None } else { Some(Self::acha_periodo(a, |p| &self.buf[m - p..m - p + l], Self::PMIN, Self::PMAX)) };
                match p {
                    Some(p) if p as f32 <= self.credito => {
                        for i in 0..l {
                            let w = Self::peso(i, l);
                            out[m + i] = self.buf[m + i] * (1.0 - w) + self.buf[m - p + i] * w;
                        }
                        consumido = n - p;
                        feito = Escala::Estica;
                        self.credito -= p as f32;
                        self.repetidas += p as u64;
                    }
                    _ => out[m..n].copy_from_slice(&self.buf[m..n]),
                }
            }
        }
        self.buf.drain(..consumido);
        Some(feito)
    }

    /// Janela de subida do *crossfade* (cosseno elevado, soma das duas vias = 1).
    fn peso(i: usize, l: usize) -> f32 {
        0.5 - 0.5 * (std::f32::consts::PI * (i as f32 + 0.5) / l as f32).cos()
    }

    /// Período (em amostras) em `pmin..=pmax` que maximiza a correlação normalizada entre `a` e `candidato(p)`; o primeiro
    /// pico local com pelo menos 90% do máximo (evita escolher o dobro do período).
    fn acha_periodo<'a>(a: &[f32], candidato: impl Fn(usize) -> &'a [f32], pmin: usize, pmax: usize) -> usize {
        let ea: f32 = a.iter().map(|v| v * v).sum::<f32>() + 1e-9;
        let mut pontos = [0f32; Self::PMAX + 1];
        let mut melhor = 0.0f32;
        for p in pmin..=pmax {
            let b = candidato(p);
            let (mut c, mut eb) = (0f32, 1e-9f32);
            for (x, y) in a.iter().zip(b) {
                c += x * y;
                eb += y * y;
            }
            let s = c / (ea * eb).sqrt();
            pontos[p] = s;
            melhor = melhor.max(s);
        }
        let limiar = if melhor > 0.0 { melhor * 0.9 } else { melhor };
        // primeiro pico local acima do limiar (o pico do dobro do período também passa, mas vem depois)
        (pmin + 1..pmax).find(|&p| pontos[p] >= limiar && pontos[p] >= pontos[p - 1] && pontos[p] >= pontos[p + 1]).unwrap_or_else(|| {
            (pmin..=pmax).max_by(|a, b| pontos[*a].total_cmp(&pontos[*b])).unwrap_or(pmin)
        })
    }
}

impl Default for Esticador {
    fn default() -> Self {
        Self::new()
    }
}

// ───────────────────────────── cancelamento de eco ─────────────────────────────

/// Cancelador de eco acústico opcional: AEC3 do WebRTC (a porta em Rust puro `sonora`), só o cancelador (sem o NS nem o
/// AGC2 dela). Recebe o que o app está tocando (`referência`) no mesmo compasso do microfone e acha sozinho o atraso do
/// caminho (alto-falante → sala → microfone, de ~50 a ~500 ms). Trabalha em blocos de 10 ms a 48 kHz (480 amostras).
/// Ligado só quando o usuário pede: custa ~5,7 MB de memória e ~0,8% de um núcleo.
pub struct Eco {
    apm: sonora::AudioProcessing,
    ref_buf: [f32; Eco::BLOCO],
    ref_out: [f32; Eco::BLOCO],
    mic_in: [f32; Eco::BLOCO],
    mic_out: [f32; Eco::BLOCO],
}

/// Números do cancelador para o painel de estatísticas.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EcoStats {
    /// ERLE: quanto do eco o filtro removeu, em dB (só existe depois de o filtro convergir com fala do outro lado).
    pub erle_db: Option<f32>,
    /// Atraso do caminho do eco estimado, em ms.
    pub delay_ms: Option<i32>,
}

impl Eco {
    pub const BLOCO: usize = 480;

    pub fn new() -> Eco {
        use sonora::config::{EchoCanceller, MaxProcessingRate, Pipeline};
        let cfg = sonora::Config {
            pipeline: Pipeline { maximum_internal_processing_rate: MaxProcessingRate::Rate48kHz, ..Default::default() },
            // a passa-alta de 80 Hz já foi aplicada antes (ver `PassaAlta`)
            echo_canceller: Some(EchoCanceller { enforce_high_pass_filtering: false, ..Default::default() }),
            ..Default::default()
        };
        let apm = sonora::AudioProcessing::builder()
            .config(cfg)
            .capture_config(sonora::StreamConfig::new(48_000, 1))
            .render_config(sonora::StreamConfig::new(48_000, 1))
            .build();
        Eco { apm, ref_buf: [0.0; Self::BLOCO], ref_out: [0.0; Self::BLOCO], mic_in: [0.0; Self::BLOCO], mic_out: [0.0; Self::BLOCO] }
    }

    /// Processa `mic` no lugar; `referencia` é o que o app tocou no mesmo intervalo (mesmo tamanho, múltiplo de 480).
    pub fn process(&mut self, mic: &mut [f32], referencia: &[f32]) {
        for (m, r) in mic.chunks_exact_mut(Self::BLOCO).zip(referencia.chunks_exact(Self::BLOCO)) {
            self.ref_buf.copy_from_slice(r);
            let _ = self.apm.process_render_f32(&[&self.ref_buf], &mut [&mut self.ref_out]);
            self.mic_in.copy_from_slice(m);
            if self.apm.process_capture_f32(&[&self.mic_in], &mut [&mut self.mic_out]).is_ok() {
                for (d, s) in m.iter_mut().zip(&self.mic_out) {
                    *d = if s.is_finite() { s.clamp(-1.0, 1.0) } else { 0.0 };
                }
            }
        }
    }

    pub fn stats(&self) -> EcoStats {
        let s = self.apm.statistics();
        EcoStats { erle_db: s.echo_return_loss_enhancement.map(|v| v as f32), delay_ms: s.delay_ms }
    }
}

// ───────────────────────────── jitter de chegada ─────────────────────────────

/// Jitter de chegada dos pacotes (RFC 3550): `J += (|D| - J) / 16`, com `D` = diferença entre o intervalo de chegada e o
/// intervalo esperado pelo `seq` (20 ms por pacote). Pausas de fala (o remetente não manda nada com o VAD fechado, mas o
/// `seq` continua contíguo) são ignoradas: intervalo maior que 200 ms recomeça a referência.
#[derive(Default)]
pub struct JitterChegada {
    ultimo: Option<(f64, u32)>,
    j: f64,
}

impl JitterChegada {
    pub fn new() -> JitterChegada {
        JitterChegada::default()
    }

    /// `t_ms`: relógio monotônico em ms; `seq`: número do pacote.
    pub fn update(&mut self, t_ms: f64, seq: u32) {
        if let Some((t0, s0)) = self.ultimo {
            let ds = seq.wrapping_sub(s0) as i32;
            if ds <= 0 {
                return; // atrasado/duplicado: não serve de referência
            }
            let dt = t_ms - t0;
            if dt > 200.0 || ds > 10 {
                self.ultimo = Some((t_ms, seq));
                return;
            }
            let d = (dt - ds as f64 * 20.0).abs();
            self.j += (d - self.j) / 16.0;
        }
        self.ultimo = Some((t_ms, seq));
    }

    pub fn ms(&self) -> f32 {
        self.j as f32
    }
}

// ───────────────────────────── testes ─────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::PI;

    // ── passa-alta ──

    fn seno(freq: f32, amp: f32, n: usize) -> Vec<f32> {
        (0..n).map(|i| amp * (2.0 * PI * freq * i as f32 / 48000.0).sin()).collect()
    }

    fn rms(x: &[f32]) -> f32 {
        (x.iter().map(|v| v * v).sum::<f32>() / x.len().max(1) as f32).sqrt()
    }

    fn ganho_db_do_filtro(freq: f32) -> f32 {
        let mut f = PassaAlta::new();
        let mut x = seno(freq, 0.3, 48000);
        let entrada = rms(&x[24000..]);
        f.process(&mut x);
        20.0 * (rms(&x[24000..]) / entrada).log10()
    }

    #[test]
    fn passa_alta_atenua_40_hz_e_preserva_1_khz() {
        let g40 = ganho_db_do_filtro(40.0);
        assert!(g40 <= -12.0, "40 Hz deveria cair ≥ 12 dB, veio {g40:.2} dB");
        let g1k = ganho_db_do_filtro(1000.0);
        assert!(g1k.abs() < 0.1, "1 kHz deveria passar intacto, veio {g1k:.3} dB");
        let g20 = ganho_db_do_filtro(20.0);
        assert!(g20 < -24.0, "20 Hz: {g20:.1} dB");
        let g80 = ganho_db_do_filtro(80.0);
        assert!((g80 + 3.0).abs() < 0.3, "corte em 80 Hz = -3 dB, veio {g80:.2}");
    }

    #[test]
    fn passa_alta_tira_o_nivel_dc_e_aguenta_nan() {
        let mut f = PassaAlta::new();
        let mut x = vec![0.2f32; 48000];
        x[10] = f32::NAN;
        f.process(&mut x);
        assert!(x.iter().all(|v| v.is_finite()));
        assert!(x[40000].abs() < 1e-3, "DC deveria sumir: {}", x[40000]);
    }

    // ── pré-AGC ──

    struct Tom {
        fase: f32,
    }
    impl Tom {
        /// 20 ms de um tom de fala (220 Hz) com `rms_db` dBFS de RMS.
        fn quadro(&mut self, rms_db: f32) -> Vec<f32> {
            let a = 10f32.powf(rms_db / 20.0) * 2f32.sqrt();
            (0..FRAME)
                .map(|_| {
                    self.fase += 2.0 * PI * 220.0 / 48000.0;
                    a * self.fase.sin()
                })
                .collect()
        }
    }

    fn pico_db(q: &[f32]) -> f32 {
        20.0 * q.iter().fold(0f32, |m, v| m.max(v.abs())).max(1e-6).log10()
    }

    #[test]
    fn pre_agc_sobe_fala_baixa_em_direcao_ao_alvo() {
        let mut g = PreAgc::new();
        let mut t = Tom { fase: 0.0 };
        // 1 s de silêncio de fundo (-75 dB) para o piso, depois fala a -50 dB
        for _ in 0..50 {
            g.process(&mut t.quadro(-75.0));
        }
        let mut db_saida = 0.0;
        for _ in 0..100 {
            let mut q = t.quadro(-50.0);
            g.process(&mut q);
            db_saida = rms_db(&q);
        }
        assert!(g.gain_db() > 15.0 && g.gain_db() <= PreAgc::MAX_DB, "ganho {}", g.gain_db());
        assert!(db_saida > -38.0, "a fala deveria chegar perto de -30 dB, veio {db_saida}");
    }

    #[test]
    fn pre_agc_nao_amplifica_ruido_constante_nem_fala_alta() {
        let mut g = PreAgc::new();
        let mut t = Tom { fase: 0.0 };
        for _ in 0..300 {
            g.process(&mut t.quadro(-60.0)); // ruído constante: vira o próprio piso
        }
        assert!(g.gain_db() < 0.5, "ganho no ruído constante: {}", g.gain_db());
        let mut g = PreAgc::new();
        for _ in 0..100 {
            let mut q = t.quadro(-12.0);
            g.process(&mut q);
        }
        assert_eq!(g.gain_db(), 0.0, "fala alta nunca é amplificada");
    }

    #[test]
    fn pre_agc_desce_devagar_sem_evidencia_e_desligado_rampeia() {
        let mut g = PreAgc::new();
        let mut t = Tom { fase: 0.0 };
        for _ in 0..50 {
            g.process(&mut t.quadro(-75.0));
        }
        for _ in 0..60 {
            g.process(&mut t.quadro(-50.0));
        }
        let alto = g.gain_db();
        assert!(alto > 10.0);
        for _ in 0..10 {
            g.process(&mut t.quadro(-80.0));
        }
        assert!(g.gain_db() > alto - 1.5, "desce ~4 dB/s: {} → {}", alto, g.gain_db());
        // desligar: o ganho vai a zero dentro do quadro, sem degrau entre amostras
        g.ligado = false;
        let mut q = vec![0.05f32; FRAME];
        g.process(&mut q);
        assert_eq!(g.gain_db(), 0.0);
        let passo = q.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max);
        assert!(passo < 0.002, "rampa suave, passo {passo}");
        assert!((q[FRAME - 1] - 0.05).abs() < 1e-3, "termina em ganho unitário");
    }

    // ── AGC2 (mesmos casos de Agc2Test.kt) ──

    fn pico(q: &[f32]) -> f32 {
        Agc2::dbfs(q.iter().fold(0f64, |m, v| m.max((*v as f64 * 32768.0).abs()))) as f32
    }

    #[test]
    fn agc2_fala_baixa_sobe_6_db_por_segundo_e_so_depois_de_120_ms() {
        let mut g = Agc2::new();
        let mut t = Tom { fase: 0.0 };
        for _ in 0..5 {
            g.process(&mut t.quadro(-45.0), 1.0); // 100 ms de fala: ainda não sobe
        }
        assert_eq!(g.gain_db(), 0.0);
        for _ in 0..50 {
            g.process(&mut t.quadro(-45.0), 1.0); // +1 s
        }
        assert!((4.5..=7.5).contains(&g.gain_db()), "ganho {}", g.gain_db());
    }

    #[test]
    fn agc2_leva_o_pico_da_fala_perto_de_menos_5_dbfs() {
        let mut g = Agc2::new();
        let mut t = Tom { fase: 0.0 };
        let mut q = t.quadro(-40.0);
        // fala com pausas (o piso de ruído vem das pausas: um tom contínuo seria "ruído" e travaria o ganho)
        for i in 0..1400 {
            if i % 175 < 150 {
                q = t.quadro(-40.0);
                g.process(&mut q, 1.0);
            } else {
                g.process(&mut t.quadro(-75.0), 0.0);
            }
        } // ~28 s
        let alvo = -5.0 - (g.speech_level_db() + g.margin_db());
        assert!(
            (alvo - g.gain_db()).abs() <= 1.0,
            "alvo={alvo} ganho={} fala={} margem={}",
            g.gain_db(),
            g.speech_level_db(),
            g.margin_db()
        );
        assert!(g.gain_db() <= 50.0);
        assert!(pico(&q) > -25.0, "pico={}", pico(&q));
    }

    #[test]
    fn agc2_fala_alta_nao_e_amplificada_nem_estoura() {
        let mut g = Agc2::new();
        let mut t = Tom { fase: 0.0 };
        let mut q = t.quadro(-10.0);
        for _ in 0..300 {
            q = t.quadro(-10.0);
            g.process(&mut q, 1.0);
        }
        assert_eq!(g.gain_db(), 0.0);
        assert!(q.iter().all(|v| v.abs() <= 1.0));
        assert!(pico(&q) <= 0.1);
    }

    #[test]
    fn agc2_rajada_curta_de_fala_nao_adapta() {
        let mut g = Agc2::new();
        let mut t = Tom { fase: 0.0 };
        for i in 0..300 {
            g.process(&mut t.quadro(-45.0), if i % 8 < 5 { 1.0 } else { 0.0 }); // rajadas de 10 quadros de 10 ms (< 12)
        }
        assert_eq!(g.gain_db(), 0.0);
    }

    #[test]
    fn agc2_sem_fala_o_ganho_nao_sobe() {
        let mut g = Agc2::new();
        let mut t = Tom { fase: 0.0 };
        for _ in 0..500 {
            g.process(&mut t.quadro(-50.0), 0.1);
        }
        assert_eq!(g.gain_db(), 0.0);
    }

    /// Gaussiana determinística (soma de 12 uniformes), desvio ~1.
    struct Gauss(u32);
    impl Gauss {
        fn next(&mut self) -> f32 {
            let mut s = 0.0;
            for _ in 0..12 {
                self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                s += (self.0 >> 8) as f32 / (1u32 << 24) as f32;
            }
            s - 6.0
        }
    }

    #[test]
    fn agc2_ruido_de_fundo_alto_limita_o_ganho() {
        let mut g = Agc2::new();
        let mut r = Gauss(7);
        let a = 10f32.powf(-48.0 / 20.0); // ruído a -48 dBFS RMS, constante, sem fala
        for _ in 0..2500 {
            // 50 s: a estimativa sobe pela metade a cada janela de 5 s
            let mut q: Vec<f32> = (0..FRAME).map(|_| r.next() * a).collect();
            g.process(&mut q, 0.0);
        }
        assert!(g.noise_db() > -52.0, "ruído {}", g.noise_db());
        let mut t = Tom { fase: 0.0 };
        for _ in 0..900 {
            g.process(&mut t.quadro(-45.0), 1.0);
        }
        assert!(g.gain_db() < 4.0, "ganho {}", g.gain_db()); // -50 - (-48) < 0 → só 0
    }

    #[test]
    fn agc2_desligado_nao_mexe_no_sinal() {
        let mut g = Agc2::new();
        g.ligado = false;
        let mut t = Tom { fase: 0.0 };
        for _ in 0..100 {
            g.process(&mut t.quadro(-40.0), 1.0);
        }
        let mut q = t.quadro(-40.0);
        let c = q.clone();
        g.process(&mut q, 1.0);
        assert_eq!(g.gain_db(), 0.0);
        assert_eq!(q, c);
    }

    #[test]
    fn agc2_nunca_gera_nan_nem_passa_de_zero_dbfs() {
        let mut g = Agc2::new();
        let mut q = vec![0.5f32; FRAME];
        q[7] = f32::NAN;
        g.process(&mut q, f32::NAN);
        let mut t = Tom { fase: 0.0 };
        for _ in 0..400 {
            let mut q = t.quadro(-12.0);
            g.process(&mut q, 1.0);
            assert!(q.iter().all(|v| v.is_finite() && v.abs() <= 1.0));
        }
    }

    // ── esticador ──

    /// Vogal sintética: harmônicos de `f0` sob um envelope de formantes (periódica, como a fala sonora).
    fn vogal(f0: f32, n: usize, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| {
                let t = i as f32 / 48000.0;
                let mut v = 0.0;
                for h in 1..30 {
                    let f = f0 * h as f32;
                    if f > 4000.0 {
                        break;
                    }
                    let env = (-((f - 700.0) / 350.0).powi(2)).exp() + 0.5 * (-((f - 1500.0) / 500.0).powi(2)).exp();
                    v += env / h as f32 * (2.0 * PI * f * t).sin();
                }
                v * amp
            })
            .collect()
    }

    fn passo_max(x: &[f32]) -> f32 {
        x.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0, f32::max)
    }

    fn roda_esticador(entrada: &[f32], modo: Escala, taxa: f32) -> (Vec<f32>, Esticador) {
        let mut e = Esticador::new();
        let mut saida = Vec::new();
        let mut q = vec![0f32; FRAME];
        for bloco in entrada.chunks_exact(FRAME) {
            e.push(bloco);
            while e.disponivel() >= Esticador::precisa(modo) {
                e.pull(&mut q, modo, taxa);
                saida.extend_from_slice(&q);
            }
        }
        (saida, e)
    }

    #[test]
    fn esticador_normal_e_identidade() {
        let x = vogal(140.0, FRAME * 10, 0.2);
        let (y, _) = roda_esticador(&x, Escala::Normal, 0.0);
        assert_eq!(&y[..], &x[..y.len()]);
    }

    #[test]
    fn esticador_comprime_ate_20_por_cento_sem_degrau() {
        let x = vogal(140.0, FRAME * 100, 0.2);
        let (y, e) = roda_esticador(&x, Escala::Comprime, 0.2);
        let removido = e.removidas as f32 / x.len() as f32;
        assert!(removido > 0.12 && removido <= 0.205, "fração removida {removido:.3}");
        assert!(y.len() + e.removidas as usize + e.disponivel() == x.len(), "contas: {} + {} + {} vs {}", y.len(), e.removidas, e.disponivel(), x.len());
        let natural = passo_max(&x);
        let obtido = passo_max(&y);
        assert!(obtido < natural * 1.6, "degrau {obtido} contra {natural} do sinal original");
        // o pitch não muda ao comprimir o tempo: o período da saída continua o da entrada (343 amostras)
        let p = periodo(&y[FRAME * 10..FRAME * 70]);
        assert!((p as i32 - 343).abs() <= 3, "período da saída {p} (entrada 343)");
    }

    /// Período (lag da primeira autocorrelação que passa de 97% do máximo) entre 100 e 700 amostras.
    fn periodo(x: &[f32]) -> usize {
        let ac: Vec<f32> = (100..700).map(|lag| x.iter().zip(&x[lag..]).map(|(a, b)| a * b).sum::<f32>()).collect();
        let m = ac.iter().cloned().fold(f32::MIN, f32::max);
        100 + ac.iter().position(|v| *v >= 0.97 * m).unwrap()
    }

    #[test]
    fn esticador_estica_ate_20_por_cento_sem_degrau() {
        let x = vogal(180.0, FRAME * 100, 0.2);
        let (y, e) = roda_esticador(&x, Escala::Estica, 0.2);
        let repetido = e.repetidas as f32 / y.len() as f32; // velocidade = 1 - repetido (≥ 0,8)
        assert!(repetido > 0.12 && repetido <= 0.205, "fração repetida {repetido:.3}");
        assert_eq!(y.len() as u64, x.len() as u64 + e.repetidas - e.disponivel() as u64);
        let natural = passo_max(&x);
        let obtido = passo_max(&y);
        assert!(obtido < natural * 1.6, "degrau {obtido} contra {natural}");
    }

    #[test]
    fn esticador_acha_o_periodo_de_um_tom_puro() {
        let x = seno(200.0, 0.3, FRAME * 4); // período = 240 amostras
        let m = FRAME - Esticador::L;
        let a = &x[m..m + Esticador::L];
        let p = Esticador::acha_periodo(a, |p| &x[m + p..m + p + Esticador::L], Esticador::PMIN, Esticador::PMAX);
        assert!((p as i32 - 240).abs() <= 1, "deveria escolher o menor (240), veio {p}");
    }

    #[test]
    fn esticador_silencio_pula_quadro_inteiro_ao_comprimir_e_nao_estica() {
        let x = vec![0.0001f32; FRAME * 6];
        let (_, e) = roda_esticador(&x, Escala::Comprime, 0.2);
        assert!(e.removidas >= FRAME as u64, "silêncio deveria pular quadros: {}", e.removidas);
        let (y, e) = roda_esticador(&x, Escala::Estica, 0.2);
        assert_eq!(e.repetidas, 0);
        assert_eq!(y.len(), x.len());
    }

    #[test]
    fn esticador_sem_dados_nao_consome_nada() {
        let mut e = Esticador::new();
        e.push(&vec![0.1; FRAME - 1]);
        let mut q = vec![0f32; FRAME];
        assert_eq!(e.pull(&mut q, Escala::Normal, 0.0), None);
        assert_eq!(e.disponivel(), FRAME - 1);
        // comprimir sem dados suficientes vira normal
        e.push(&vec![0.1; 1]);
        assert_eq!(e.pull(&mut q, Escala::Comprime, 0.2), Some(Escala::Normal));
    }

    // ── jitter de chegada ──

    #[test]
    fn jitter_chegada_zero_na_cadencia_certa_e_sobe_com_variacao() {
        let mut j = JitterChegada::new();
        for i in 0..100u32 {
            j.update(i as f64 * 20.0, i);
        }
        assert!(j.ms() < 0.01, "{}", j.ms());
        let mut j = JitterChegada::new();
        for i in 0..200u32 {
            let atraso = if i % 2 == 0 { 0.0 } else { 10.0 };
            j.update(i as f64 * 20.0 + atraso, i);
        }
        assert!((8.0..=11.0).contains(&j.ms()), "{}", j.ms());
    }

    #[test]
    fn jitter_chegada_ignora_pausa_de_fala_e_pacote_atrasado() {
        let mut j = JitterChegada::new();
        for i in 0..50u32 {
            j.update(i as f64 * 20.0, i);
        }
        // pausa de 3 s com o seq contíguo
        for i in 50..100u32 {
            j.update(3000.0 + i as f64 * 20.0, i);
        }
        assert!(j.ms() < 0.5, "pausa de fala não é jitter: {}", j.ms());
        j.update(3000.0 + 99.0 * 20.0 + 1.0, 70); // seq antigo
        assert!(j.ms() < 0.5);
    }
}
