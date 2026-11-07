//! Atualização do app com conferência de integridade.
//!
//! O servidor publica um manifesto (versão, SHA-256, tamanho e, se houver chave, assinatura Ed25519).
//! O app baixa o executável, calcula o SHA-256 **enquanto baixa**, e só troca o executável se:
//! 1. o tamanho e o SHA-256 do arquivo baixado batem com o manifesto;
//! 2. havendo chave pública embutida (`TELACAST_UPDATE_PUBKEY` na compilação), a assinatura do manifesto é válida
//!    (sem isso o hash só prova que o arquivo não se corrompeu; quem controla o servidor trocaria arquivo e hash juntos).
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    path::PathBuf,
};
use telacast_proto::{update_signing_message, version_is_newer, UpdateManifest};

/// Versão deste executável.
pub const CURRENT: &str = env!("CARGO_PKG_VERSION");

/// Limite de segurança: nenhum executável do app passa disso.
const MAX_SIZE: u64 = 400 << 20;

#[derive(Debug, PartialEq)]
pub enum VerifyError {
    SizeMismatch { expected: u64, got: u64 },
    HashMismatch { expected: String, got: String },
    BadHash,
    MissingSignature,
    BadSignature,
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyError::SizeMismatch { expected, got } => write!(f, "o arquivo baixado tem {got} bytes e o manifesto diz {expected}"),
            VerifyError::HashMismatch { expected, got } => {
                write!(f, "o hash do arquivo baixado não confere (esperado {}…, obtido {}…)", &expected[..expected.len().min(12)], &got[..got.len().min(12)])
            }
            VerifyError::BadHash => write!(f, "o manifesto traz um hash inválido"),
            VerifyError::MissingSignature => write!(f, "o manifesto não está assinado e este app exige assinatura"),
            VerifyError::BadSignature => write!(f, "a assinatura do manifesto não é válida"),
        }
    }
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn valid_sha(h: &str) -> bool {
    h.len() == 64 && h.bytes().all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

/// Chave pública que assina as atualizações: variável de ambiente (testes) ou a embutida na compilação.
pub fn trusted_key() -> Option<[u8; 32]> {
    let h = std::env::var("TELACAST_UPDATE_PUBKEY").ok().or_else(|| option_env!("TELACAST_UPDATE_PUBKEY").map(str::to_string))?;
    <[u8; 32]>::try_from(unhex(h.trim())?).ok()
}

/// A assinatura do manifesto vale para a chave `key`?
pub fn verify_signature(m: &UpdateManifest, key: &[u8; 32]) -> Result<(), VerifyError> {
    if m.signature.is_empty() {
        return Err(VerifyError::MissingSignature);
    }
    let sig = unhex(&m.signature).and_then(|v| <[u8; 64]>::try_from(v).ok()).ok_or(VerifyError::BadSignature)?;
    let vk = VerifyingKey::from_bytes(key).map_err(|_| VerifyError::BadSignature)?;
    vk.verify(&update_signing_message(&m.version, &m.platform, &m.sha256, m.size), &Signature::from_bytes(&sig))
        .map_err(|_| VerifyError::BadSignature)
}

/// O arquivo baixado (tamanho e SHA-256 já calculados) é o que o manifesto promete?
pub fn verify_download(m: &UpdateManifest, got_size: u64, got_sha: &str) -> Result<(), VerifyError> {
    if !valid_sha(&m.sha256) {
        return Err(VerifyError::BadHash);
    }
    if got_size != m.size {
        return Err(VerifyError::SizeMismatch { expected: m.size, got: got_size });
    }
    if !got_sha.eq_ignore_ascii_case(&m.sha256) {
        return Err(VerifyError::HashMismatch { expected: m.sha256.clone(), got: got_sha.to_string() });
    }
    Ok(())
}

pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().tls_config(crate::net::tls_config()).timeout(std::time::Duration::from_secs(120)).build()
}

