//! Fontes de captura X11 (monitores e janelas), miniaturas e escalonador.
//!
//! O diálogo "Transmitir" usa `list_monitors`, `list_windows` e `thumbnail`;
//! o `publisher` usa `Grabber` (um quadro BGRA por chamada) e `Scaler`.
//! Tudo aqui é tolerante a falhas: janela que some, X sem RandR ou sem WM
//! viram `Err`/`None`/lista vazia, nunca pânico.
use anyhow::{anyhow, bail, Context};
use x11rb::{
    connection::Connection,
    protocol::{
        randr::ConnectionExt as _,
        xproto::{Atom, AtomEnum, ConnectionExt as _, ImageFormat, MapState, Window},
    },
    rust_connection::RustConnection,
};

/// De onde vem a imagem transmitida.
#[derive(Clone, PartialEq, Debug)]
pub enum Source {
    /// Índice na lista de `list_monitors` (ordem do RandR).
    Monitor(usize),
    /// XID da janela de aplicativo (a janela cliente, não o quadro do WM).
    Window(u32),
}

#[derive(Clone, PartialEq, Debug)]
pub struct SourceInfo {
    pub source: Source,
    /// Nome do monitor (ex.: "DP-1") ou título da janela.
    pub title: String,
    pub width: u32,
    pub height: u32,
}

/// Miniatura RGBA (alfa sempre 255), pronta para `ColorImage::from_rgba_unmultiplied`.
pub struct Thumb {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Janelas menores que isto (em qualquer lado) não entram na lista.
const MIN_WINDOW: u32 = 64;
/// Título da própria janela do Telacast (não faz sentido transmiti-la).
const OWN_TITLE: &str = "Telacast";

// ---------------------------------------------------------------- conexão

fn connect() -> anyhow::Result<(RustConnection, Window, u16, u16)> {
    let (conn, num) = x11rb::connect(None).context("não consegui conectar ao X")?;
    let s = &conn.setup().roots[num];
    let (root, w, h) = (s.root, s.width_in_pixels, s.height_in_pixels);
    Ok((conn, root, w, h))
}

struct MonGeom {
    name: String,
    x: i16,
    y: i16,
    w: u16,
    h: u16,
}

/// Monitores do RandR; sem RandR (ou sem monitores) vira uma tela só, do tamanho da raiz.
fn monitors(conn: &RustConnection, root: Window, sw: u16, sh: u16) -> Vec<MonGeom> {
    let mut out = Vec::new();
    if let Some(reply) = conn.randr_get_monitors(root, true).ok().and_then(|c| c.reply().ok()) {
        for m in reply.monitors {
            if m.width == 0 || m.height == 0 {
                continue;
            }
            let name = conn
                .get_atom_name(m.name)
                .ok()
                .and_then(|c| c.reply().ok())
                .map(|r| String::from_utf8_lossy(&r.name).into_owned())
                .unwrap_or_default();
            out.push(MonGeom { name, x: m.x, y: m.y, w: m.width, h: m.height });
        }
    }
    if out.is_empty() {
        out.push(MonGeom { name: String::new(), x: 0, y: 0, w: sw, h: sh });
    }
    out
}

// ---------------------------------------------------------------- listas

pub fn list_monitors() -> Vec<SourceInfo> {
    let Ok((conn, root, sw, sh)) = connect() else { return Vec::new() };
    monitors(&conn, root, sw, sh)
        .into_iter()
        .enumerate()
        .map(|(i, m)| SourceInfo {
            source: Source::Monitor(i),
            title: if m.name.is_empty() { format!("Tela {}", i + 1) } else { m.name },
            width: m.w as u32,
            height: m.h as u32,
        })
        .collect()
}

struct Atoms {
    client_list: Atom,
    wm_name: Atom,
    utf8: Atom,
    state: Atom,
    hidden: Atom,
    wtype: Atom,
    dock: Atom,
    desktop: Atom,
}

impl Atoms {
    fn new(conn: &RustConnection) -> anyhow::Result<Self> {
        let names: [&[u8]; 8] = [
            b"_NET_CLIENT_LIST",
            b"_NET_WM_NAME",
            b"UTF8_STRING",
            b"_NET_WM_STATE",
            b"_NET_WM_STATE_HIDDEN",
            b"_NET_WM_WINDOW_TYPE",
            b"_NET_WM_WINDOW_TYPE_DOCK",
            b"_NET_WM_WINDOW_TYPE_DESKTOP",
        ];
        let cookies: Vec<_> = names
            .iter()
            .map(|n| conn.intern_atom(false, n))
            .collect::<Result<_, _>>()?;
        let mut a = [0; 8];
        for (i, c) in cookies.into_iter().enumerate() {
            a[i] = c.reply()?.atom;
        }
        Ok(Atoms {
            client_list: a[0],
            wm_name: a[1],
            utf8: a[2],
            state: a[3],
            hidden: a[4],
            wtype: a[5],
            dock: a[6],
            desktop: a[7],
        })
    }
}

/// Lista de átomos (formato 32) de uma propriedade; vazio se não existe.
fn atom_list(conn: &RustConnection, win: Window, prop: Atom) -> Vec<u32> {
    conn.get_property(false, win, prop, AtomEnum::ANY, 0, 256)
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|r| r.value32().map(|v| v.collect()))
        .unwrap_or_default()
}

