//! A small PNG writer that takes the picture in strips (bands of rows), so a
//! very big picture never has to be in memory at once.
//!
//! Rows use the "up" filter (difference to the row above: fast, and small
//! files for line drawings) and are compressed with `fdeflate`, the same fast
//! compressor the `png` crate uses for its fast level.

use crate::canvas::{unpremul, Canvas, Format};
use std::io::{self, Write};

/// PNG pixel format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PngFormat {
    /// 8-bit grey (the page must be grey: monochrome policy, grey background).
    Gray,
    /// 8-bit RGB (opaque page).
    Rgb,
    /// 8-bit RGBA, not premultiplied (transparent background).
    Rgba,
}

impl PngFormat {
    /// The smallest format that keeps the picture: grey for monochrome, RGB
    /// for an opaque background, else RGBA.
    pub fn for_options(o: &cadrender_core::RenderOptions) -> PngFormat {
        if o.background.a < 255 { PngFormat::Rgba }
        else if o.color == cadrender_core::ColorPolicy::MonochromeLight && o.background.is_gray() { PngFormat::Gray }
        else { PngFormat::Rgb }
    }
    fn channels(self) -> usize { match self { PngFormat::Gray => 1, PngFormat::Rgb => 3, PngFormat::Rgba => 4 } }
    fn color_type(self) -> u8 { match self { PngFormat::Gray => 0, PngFormat::Rgb => 2, PngFormat::Rgba => 6 } }
}

fn chunk<W: Write>(out: &mut W, kind: &[u8; 4], data: &[u8]) -> io::Result<()> {
    out.write_all(&(data.len() as u32).to_be_bytes())?;
    out.write_all(kind)?;
    out.write_all(data)?;
    let mut h = crc32fast::Hasher::new();
    h.update(kind);
    h.update(data);
    out.write_all(&h.finalize().to_be_bytes())
}

/// Collects the zlib stream and writes it out as IDAT chunks.
struct Idat<W: Write> { out: W, buf: Vec<u8> }

const IDAT_SIZE: usize = 1 << 18;

impl<W: Write> Write for Idat<W> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.extend_from_slice(data);
        if self.buf.len() >= IDAT_SIZE {
            chunk(&mut self.out, b"IDAT", &self.buf)?;
            self.buf.clear();
        }
        Ok(data.len())
    }
    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}

/// Writes one PNG, strip by strip, top to bottom.
pub struct PngStream<W: Write> {
    z: fdeflate::Compressor<Idat<W>>,
    fmt: PngFormat,
    w: usize,
    rows_left: u32,
    prev: Vec<u8>,
    cur: Vec<u8>,
    line: Vec<u8>,
}

impl<W: Write> PngStream<W> {
    pub fn new(mut out: W, w: u32, h: u32, fmt: PngFormat) -> io::Result<Self> {
        out.write_all(b"\x89PNG\r\n\x1a\n")?;
        let mut ihdr = [0u8; 13];
        ihdr[..4].copy_from_slice(&w.to_be_bytes());
        ihdr[4..8].copy_from_slice(&h.to_be_bytes());
        ihdr[8] = 8;
        ihdr[9] = fmt.color_type();
        chunk(&mut out, b"IHDR", &ihdr)?;
        let n = w as usize * fmt.channels();
        let z = fdeflate::Compressor::new(Idat { out, buf: Vec::with_capacity(IDAT_SIZE + 4096) })?;
        Ok(PngStream { z, fmt, w: w as usize, rows_left: h, prev: vec![0; n], cur: vec![0; n], line: vec![0; n + 1] })
    }

    /// Add all rows of `c` (its width must be the picture width).
    pub fn write_canvas(&mut self, c: &Canvas) -> io::Result<()> {
        if c.w as usize != self.w || c.h > self.rows_left { return Err(io::Error::new(io::ErrorKind::InvalidInput, "strip does not fit the picture")); }
        let bpp = c.fmt.bpp();
        for row in c.data.chunks_exact(self.w * bpp) {
            convert_row(row, c.fmt, &mut self.cur, self.fmt);
            self.line[0] = 2; // "up"
            for (o, (a, b)) in self.line[1..].iter_mut().zip(self.cur.iter().zip(&self.prev)) { *o = a.wrapping_sub(*b); }
            self.z.write_data(&self.line)?;
            std::mem::swap(&mut self.prev, &mut self.cur);
        }
        self.rows_left -= c.h;
        Ok(())
    }

