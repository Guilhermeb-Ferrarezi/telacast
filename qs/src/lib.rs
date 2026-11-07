//! Lógica do seletor rápido (Ctrl+K): prefixos, pontuação e ordenação. Sem dependências.
//!
//! A pontuação segue a escala do cliente do Discord: 10 igual, 7 começa com, 5 contém,
//! 3 todos os termos, 1 subsequência (fuzzy), 0 nada. A nota final é
//! `1000 × nota × reforço`, com `reforço = 1 + frequência/maior_frequência (+0,2 amigo, +0,1 com DM)`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    All,
    User,
    Text,
    Server,
}

/// `@x` usuários, `#x` canais de texto, `*x` servidores; sem prefixo, tudo.
pub fn parse_prefix(q: &str) -> (Mode, &str) {
    match q.chars().next() {
        Some('@') => (Mode::User, &q[1..]),
        Some('#') => (Mode::Text, &q[1..]),
        Some('*') => (Mode::Server, &q[1..]),
        _ => (Mode::All, q),
    }
}

/// Minúsculas e sem acento (suficiente para português e espanhol).
pub fn fold(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            other => other,
        })
        .collect()
}

fn is_subsequence(q: &str, n: &str) -> bool {
    let mut it = n.chars();
    q.chars().all(|c| it.any(|x| x == c))
}

/// Nota de um campo para a consulta (0 a 10).
pub fn field_score(name: &str, query: &str) -> u32 {
    let (n, q) = (fold(name), fold(query.trim()));
    if q.is_empty() {
        return 0;
    }
    if n == q {
        return 10;
    }
    if n.starts_with(&q) || n.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(&q)) {
        return 7;
    }
    if n.contains(&q) {
        return 5;
    }
    let terms: Vec<&str> = q.split([',', ' ']).filter(|t| !t.is_empty()).collect();
    if terms.len() > 1 && terms.iter().all(|t| n.contains(t)) {
        return 3;
    }
    if is_subsequence(&q.replace(' ', ""), &n) {
        return 1;
    }
    0
}

/// Reforço de uso: itens visitados com frequência e amigos sobem.
pub fn boost(freq: u32, freq_max: u32, friend: bool, has_dm: bool) -> f32 {
    let f = if freq_max == 0 { 0.0 } else { freq as f32 / freq_max as f32 };
    1.0 + f + if friend { 0.2 } else { 0.0 } + if has_dm { 0.1 } else { 0.0 }
}

pub fn final_score(field: u32, boost: f32) -> f32 {
    1000.0 * field as f32 * boost
}

/// Ordena por nota decrescente; empate por ordem alfabética do rótulo (sem acento).
pub fn rank<T>(items: &mut [(T, f32, String)]) {
    items.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal).then_with(|| fold(&a.2).cmp(&fold(&b.2))));
}

/// Próxima linha selecionável, em círculo, pulando cabeçalhos.
pub fn step(selectable: &[bool], from: usize, dir: i32) -> usize {
    let n = selectable.len();
    if n == 0 || !selectable.iter().any(|&s| s) {
        return 0;
    }
    let mut i = from as i32;
    loop {
        i = (i + dir).rem_euclid(n as i32);
        if selectable[i as usize] {
            return i as usize;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixos() {
        assert_eq!(parse_prefix("@ana"), (Mode::User, "ana"));
        assert_eq!(parse_prefix("#geral"), (Mode::Text, "geral"));
        assert_eq!(parse_prefix("*jogos"), (Mode::Server, "jogos"));
        assert_eq!(parse_prefix("ana"), (Mode::All, "ana"));
        assert_eq!(parse_prefix("@"), (Mode::User, ""));
        assert_eq!(parse_prefix(""), (Mode::All, ""));
    }

    #[test]
    fn escala_de_notas() {
        assert_eq!(field_score("Ana", "ana"), 10, "igual, sem diferenciar maiúsculas");
        assert_eq!(field_score("anabela", "ana"), 7, "começa com");
        assert_eq!(field_score("noite-de-jogos", "jogos"), 7, "começa uma palavra");
        assert_eq!(field_score("mariana", "ana"), 5, "contém");
        assert_eq!(field_score("noite de jogos", "jogos noite"), 3, "todos os termos");
        assert_eq!(field_score("geral", "grl"), 1, "subsequência");
        assert_eq!(field_score("geral", "xyz"), 0);
        assert_eq!(field_score("geral", ""), 0);
    }

    #[test]
    fn ignora_acentos() {
        assert_eq!(field_score("Ação", "acao"), 10);
        assert_eq!(field_score("canção", "cancao"), 10);
        assert_eq!(field_score("Jogos da Galera", "galéra"), 7);
    }

    #[test]
    fn ordenacao_e_desempate() {
        let mut v = vec![
            ("b", final_score(7, 1.0), "Beto".to_string()),
            ("a", final_score(7, 1.0), "ana".to_string()),
            ("c", final_score(10, 1.0), "Zé".to_string()),
            ("d", final_score(7, 1.5), "Carla".to_string()),
        ];
        rank(&mut v);
        let ordem: Vec<&str> = v.iter().map(|x| x.0).collect();
        // d (7 × 1,5 = 10,5) passa c (10 × 1,0): quem você usa muito sobe mesmo sem casar exato
        assert_eq!(ordem, vec!["d", "c", "a", "b"], "nota × reforço, depois empate alfabético");
    }

    #[test]
    fn reforco() {
        assert_eq!(boost(0, 0, false, false), 1.0);
        assert!((boost(5, 10, true, true) - 1.8).abs() < 1e-6);
        assert!(final_score(5, boost(10, 10, false, false)) > final_score(5, boost(0, 10, false, false)));
    }

    #[test]
    fn teclado_circular_pula_cabecalhos() {
        let sel = [false, true, true, false, true];
        assert_eq!(step(&sel, 1, 1), 2);
        assert_eq!(step(&sel, 2, 1), 4, "pulou o cabeçalho");
        assert_eq!(step(&sel, 4, 1), 1, "deu a volta");
        assert_eq!(step(&sel, 1, -1), 4, "para cima também em círculo");
        assert_eq!(step(&[false, false], 0, 1), 0);
        assert_eq!(step(&[], 0, 1), 0);
    }
}