fn window_title(conn: &RustConnection, win: Window, at: &Atoms) -> String {
    let get = |prop: Atom, ty: Atom| -> Option<String> {
        let r = conn.get_property(false, win, prop, ty, 0, 256).ok()?.reply().ok()?;
        let s = String::from_utf8_lossy(&r.value).trim().to_string();
        (!s.is_empty()).then_some(s)
    };
    get(at.wm_name, at.utf8)
        .or_else(|| get(AtomEnum::WM_NAME.into(), AtomEnum::ANY.into()))
        .unwrap_or_default()
}

/// Janelas de aplicativos visíveis. Usa `_NET_CLIENT_LIST` (EWMH); sem gerenciador
/// de janelas cai para as janelas de nível superior da raiz.
pub fn list_windows() -> Vec<SourceInfo> {
    list_windows_impl().unwrap_or_default()
}

fn list_windows_impl() -> anyhow::Result<Vec<SourceInfo>> {
    let (conn, root, _, _) = connect()?;
    let at = Atoms::new(&conn)?;
    let mut ids = atom_list(&conn, root, at.client_list);
    if ids.is_empty() {
        ids = conn.query_tree(root)?.reply()?.children;
    }
    // Do topo da pilha para baixo (o EWMH lista do mais antigo ao mais novo).
    let mut out = Vec::new();
    for id in ids {
        let Some(attrs) = conn.get_window_attributes(id).ok().and_then(|c| c.reply().ok()) else {
            continue;
        };
        if attrs.map_state != MapState::VIEWABLE || attrs.override_redirect {
            continue;
        }
        if atom_list(&conn, id, at.state).contains(&at.hidden) {
            continue; // minimizada
        }
        let wtype = atom_list(&conn, id, at.wtype);
        if wtype.contains(&at.dock) || wtype.contains(&at.desktop) {
            continue;
        }
        let Some(g) = conn.get_geometry(id).ok().and_then(|c| c.reply().ok()) else { continue };
        let (w, h) = (g.width as u32, g.height as u32);
        if w < MIN_WINDOW || h < MIN_WINDOW {
            continue;
        }
        let title = window_title(&conn, id, &at);
        if title.is_empty() || title == OWN_TITLE {
            continue;
        }
        out.push(SourceInfo { source: Source::Window(id), title, width: w, height: h });
    }
    out.reverse();
    Ok(out)
}

// ---------------------------------------------------------------- captura

/// Quadro cru do X: BGRX, 4 bytes por pixel, sem preenchimento de linha.
pub struct Raw {
    pub w: u32,
    pub h: u32,
    pub data: Vec<u8>,
}

enum Kind {
    Monitor { x: i16, y: i16, w: u16, h: u16 },
    Window(Window),
}

/// Conexão X ligada a uma fonte; `grab` devolve um quadro por chamada.
pub struct Grabber {
    conn: RustConnection,
    root: Window,
    sw: u16,
    sh: u16,
    kind: Kind,
}

impl Grabber {
    pub fn new(source: &Source) -> anyhow::Result<Self> {
        let (conn, root, sw, sh) = connect()?;
        let kind = match *source {
            Source::Monitor(i) => {
                let m = monitors(&conn, root, sw, sh)
                    .into_iter()
                    .nth(i)
                    .ok_or_else(|| anyhow!("tela não encontrada"))?;
                Kind::Monitor { x: m.x, y: m.y, w: m.w, h: m.h }
            }
            Source::Window(id) => Kind::Window(id),
        };
        Ok(Grabber { conn, root, sw, sh, kind })
    }