/// Pergunta ao servidor se há versão mais nova. `Ok(None)` = está em dia (ou o servidor não publica).
pub fn check(base: &str) -> Result<Option<UpdateManifest>, String> {
    let url = format!("{base}/update/manifest?platform={}", platform());
    let m: UpdateManifest = match agent().get(&url).call() {
        Ok(r) => r.into_json().map_err(|e| format!("manifesto ilegível ({e})"))?,
        Err(ureq::Error::Status(404, _)) => return Ok(None),
        Err(e) => return Err(format!("não consegui consultar atualizações ({e})")),
    };
    check_manifest(m, CURRENT, trusted_key().as_ref())
}

/// Parte pura de `check`: aceita o manifesto só se for desta plataforma, mais novo e (com chave) bem assinado.
pub fn check_manifest(m: UpdateManifest, current: &str, key: Option<&[u8; 32]>) -> Result<Option<UpdateManifest>, String> {
    if m.platform != platform() || !version_is_newer(&m.version, current) {
        return Ok(None);
    }
    if !valid_sha(&m.sha256) || m.size == 0 || m.size > MAX_SIZE {
        return Err("o manifesto de atualização é inválido".into());
    }
    if let Some(k) = key {
        verify_signature(&m, k).map_err(|e| e.to_string())?;
    }
    Ok(Some(m))
}

/// Baixa o executável para ao lado do atual, calculando o SHA-256 no caminho, e confere tudo.
/// `progress(baixado, total)`. Devolve o caminho do arquivo verificado (ainda não instalado).
pub fn download(base: &str, m: &UpdateManifest, progress: &dyn Fn(u64, u64)) -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| format!("não sei onde o app está ({e})"))?;
    let part = exe.with_file_name(".telacast-update.part");
    let res = download_to(base, m, &part, progress);
    if res.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    res.map(|_| part)
}

fn download_to(base: &str, m: &UpdateManifest, part: &std::path::Path, progress: &dyn Fn(u64, u64)) -> Result<(), String> {
    let resp = agent().get(&format!("{base}/update/file/{}", m.sha256)).call().map_err(|e| format!("falha ao baixar ({e})"))?;
    let mut reader = resp.into_reader();
    let mut out = std::fs::File::create(part).map_err(|e| format!("sem permissão para gravar em {} ({e})", part.display()))?;
    let mut hasher = Sha256::new();
    let (mut got, mut buf) = (0u64, vec![0u8; 64 << 10]);
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("a conexão caiu durante o download ({e})"))?;
        if n == 0 {
            break;
        }
        got += n as u64;
        if got > m.size.min(MAX_SIZE) {
            return Err("o servidor mandou mais bytes do que o manifesto promete".into());
        }
        hasher.update(&buf[..n]);
        out.write_all(&buf[..n]).map_err(|e| format!("não consegui gravar o arquivo ({e})"))?;
        progress(got, m.size);
    }
    out.flush().map_err(|e| e.to_string())?;
    drop(out);
    verify_download(m, got, &hex(&hasher.finalize())).map_err(|e| e.to_string())
}

/// Troca o executável pelo arquivo já verificado (rename atômico; no Linux vale mesmo com o app rodando).
pub fn install(verified: &std::path::Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(verified, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    }
    std::fs::rename(verified, &exe).map_err(|e| format!("não consegui trocar o executável ({e})"))
}

/// Abre a versão nova e fecha esta (a nova espera um instante para a trava de instância única soltar).
pub fn restart() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new("sh")
            .arg("-c")
            .arg("sleep 1; exec \"$0\" \"$@\"")
            .arg(exe)
            .args(std::env::args().skip(1))
            .spawn();
    }
    std::process::exit(0);
}