    /// End the picture. All rows must have been written.
    pub fn finish(self) -> io::Result<W> {
        if self.rows_left != 0 { return Err(io::Error::new(io::ErrorKind::InvalidInput, "missing rows")); }
        let mut idat = self.z.finish()?;
        if !idat.buf.is_empty() { chunk(&mut idat.out, b"IDAT", &idat.buf)?; }
        chunk(&mut idat.out, b"IEND", &[])?;
        Ok(idat.out)
    }
}

/// One canvas row into PNG bytes (straight alpha).
fn convert_row(s: &[u8], sf: Format, d: &mut [u8], df: PngFormat) {
    match (sf, df) {
        (Format::Gray, PngFormat::Gray) => d.copy_from_slice(s),
        (Format::Gray, _) => { let c = df.channels(); for (o, &g) in d.chunks_exact_mut(c).zip(s) { o[..3].fill(g); if c == 4 { o[3] = 255; } } }
        (Format::GrayAlpha, _) => {
            let c = df.channels();
            for (o, p) in d.chunks_exact_mut(c).zip(s.chunks_exact(2)) {
                let g = unpremul(p[0], p[1]);
                o[..c.min(3)].fill(g);
                if c == 4 { o[3] = p[1]; }
            }
        }
        (Format::Rgba, PngFormat::Gray) => for (o, p) in d.iter_mut().zip(s.chunks_exact(4)) { *o = unpremul(p[1], p[3]); },
        (Format::Rgba, _) => {
            let c = df.channels();
            for (o, p) in d.chunks_exact_mut(c).zip(s.chunks_exact(4)) {
                for k in 0..3 { o[k] = unpremul(p[k], p[3]); }
                if c == 4 { o[3] = p[3]; }
            }
        }
    }
}

/// Encode a whole canvas as PNG into a byte vector.
pub fn png_bytes(c: &Canvas, format: PngFormat) -> io::Result<Vec<u8>> {
    let mut s = PngStream::new(Vec::with_capacity(c.w as usize * c.h as usize / 8), c.w, c.h, format)?;
    s.write_canvas(c)?;
    s.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadrender_core::color::Rgba;

    fn decode(b: &[u8]) -> (u32, u32, png::ColorType, Vec<u8>) {
        let mut r = png::Decoder::new(b).read_info().unwrap();
        let mut buf = vec![0; r.output_buffer_size()];
        let info = r.next_frame(&mut buf).unwrap();
        buf.truncate(info.buffer_size());
        (info.width, info.height, info.color_type, buf)
    }

    #[test]
    fn strips_make_one_valid_png() {
        let mut s = PngStream::new(Vec::new(), 3, 5, PngFormat::Gray).unwrap();
        for (k, rows) in [2u32, 3].iter().enumerate() {
            let mut c = Canvas::new(3, *rows, Rgba::gray(10 * k as u8 + 7), true);
            c.data[0] = 200;
            s.write_canvas(&c).unwrap();
        }
        let (w, h, ct, px) = decode(&s.finish().unwrap());
        assert_eq!((w, h, ct), (3, 5, png::ColorType::Grayscale));
        assert_eq!(px, vec![200, 7, 7, 7, 7, 7, 200, 17, 17, 17, 17, 17, 17, 17, 17]);
    }

    #[test]
    fn formats_round_trip() {
        let mut c = Canvas::new(2, 1, Rgba::WHITE, true);
        c.promote();
        c.data[..4].copy_from_slice(&[255, 0, 0, 255]);
        let (_, _, ct, px) = decode(&png_bytes(&c, PngFormat::Rgb).unwrap());
        assert_eq!((ct, px), (png::ColorType::Rgb, vec![255, 0, 0, 255, 255, 255]));
        let t = Canvas::new(2, 1, Rgba::TRANSPARENT, true);
        let (_, _, ct, px) = decode(&png_bytes(&t, PngFormat::Rgba).unwrap());
        assert_eq!((ct, px), (png::ColorType::Rgba, vec![0; 8]));
        let mut s = PngStream::new(Vec::new(), 2, 2, PngFormat::Gray).unwrap();
        s.write_canvas(&Canvas::new(2, 1, Rgba::WHITE, true)).unwrap();
        assert!(s.finish().is_err(), "missing rows are an error");
    }
}
