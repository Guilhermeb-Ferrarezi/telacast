//! Miniaturas de imagem geradas sob demanda e guardadas em disco (`<files_dir>/thumbs/<hash>.jpg|png`).
//!
//! Contrato (ver `docs/PROTOCOL.md`): lado maior <= 1024 px, nunca amplia, JPEG qualidade 80 quando não há
//! transparência e PNG quando há. Tudo que não puder virar uma miniatura menor que o original (GIF, WebP/PNG
//! animado, arquivo que não decodifica, imagem enorme, imagem que já cabe em 1024 px, resultado maior que o
//! original) cai para o original, e o servidor lembra disso com um marcador vazio `<hash>.none`.
use crate::files::files_dir;
use image::{codecs::jpeg::JpegEncoder, codecs::png::PngEncoder, imageops::FilterType, DynamicImage, ExtendedColorType, ImageDecoder, ImageEncoder, ImageFormat, ImageReader, Limits};
use std::{
    collections::HashMap,
    io::Cursor,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};
use tokio::sync::Semaphore;
use tracing::{error, warn};

/// Lado maior da miniatura.
pub const MAX_SIDE: u32 = 1024;
/// Qualidade do JPEG.
const JPEG_QUALITY: u8 = 80;
/// Acima disso (largura x altura) a imagem não é decodificada: protege a memória contra "bombas".
pub const MAX_PIXELS: u64 = 64_000_000;
/// Acima disso de bytes já decodificados (RGBA8 de 64 MP = 256 MiB) também recusa.
const MAX_DECODED_BYTES: u64 = 300 << 20;
/// Quantas miniaturas são geradas ao mesmo tempo.
const CONCURRENCY: usize = 2;

pub struct Thumb {
    pub bytes: Vec<u8>,
    pub mime: &'static str,
    pub ext: &'static str,
}

/// Resultado de uma tentativa: miniatura pronta ou "use o original".
pub enum Outcome {
    Thumb(Thumb),
    Original,
}

fn format_of(mime: &str) -> Option<ImageFormat> {
    match mime {
        "image/png" => Some(ImageFormat::Png),
        "image/jpeg" => Some(ImageFormat::Jpeg),
        "image/webp" => Some(ImageFormat::WebP),
        _ => None,
    }
}

/// PNG animado (APNG): tem um bloco `acTL` antes dos dados.
fn is_apng(b: &[u8]) -> bool {
    let mut i = 8;
    while i + 8 <= b.len() {
        let len = u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as usize;
        match &b[i + 4..i + 8] {
            b"acTL" => return true,
            b"IDAT" | b"IEND" => return false,
            _ => {}
        }
        i = i.saturating_add(12).saturating_add(len);
    }
    false
}

/// WebP animado: o cabeçalho estendido (`VP8X`) liga o bit de animação.
fn is_animated_webp(b: &[u8]) -> bool {
    b.len() > 21 && &b[12..16] == b"VP8X" && b[20] & 0x02 != 0
}

fn too_large(w: u32, h: u32, decoded_bytes: u64) -> bool {
    w == 0 || h == 0 || u64::from(w) * u64::from(h) > MAX_PIXELS || decoded_bytes > MAX_DECODED_BYTES
}

/// Faz a miniatura. Função pura e síncrona (roda dentro de `spawn_blocking`); não toca em disco.
pub fn make(bytes: &[u8], mime: &str) -> Outcome {
    let Some(fmt) = format_of(mime) else { return Outcome::Original };
    if (fmt == ImageFormat::Png && is_apng(bytes)) || (fmt == ImageFormat::WebP && is_animated_webp(bytes)) {
        return Outcome::Original;
    }
    let mut limits = Limits::default();
    limits.max_alloc = Some(400 << 20);
    let mut reader = ImageReader::with_format(Cursor::new(bytes), fmt);
    reader.limits(limits);
    let Ok(mut decoder) = reader.into_decoder() else { return Outcome::Original };
    let (w, h) = decoder.dimensions();
    // a checagem vem do cabeçalho, antes de alocar qualquer pixel
    if too_large(w, h, decoder.total_bytes()) {
        return Outcome::Original;
    }
    // já cabe na miniatura: não vale reencodar (perderia qualidade sem economizar nada)
    if w.max(h) <= MAX_SIDE {
        return Outcome::Original;
    }
    // orientação EXIF (fotos de celular): a miniatura sai em pé e o EXIF não é copiado
    let orientation = decoder.orientation().unwrap_or(image::metadata::Orientation::NoTransforms);
    let Ok(mut img) = DynamicImage::from_decoder(decoder) else { return Outcome::Original };
    img.apply_orientation(orientation);
    let small = img.resize(MAX_SIDE, MAX_SIDE, FilterType::Lanczos3);
    drop(img);
    let Some(t) = encode(small) else { return Outcome::Original };
    if t.bytes.len() >= bytes.len() {
        return Outcome::Original;
    }
    Outcome::Thumb(t)
}

