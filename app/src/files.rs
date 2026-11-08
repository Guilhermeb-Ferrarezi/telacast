//! Anexos no cliente: envio por HTTP, download de imagens em segundo plano e cache de texturas.
use eframe::egui;
use std::{
    collections::HashMap,
    io::Read,
    path::{Path, PathBuf},
    sync::mpsc::{channel, Receiver, Sender},
};
use telacast_proto as p;

const MAX_UPLOAD: u64 = 10 << 20; // o servidor também limita (padrão 10 MiB)
const MAX_DOWNLOAD: u64 = 25 << 20;
const MAX_INFLIGHT: usize = 4;

/// `ws://host` vira `http://host`, `wss://host` vira `https://host`.
pub fn http_base(server: &str) -> String {
    let s = server.trim().trim_end_matches('/');
    if let Some(r) = s.strip_prefix("wss://") {
        format!("https://{r}")
    } else if let Some(r) = s.strip_prefix("ws://") {
        format!("http://{r}")
    } else {
        s.to_string()
    }
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.') { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

/// Endereço do arquivo (com o token da sessão, como o servidor exige).
pub fn url(base: &str, a: &p::Attachment, token: &str) -> String {
    format!("{base}/files/{}/{}?token={token}", a.id, enc(&a.name))
}

/// Endereço da miniatura (`thumb=1`): o servidor devolve uma versão de até 1024 px, ou o original se não der
/// para reduzir (GIF, imagem pequena...). Servidor antigo ignora o parâmetro e devolve o original.
pub fn thumb_url(base: &str, a: &p::Attachment, token: &str) -> String {
    format!("{}&thumb=1", url(base, a, token))
}

/// O servidor só gera miniatura de PNG, JPEG e WebP (GIF fica de fora para não perder a animação).
fn thumbable(a: &p::Attachment) -> bool {
    matches!(a.mime.as_str(), "image/png" | "image/jpeg" | "image/webp")
}

/// Lado maior da miniatura do servidor: imagem que já cabe aqui vem inteira, miniatura e original são iguais.
const THUMB_SIDE: u32 = 1024;

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().tls_config(crate::net::tls_config()).timeout(std::time::Duration::from_secs(60)).build()
}

pub fn human_size(n: u64) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1 << 20 {
        format!("{:.1} KB", n as f64 / 1024.0)
    } else {
        format!("{:.1} MB", n as f64 / (1 << 20) as f64)
    }
}

/// Envia o arquivo para o canal e devolve os metadados que o servidor guardou.
pub fn upload(base: &str, token: &str, channel: i64, path: &Path) -> Result<p::Attachment, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("não consegui abrir o arquivo ({e})"))?;
    if !meta.is_file() {
        return Err("isso não é um arquivo".into());
    }
    if meta.len() > MAX_UPLOAD {
        return Err(format!("o arquivo tem {} e o limite é {}", human_size(meta.len()), human_size(MAX_UPLOAD)));
    }
    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "arquivo".into());
    let bytes = std::fs::read(path).map_err(|e| format!("não consegui ler o arquivo ({e})"))?;
    let u = format!("{base}/upload?token={token}&channel={channel}&name={}", enc(&name));
    match agent().post(&u).send_bytes(&bytes) {
        Ok(r) => {
            let j: serde_json::Value = r.into_json().map_err(|e| format!("resposta inesperada ({e})"))?;
            Ok(p::Attachment {
                id: j["id"].as_i64().ok_or("resposta sem id")?,
                name: j["name"].as_str().unwrap_or(&name).to_string(),
                mime: j["mime"].as_str().unwrap_or("application/octet-stream").to_string(),
                size: j["size"].as_u64().unwrap_or(bytes.len() as u64),
                width: j["width"].as_u64().unwrap_or(0) as u32,
                height: j["height"].as_u64().unwrap_or(0) as u32,
            })
        }
        Err(ureq::Error::Status(413, _)) => Err("o servidor recusou: arquivo grande demais".into()),
        Err(ureq::Error::Status(403, _)) => Err("você não pode enviar arquivos neste canal".into()),
        Err(ureq::Error::Status(429, _)) => Err("muitos envios seguidos; espere um instante".into()),
        Err(ureq::Error::Status(c, _)) => Err(format!("o servidor respondeu {c}")),
        Err(e) => Err(format!("falha de rede ({e})")),
    }
}

/// Abre o seletor de arquivos do sistema (kdialog ou zenity); `None` se cancelou ou não há nenhum instalado.
#[cfg(windows)]
pub fn pick_files() -> Vec<PathBuf> {
    rfd::FileDialog::new().set_title("Anexar arquivos").pick_files().unwrap_or_default()
}

