//! Pixel buffers and blending.
//!
//! A canvas is 8-bit grey (opaque page), grey + alpha (a viewport region over
//! a grey page) or RGBA (premultiplied). Grey is 4 times less memory to clear,
//! blend, paste and encode than RGBA, and the preview look is all grey. A canvas
//! starts grey when its background is grey and turns into RGBA by itself the
//! first time a colour that is not grey is drawn ("promotion").

use cadrender_core::color::Rgba;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// 1 byte per pixel, opaque.
    Gray,
    /// 2 bytes per pixel: premultiplied grey, alpha.
    GrayAlpha,
    /// 4 bytes per pixel, premultiplied.
    Rgba,
}

impl Format {
    pub fn bpp(self) -> usize { match self { Format::Gray => 1, Format::GrayAlpha => 2, Format::Rgba => 4 } }
}

/// `x / 255`, rounded, for `x <= 255 * 255`.
#[inline(always)]
pub fn div255(x: u32) -> u32 { (x + 128 + ((x + 128) >> 8)) >> 8 }

/// Rows per dirty band (see `Canvas::dirty`).
pub const BAND: u32 = 16;

pub struct Canvas {
    pub w: u32,
    pub h: u32,
    pub fmt: Format,
    pub data: Vec<u8>,
    /// For each band of `BAND` rows: the touched x range `[x0, x1)`
    /// (`x0 >= x1` = untouched). Pasting a region only reads touched parts.
    pub dirty: Vec<(u32, u32)>,
}

impl Canvas {
    /// A canvas filled with `bg`. Grey if `bg` is an opaque grey (or fully
    /// transparent and `gray` is asked), else RGBA.
    pub fn new(w: u32, h: u32, bg: Rgba, gray: bool) -> Canvas {
        let (w, h) = (w.max(1), h.max(1));
        let n = w as usize * h as usize;
        let fmt = if bg.a == 255 && bg.is_gray() { Format::Gray } else if bg.a == 0 && gray { Format::GrayAlpha } else { Format::Rgba };
        let data = match fmt {
            Format::Gray => vec![bg.r; n],
            _ if bg.a == 0 => vec![0; n * fmt.bpp()],
            _ => {
                let p = premul(bg);
                let mut v = Vec::with_capacity(n * 4);
                for _ in 0..n { v.extend_from_slice(&p); }
                v
            }
        };
        Canvas { w, h, fmt, data, dirty: vec![(u32::MAX, 0); h.div_ceil(BAND) as usize] }
    }

    #[inline]
    pub fn mark(&mut self, x0: u32, x1: u32, y: u32) {
        let d = &mut self.dirty[(y / BAND) as usize];
        if x0 < d.0 { d.0 = x0; }
        if x1 > d.1 { d.1 = x1; }
    }

    /// Make room for colour: grey formats become RGBA.
    pub fn promote(&mut self) {
        let n = self.w as usize * self.h as usize;
        let mut v = Vec::with_capacity(n * 4);
        match self.fmt {
            Format::Rgba => return,
            Format::Gray => for &g in &self.data { v.extend_from_slice(&[g, g, g, 255]); },
            Format::GrayAlpha => for p in self.data.chunks_exact(2) { v.extend_from_slice(&[p[0], p[0], p[0], p[1]]); },
        }
        self.data = v;
        self.fmt = Format::Rgba;
    }

    /// Pixel as straight (not premultiplied) RGBA.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.w || y >= self.h { return None; }
        let i = (y as usize * self.w as usize + x as usize) * self.fmt.bpp();
        let d = &self.data;
        Some(match self.fmt {
            Format::Gray => [d[i], d[i], d[i], 255],
            Format::GrayAlpha => { let g = unpremul(d[i], d[i + 1]); [g, g, g, d[i + 1]] }
            Format::Rgba => { let a = d[i + 3]; [unpremul(d[i], a), unpremul(d[i + 1], a), unpremul(d[i + 2], a), a] }
        })
    }
}

pub fn premul(c: Rgba) -> [u8; 4] {
    let a = c.a as u32;
    [div255(c.r as u32 * a) as u8, div255(c.g as u32 * a) as u8, div255(c.b as u32 * a) as u8, c.a]
}

#[inline]
pub fn unpremul(v: u8, a: u8) -> u8 { if a == 0 { 0 } else { ((v as u32 * 255 + a as u32 / 2) / a as u32).min(255) as u8 } }

/// The colour of one draw call, premultiplied by its alpha.
#[derive(Clone, Copy, Debug)]
pub struct Ink {
    /// Premultiplied r, g, b, a (for grey formats `p[1]` is the grey).
    pub p: [u8; 4],
}

impl Ink {
    pub fn new(c: Rgba) -> Ink { Ink { p: premul(c) } }
}

/// Blending of one pixel format. `s` is the premultiplied ink, `e` the
/// coverage (0..=255).
pub trait Px {
    const BPP: usize;
    fn blend(d: &mut [u8], s: &[u8; 4], e: u32);
    /// `e = 255` and an opaque ink: just store it.
    fn store(d: &mut [u8], s: &[u8; 4]);
}