fn encode(img: DynamicImage) -> Option<Thumb> {
    let (w, h) = (img.width(), img.height());
    let mut out = Vec::new();
    if img.color().has_alpha() {
        let rgba = img.to_rgba8();
        if rgba.pixels().any(|p| p.0[3] < 255) {
            PngEncoder::new(&mut out).write_image(rgba.as_raw(), w, h, ExtendedColorType::Rgba8).ok()?;
            return Some(Thumb { bytes: out, mime: "image/png", ext: "png" });
        }
    }
    // sem transparência de verdade (inclui PNG RGBA todo opaco): JPEG
    let rgb = img.to_rgb8();
    JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY).write_image(rgb.as_raw(), w, h, ExtendedColorType::Rgb8).ok()?;
    Some(Thumb { bytes: out, mime: "image/jpeg", ext: "jpg" })
}

// ------------------------------------------------------------------ cache em disco e concorrência

struct State {
    sem: Semaphore,
    /// Um trava por miniatura em andamento: pedidos iguais esperam o primeiro e acham o arquivo pronto.
    locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

fn state() -> &'static State {
    static S: OnceLock<State> = OnceLock::new();
    S.get_or_init(|| State { sem: Semaphore::new(CONCURRENCY), locks: Mutex::new(HashMap::new()) })
}

fn dir() -> PathBuf {
    files_dir().join("thumbs")
}

/// Lê do cache: `Some(Outcome)` se já foi decidido antes.
async fn cached(hash: &str) -> Option<Outcome> {
    let d = dir();
    for (ext, mime) in [("jpg", "image/jpeg"), ("png", "image/png")] {
        if let Ok(bytes) = tokio::fs::read(d.join(format!("{hash}.{ext}"))).await {
            return Some(Outcome::Thumb(Thumb { bytes, mime, ext }));
        }
    }
    if tokio::fs::metadata(d.join(format!("{hash}.none"))).await.is_ok() {
        return Some(Outcome::Original);
    }
    None
}

async fn store(hash: &str, name: &str, bytes: &[u8]) {
    let d = dir();
    let tmp = d.join(format!("{hash}.{}.tmp", std::process::id()));
    let res = async {
        tokio::fs::create_dir_all(&d).await?;
        tokio::fs::write(&tmp, bytes).await?;
        tokio::fs::rename(&tmp, d.join(name)).await
    }
    .await;
    if let Err(e) = res {
        error!("gravar miniatura: {e}");
        let _ = tokio::fs::remove_file(&tmp).await;
    }
}

/// Miniatura do arquivo com este hash (cache em disco; gera na primeira vez). `original` é o caminho do arquivo.
pub async fn get(hash: &str, mime: &str, original: &Path) -> Outcome {
    if format_of(mime).is_none() {
        return Outcome::Original;
    }
    if let Some(o) = cached(hash).await {
        return o;
    }
    let st = state();
    let lock = st.locks.lock().unwrap().entry(hash.to_string()).or_default().clone();
    let outcome = {
        let _g = lock.lock().await;
        // quem esperava pela trava acha o resultado do primeiro
        match cached(hash).await {
            Some(o) => o,
            None => generate(hash, mime, original).await,
        }
    };
    let mut locks = st.locks.lock().unwrap();
    // 2 = o mapa + esta cópia: ninguém mais está esperando
    if locks.get(hash).is_some_and(|l| Arc::strong_count(l) <= 2) {
        locks.remove(hash);
    }
    outcome
}