    pub fn is_window(&self) -> bool {
        matches!(self.kind, Kind::Window(_))
    }

    /// Tamanho atual da fonte.
    pub fn size(&self) -> anyhow::Result<(u32, u32)> {
        match self.kind {
            Kind::Monitor { w, h, .. } => Ok((w as u32, h as u32)),
            Kind::Window(id) => {
                let g = self
                    .conn
                    .get_geometry(id)?
                    .reply()
                    .map_err(|_| anyhow!("a janela foi fechada"))?;
                Ok((g.width as u32, g.height as u32))
            }
        }
    }

    pub fn grab(&self) -> anyhow::Result<Raw> {
        match self.kind {
            Kind::Monitor { x, y, w, h } => self.image(self.root, x, y, w, h),
            Kind::Window(id) => self.grab_window(id),
        }
    }

    fn image(&self, d: u32, x: i16, y: i16, w: u16, h: u16) -> anyhow::Result<Raw> {
        let img = self
            .conn
            .get_image(ImageFormat::Z_PIXMAP, d, x, y, w, h, !0)?
            .reply()?;
        let need = w as usize * h as usize * 4;
        if img.data.len() < need {
            bail!("formato de tela não suportado (precisa de 32 bpp)");
        }
        let mut data = img.data;
        data.truncate(need);
        Ok(Raw { w: w as u32, h: h as u32, data })
    }

    fn grab_window(&self, id: Window) -> anyhow::Result<Raw> {
        let g = self
            .conn
            .get_geometry(id)?
            .reply()
            .map_err(|_| anyhow!("a janela foi fechada"))?;
        let attrs = self
            .conn
            .get_window_attributes(id)?
            .reply()
            .map_err(|_| anyhow!("a janela foi fechada"))?;
        if attrs.map_state != MapState::VIEWABLE {
            bail!("a janela está minimizada ou oculta");
        }
        if g.width == 0 || g.height == 0 {
            bail!("a janela está sem tamanho");
        }
        match self.image(id, 0, 0, g.width, g.height) {
            Ok(r) => Ok(r),
            Err(first) => {
                // O X recusa (BadMatch) janela que sai da tela: captura da raiz só a parte visível.
                let t = self.conn.translate_coordinates(id, self.root, 0, 0)?.reply()?;
                let (rx, ry) = (t.dst_x as i32, t.dst_y as i32);
                let x0 = rx.max(0);
                let y0 = ry.max(0);
                let x1 = (rx + g.width as i32).min(self.sw as i32);
                let y1 = (ry + g.height as i32).min(self.sh as i32);
                if x1 <= x0 || y1 <= y0 {
                    bail!("a janela está fora da tela");
                }
                self.image(self.root, x0 as i16, y0 as i16, (x1 - x0) as u16, (y1 - y0) as u16)
                    .map_err(|e| anyhow!("não consegui capturar a janela ({first:#}; {e:#})"))
            }
        }
    }
}

// ---------------------------------------------------------------- miniatura

/// Miniatura de uma fonte com no máximo `max_w` de largura (proporção mantida,
/// nunca amplia). `None` se a fonte sumiu ou não deu para capturar.
pub fn thumbnail(source: &Source, max_w: u32) -> Option<Thumb> {
    let raw = Grabber::new(source).ok()?.grab().ok()?;
    if raw.w == 0 || raw.h == 0 {
        return None;
    }
    let dw = raw.w.min(max_w.max(1));
    let dh = ((raw.h as u64 * dw as u64 + raw.w as u64 / 2) / raw.w as u64).max(1) as u32;
    let mut px = vec![0u8; dw as usize * dh as usize * 4];
    Scaler::default().scale(
        &raw.data,
        raw.w as usize,
        raw.w as usize,
        raw.h as usize,
        &mut px,
        dw as usize,
        dw as usize,
        dh as usize,
    );
    // BGRX -> RGBA opaco.
    for p in px.chunks_exact_mut(4) {
        p.swap(0, 2);
        p[3] = 255;
    }
    Some(Thumb { width: dw, height: dh, rgba: px })
}

// ---------------------------------------------------------------- escala

/// Reduz imagens de 4 bytes por pixel por média de caixa (área) com inteiros.
/// Reaproveita os buffers internos entre quadros.
#[derive(Default)]
pub struct Scaler {
    acc: Vec<u32>,
    xs: Vec<usize>,
    xs_key: (usize, usize),
}

