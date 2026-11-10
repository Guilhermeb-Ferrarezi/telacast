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
        if src.len() < ((ch - 1) * src_stride + cw) * 4
            || dst.len() < ((dh - 1) * dst_stride + dw) * 4
        {
            return;
        }
        // Mesmo tamanho: só copia as linhas (recorte).
        if cw == dw && ch == dh {
            for y in 0..dh {
                dst[y * dst_stride * 4..][..dw * 4]
                    .copy_from_slice(&src[y * src_stride * 4..][..cw * 4]);
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
