//! Anexos: envio e download por HTTP (fora do WebSocket), com o arquivo em disco endereçado pelo hash.
use crate::{auth, thumbs, App};
use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};
use std::{collections::HashMap, path::PathBuf, sync::Arc, time::Duration};
use tracing::error;

/// Tamanho máximo de um arquivo (padrão 10 MiB; `TELACAST_MAX_UPLOAD_MB` muda).
pub fn max_upload() -> usize {
    std::env::var("TELACAST_MAX_UPLOAD_MB").ok().and_then(|v| v.parse::<usize>().ok()).unwrap_or(10).max(1) << 20
}

pub fn files_dir() -> PathBuf {
    std::env::var("TELACAST_FILES_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from("files"))
}

/// Tira pastas e caracteres de controle do nome que o cliente mandou.
fn clean_name(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or("");
    let s: String = base.chars().filter(|c| !c.is_control()).take(200).collect();
    let s = s.trim().to_string();
    if s.is_empty() { "arquivo".into() } else { s }
}

/// O tipo vem do conteúdo, nunca do que o cliente diz: só imagens conhecidas são mostradas inline.
fn sniff(b: &[u8]) -> &'static str {
    if b.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        "image/png"
    } else if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "image/jpeg"
    } else if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        "image/gif"
    } else if b.len() > 12 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        "image/webp"
    } else {
        "application/octet-stream"
    }
}

pub(crate) fn path_for(hash: &str) -> PathBuf {
    files_dir().join(&hash[..2]).join(hash)
}

async fn auth_user(app: &Arc<App>, q: &HashMap<String, String>) -> Option<i64> {
    let token = q.get("token")?;
    app.store.session_user(&auth::hash_token(token)).await.ok().flatten()
}

/// `POST /upload?token=T&channel=ID&name=ARQUIVO` com o arquivo no corpo. Devolve os metadados em JSON.
pub async fn upload(State(app): State<Arc<App>>, Query(q): Query<HashMap<String, String>>, body: Bytes) -> Response {
    let Some(uid) = auth_user(&app, &q).await else { return StatusCode::UNAUTHORIZED.into_response() };
    let Some(channel) = q.get("channel").and_then(|c| c.parse::<i64>().ok()) else { return StatusCode::BAD_REQUEST.into_response() };
    if !app.store.can_access(channel, uid).await.unwrap_or(false) {
        return StatusCode::FORBIDDEN.into_response();
    }
    if !app.limits.allow(&format!("upload:{uid}"), 20, Duration::from_secs(60)) {
        return StatusCode::TOO_MANY_REQUESTS.into_response();
    }
    if body.is_empty() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let name = clean_name(q.get("name").map(String::as_str).unwrap_or(""));
    let mime = sniff(&body);
    let (w, h) = if mime.starts_with("image/") {
        imagesize::blob_size(&body).map(|s| (Some(s.width as i32), Some(s.height as i32))).unwrap_or((None, None))
    } else {
        (None, None)
    };
    let hash: String = Sha256::digest(&body).iter().map(|b| format!("{b:02x}")).collect();
    let path = path_for(&hash);
    if tokio::fs::metadata(&path).await.is_err() {
        let Some(dir) = path.parent() else { return StatusCode::INTERNAL_SERVER_ERROR.into_response() };
        if let Err(e) = tokio::fs::create_dir_all(dir).await {
            error!("criar pasta de anexos: {e}");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        // grava num arquivo temporário e renomeia: ninguém lê um arquivo pela metade
        let tmp = dir.join(format!("{hash}.{}.tmp", app.sf.next()));
        if let Err(e) = tokio::fs::write(&tmp, &body).await.and(tokio::fs::rename(&tmp, &path).await) {
            error!("gravar anexo: {e}");
            let _ = tokio::fs::remove_file(&tmp).await;
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    }
    let id = app.sf.next();
    if let Err(e) = app.store.insert_attachment(id, uid, channel, &name, mime, body.len() as i64, w, h, &hash).await {
        error!("insert_attachment: {e}");
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    }
    let json = serde_json::json!({ "id": id, "name": name, "mime": mime, "size": body.len(), "width": w.unwrap_or(0), "height": h.unwrap_or(0) });
    ([(header::CONTENT_TYPE, "application/json")], json.to_string()).into_response()
}

/// `GET /files/{id}/{nome}?token=T[&thumb=1]`: só quem pode ler o canal da mensagem baixa o arquivo.
/// Com `thumb=1` devolve a miniatura (com `X-Telacast-Thumb: 1`) quando ela existe; senão, o original.
pub async fn download(State(app): State<Arc<App>>, Path((id, _name)): Path<(i64, String)>, Query(q): Query<HashMap<String, String>>) -> Response {
    let Some(uid) = auth_user(&app, &q).await else { return StatusCode::UNAUTHORIZED.into_response() };
    let Ok(Some((channel, name, mime, hash))) = app.store.attachment_row(id).await else { return StatusCode::NOT_FOUND.into_response() };
    if !app.store.can_access(channel, uid).await.unwrap_or(false) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let original = path_for(&hash);
    let thumb = match q.get("thumb").map(String::as_str) {
        Some("1") => match thumbs::get(&hash, &mime, &original).await {
            thumbs::Outcome::Thumb(t) => Some(t),
            thumbs::Outcome::Original => None,
        },
        _ => None,
    };
    let is_thumb = thumb.is_some();
    let (bytes, mime) = match thumb {
        Some(t) => (t.bytes, t.mime.to_string()),
        None => match tokio::fs::read(&original).await {
            Ok(b) => (b, mime),
            Err(_) => return StatusCode::NOT_FOUND.into_response(),
        },
    };
    app.count_egress(bytes.len());
    let mut h = HeaderMap::new();
    h.insert(header::CONTENT_TYPE, HeaderValue::from_str(&mime).unwrap_or(HeaderValue::from_static("application/octet-stream")));
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, max-age=31536000, immutable"));
    h.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    if is_thumb {
        h.insert("x-telacast-thumb", HeaderValue::from_static("1"));
    }
    let inline = mime.starts_with("image/");
    let safe: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ' ')).collect();
    let disp = format!("{}; filename=\"{}\"", if inline { "inline" } else { "attachment" }, if safe.is_empty() { "arquivo" } else { &safe });
    if let Ok(v) = HeaderValue::from_str(&disp) {
        h.insert(header::CONTENT_DISPOSITION, v);
    }
    (h, bytes).into_response()
}