/// Estado do banner de atualização.
#[derive(Clone, Debug, Default)]
pub enum State {
    #[default]
    Idle,
    Available(UpdateManifest),
    Downloading { got: u64, total: u64 },
    Ready,
    Failed(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn manifest(bytes: &[u8]) -> UpdateManifest {
        UpdateManifest {
            version: "9.9.9".into(),
            platform: platform(),
            sha256: hex(&Sha256::digest(bytes)),
            size: bytes.len() as u64,
            notes: String::new(),
            signature: String::new(),
        }
    }

    fn sign(m: &mut UpdateManifest, key: &SigningKey) {
        m.signature = hex(&key.sign(&update_signing_message(&m.version, &m.platform, &m.sha256, m.size)).to_bytes());
    }

    #[test]
    fn download_confere_hash_e_tamanho() {
        let m = manifest(b"executavel de teste");
        let sha = m.sha256.clone();
        assert_eq!(verify_download(&m, m.size, &sha), Ok(()));
        assert_eq!(verify_download(&m, m.size, &sha.to_uppercase()), Ok(()));
        let ruim = hex(&Sha256::digest(b"executavel adulterado"));
        assert!(matches!(verify_download(&m, m.size, &ruim), Err(VerifyError::HashMismatch { .. })));
        assert!(matches!(verify_download(&m, m.size - 1, &sha), Err(VerifyError::SizeMismatch { .. })));
        let mut m2 = m.clone();
        m2.sha256 = "zz".into();
        assert_eq!(verify_download(&m2, m.size, &sha), Err(VerifyError::BadHash));
    }

    #[test]
    fn assinatura_valida_e_adulterada() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let pubk = *key.verifying_key().as_bytes();
        let mut m = manifest(b"abc");
        assert_eq!(verify_signature(&m, &pubk), Err(VerifyError::MissingSignature));
        sign(&mut m, &key);
        assert_eq!(verify_signature(&m, &pubk), Ok(()));
        // trocar qualquer campo assinado invalida
        let mut t = m.clone();
        t.sha256 = hex(&Sha256::digest(b"outro"));
        assert_eq!(verify_signature(&t, &pubk), Err(VerifyError::BadSignature));
        let mut t = m.clone();
        t.version = "10.0.0".into();
        assert_eq!(verify_signature(&t, &pubk), Err(VerifyError::BadSignature));
        let mut t = m.clone();
        t.size += 1;
        assert_eq!(verify_signature(&t, &pubk), Err(VerifyError::BadSignature));
        // chave errada
        let outra = *SigningKey::from_bytes(&[8u8; 32]).verifying_key().as_bytes();
        assert_eq!(verify_signature(&m, &outra), Err(VerifyError::BadSignature));
        // assinatura em lixo
        let mut t = m.clone();
        t.signature = "00".repeat(10);
        assert_eq!(verify_signature(&t, &pubk), Err(VerifyError::BadSignature));
    }

    #[test]
    fn check_manifest_filtra_versao_plataforma_e_assinatura() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let pubk = *key.verifying_key().as_bytes();
        let mut m = manifest(b"abc");
        sign(&mut m, &key);
        assert!(check_manifest(m.clone(), "0.1.0", Some(&pubk)).unwrap().is_some());
        // mesma versão ou mais velha: nada a fazer
        assert!(check_manifest(m.clone(), "9.9.9", Some(&pubk)).unwrap().is_none());
        // outra plataforma: ignora
        let mut o = m.clone();
        o.platform = "plan9-mips".into();
        assert!(check_manifest(o, "0.1.0", Some(&pubk)).unwrap().is_none());
        // sem assinatura com chave configurada: recusa
        let mut u = manifest(b"abc");
        u.signature.clear();
        assert!(check_manifest(u.clone(), "0.1.0", Some(&pubk)).is_err());
        // sem chave configurada: aceita só com hash
        assert!(check_manifest(u, "0.1.0", None).unwrap().is_some());
        // tamanho absurdo ou hash inválido
        let mut g = manifest(b"abc");
        g.size = MAX_SIZE + 1;
        assert!(check_manifest(g, "0.1.0", None).is_err());
        let mut h = manifest(b"abc");
        h.sha256 = "123".into();
        assert!(check_manifest(h, "0.1.0", None).is_err());
    }

    #[test]
    fn hex_ida_e_volta() {
        let b = [1u8, 2, 0xfe];
        assert_eq!(unhex(&hex(&b)).unwrap(), b);
        assert!(unhex("a").is_none());
    }
}