pub struct GrayPx;
pub struct GrayAlphaPx;
pub struct RgbaPx;

impl Px for GrayPx {
    const BPP: usize = 1;
    #[inline(always)]
    fn blend(d: &mut [u8], s: &[u8; 4], e: u32) {
        let ia = 255 - div255(s[3] as u32 * e);
        d[0] = (div255(s[1] as u32 * e) + div255(d[0] as u32 * ia)) as u8;
    }
    #[inline(always)]
    fn store(d: &mut [u8], s: &[u8; 4]) { d[0] = s[1]; }
}

impl Px for GrayAlphaPx {
    const BPP: usize = 2;
    #[inline(always)]
    fn blend(d: &mut [u8], s: &[u8; 4], e: u32) {
        let ea = div255(s[3] as u32 * e);
        let ia = 255 - ea;
        d[0] = (div255(s[1] as u32 * e) + div255(d[0] as u32 * ia)) as u8;
        d[1] = (ea + div255(d[1] as u32 * ia)) as u8;
    }
    #[inline(always)]
    fn store(d: &mut [u8], s: &[u8; 4]) { d[0] = s[1]; d[1] = 255; }
}

impl Px for RgbaPx {
    const BPP: usize = 4;
    #[inline(always)]
    fn blend(d: &mut [u8], s: &[u8; 4], e: u32) {
        let ia = 255 - div255(s[3] as u32 * e);
        for c in 0..3 { d[c] = (div255(s[c] as u32 * e) + div255(d[c] as u32 * ia)) as u8; }
        d[3] = (div255(s[3] as u32 * e) + div255(d[3] as u32 * ia)) as u8;
    }
    #[inline(always)]
    fn store(d: &mut [u8], s: &[u8; 4]) { d[..4].copy_from_slice(s); }
}

/// Coverage sink used by the rasterisers: blends the ink into a canvas.
/// All coordinates are checked, so callers may pass pixels outside.
pub struct Blit<'a, P: Px> {
    data: &'a mut [u8],
    dirty: &'a mut [(u32, u32)],
    w: i32,
    h: i32,
    ink: [u8; 4],
    _p: std::marker::PhantomData<P>,
}

impl<'a, P: Px> Blit<'a, P> {
    pub fn new(c: &'a mut Canvas, ink: Ink) -> Self {
        debug_assert_eq!(c.fmt.bpp(), P::BPP);
        Blit { w: c.w as i32, h: c.h as i32, data: &mut c.data, dirty: &mut c.dirty, ink: ink.p, _p: std::marker::PhantomData }
    }
    #[inline(always)]
    pub fn width(&self) -> i32 { self.w }
    #[inline(always)]
    pub fn height(&self) -> i32 { self.h }

    #[inline(always)]
    fn mark(&mut self, x0: i32, x1: i32, y: i32) {
        let d = &mut self.dirty[(y as u32 / BAND) as usize];
        if (x0 as u32) < d.0 { d.0 = x0 as u32; }
        if (x1 as u32) > d.1 { d.1 = x1 as u32; }
    }

    /// One pixel with coverage `cov` (0..=255).
    #[inline(always)]
    pub fn px(&mut self, x: i32, y: i32, cov: u32) {
        if cov == 0 || x < 0 || y < 0 || x >= self.w || y >= self.h { return; }
        self.mark(x, x + 1, y);
        let i = (y as usize * self.w as usize + x as usize) * P::BPP;
        P::blend(&mut self.data[i..i + P::BPP], &self.ink, cov);
    }

    /// Pixels `y0..y1` of column `x`, all with coverage `cov`.
    #[inline]
    pub fn vspan(&mut self, x: i32, y0: i32, y1: i32, cov: u32) {
        if cov == 0 || x < 0 || x >= self.w { return; }
        let (y0, y1) = (y0.max(0), y1.min(self.h));
        if y0 >= y1 { return; }
        for b in (y0 as u32 / BAND)..=((y1 - 1) as u32 / BAND) { self.mark(x, x + 1, (b * BAND) as i32); }
        let stride = self.w as usize * P::BPP;
        let mut i = (y0 as usize * self.w as usize + x as usize) * P::BPP;
        let store = cov == 255 && self.ink[3] == 255;
        for _ in y0..y1 {
            let d = &mut self.data[i..i + P::BPP];
            if store { P::store(d, &self.ink) } else { P::blend(d, &self.ink, cov) }
            i += stride;
        }
    }

    /// Pixels `x0..x1` of row `y`, all with coverage `cov`.
    #[inline]
    pub fn span(&mut self, x0: i32, x1: i32, y: i32, cov: u32) {
        if cov == 0 || y < 0 || y >= self.h { return; }
        let (x0, x1) = (x0.max(0), x1.min(self.w));
        if x0 >= x1 { return; }
        self.mark(x0, x1, y);
        let row = y as usize * self.w as usize;
        let d = &mut self.data[(row + x0 as usize) * P::BPP..(row + x1 as usize) * P::BPP];
        if cov == 255 && self.ink[3] == 255 {
            if P::BPP == 1 { d.fill(self.ink[1]); } else { for p in d.chunks_exact_mut(P::BPP) { P::store(p, &self.ink); } }
        } else {
            for p in d.chunks_exact_mut(P::BPP) { P::blend(p, &self.ink, cov); }
        }
    }
}