impl Scaler {
    /// Escala a região `cw`x`ch` (pixels) de `src` (linhas de `src_stride` pixels)
    /// para `dw`x`dh` em `dst` (linhas de `dst_stride` pixels). Cada pixel de destino
    /// é a média exata dos pixels de origem que ele cobre; se `dw > cw` repete pixels
    /// (vizinho mais próximo), pois aqui só se reduz. Argumentos inconsistentes
    /// (buffers curtos, zero) fazem a função não escrever nada.
    #[allow(clippy::too_many_arguments)]
    pub fn scale(
        &mut self,
        src: &[u8],
        src_stride: usize,
        cw: usize,
        ch: usize,
        dst: &mut [u8],
        dst_stride: usize,
        dw: usize,
        dh: usize,
    ) {
        if cw == 0 || ch == 0 || dw == 0 || dh == 0 || src_stride < cw || dst_stride < dw {
            return;
        }
        if src.len() < ((ch - 1) * src_stride + cw) * 4 || dst.len() < ((dh - 1) * dst_stride + dw) * 4 {
            return;
        }
        // Mesmo tamanho: só copia as linhas (recorte).
        if cw == dw && ch == dh {
            for y in 0..dh {
                dst[y * dst_stride * 4..][..dw * 4].copy_from_slice(&src[y * src_stride * 4..][..cw * 4]);
            }
            return;
        }
        if self.xs_key != (cw, dw) || self.xs.len() != dw + 1 {
            self.xs.clear();
            self.xs.extend((0..=dw).map(|i| i * cw / dw));
            self.xs_key = (cw, dw);
        }
        self.acc.clear();
        self.acc.resize(dw * 4, 0);
        for dy in 0..dh {
            let y0 = dy * ch / dh;
            let y1 = ((dy + 1) * ch / dh).max(y0 + 1).min(ch);
            self.acc.fill(0);
            for sy in y0..y1 {
                let row = &src[sy * src_stride * 4..][..cw * 4];
                for dx in 0..dw {
                    // Em ampliação o intervalo pode ser vazio: usa ao menos 1 pixel.
                    let a = self.xs[dx].min(cw - 1);
                    let b = self.xs[dx + 1].max(a + 1).min(cw);
                    let acc = &mut self.acc[dx * 4..dx * 4 + 4];
                    for p in row[a * 4..b * 4].chunks_exact(4) {
                        acc[0] += p[0] as u32;
                        acc[1] += p[1] as u32;
                        acc[2] += p[2] as u32;
                        acc[3] += p[3] as u32;
                    }
                }
            }
            let out = &mut dst[dy * dst_stride * 4..][..dw * 4];
            for dx in 0..dw {
                let a = self.xs[dx].min(cw - 1);
                let b = self.xs[dx + 1].max(a + 1).min(cw);
                let n = ((y1 - y0) * (b - a)) as u32;
                for c in 0..4 {
                    out[dx * 4 + c] = ((self.acc[dx * 4 + c] + n / 2) / n) as u8;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: usize, h: usize, bgra: [u8; 4]) -> Vec<u8> {
        bgra.iter().cycle().take(w * h * 4).copied().collect()
    }

    #[test]
    fn media_de_caixa_exata() {
        // 4x2 -> 2x1: cada destino é a média de um bloco 2x2.
        let mut src = vec![0u8; 4 * 2 * 4];
        let set = |s: &mut Vec<u8>, x: usize, y: usize, v: u8| {
            for c in 0..4 {
                s[(y * 4 + x) * 4 + c] = v;
            }
        };
        set(&mut src, 0, 0, 0);
        set(&mut src, 1, 0, 100);
        set(&mut src, 0, 1, 200);
        set(&mut src, 1, 1, 100);
        set(&mut src, 2, 0, 255);
        set(&mut src, 3, 0, 255);
        set(&mut src, 2, 1, 255);
        set(&mut src, 3, 1, 255);
        let mut dst = vec![0u8; 2 * 4];
        Scaler::default().scale(&src, 4, 4, 2, &mut dst, 2, 2, 1);
        assert_eq!(&dst[0..4], &[100, 100, 100, 100]);
        assert_eq!(&dst[4..8], &[255, 255, 255, 255]);
    }

    #[test]
    fn cor_solida_continua_solida_em_razoes_quebradas() {
        let src = solid(1920, 1080, [10, 20, 30, 255]);
        let (dw, dh) = (853, 480); // razão não inteira
        let mut dst = vec![0u8; dw * dh * 4];
        Scaler::default().scale(&src, 1920, 1920, 1080, &mut dst, dw, dw, dh);
        assert!(dst.chunks_exact(4).all(|p| p == [10, 20, 30, 255]));
    }

    #[test]
    fn recorte_e_stride() {
        // Origem 3 px de largura, recorta 2 (largura par) sem ampliar.
        let mut src = vec![0u8; 3 * 2 * 4];
        for (i, p) in src.chunks_exact_mut(4).enumerate() {
            p.copy_from_slice(&[i as u8; 4]);
        }
        let mut dst = vec![0u8; 2 * 2 * 4];
        Scaler::default().scale(&src, 3, 2, 2, &mut dst, 2, 2, 2);
        let got: Vec<u8> = dst.chunks_exact(4).map(|p| p[0]).collect();
        assert_eq!(got, vec![0, 1, 3, 4]);
    }

    #[test]
    fn destino_com_deslocamento_e_stride() {
        // Letterbox: escreve 2x2 dentro de uma tela 4x4 sem tocar o resto.
        let src = solid(4, 4, [200, 200, 200, 255]);
        let mut canvas = vec![0u8; 4 * 4 * 4];
        let off = (1 * 4 + 1) * 4;
        Scaler::default().scale(&src, 4, 4, 4, &mut canvas[off..], 4, 2, 2);
        assert_eq!(canvas[0], 0);
        assert_eq!(canvas[off], 200);
        assert_eq!(canvas[(2 * 4 + 2) * 4], 200);
        assert_eq!(canvas[(3 * 4 + 3) * 4], 0);
    }

    #[test]
    fn argumentos_ruins_nao_entram_em_panico() {
        let mut dst = vec![7u8; 16];
        let mut s = Scaler::default();
        s.scale(&[], 0, 0, 0, &mut dst, 2, 2, 2);
        s.scale(&[1, 2, 3], 1, 1, 1, &mut dst, 2, 2, 2); // origem curta
        s.scale(&solid(2, 2, [1, 1, 1, 1]), 2, 2, 2, &mut dst[..4], 2, 2, 2); // destino curto
        assert!(dst.iter().all(|&b| b == 7));
    }

    #[test]
    fn velocidade_1080p() {
        let src = solid(1920, 1080, [1, 2, 3, 4]);
        let mut dst = vec![0u8; 320 * 180 * 4];
        let mut s = Scaler::default();
        let t = std::time::Instant::now();
        for _ in 0..5 {
            s.scale(&src, 1920, 1920, 1080, &mut dst, 320, 320, 180);
        }
        eprintln!("1080p -> 320x180: {:?} por quadro", t.elapsed() / 5);
    }

    /// Precisa de um X de teste: `Xvfb :98 -screen 0 1600x900x24 -nolisten tcp &`
    /// e `DISPLAY=:98 cargo test -p telacast -- --ignored x11_`.
    #[test]
    #[ignore]
    fn x11_listas_e_miniaturas() {
        let mons = list_monitors();
        eprintln!("monitores: {mons:?}");
        assert!(!mons.is_empty());
        let wins = list_windows();
        eprintln!("janelas: {wins:?}");
        let dir = std::env::var("THUMB_DIR").unwrap_or_else(|_| "/tmp".into());
        let t = std::time::Instant::now();
        let th = thumbnail(&mons[0].source, 320).expect("miniatura do monitor");
        eprintln!("miniatura do monitor: {}x{} em {:?}", th.width, th.height, t.elapsed());
        save_png(&format!("{dir}/thumb-monitor.png"), &th);
        for (i, w) in wins.iter().enumerate() {
            let th = thumbnail(&w.source, 320).expect("miniatura da janela");
            save_png(&format!("{dir}/thumb-janela{i}.png"), &th);
        }
        // Janela inexistente: sem pânico.
        assert!(thumbnail(&Source::Window(0x7fff_fff0), 320).is_none());
        assert!(thumbnail(&Source::Monitor(99), 320).is_none());
    }

    fn save_png(path: &str, t: &Thumb) {
        image::save_buffer(path, &t.rgba, t.width, t.height, image::ColorType::Rgba8).unwrap();
        eprintln!("gravado {path}");
    }
}
