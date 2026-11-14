//! cadrender-raster: a [`Sink`] that draws pixels, and a PNG encoder.
//!
//! The rasteriser is our own, made for CAD pictures (see `docs/raster.md`):
//! thin lines go through a direct line drawer (`hairline`), fills and thick
//! lines through an exact-area polygon filler (`fill`). tiny-skia is only used
//! to turn a thick line into its outline. Pictures are 8-bit grey when the
//! page is grey, RGBA when a colour shows up.

pub mod canvas;
pub mod fill;
pub mod hairline;
pub mod image;
pub mod path;
pub mod pngout;

use cadrender_core::color::Rgba;
use cadrender_core::sink::{FillRule, IRect, Paint, PaintKind, Sink};
use canvas::{Blit, Canvas, Format, GrayAlphaPx, GrayPx, Ink, Px, RgbaPx};
use fill::Filler;
use hairline::Hair;
use path::{Path, Poly};
pub use pngout::{png_bytes, PngFormat, PngStream};

/// Draws into a [`Canvas`].
pub struct RasterSink {
    pub canvas: Canvas,
    path: Path,
    poly: Poly,
    filler: Filler,
    /// Scratch path for outlines of thick lines.
    outline: Path,
}

impl RasterSink {
    pub fn new(w: u32, h: u32, background: Rgba) -> RasterSink {
        RasterSink { canvas: Canvas::new(w, h, background, true), path: Path::default(), poly: Poly::default(), filler: Filler::default(), outline: Path::default() }
    }
    pub fn width(&self) -> u32 { self.canvas.w }
    pub fn height(&self) -> u32 { self.canvas.h }
    pub fn format(&self) -> Format { self.canvas.fmt }
    /// Pixel as straight RGBA.
    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> { self.canvas.pixel(x, y) }
    /// Encode as PNG.
    pub fn png(&self, format: PngFormat) -> std::io::Result<Vec<u8>> { png_bytes(&self.canvas, format) }
}

impl Sink for RasterSink {
    type Config = ();

    fn page(_: &(), w: u32, h: u32, background: Rgba) -> Self { RasterSink::new(w, h, background) }

    fn region(&self, rect: IRect) -> Self {
        let mut s = RasterSink::new(rect.w, rect.h, Rgba::TRANSPARENT);
        if self.canvas.fmt == Format::Rgba { s.canvas.promote(); }
        s
    }

    fn composite(&mut self, region: Self, rect: IRect, clip: Option<&[[f32; 2]]>) {
        let mask = clip.map(|poly| clip_mask(poly, region.canvas.w, region.canvas.h));
        canvas::paste(&mut self.canvas, &region.canvas, rect.x, rect.y, mask.as_deref());
    }

    #[inline] fn move_to(&mut self, x: f32, y: f32) { self.path.verbs.push(path::M); self.path.pts.push([x, y]); }
    #[inline] fn line_to(&mut self, x: f32, y: f32) { self.path.verbs.push(path::L); self.path.pts.push([x, y]); }
    #[inline] fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) { self.path.verbs.push(path::Q); self.path.pts.extend_from_slice(&[[x1, y1], [x, y]]); }
    #[inline] fn cubic_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) { self.path.verbs.push(path::C); self.path.pts.extend_from_slice(&[[x1, y1], [x2, y2], [x, y]]); }
    #[inline] fn close(&mut self) { self.path.verbs.push(path::Z); }

    fn draw(&mut self, paint: &Paint<'_>) {
        if !self.path.is_empty() && paint.color.a > 0 {
            if !paint.color.is_gray() && self.canvas.fmt != Format::Rgba { self.canvas.promote(); }
            match self.canvas.fmt {
                Format::Gray => self.draw_with::<GrayPx>(paint),
                Format::GrayAlpha => self.draw_with::<GrayAlphaPx>(paint),
                Format::Rgba => self.draw_with::<RgbaPx>(paint),
            }
        }
        self.path.clear();
    }

    fn discard(&mut self) { self.path.clear(); }

