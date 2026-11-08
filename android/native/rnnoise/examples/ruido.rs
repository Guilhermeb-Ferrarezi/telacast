// Experimento: o que o nnnoiseless puro (sem JNI) faz com ruído rosa/marrom, ruído branco e fala sintética.
use nnnoiseless::DenoiseState;
struct Rng(u64);
impl Rng {
    fn u(&mut self) -> f64 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; (self.0 >> 11) as f64 / (1u64 << 53) as f64 }
    fn g(&mut self) -> f64 { let u1 = self.u().max(1e-12); let u2 = self.u(); (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos() }
}
fn rosa(n: usize, amp: f64, sem: u64) -> Vec<f32> { // Paul Kellet
    let mut r = Rng(sem); let (mut b0, mut b1, mut b2, mut b3, mut b4, mut b5, mut b6) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    (0..n).map(|_| { let w = r.g(); b0 = 0.99886 * b0 + w * 0.0555179; b1 = 0.99332 * b1 + w * 0.0750759; b2 = 0.96900 * b2 + w * 0.1538520; b3 = 0.86650 * b3 + w * 0.3104856; b4 = 0.55000 * b4 + w * 0.5329522; b5 = -0.7616 * b5 - w * 0.0168980;
        let o = b0 + b1 + b2 + b3 + b4 + b5 + b6 + w * 0.5362; b6 = w * 0.115926; (o * 0.11 * amp) as f32 }).collect()
}
fn fala(n: usize, amp: f64) -> Vec<f32> {
    (0..n).map(|i| { let t = i as f64 / 48000.0; let f0 = 140.0 * (1.0 + 0.03 * (2.0 * std::f64::consts::PI * 5.0 * t).sin());
        let sil = 0.5 + 0.5 * (2.0 * std::f64::consts::PI * 4.0 * t - std::f64::consts::FRAC_PI_2).sin();
        let mut v = 0.0; for h in 1..=20 { v += (2.0 * std::f64::consts::PI * f0 * h as f64 * t).sin() / (h as f64 * 0.8); }
        (amp * sil * v / 2.5) as f32 }).collect()
}
fn rodar(x: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let mut den = DenoiseState::new(); let mut out = vec![0.0f32; x.len()]; let mut ps = vec![];
    for q in 0..x.len() / 480 { let mut o = [0.0f32; 480]; ps.push(den.process_frame(&mut o, &x[q * 480..q * 480 + 480])); out[q * 480..q * 480 + 480].copy_from_slice(&o); }
    (out, ps)
}
fn e(x: &[f32], de: usize) -> f64 { x[de..].iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / (x.len() - de) as f64 }
fn db(x: f64) -> f64 { 10.0 * (x + 1e-12).log10() }
fn envelope(i: usize) -> f64 { let t = i as f64 / 48000.0; 0.5 + 0.5 * (2.0 * std::f64::consts::PI * 4.0 * t - std::f64::consts::FRAC_PI_2).sin() }
/// SNR "prático": energia nos quadros de 10 ms de pico da sílaba contra a dos quadros de intervalo (com 1 quadro de folga dos dois lados).
fn snr_pico_intervalo(x: &[f32], de: usize) -> f64 {
    let nq = x.len() / 480; let (mut pico, mut np, mut gap, mut ng) = (0.0, 0, 0.0, 0);
    for q in (de / 480 + 1)..(nq - 1) {
        let env: Vec<f64> = (q - 1..=q + 1).map(|k| envelope(k * 480 + 240)).collect();
        let en: f64 = x[q * 480..q * 480 + 480].iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / 480.0;
        if env.iter().all(|e| *e > 0.8) { pico += en; np += 1 } else if env.iter().all(|e| *e < 0.15) { gap += en; ng += 1 }
    }
    db(pico / np as f64) - db(gap / ng as f64)
}
fn main() {
    if std::env::args().nth(1).as_deref() == Some("vetores") { return vetores(); }
    let n = 48000 * 6; let de = 48000;
    let v = fala(n, 6000.0);
    for amp in [100.0, 300.0, 1000.0, 3000.0] {
        let r = rosa(n, amp, 9); let x: Vec<f32> = v.iter().zip(&r).map(|(a, b)| a + b).collect(); let (y, _) = rodar(&x);
        let (ys, ps) = rodar(&r);
        println!("rosa {amp}: sozinho atenua {:.1} dB (prob {:.2}); fala+rosa pico/intervalo antes {:.1} dB depois {:.1} dB; energia do pico {:.1} dB",
            db(e(&r, de)) - db(e(&ys, de)), ps[100..].iter().sum::<f32>() / (ps.len() - 100) as f32,
            snr_pico_intervalo(&x, de), snr_pico_intervalo(&y, de), db(e(&y, de)) - db(e(&x, de)));
    }
    let (y, p) = rodar(&v); println!("fala limpa: perda {:.1} dB prob {:.2}", db(e(&v, 24000)) - db(e(&y, 24000)), p[50..].iter().sum::<f32>() / (p.len() - 50) as f32);
}

/// Gera os vetores de referência dos testes de unidade da ponte JNI (entrada, saída e probabilidades, f32 little-endian):
/// `cargo run --release --example ruido -- vetores <pasta>`. Saída do nnnoiseless puro, sem JNI no meio.
fn vetores() {
    let pasta = std::env::args().nth(2).expect("pasta de destino");
    let n = 480 * 100;
    let v = fala(n, 6000.0); let r = rosa(n, 1000.0, 11);
    let x: Vec<f32> = v.iter().zip(&r).map(|(a, b)| a + b).collect();
    let (y, p) = rodar(&x);
    let bytes = |a: &[f32]| a.iter().flat_map(|f| f.to_le_bytes()).collect::<Vec<u8>>();
    std::fs::create_dir_all(&pasta).unwrap();
    std::fs::write(format!("{pasta}/entrada.f32"), bytes(&x)).unwrap();
    std::fs::write(format!("{pasta}/saida.f32"), bytes(&y)).unwrap();
    std::fs::write(format!("{pasta}/prob.f32"), bytes(&p)).unwrap();
    println!("vetores gravados em {pasta}: {n} amostras, {} probabilidades", p.len());
}
