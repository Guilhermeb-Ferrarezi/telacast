//! Atualizações do app: o servidor guarda os executáveis e um manifesto assinado por plataforma.
//!
//! Pasta (`TELACAST_UPDATES_DIR`, padrão `./updates`): `<sha256>` (o executável, endereçado pelo hash) e
//! `manifest-<plataforma>.json`. Publicar: `telacast-server publish-update --file ... --version ...`.
//! A assinatura Ed25519 usa a chave secreta de `TELACAST_UPDATE_KEY_FILE` (criada por `update-keygen`).
use axum::{
    extract::{Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};
use ed25519_dalek::{Signer, SigningKey};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, path::PathBuf};
use std::sync::Arc;
use crate::App;
use telacast_proto::{update_signing_message, UpdateManifest};

pub fn dir() -> PathBuf {
    std::env::var("TELACAST_UPDATES_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("updates"))
}

/// Só letras minúsculas, números, `_` e `-` (a plataforma vira parte de um nome de arquivo).
fn valid_platform(p: &str) -> bool {
    !p.is_empty() && p.len() <= 40 && p.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '-'))
}

fn valid_hash(h: &str) -> bool {
    h.len() == 64 && h.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
}

/// `GET /update/manifest?platform=linux-x86_64`
pub async fn manifest(Query(q): Query<HashMap<String, String>>) -> Response {
    let Some(platform) = q.get("platform").filter(|p| valid_platform(p)) else { return StatusCode::BAD_REQUEST.into_response() };
    match tokio::fs::read(dir().join(format!("manifest-{platform}.json"))).await {
        Ok(b) => ([(header::CONTENT_TYPE, "application/json")], b).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// `GET /update/file/{sha256}`
pub async fn file(State(app): State<Arc<App>>, Path(hash): Path<String>) -> Response {
    if !valid_hash(&hash) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    match tokio::fs::read(dir().join(&hash)).await {
        Ok(b) => { app.count_egress(b.len()); ([(header::CONTENT_TYPE, "application/octet-stream")], b).into_response() },
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

/// `GET /download/{plataforma}`: baixa a versão mais recente já com nome e tipo certos (link para mandar a um amigo:
/// `https://servidor/download/android-arm64`). Não confere a assinatura (o app confere ao se atualizar).
pub async fn download(Path(platform): Path<String>) -> Response {
    let platform = apelido(platform.trim_end_matches('/'));
    if !valid_platform(&platform) {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Ok(raw) = tokio::fs::read(dir().join(format!("manifest-{platform}.json"))).await else { return StatusCode::NOT_FOUND.into_response() };
    let Ok(m) = serde_json::from_slice::<UpdateManifest>(&raw) else { return StatusCode::INTERNAL_SERVER_ERROR.into_response() };
    if !valid_hash(&m.sha256) || !valid_version(&m.version) {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let Ok(bytes) = tokio::fs::read(dir().join(&m.sha256)).await else { return StatusCode::NOT_FOUND.into_response() };
    let (tipo, ext) = if platform.starts_with("android") {
        ("application/vnd.android.package-archive", "apk")
    } else if platform.ends_with("-msi") || platform.ends_with("-installer") {
        ("application/x-msi", "msi")
    } else if platform.starts_with("windows") {
        ("application/octet-stream", "exe")
    } else {
        ("application/octet-stream", "bin")
    };
    let nome = format!("telacast-{}-{}.{}", m.version, platform, ext);
    (
        [
            (header::CONTENT_TYPE, tipo.to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{nome}\"")),
            (header::CACHE_CONTROL, "no-cache".to_string()),
        ],
        bytes,
    )
        .into_response()
}

/// Nomes curtos para o link: `/android`, `/windows`, `/linux` (e `/download/android` etc.).
fn apelido(p: &str) -> String {
    match p {
        "android" => "android-arm64",
        "windows" => "windows-msi",
        "linux" => "linux-x86_64",
        outro => outro,
    }
    .to_string()
}

pub async fn download_android() -> Response {
    download(Path("android".into())).await
}
pub async fn download_windows() -> Response {
    download(Path("windows".into())).await
}
pub async fn download_linux() -> Response {
    download(Path("linux".into())).await
}

/// `GET /download` (e `/baixar`): página simples com os links das versões publicadas.
pub async fn pagina() -> Response {
    let mut itens = String::new();
    if let Ok(mut rd) = tokio::fs::read_dir(dir()).await {
        let mut plats: Vec<(String, String)> = Vec::new();
        while let Ok(Some(e)) = rd.next_entry().await {
            let nome = e.file_name().to_string_lossy().to_string();
            if let Some(plat) = nome.strip_prefix("manifest-").and_then(|n| n.strip_suffix(".json")) {
                if let Ok(b) = tokio::fs::read(e.path()).await {
                    if let Ok(m) = serde_json::from_slice::<UpdateManifest>(&b) {
                        if valid_platform(plat) && valid_version(&m.version) {
                            plats.push((plat.to_string(), m.version));
                        }
                    }
                }
            }
        }
        plats.sort();
        for (plat, versao) in plats {
            let rotulo = match plat.as_str() {
                "android-arm64" => "Android (APK)",
                "windows-msi" => "Windows (instalador)",
                "windows-x86_64" => "Windows (executável avulso)",
                "linux-x86_64" => "Linux",
                outro => outro,
            };
            itens.push_str(&format!("<li><a href=\"/download/{plat}\">{rotulo}</a> <small>versão {versao}</small></li>"));
        }
    }
    if itens.is_empty() {
        itens.push_str("<li>Nenhuma versão publicada ainda.</li>");
    }
    let html = format!(
        "<!doctype html><html lang=\"pt-BR\"><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>Telacast</title>\
<style>body{{background:#141416;color:#e8e6e1;font:16px/1.5 system-ui,sans-serif;margin:0;padding:32px 20px;max-width:520px;margin:auto}}\
h1{{font:500 34px Georgia,serif;margin:0 0 4px}}p{{color:#9a978f}}ul{{list-style:none;padding:0}}li{{margin:10px 0;padding:14px;background:#1c1c1f;border:1px solid #2a2a2e;border-radius:10px}}\
a{{color:#e9b35e;font-weight:600;text-decoration:none}}small{{color:#9a978f;margin-left:8px}}</style>\
<h1>Telacast</h1><p>Baixe o app para o seu aparelho.</p><ul>{itens}</ul>"
    );
    ([(header::CONTENT_TYPE, "text/html; charset=utf-8"), (header::CACHE_CONTROL, "no-cache")], html).into_response()
}

/// Versão só com dígitos, pontos e hífen/letras simples (vira parte do nome do arquivo no cabeçalho).
fn valid_version(v: &str) -> bool {
    !v.is_empty() && v.len() <= 32 && v.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok()).collect()
}

/// Cria uma chave Ed25519 (semente de 32 bytes em hex) em `out` e devolve a chave pública em hex.
pub fn keygen(out: &std::path::Path) -> Result<String, String> {
    if out.exists() {
        return Err(format!("{} já existe; não vou sobrescrever", out.display()));
    }
    let mut seed = [0u8; 32];
    getrandom::fill(&mut seed).map_err(|e| format!("sem aleatoriedade do sistema: {e}"))?;
    let key = SigningKey::from_bytes(&seed);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(out).map_err(|e| e.to_string())?;
        f.write_all(hex(&seed).as_bytes()).map_err(|e| e.to_string())?;
    }
    #[cfg(not(unix))]
    std::fs::write(out, hex(&seed)).map_err(|e| e.to_string())?;
    Ok(hex(key.verifying_key().as_bytes()))
}

/// Copia `file` para a pasta de atualizações (nome = SHA-256) e escreve o manifesto da plataforma.
/// Assina se `TELACAST_UPDATE_KEY_FILE` apontar para uma chave de `update-keygen`.
pub fn publish(file: &std::path::Path, version: &str, platform: &str, notes: &str) -> Result<UpdateManifest, String> {
    if !valid_platform(platform) {
        return Err("plataforma inválida (use por exemplo linux-x86_64)".into());
    }
    if version.trim().is_empty() || version.len() > 32 {
        return Err("versão inválida".into());
    }
    let bytes = std::fs::read(file).map_err(|e| format!("não consegui ler {}: {e}", file.display()))?;
    let sha256 = hex(&Sha256::digest(&bytes));
    let size = bytes.len() as u64;
    let signature = match std::env::var("TELACAST_UPDATE_KEY_FILE") {
        Ok(path) => {
            let seed = unhex(&std::fs::read_to_string(&path).map_err(|e| format!("chave {path}: {e}"))?)
                .and_then(|v| <[u8; 32]>::try_from(v).ok())
                .ok_or("a chave precisa ter 64 caracteres hexadecimais")?;
            let key = SigningKey::from_bytes(&seed);
            hex(&key.sign(&update_signing_message(version, platform, &sha256, size)).to_bytes())
        }
        Err(_) => String::new(),
    };
    let m = UpdateManifest { version: version.into(), platform: platform.into(), sha256: sha256.clone(), size, notes: notes.into(), signature };
    let d = dir();
    std::fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    std::fs::write(d.join(&sha256), &bytes).map_err(|e| e.to_string())?;
    std::fs::write(d.join(format!("manifest-{platform}.json")), serde_json::to_vec_pretty(&m).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validacoes() {
        assert!(valid_platform("linux-x86_64"));
        assert!(!valid_platform("../x"));
        assert!(!valid_platform("Linux"));
        assert!(valid_hash(&"a".repeat(64)));
        assert!(!valid_hash(&"A".repeat(64)));
        assert!(!valid_hash("../../etc/passwd"));
    }

    #[test]
    fn hex_ida_e_volta() {
        let b = [0u8, 1, 0xab, 0xff];
        assert_eq!(unhex(&hex(&b)).unwrap(), b);
        assert!(unhex("abc").is_none());
        assert!(unhex("zz").is_none());
    }
}