/// Paste `src` (a region with alpha) onto `dst` at `(x, y)`, source-over.
/// `mask` (one byte per `src` pixel) keeps only the covered part. Only the
/// touched bands of `src` are read.
pub fn paste(dst: &mut Canvas, src: &Canvas, x: i32, y: i32, mask: Option<&[u8]>) {
    if src.fmt == Format::Rgba && dst.fmt != Format::Rgba { dst.promote(); }
    let (dw, dh) = (dst.w as i64, dst.h as i64);
    let (sw, sh) = (src.w as i64, src.h as i64);
    let (x, y) = (x as i64, y as i64);
    let (cx0, cy0) = ((-x).max(0), (-y).max(0));
    let (cx1, cy1) = (sw.min(dw - x), sh.min(dh - y));
    if cx1 <= cx0 || cy1 <= cy0 { return; }
    let (sb, db) = (src.fmt.bpp(), dst.fmt.bpp());
    for (band, &(bx0, bx1)) in src.dirty.iter().enumerate() {
        let (bx0, bx1) = ((bx0 as i64).max(cx0), (bx1 as i64).min(cx1));
        if bx0 >= bx1 { continue; }
        let r0 = (band as i64 * BAND as i64).max(cy0);
        let r1 = ((band as i64 + 1) * BAND as i64).min(cy1);
        for row in r0..r1 {
            let s = &src.data[((row * sw + bx0) as usize) * sb..((row * sw + bx1) as usize) * sb];
            let o = ((y + row) * dw + x + bx0) as usize * db;
            let d = &mut dst.data[o..o + (bx1 - bx0) as usize * db];
            let m = mask.map(|m| &m[(row * sw + bx0) as usize..(row * sw + bx1) as usize]);
            paste_row(d, dst.fmt, s, src.fmt, m);
        }
        let (px0, px1) = ((x + bx0) as u32, (x + bx1) as u32);
        let (pr0, pr1) = ((y + r0) as u32, (y + r1 - 1) as u32);
        for b in pr0 / BAND..=pr1 / BAND { dst.mark(px0, px1, b * BAND); }
    }
}

#[inline]
fn paste_row(d: &mut [u8], df: Format, s: &[u8], sf: Format, m: Option<&[u8]>) {
    match (sf, df) {
        (Format::GrayAlpha, Format::Gray) => {
            // most of a region is empty: test 8 pixels (16 bytes) at a time
            let n = d.len();
            let mut k = 0;
            while k < n {
                if k + 8 <= n && u128::from_ne_bytes(s[2 * k..2 * k + 16].try_into().unwrap()) == 0 { k += 8; continue; }
                let end = (k + 8).min(n);
                for j in k..end {
                    let mut a = s[2 * j + 1] as u32;
                    if a == 0 { continue; }
                    let mut g = s[2 * j] as u32;
                    if let Some(m) = m { let c = m[j] as u32; if c < 255 { a = div255(a * c); g = div255(g * c); } }
                    d[j] = (g + div255(d[j] as u32 * (255 - a))) as u8;
                }
                k = end;
            }
        }
        (Format::GrayAlpha, Format::GrayAlpha) => {
            for (k, (sp, dp)) in s.chunks_exact(2).zip(d.chunks_exact_mut(2)).enumerate() {
                let (mut g, mut a) = (sp[0] as u32, sp[1] as u32);
                if a == 0 { continue; }
                if let Some(m) = m { let c = m[k] as u32; if c < 255 { a = div255(a * c); g = div255(g * c); } }
                let ia = 255 - a;
                dp[0] = (g + div255(dp[0] as u32 * ia)) as u8;
                dp[1] = (a + div255(dp[1] as u32 * ia)) as u8;
            }
        }
        (sf, Format::Rgba) => {
            let n = d.len() / 4;
            for k in 0..n {
                let sp: [u32; 4] = if sf == Format::Rgba { [s[4 * k] as u32, s[4 * k + 1] as u32, s[4 * k + 2] as u32, s[4 * k + 3] as u32] } else { let g = s[2 * k] as u32; [g, g, g, s[2 * k + 1] as u32] };
                let mut sp = sp;
                if sp[3] == 0 { continue; }
                if let Some(m) = m { let c = m[k] as u32; if c < 255 { for v in &mut sp { *v = div255(*v * c); } } }
                let ia = 255 - sp[3];
                let dp = &mut d[4 * k..4 * k + 4];
                for c in 0..4 { dp[c] = (sp[c] + div255(dp[c] as u32 * ia)) as u8; }
            }
        }
        _ => unreachable!("regions have alpha; the page is promoted before an RGBA paste"),
    }
}