#[cfg(not(windows))]
pub fn pick_files() -> Vec<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_default();
    let tries: [(&str, Vec<String>); 2] = [
        ("kdialog", vec!["--getopenfilename".into(), home.clone(), "--multiple".into(), "--separate-output".into()]),
        ("zenity", vec!["--file-selection".into(), "--multiple".into(), "--separator=\n".into(), format!("--filename={home}/")]),
    ];
    for (cmd, args) in tries {
        if let Ok(out) = std::process::Command::new(cmd).args(&args).output() {
            if !out.status.success() {
                return Vec::new();
            }
            return String::from_utf8_lossy(&out.stdout).lines().map(str::trim).filter(|l| !l.is_empty()).map(PathBuf::from).collect();
        }
    }
    Vec::new()
}

/// Arquivo que está sendo enviado (ou já foi) para o campo de mensagem de um canal.
pub struct Upload {
    pub key: u64,
    pub channel: i64,
    pub name: String,
    pub path: std::path::PathBuf,
    pub state: UpState,
    /// Já pertence a uma mensagem que espera os envios terminarem.
    pub armed: bool,
    /// Só tenta de novo depois disso (servidor pediu calma com 429).
    pub not_before: Option<std::time::Instant>,
}

pub enum UpState {
    /// Esperando vaga (há no máximo 3 envios ao mesmo tempo).
    Queued,
    Sending,
    Done(p::Attachment),
    Failed(String),
}

pub enum Img {
    Loading,
    Ready(egui::TextureHandle),
    Failed,
}

/// Cache de imagens dos anexos: baixa e decodifica em segundo plano, cria a textura na thread da interface.
pub struct Images {
    map: HashMap<i64, Img>,
    tx: Sender<(i64, Option<egui::ColorImage>)>,
    rx: Receiver<(i64, Option<egui::ColorImage>)>,
    inflight: usize,
}

impl Images {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        Self { map: HashMap::new(), tx, rx, inflight: 0 }
    }

    /// Recebe o que as threads terminaram e vira textura.
    pub fn pump(&mut self, ctx: &egui::Context) {
        while let Ok((id, img)) = self.rx.try_recv() {
            self.inflight = self.inflight.saturating_sub(1);
            self.map.insert(
                id,
                match img {
                    Some(ci) => Img::Ready(ctx.load_texture(format!("att{id}"), ci, egui::TextureOptions::LINEAR)),
                    None => Img::Failed,
                },
            );
            ctx.request_repaint();
        }
    }

    /// Imagem do chat: baixa a miniatura do servidor (chave `id`).
    pub fn get(&mut self, ctx: &egui::Context, base: &str, token: &str, a: &p::Attachment) -> &Img {
        let u = if thumbable(a) { thumb_url(base, a, token) } else { url(base, a, token) };
        self.load(a.id, ctx, u, (1200, 900))
    }

    /// Original para a prévia ampliada (até 4096 px de lado). Fica sob a chave `-id` e só é pedido quando a
    /// prévia abre. Se a imagem já cabe na miniatura, ela é o original e não baixa de novo.
    pub fn get_full(&mut self, ctx: &egui::Context, base: &str, token: &str, a: &p::Attachment) -> &Img {
        if a.width > 0 && a.width.max(a.height) <= THUMB_SIDE {
            return self.get(ctx, base, token, a);
        }
        self.load(-a.id, ctx, url(base, a, token), (4096, 4096))
    }

    /// Miniatura de um arquivo local (bandeja de anexos pendentes). Chave própria por envio.
    pub fn get_local(&mut self, ctx: &egui::Context, upload_key: u64, path: &std::path::Path, full: bool) -> &Img {
        let path = path.to_path_buf();
        let (key, lim) = if full { (-(1 << 61) - upload_key as i64, (4096, 4096)) } else { (-(1 << 60) - upload_key as i64, (480, 480)) };
        self.load_with(key, ctx, move || {
            let bytes = std::fs::read(&path).ok()?;
            decode(&bytes, lim)
        })
    }

    fn load(&mut self, key: i64, ctx: &egui::Context, u: String, lim: (u32, u32)) -> &Img {
        self.load_with(key, ctx, move || fetch_image(&u, lim))
    }

    fn load_with(&mut self, key: i64, ctx: &egui::Context, f: impl FnOnce() -> Option<egui::ColorImage> + Send + 'static) -> &Img {
        if !self.map.contains_key(&key) && self.inflight < MAX_INFLIGHT {
            self.map.insert(key, Img::Loading);
            self.inflight += 1;
            let (tx, ctx2) = (self.tx.clone(), ctx.clone());
            std::thread::spawn(move || {
                let _ = tx.send((key, f()));
                ctx2.request_repaint();
            });
        }
        self.map.get(&key).unwrap_or(&Img::Loading)
    }
}