async fn generate(hash: &str, mime: &str, original: &Path) -> Outcome {
    let Ok(_permit) = state().sem.acquire().await else { return Outcome::Original };
    let (path, mime2) = (original.to_path_buf(), mime.to_string());
    let res = tokio::task::spawn_blocking(move || std::fs::read(&path).map(|b| make(&b, &mime2))).await;
    match res {
        Ok(Ok(Outcome::Thumb(t))) => {
            store(hash, &format!("{hash}.{}", t.ext), &t.bytes).await;
            Outcome::Thumb(t)
        }
        Ok(Ok(Outcome::Original)) => {
            store(hash, &format!("{hash}.none"), &[]).await;
            Outcome::Original
        }
        // arquivo sumiu ou a tarefa quebrou: não grava nada, tenta de novo no próximo pedido
        Ok(Err(e)) => {
            warn!("ler o original para a miniatura: {e}");
            Outcome::Original
        }
        Err(e) => {
            error!("tarefa da miniatura: {e}");
            Outcome::Original
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb, Rgba};

    /// Imagem com cara de foto (gradiente suave + formas), nada de ruído puro.
    fn paisagem(w: u32, h: u32) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
        ImageBuffer::from_fn(w, h, |x, y| {
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            let sol = (-(((fx - 0.7).powi(2) + (fy - 0.3).powi(2)) * 40.0)).exp();
            let morro = if fy > 0.6 + 0.1 * (fx * 9.0).sin() { 0.3 } else { 1.0 };
            let r = ((60.0 + 150.0 * fy + 200.0 * sol) * morro).min(255.0);
            let g = ((90.0 + 90.0 * fy + 150.0 * sol) * morro).min(255.0);
            let b = ((200.0 - 100.0 * fy) * morro).min(255.0);
            Rgb([r as u8, g as u8, b as u8])
        })
    }

    fn png(img: &DynamicImage) -> Vec<u8> {
        let mut v = Vec::new();
        img.write_to(&mut Cursor::new(&mut v), ImageFormat::Png).unwrap();
        v
    }

    fn dims(b: &[u8]) -> (u32, u32) {
        let s = imagesize::blob_size(b).unwrap();
        (s.width as u32, s.height as u32)
    }

    #[test]
    fn png_grande_vira_jpeg_menor() {
        let orig = png(&DynamicImage::ImageRgb8(paisagem(3000, 2000)));
        let Outcome::Thumb(t) = make(&orig, "image/png") else { panic!("devia gerar miniatura") };
        assert_eq!((t.mime, t.ext), ("image/jpeg", "jpg"));
        assert!(t.bytes.starts_with(&[0xFF, 0xD8, 0xFF]));
        let (w, h) = dims(&t.bytes);
        assert_eq!((w, h), (1024, 683), "mantém a proporção, lado maior 1024");
        assert!(t.bytes.len() < orig.len(), "{} >= {}", t.bytes.len(), orig.len());
    }

    #[test]
    fn retrato_e_nunca_amplia() {
        let orig = png(&DynamicImage::ImageRgb8(paisagem(1500, 3000)));
        let Outcome::Thumb(t) = make(&orig, "image/png") else { panic!() };
        let (w, h) = dims(&t.bytes);
        assert!(h == 1024 && w <= 1024);
        // já cabe em 1024: devolve o original
        let pequena = png(&DynamicImage::ImageRgb8(paisagem(1024, 700)));
        assert!(matches!(make(&pequena, "image/png"), Outcome::Original));
    }

    #[test]
    fn alfa_vira_png_e_opaco_vira_jpeg() {
        let alfa = ImageBuffer::from_fn(2400, 1600, |x, y| {
            let dentro = ((x as f32 - 1200.0).powi(2) + (y as f32 - 800.0).powi(2)).sqrt() < 700.0;
            if dentro { Rgba([220, 90, 20, 255]) } else { Rgba([0, 0, 0, 0]) }
        });
        let orig = png(&DynamicImage::ImageRgba8(alfa));
        let Outcome::Thumb(t) = make(&orig, "image/png") else { panic!("devia gerar miniatura") };
        assert_eq!((t.mime, t.ext), ("image/png", "png"));
        let dec = image::load_from_memory(&t.bytes).unwrap().to_rgba8();
        assert_eq!(dec.dimensions(), (1024, 683));
        assert_eq!(dec.get_pixel(2, 2).0[3], 0, "o canto continua transparente");
        assert_eq!(dec.get_pixel(512, 341).0[3], 255);
        // RGBA sem nenhum pixel transparente: JPEG
        let opaco = DynamicImage::ImageRgb8(paisagem(2400, 1600)).to_rgba8();
        let Outcome::Thumb(t) = make(&png(&DynamicImage::ImageRgba8(opaco)), "image/png") else { panic!() };
        assert_eq!(t.mime, "image/jpeg");
    }

    #[test]
    fn jpeg_entra_e_sai_e_orientacao_exif() {
        let mut jpg = Vec::new();
        JpegEncoder::new_with_quality(&mut jpg, 95).write_image(paisagem(3000, 2000).as_raw(), 3000, 2000, ExtendedColorType::Rgb8).unwrap();
        let Outcome::Thumb(t) = make(&jpg, "image/jpeg") else { panic!() };
        assert_eq!(dims(&t.bytes), (1024, 683));
        assert!(t.bytes.len() < jpg.len());
        // EXIF com Orientation = 6 (girar 90 graus): a miniatura sai em pé (683x1024)
        let exif: Vec<u8> = [
            &b"Exif\0\0"[..],
            &[b'M', b'M', 0, 42, 0, 0, 0, 8, 0, 1, 0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, 6, 0, 0, 0, 0, 0, 0],
        ]
        .concat();
        let mut com_exif = vec![0xFF, 0xD8, 0xFF, 0xE1];
        com_exif.extend(((exif.len() + 2) as u16).to_be_bytes());
        com_exif.extend(&exif);
        com_exif.extend(&jpg[2..]);
        let Outcome::Thumb(t) = make(&com_exif, "image/jpeg") else { panic!() };
        assert_eq!(dims(&t.bytes), (683, 1024), "respeita a orientação EXIF");
    }

    #[test]
    fn cai_para_o_original() {
        let grande = png(&DynamicImage::ImageRgb8(paisagem(2000, 1500)));
        // GIF fica de fora (animação), qualquer outro tipo também
        assert!(matches!(make(&grande, "image/gif"), Outcome::Original));
        assert!(matches!(make(&grande, "application/octet-stream"), Outcome::Original));
        // lixo e arquivo cortado não decodificam
        assert!(matches!(make(b"isto nao e uma imagem", "image/png"), Outcome::Original));
        assert!(matches!(make(&grande[..grande.len() / 3], "image/png"), Outcome::Original));
        assert!(matches!(make(&[0xFF, 0xD8, 0xFF, 0x00, 0x01], "image/jpeg"), Outcome::Original));
        // PNG animado (acTL antes do IDAT)
        let mut apng = grande[..33].to_vec(); // assinatura + IHDR
        apng.extend([0, 0, 0, 8, b'a', b'c', b'T', b'L', 0, 0, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0]);
        apng.extend(&grande[33..]);
        assert!(matches!(make(&apng, "image/png"), Outcome::Original));
    }

    /// Cabeçalho PNG que anuncia o tamanho dado, sem pixels de verdade (a "bomba").
    fn bomba(w: u32, h: u32) -> Vec<u8> {
        fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
            out.extend((data.len() as u32).to_be_bytes());
            out.extend(tag);
            out.extend(data);
            out.extend(crc32(&[tag.as_slice(), data].concat()).to_be_bytes());
        }
        let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        let mut ihdr = Vec::new();
        ihdr.extend(w.to_be_bytes());
        ihdr.extend(h.to_be_bytes());
        ihdr.extend([8, 2, 0, 0, 0]);
        chunk(&mut out, b"IHDR", &ihdr);
        chunk(&mut out, b"IDAT", &[0x78, 0x9C, 0x63, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01]);
        chunk(&mut out, b"IEND", &[]);
        out
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &b in data {
            c ^= u32::from(b);
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB8_8320 ^ (c >> 1) } else { c >> 1 };
            }
        }
        !c
    }

    #[test]
    fn bomba_de_pixels_e_recusada() {
        // 30000 x 30000 = 900 MP anunciados num arquivo de poucos bytes
        let b = bomba(30_000, 30_000);
        assert!(b.len() < 100);
        let t = std::time::Instant::now();
        assert!(matches!(make(&b, "image/png"), Outcome::Original));
        assert!(t.elapsed() < std::time::Duration::from_secs(1), "recusa pelo cabeçalho, sem alocar");
        // 8001 x 8001 = 64,016 MP: logo acima do limite (e 8000 x 8000 = 64 MP ainda passa)
        assert!(too_large(8001, 8001, 0) && !too_large(8000, 8000, 0) && too_large(100, 100, 400 << 20));
        assert!(too_large(65_535, 65_535, 0) && too_large(0, 10, 0));
        assert!(matches!(make(&bomba(8001, 8001), "image/png"), Outcome::Original));
    }

    #[test]
    fn webp_animado_fica_de_fora() {
        let mut w = b"RIFF\0\0\0\0WEBPVP8X".to_vec();
        w.extend([10, 0, 0, 0, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert!(is_animated_webp(&w));
        assert!(matches!(make(&w, "image/webp"), Outcome::Original));
    }
}
