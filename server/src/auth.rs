//! Tokens, códigos de convite e hash de senha.
use argon2::{
    password_hash::{phc::PasswordHash, PasswordHasher, PasswordVerifier},
    Argon2,
};
use sha2::{Digest, Sha256};

const B32: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).expect("sem fonte de aleatoriedade do sistema");
    b
}

/// Token de sessão: 256 bits em hexadecimal.
pub fn new_token() -> String {
    random::<32>().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

/// Código de convite: 128 bits em Crockford base32 (26 caracteres, sem I, L, O, U).
pub fn new_code() -> String {
    let bytes = random::<16>();
    let mut out = String::with_capacity(26);
    let (mut acc, mut bits) = (0u32, 0u32);
    for b in bytes {
        acc = (acc << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(B32[((acc >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        out.push(B32[((acc << (5 - bits)) & 31) as usize] as char);
    }
    out
}

/// Normaliza o que o usuário digitou: maiúsculas, sem espaços nem hífens, I/L→1 e O→0.
pub fn normalize_code(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| match c.to_ascii_uppercase() {
            'I' | 'L' => '1',
            'O' => '0',
            c => c,
        })
        .collect()
}

pub fn hash_password(pw: &str) -> Result<String, String> {
    Argon2::default().hash_password(pw.as_bytes()).map(|h| h.to_string()).map_err(|e| e.to_string())
}

pub fn verify_password(pw: &str, hash: &str) -> bool {
    PasswordHash::new(hash).is_ok_and(|p| Argon2::default().verify_password(pw.as_bytes(), &p).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codigo_tem_26_caracteres_validos() {
        let c = new_code();
        assert_eq!(c.len(), 26);
        assert!(c.bytes().all(|b| B32.contains(&b)));
        assert_ne!(c, new_code());
    }

    #[test]
    fn normaliza_o_que_o_usuario_digita() {
        assert_eq!(normalize_code(" ab-cd o1l "), "ABCD011");
    }

    #[test]
    fn token_e_hash() {
        let t = new_token();
        assert_eq!(t.len(), 64);
        assert_eq!(hash_token(&t), hash_token(&t));
        assert_ne!(hash_token(&t), hash_token("outro"));
    }

    #[test]
    fn senha_confere() {
        let h = hash_password("segredo123").unwrap();
        assert!(verify_password("segredo123", &h));
        assert!(!verify_password("errada", &h));
    }
}