fn fetch_image(url: &str, lim: (u32, u32)) -> Option<egui::ColorImage> {
    let resp = agent().get(url).call().ok()?;
    let mut bytes = Vec::new();
    resp.into_reader().take(MAX_DOWNLOAD).read_to_end(&mut bytes).ok()?;
    decode(&bytes, lim)
}

fn decode(bytes: &[u8], lim: (u32, u32)) -> Option<egui::ColorImage> {
    let img = decode_upright(bytes)?;
    // miniatura do chat: mostra no máximo 400x300, guardamos 1200x900 para telas densas
    let img = if img.width() > lim.0 || img.height() > lim.1 { img.thumbnail(lim.0, lim.1) } else { img };
    let rgba = img.to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw()))
}

/// Decodifica respeitando a orientação EXIF (foto de celular): a miniatura do servidor já vem em pé, o
/// original precisa ser girado aqui para as duas versões combinarem.
fn decode_upright(bytes: &[u8]) -> Option<image::DynamicImage> {
    let reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().ok()?;
    let mut dec = reader.into_decoder().ok()?;
    let orientation = image::ImageDecoder::orientation(&mut dec).unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = image::DynamicImage::from_decoder(dec).ok()?;
    img.apply_orientation(orientation);
    Some(img)
}

// ------------------------------------------------------------ prévia de texto

/// Quantos bytes do começo do arquivo a prévia lê (como no Discord: `Range: bytes=0-50000`).
const TEXT_PREVIEW_BYTES: u64 = 50_000;

/// Arquivo de texto/código que ganha prévia no chat (pela extensão ou pelo tipo `text/*`).
pub fn is_text_like(a: &p::Attachment) -> bool {
    const EXT: &[&str] = &[
        "txt", "md", "markdown", "log", "json", "jsonc", "toml", "yaml", "yml", "ini", "cfg", "conf", "csv", "tsv", "xml", "html", "htm", "css",
        "js", "mjs", "ts", "tsx", "jsx", "rs", "py", "rb", "go", "java", "kt", "c", "h", "cpp", "hpp", "cs", "sh", "bash", "fish", "sql", "lua",
        "php", "swift", "diff", "patch", "env", "lock", "gradle", "properties",
    ];
    let ext = Path::new(&a.name).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    (a.mime.starts_with("text/") || EXT.contains(&ext.as_str())) && !a.mime.starts_with("image/")
}

pub enum Text {
    Loading,
    /// `bytes_left`: o que ficou de fora (0 = arquivo inteiro).
    Ready { text: String, lines: usize, bytes_left: u64 },
    Failed,
}

/// Cache das prévias de texto: lê só o começo do arquivo, em segundo plano.
pub struct Texts {
    map: HashMap<i64, Text>,
    tx: Sender<(i64, Option<(String, u64)>)>,
    rx: Receiver<(i64, Option<(String, u64)>)>,
}

impl Texts {
    pub fn new() -> Self {
        let (tx, rx) = channel();
        Self { map: HashMap::new(), tx, rx }
    }

    pub fn pump(&mut self, ctx: &egui::Context) {
        while let Ok((id, res)) = self.rx.try_recv() {
            self.map.insert(
                id,
                match res {
                    Some((text, bytes_left)) => {
                        let lines = text.lines().count().max(1);
                        Text::Ready { text, lines, bytes_left }
                    }
                    None => Text::Failed,
                },
            );
            ctx.request_repaint();
        }
    }

    pub fn get(&mut self, ctx: &egui::Context, base: &str, token: &str, a: &p::Attachment) -> &Text {
        if !self.map.contains_key(&a.id) {
            self.map.insert(a.id, Text::Loading);
            let (tx, ctx2, u, total, id) = (self.tx.clone(), ctx.clone(), url(base, a, token), a.size, a.id);
            std::thread::spawn(move || {
                let _ = tx.send((id, fetch_text(&u, total)));
                ctx2.request_repaint();
            });
        }
        self.map.get(&a.id).unwrap_or(&Text::Loading)
    }
}

fn fetch_text(url: &str, total: u64) -> Option<(String, u64)> {
    let resp = agent().get(url).set("Range", &format!("bytes=0-{}", TEXT_PREVIEW_BYTES)).call().ok()?;
    let mut bytes = Vec::new();
    resp.into_reader().take(TEXT_PREVIEW_BYTES + 1).read_to_end(&mut bytes).ok()?;
    let truncated = (bytes.len() as u64) > TEXT_PREVIEW_BYTES || total > bytes.len() as u64;
    bytes.truncate(TEXT_PREVIEW_BYTES as usize);
    let mut text = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    if truncated {
        // o corte pode ter partido um caractere no meio
        while text.ends_with('\u{FFFD}') {
            text.pop();
        }
    }
    let left = if truncated { total.saturating_sub(bytes.len() as u64).max(1) } else { 0 };
    Some((text, left))
}
