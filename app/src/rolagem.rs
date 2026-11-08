//! Rolagem da roda do mouse: passo maior que o padrão do egui (40 px por clique), aceleração quando a roda gira
//! rápido e deslizamento suave (como o navegador). O egui só suaviza 0,1 s e por clique é curto; aqui o passo vai para
//! uma fila que escoa em ~0,2 s.
use eframe::egui::{self, Vec2};
use std::{cell::RefCell, time::Instant};

/// Pontos por "clique" da roda (o egui usa 40; navegadores ficam entre 50 e 100).
pub const PASSO: f32 = 80.0;
/// Constante de tempo do deslizamento: 90% do movimento acontece em `TAU * 2,3` ≈ 0,2 s.
const TAU: f32 = 0.085;
/// Cliques seguidos com menos que isso de intervalo contam como giro rápido.
const JANELA_ACELERACAO: f64 = 0.15;
/// Cada clique seguido soma isso ao multiplicador, até `FATOR_MAX`.
const ACELERACAO: f32 = 0.18;
const FATOR_MAX: f32 = 2.6;

#[derive(Default)]
pub struct Suave {
    pendente: Vec2,
    ultimo_clique: Option<f64>,
    sequencia: u32,
    sentido: f32,
}

impl Suave {
    /// Um evento de roda em linhas (`t` em segundos). Soma o movimento, já com aceleração, à fila.
    pub fn entrada(&mut self, t: f64, linhas: Vec2) {
        let sentido = if linhas.y != 0.0 { linhas.y.signum() } else { self.sentido };
        let rapido = self.ultimo_clique.is_some_and(|u| t - u < JANELA_ACELERACAO) && sentido == self.sentido;
        self.sequencia = if rapido { (self.sequencia + 1).min(20) } else { 0 };
        self.sentido = sentido;
        self.ultimo_clique = Some(t);
        let fator = (1.0 + ACELERACAO * self.sequencia as f32).min(FATOR_MAX);
        self.pendente += linhas * PASSO * fator;
    }

    /// Quanto rolar neste quadro (`dt` em segundos, já limitado). Sai uma fração da fila; o resto fica para os
    /// próximos quadros, então o movimento desacelera até parar.
    pub fn saida(&mut self, dt: f32) -> Vec2 {
        if self.pendente == Vec2::ZERO {
            return Vec2::ZERO;
        }
        let f = 1.0 - (-dt.max(0.0) / TAU).exp();
        let mut s = self.pendente * f;
        if self.pendente.length() < 1.0 || (self.pendente - s).length() < 0.5 {
            s = self.pendente;
        }
        self.pendente -= s;
        s
    }

    pub fn ativo(&self) -> bool {
        self.pendente != Vec2::ZERO
    }
}

thread_local! {
    static ESTADO: RefCell<(Suave, Option<Instant>)> = RefCell::new((Suave::default(), None));
}

/// Chamado de `App::raw_input_hook`: troca os cliques da roda (sem modificadores) pela rolagem suave.
pub fn filtrar(ctx: &egui::Context, raw: &mut egui::RawInput) {
    ESTADO.with(|e| {
        let (suave, ultimo) = &mut *e.borrow_mut();
        let agora = Instant::now();
        let dt = ultimo.map_or(1.0 / 60.0, |u| agora.duration_since(u).as_secs_f32()).clamp(0.001, 0.05);
        *ultimo = Some(agora);
        let t = raw.time.unwrap_or_else(|| agora.elapsed().as_secs_f64());
        raw.events.retain(|ev| match ev {
            egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Line, delta, modifiers, .. } if modifiers.is_none() => {
                if std::env::var_os("TELACAST_DEBUG_ROLAGEM").is_some() {
                    eprintln!("roda t={t:.3} delta={delta:?}");
                }
                suave.entrada(t, *delta);
                false
            }
            _ => true,
        });
        let s = suave.saida(dt);
        if s != Vec2::ZERO {
            // abaixo de 8 pt por evento o egui aplica direto, sem a suavização dele (que somaria mais 0,1 s)
            let n = (s.length() / 7.5).ceil().max(1.0);
            for _ in 0..n as usize {
                raw.events.push(egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: s / n,
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: egui::Modifiers::NONE,
                });
            }
        }
        if suave.ativo() {
            ctx.request_repaint();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rolar_tudo(s: &mut Suave) -> (Vec2, usize) {
        let (mut total, mut quadros) = (Vec2::ZERO, 0);
        while s.ativo() && quadros < 600 {
            total += s.saida(1.0 / 60.0);
            quadros += 1;
        }
        (total, quadros)
    }

    #[test]
    fn um_clique_rola_um_passo_inteiro_e_desliza() {
        let mut s = Suave::default();
        s.entrada(0.0, Vec2::new(0.0, -1.0));
        let primeiro = s.saida(1.0 / 60.0);
        assert!(primeiro.y < 0.0 && primeiro.y.abs() < PASSO * 0.4, "o primeiro quadro é só uma parte: {primeiro:?}");
        let (resto, quadros) = rolar_tudo(&mut s);
        assert!((primeiro.y + resto.y + PASSO).abs() < 0.01, "soma = um passo");
        assert!((8..=30).contains(&quadros), "desliza por alguns quadros, não instantâneo nem eterno: {quadros}");
    }

    #[test]
    fn giro_rapido_acelera_e_pausa_zera() {
        let mut s = Suave::default();
        for i in 0..8 {
            s.entrada(i as f64 * 0.05, Vec2::new(0.0, -1.0));
        }
        let (rapido, _) = rolar_tudo(&mut s);
        assert!(rapido.y.abs() > 8.0 * PASSO * 1.5, "8 cliques rápidos rolam bem mais que 8 passos: {}", rapido.y);
        // depois de uma pausa longa o próximo clique volta ao passo normal
        s.entrada(10.0, Vec2::new(0.0, -1.0));
        let (um, _) = rolar_tudo(&mut s);
        assert!((um.y + PASSO).abs() < 0.01);
    }

    #[test]
    fn inverter_o_sentido_nao_acelera() {
        let mut s = Suave::default();
        s.entrada(0.0, Vec2::new(0.0, -1.0));
        s.entrada(0.05, Vec2::new(0.0, -1.0));
        s.entrada(0.10, Vec2::new(0.0, 1.0)); // virou
        let (t, _) = rolar_tudo(&mut s);
        // -1*80 + -1*80*1.18 + 1*80 = -94.4
        assert!((t.y + 94.4).abs() < 0.1, "{}", t.y);
    }

    #[test]
    fn quadro_longo_nao_pula_o_movimento() {
        let mut s = Suave::default();
        s.entrada(0.0, Vec2::new(0.0, -1.0));
        let q = s.saida(0.05);
        assert!(q.y.abs() < PASSO && q.y.abs() > 20.0, "{q:?}");
    }
}
