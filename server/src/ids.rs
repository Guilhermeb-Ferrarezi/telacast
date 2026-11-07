//! IDs estilo snowflake: 41 bits de milissegundos desde 2026-01-01, 10 bits de nó, 12 de sequência.
//! Cabem num `i64` positivo e ordenam por tempo.
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// 2026-01-01T00:00:00Z em milissegundos desde a época Unix.
pub const EPOCH_MS: i64 = 1_767_225_600_000;

pub struct Snowflake {
    node: i64,
    state: Mutex<(i64, i64)>, // (último ms, sequência)
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

impl Snowflake {
    pub fn new(node: u16) -> Self {
        Self { node: (node & 0x3FF) as i64, state: Mutex::new((0, 0)) }
    }

    pub fn next(&self) -> i64 {
        let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut ms = now_ms() - EPOCH_MS;
        if ms < s.0 {
            ms = s.0; // relógio voltou: mantém a monotonia
        }
        if ms == s.0 {
            s.1 = (s.1 + 1) & 0xFFF;
            if s.1 == 0 {
                ms += 1; // 4096 ids no mesmo ms: avança
            }
        } else {
            s.1 = 0;
        }
        s.0 = ms;
        (ms << 22) | (self.node << 12) | s.1
    }
}

/// Instante de criação (ms Unix) embutido no id.
pub fn ts_ms(id: i64) -> i64 {
    (id >> 22) + EPOCH_MS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_crescem_e_sao_unicos() {
        let sf = Snowflake::new(1);
        let mut prev = 0;
        for _ in 0..20_000 {
            let id = sf.next();
            assert!(id > prev, "id {id} não é maior que {prev}");
            prev = id;
        }
    }

    #[test]
    fn timestamp_bate_com_o_relogio() {
        let id = Snowflake::new(0).next();
        let diff = (ts_ms(id) - now_ms()).abs();
        assert!(diff < 2000, "diferença de {diff} ms");
    }
}