    fn image(&mut self, img: &cadrender_core::sink::ImageDraw<'_>) -> bool { image::draw(&mut self.canvas, &mut self.filler, img) }
}

impl RasterSink {
    fn draw_with<P: Px>(&mut self, paint: &Paint<'_>) {
        let RasterSink { canvas, path, poly, filler, outline } = self;
        let mut b = Blit::<P>::new(canvas, Ink::new(paint.color));
        match paint.kind {
            PaintKind::Stroke { width, dash } if width <= hairline::HAIR_MAX => {
                let h = Hair::new(width);
                path.for_each_poly(poly, |p| {
                    hairline::snap_poly(p, &h);
                    hairline::stroke_poly(&mut b, p, &h, dash);
                });
            }
            PaintKind::Stroke { width, dash } => {
                if !thick_outline(path, width, dash, outline) { return; }
                filler.begin(b.width(), b.height());
                outline.for_each_poly(poly, |p| filler.add_poly(p));
                filler.fill(&mut b, FillRule::NonZero);
            }
            PaintKind::Fill { rule } => {
                filler.begin(b.width(), b.height());
                path.for_each_poly(poly, |p| filler.add_poly(p));
                filler.fill(&mut b, rule);
            }
        }
    }
}

/// Outline of a thick line (tiny-skia's stroker), into `out`. False if empty.
fn thick_outline(p: &Path, width: f32, dash: Option<&[f32]>, out: &mut Path) -> bool {
    let mut pb = tiny_skia::PathBuilder::new();
    let mut i = 0;
    for &v in &p.verbs {
        match v {
            path::M => { pb.move_to(p.pts[i][0], p.pts[i][1]); i += 1; }
            path::L => { pb.line_to(p.pts[i][0], p.pts[i][1]); i += 1; }
            path::Q => { pb.quad_to(p.pts[i][0], p.pts[i][1], p.pts[i + 1][0], p.pts[i + 1][1]); i += 2; }
            path::C => { pb.cubic_to(p.pts[i][0], p.pts[i][1], p.pts[i + 1][0], p.pts[i + 1][1], p.pts[i + 2][0], p.pts[i + 2][1]); i += 3; }
            _ => pb.close(),
        }
    }
    let Some(sp) = pb.finish() else { return false };
    let sp = match dash.and_then(|d| tiny_skia::StrokeDash::new(d.to_vec(), 0.0)) {
        Some(d) => match sp.dash(&d, 1.0) { Some(x) => x, None => return false },
        None => sp,
    };
    let stroke = tiny_skia::Stroke { width, line_cap: tiny_skia::LineCap::Butt, line_join: tiny_skia::LineJoin::Round, ..Default::default() };
    let Some(o) = sp.stroke(&stroke, 1.0) else { return false };
    out.clear();
    for seg in o.segments() {
        use tiny_skia::PathSegment as S;
        match seg {
            S::MoveTo(a) => { out.verbs.push(path::M); out.pts.push([a.x, a.y]); }
            S::LineTo(a) => { out.verbs.push(path::L); out.pts.push([a.x, a.y]); }
            S::QuadTo(a, b) => { out.verbs.push(path::Q); out.pts.extend_from_slice(&[[a.x, a.y], [b.x, b.y]]); }
            S::CubicTo(a, b, c) => { out.verbs.push(path::C); out.pts.extend_from_slice(&[[a.x, a.y], [b.x, b.y], [c.x, c.y]]); }
            S::Close => out.verbs.push(path::Z),
        }
    }
    true
}

/// Coverage mask (one byte per pixel) of a clip polygon.
fn clip_mask(poly: &[[f32; 2]], w: u32, h: u32) -> Vec<u8> {
    let mut m = vec![0u8; w as usize * h as usize];
    let mut f = Filler::default();
    f.begin(w as i32, h as i32);
    let n = poly.len();
    for i in 0..n { let (a, b) = (poly[i], poly[(i + 1) % n]); f.edge(a[0], a[1], b[0], b[1]); }
    f.sweep(FillRule::NonZero, |y, x0, x1, c| {
        let (x0, x1) = (x0.max(0) as usize, (x1.min(w as i32)).max(0) as usize);
        if x0 < x1 { let row = y as usize * w as usize; m[row + x0..row + x1].fill(c as u8); }
    });
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(s: &mut RasterSink, a: (f32, f32), b: (f32, f32), w: f32) {
        s.move_to(a.0, a.1);
        s.line_to(b.0, b.1);
        s.draw(&Paint { color: Rgba::BLACK, kind: PaintKind::Stroke { width: w, dash: None } });
    }
    fn grey(s: &RasterSink, x: u32, y: u32) -> u8 { s.pixel(x, y).unwrap()[0] }

    #[test]
    fn stroke_and_composite() {
        let mut page = RasterSink::page(&(), 20, 20, Rgba::WHITE);
        let mut reg = page.region(IRect { x: 10, y: 10, w: 10, h: 10 });
        line(&mut reg, (0.0, 5.0), (10.0, 5.0), 2.0);
        page.composite(reg, IRect { x: 10, y: 10, w: 10, h: 10 }, None);
        assert_eq!(grey(&page, 15, 15), 0, "line drawn in the region");
        assert_eq!(grey(&page, 5, 15), 255, "outside the region untouched");
        assert_eq!(page.format(), Format::Gray);
        let png = page.png(PngFormat::Gray).unwrap();
        assert_eq!(&png[1..4], b"PNG");
    }

    #[test]
    fn composite_with_clip() {
        let mut page = RasterSink::page(&(), 10, 10, Rgba::WHITE);
        let mut reg = page.region(IRect { x: 0, y: 0, w: 10, h: 10 });
        for (x, y) in [(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)] { if x == 0.0 && y == 0.0 { reg.move_to(x, y) } else { reg.line_to(x, y) } }
        reg.close();
        reg.draw(&Paint { color: Rgba::BLACK, kind: PaintKind::Fill { rule: FillRule::NonZero } });
        // keep only the left half
        page.composite(reg, IRect { x: 0, y: 0, w: 10, h: 10 }, Some(&[[0.0, 0.0], [5.0, 0.0], [5.0, 10.0], [0.0, 10.0]]));
        assert_eq!(grey(&page, 2, 5), 0);
        assert_eq!(grey(&page, 8, 5), 255);
    }

    #[test]
    fn paste_outside_page_is_safe() {
        let mut page = RasterSink::page(&(), 10, 10, Rgba::WHITE);
        let mut reg = page.region(IRect { x: 8, y: 8, w: 10, h: 10 });
        line(&mut reg, (0.0, 0.5), (10.0, 0.5), 1.0);
        page.composite(reg, IRect { x: 8, y: 8, w: 10, h: 10 }, None);
        let mut reg = page.region(IRect { x: -3, y: -3, w: 5, h: 5 });
        line(&mut reg, (0.0, 4.5), (5.0, 4.5), 1.0);
        page.composite(reg, IRect { x: -3, y: -3, w: 5, h: 5 }, None);
        assert_eq!(grey(&page, 9, 8), 0);
        assert_eq!(grey(&page, 1, 1), 0);
    }

    #[test]
    fn colour_promotes_to_rgba() {
        let mut page = RasterSink::page(&(), 10, 10, Rgba::WHITE);
        page.move_to(0.0, 5.5);
        page.line_to(10.0, 5.5);
        page.draw(&Paint { color: Rgba::rgb(0xff0000), kind: PaintKind::Stroke { width: 1.0, dash: None } });
        assert_eq!(page.format(), Format::Rgba);
        assert_eq!(page.pixel(3, 5).unwrap(), [255, 0, 0, 255]);
        assert_eq!(page.pixel(3, 2).unwrap(), [255, 255, 255, 255]);
    }

    #[test]
    fn hairline_snaps_to_one_dark_row() {
        // a 1 px line exactly between two pixel rows: one full row, not two half rows
        let mut s = RasterSink::page(&(), 20, 20, Rgba::WHITE);
        line(&mut s, (2.0, 10.0), (18.0, 10.0), 1.0);
        let col: Vec<u8> = (8..13).map(|y| grey(&s, 10, y)).collect();
        assert_eq!(col.iter().filter(|&&g| g == 0).count(), 1, "{col:?}");
        assert_eq!(col.iter().filter(|&&g| g == 255).count(), 4, "{col:?}");
        // vertical too
        let mut s = RasterSink::page(&(), 20, 20, Rgba::WHITE);
        line(&mut s, (7.0, 2.0), (7.0, 18.0), 1.0);
        let row: Vec<u8> = (5..10).map(|x| grey(&s, x, 10)).collect();
        assert_eq!(row.iter().filter(|&&g| g == 0).count(), 1, "{row:?}");
    }

    #[test]
    fn two_px_line_is_two_rows() {
        let mut s = RasterSink::page(&(), 20, 20, Rgba::WHITE);
        line(&mut s, (2.0, 10.3), (18.0, 10.3), 2.0);
        let col: Vec<u8> = (7..13).map(|y| grey(&s, 10, y)).collect();
        assert_eq!(col, vec![255, 255, 0, 0, 255, 255]);
    }

    #[test]
    fn fill_even_odd_keeps_holes() {
        let mut s = RasterSink::page(&(), 20, 20, Rgba::WHITE);
        let sq = |s: &mut RasterSink, a: f32, b: f32| { s.move_to(a, a); s.line_to(b, a); s.line_to(b, b); s.line_to(a, b); s.close(); };
        sq(&mut s, 2.0, 18.0);
        sq(&mut s, 6.0, 14.0);
        s.draw(&Paint { color: Rgba::BLACK, kind: PaintKind::Fill { rule: FillRule::EvenOdd } });
        assert_eq!(grey(&s, 3, 10), 0);
        assert_eq!(grey(&s, 10, 10), 255, "hole");
        assert_eq!(grey(&s, 19, 10), 255, "outside");
        // half-covered edge pixel
        let mut s = RasterSink::page(&(), 10, 10, Rgba::WHITE);
        sq(&mut s, 2.5, 7.0);
        s.draw(&Paint { color: Rgba::BLACK, kind: PaintKind::Fill { rule: FillRule::NonZero } });
        assert!((grey(&s, 2, 4) as i32 - 128).abs() <= 2, "{}", grey(&s, 2, 4));
    }
}
