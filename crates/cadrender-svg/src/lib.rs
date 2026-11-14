//! cadrender-svg: a [`Sink`] that writes SVG.
//!
//! - One `<path>` per batch from the walk, with a short shared class
//!   (colour + width) defined once in a `<style>` block.
//! - Path data is relative, on a grid of `10^-dec` pixels; `dec` comes from
//!   the picture size (about 0.1 px when zoomed 10x), see [`auto_precision`].
//! - Viewports become nested `<svg>` elements: they clip for free. A clip
//!   polygon becomes a `<clipPath>`.
//! - Texts become real `<text>` elements (selectable, searchable), or glyph
//!   outlines with [`TextMode::Paths`].
//! - Masks and WIPEOUTs arrive as fills in the background colour, so draw
//!   order is kept by writing elements in the order they come.

mod image;
pub mod num;
pub mod style;
pub mod text;

use cadrender_core::color::Rgba;
use cadrender_core::sink::{FillRule, GradientPaint, GradientShape, IRect, Paint, PaintKind, RichText, Sink, TextRun};
use cadrender_core::{Lod, RenderOptions};
use num::{write_num, PathWriter};
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use style::{pack, Style};

/// How texts are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TextMode {
    /// Real `<text>` elements in the sans font (default).
    #[default]
    Text,
    /// Glyph outlines as filled paths: exact look, not selectable.
    Paths,
}

/// SVG settings.
#[derive(Clone, Debug, PartialEq)]
pub struct SvgConfig {
    /// Decimals of coordinates. `None` = from the picture size.
    pub precision: Option<u8>,
    pub text: TextMode,
    /// Strokes this thin (pixels) or thinner keep their screen width when the
    /// viewer zooms (`vector-effect: non-scaling-stroke`). 0 = never.
    pub hairline_px: f32,
    /// No level-of-detail shortcuts at all.
    pub full_detail: bool,
    /// The level-of-detail limits of the PNG are divided by this, because a
    /// viewer can zoom into an SVG. 1 = same as PNG.
    pub lod_zoom: f64,
}

impl Default for SvgConfig {
    fn default() -> Self { SvgConfig { precision: None, text: TextMode::Text, hairline_px: 1.0, full_detail: false, lod_zoom: 4.0 } }
}

impl SvgConfig {
    /// The render options for SVG output: the PNG options with weaker (or no)
    /// level of detail.
    pub fn render_options(&self, o: &RenderOptions) -> RenderOptions {
        let mut o = o.clone();
        let z = self.lod_zoom.max(1.0);
        let l = o.lod;
        o.lod = Lod { dot_px: l.dot_px / z, text_hide_px: l.text_hide_px / z, text_greek_px: l.text_greek_px / z, hatch_tint_px: l.hatch_tint_px / z, dash_min_period_px: l.dash_min_period_px / z, ..l };
        if self.full_detail { o.lod.enabled = false; }
        o
    }
}

/// Decimals so that one grid step is about 1/200000 of the picture size:
/// 2 decimals for 2000 px, 1 for 20000 px.
pub fn auto_precision(w: u32, h: u32) -> u8 {
    let m = w.max(h).max(1) as f64;
    (2e5 / m).log10().ceil().clamp(0.0, 4.0) as u8
}

/// Writes SVG text. Get the finished file with [`SvgSink::finish`].
pub struct SvgSink {
    pub w: u32,
    pub h: u32,
    background: Rgba,
    dec: u8,
    text_mode: TextMode,
    hairline_px: f32,
    body: Vec<u8>,
    path: PathWriter,
    styles: BTreeSet<Style>,
    has_text: bool,
    /// Counter for element ids (clip paths), shared by the page and its regions.
    ids: Arc<AtomicU32>,
}

impl SvgSink {
    fn blank(&self, w: u32, h: u32) -> SvgSink {
        SvgSink { w, h, background: Rgba::TRANSPARENT, dec: self.dec, text_mode: self.text_mode, hairline_px: self.hairline_px, body: Vec::new(), path: PathWriter::new(self.dec), styles: BTreeSet::new(), has_text: false, ids: self.ids.clone() }
    }

    /// Decimals used for coordinates.
    pub fn precision(&self) -> u8 { self.dec }

    /// The elements written so far (no header), for tests.
    pub fn body(&self) -> &[u8] { &self.body }

    /// The whole SVG file.
    pub fn finish(self) -> Vec<u8> {
        let mut o = Vec::with_capacity(self.body.len() + 1024);
        o.extend_from_slice(b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"");
        o.extend_from_slice(format!("{0}\" height=\"{1}\" viewBox=\"0 0 {0} {1}\"", self.w, self.h).as_bytes());
        // xml:space keeps runs of spaces in texts (CSS white-space alone is not enough for every viewer)
        o.extend_from_slice(b" stroke-linecap=\"butt\" stroke-linejoin=\"round\" xml:space=\"preserve\">\n<style>\n");
        for s in &self.styles { s.write_rule(&mut o, self.hairline_px); }
        if self.has_text { o.extend_from_slice(text::TEXT_RULE); }
        o.extend_from_slice(b"</style>\n");
        let bg = self.background;
        if bg.a > 0 {
            o.extend_from_slice(b"<rect width=\"100%\" height=\"100%\" fill=\"");
            style::write_color(&mut o, pack(bg));
            o.push(b'"');
            if bg.a < 255 { o.extend_from_slice(b" fill-opacity=\""); write_num(&mut o, bg.a as f64 / 255.0, 3); o.push(b'"'); }
            o.extend_from_slice(b"/>\n");
        }
        o.extend_from_slice(&self.body);
        o.extend_from_slice(b"</svg>\n");
        o
    }

    fn open_tag(&mut self, st: Style) {
        self.styles.insert(st);
        self.body.extend_from_slice(b"<path class=\"");
        st.write_class(&mut self.body);
        self.body.push(b'"');
    }
}

impl Sink for SvgSink {
    type Config = SvgConfig;
    const TEXT_RUNS: bool = true;

    fn page(cfg: &SvgConfig, w: u32, h: u32, background: Rgba) -> Self {
        let dec = cfg.precision.unwrap_or_else(|| auto_precision(w, h)).min(6);
        SvgSink { w, h, background, dec, text_mode: cfg.text, hairline_px: cfg.hairline_px, body: Vec::new(), path: PathWriter::new(dec), styles: BTreeSet::new(), has_text: false, ids: Arc::new(AtomicU32::new(0)) }
    }

    fn region(&self, rect: IRect) -> Self { self.blank(rect.w, rect.h) }

    fn composite(&mut self, region: Self, rect: IRect, clip: Option<&[[f32; 2]]>) {
        if region.body.is_empty() { return; }
        self.styles.extend(region.styles.iter().copied());
        self.has_text |= region.has_text;
        let b = &mut self.body;
        b.extend_from_slice(format!("<svg x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\">\n", rect.x, rect.y, rect.w, rect.h).as_bytes());
        let clipped = match clip {
            Some(poly) if poly.len() >= 3 => {
                let id = self.ids.fetch_add(1, Ordering::Relaxed);
                let mut p = PathWriter::new(self.dec);
                for (i, q) in poly.iter().enumerate() { if i == 0 { p.move_to(q[0], q[1]) } else { p.line_to(q[0], q[1]) } }
                p.close();
                b.extend_from_slice(format!("<clipPath id=\"c{id}\"><path d=\"").as_bytes());
                b.extend_from_slice(&p.d);
                b.extend_from_slice(format!("\"/></clipPath>\n<g clip-path=\"url(#c{id})\">\n").as_bytes());
                true
            }
            _ => false,
        };
        b.extend_from_slice(&region.body);
        if clipped { b.extend_from_slice(b"</g>\n"); }
        b.extend_from_slice(b"</svg>\n");
    }

    #[inline] fn move_to(&mut self, x: f32, y: f32) { self.path.move_to(x, y) }
    #[inline] fn line_to(&mut self, x: f32, y: f32) { self.path.line_to(x, y) }
    #[inline] fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) { self.path.quad_to(x1, y1, x, y) }
    #[inline] fn cubic_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) { self.path.cubic_to(x1, y1, x2, y2, x, y) }
    #[inline] fn close(&mut self) { self.path.close() }

    fn draw(&mut self, paint: &Paint<'_>) {
        if !self.path.drawn || paint.color.a == 0 { self.path.reset(); return; }
        let rgba = pack(paint.color);
        match paint.kind {
            PaintKind::Stroke { width, dash } => {
                self.open_tag(Style::Stroke { rgba, w16: (width.max(0.0) * 16.0).round() as u32 });
                if let Some(d) = dash.filter(|d| !d.is_empty() && d.iter().sum::<f32>() > 0.0) {
                    self.body.extend_from_slice(b" stroke-dasharray=\"");
                    for (i, v) in d.iter().enumerate() { if i > 0 { self.body.push(b' '); } write_num(&mut self.body, *v as f64, 2); }
                    self.body.push(b'"');
                }
            }
            PaintKind::Fill { rule } => self.open_tag(Style::Fill { rgba, even_odd: rule == FillRule::EvenOdd }),
        }
        self.body.extend_from_slice(b" d=\"");
        self.body.extend_from_slice(&self.path.d);
        self.body.extend_from_slice(b"\"/>\n");
        self.path.reset();
    }

    fn discard(&mut self) { self.path.reset(); }

    fn image(&mut self, img: &cadrender_core::sink::ImageDraw<'_>) -> bool {
        let id = self.ids.fetch_add(1, Ordering::Relaxed);
        image::write(&mut self.body, id, self.dec, img)
    }

    const GRADIENTS: bool = true;

    fn fill_gradient(&mut self, g: &GradientPaint<'_>) -> bool {
        if !self.path.drawn || g.stops.iter().all(|s| s.1.a == 0) { self.path.reset(); return true; }
        let id = self.ids.fetch_add(1, Ordering::Relaxed);
        let (b, dec) = (&mut self.body, self.dec);
        let attr = |b: &mut Vec<u8>, name: &str, v: f32| { b.extend_from_slice(format!(" {name}=\"").as_bytes()); write_num(b, v as f64, dec); b.push(b'"'); };
        let tag: &[u8] = match g.shape {
            GradientShape::Linear { p0, p1 } => {
                b.extend_from_slice(format!("<linearGradient id=\"g{id}\" gradientUnits=\"userSpaceOnUse\"").as_bytes());
                for (n, v) in [("x1", p0[0]), ("y1", p0[1]), ("x2", p1[0]), ("y2", p1[1])] { attr(b, n, v); }
                b"</linearGradient>\n"
            }
            GradientShape::Radial { c, r } => {
                b.extend_from_slice(format!("<radialGradient id=\"g{id}\" gradientUnits=\"userSpaceOnUse\"").as_bytes());
                for (n, v) in [("cx", c[0]), ("cy", c[1]), ("r", r.max(1e-3))] { attr(b, n, v); }
                b"</radialGradient>\n"
            }
        };
        b.push(b'>');
        for &(off, col) in g.stops {
            b.extend_from_slice(b"<stop offset=\"");
            write_num(b, off.clamp(0.0, 1.0) as f64, 4);
            b.extend_from_slice(b"\" stop-color=\"");
            style::write_color(b, pack(col));
            b.push(b'"');
            if col.a < 255 { b.extend_from_slice(b" stop-opacity=\""); write_num(b, col.a as f64 / 255.0, 3); b.push(b'"'); }
            b.extend_from_slice(b"/>");
        }
        b.extend_from_slice(tag);
        b.extend_from_slice(format!("<path fill=\"url(#g{id})\" fill-rule=\"evenodd\" d=\"").as_bytes());
        b.extend_from_slice(&self.path.d);
        b.extend_from_slice(b"\"/>\n");
        self.path.reset();
        true
    }

    fn text(&mut self, run: &TextRun<'_>) -> bool {
        if self.text_mode != TextMode::Text { return false; }
        if run.color.a == 0 { return true; }
        let st = Style::Fill { rgba: pack(run.color), even_odd: false };
        let mut class = Vec::with_capacity(12);
        st.write_class(&mut class);
        if !text::write_run(&mut self.body, run, self.dec, &[text::Look { class: &class, bold: false }], &[]) { return false; }
        self.styles.insert(st);
        self.has_text = true;
        true
    }

    fn rich_text(&mut self, run: &TextRun<'_>, rich: &RichText<'_>) -> bool {
        if self.text_mode != TextMode::Text || !text::known(run.face, run.glyphs) { return false; }
        let runs = &rich.rich.runs;
        let (mut src, mut glyphs, mut look_of) = (Vec::new(), Vec::new(), Vec::new());
        // distinct (style, bold) of the current group; class text per entry
        let mut looks: Vec<(Style, bool, Vec<u8>)> = Vec::new();
        let (mut i, mut start) = (0, 0);
        while i < runs.len() {
            let j = text::group_end(rich, i);
            src.clear(); look_of.clear(); looks.clear();
            for (r, run_i) in runs[i..j].iter().zip(i..j) {
                let end = (r.end as usize).clamp(start, run.glyphs.len());
                let col = rich.run_colors.get(run_i).copied().unwrap_or(run.color);
                if col.a > 0 {
                    let st = Style::Fill { rgba: pack(col), even_odd: false };
                    let n = looks.iter().position(|l| l.0 == st && l.1 == r.bold).unwrap_or_else(|| {
                        let mut c = Vec::with_capacity(12);
                        st.write_class(&mut c);
                        looks.push((st, r.bold, c));
                        looks.len() - 1
                    });
                    src.extend_from_slice(&run.glyphs[start..end]);
                    look_of.extend(std::iter::repeat_n(n as u16, end - start));
                }
                start = end;
            }
            let m = text::rich_part(run, rich, i, &src, &mut glyphs);
            i = j;
            if glyphs.is_empty() { continue; }
            let lk: Vec<text::Look<'_>> = looks.iter().map(|l| text::Look { class: &l.2, bold: l.1 }).collect();
            text::write_run(&mut self.body, &TextRun { glyphs: &glyphs, m, ..*run }, self.dec, &lk, &look_of);
            self.styles.extend(looks.iter().map(|l| l.0));
            self.has_text = true;
        }
        // underline, overline, strike-through, stack bars: filled boxes, one path per colour in a row
        let mut prev: Option<Rgba> = None;
        for (d, &col) in rich.rich.deco.iter().zip(rich.deco_colors) {
            if prev.is_some_and(|p| p != col) { self.draw(&Paint { color: prev.unwrap_or(col), kind: PaintKind::Fill { rule: FillRule::NonZero } }); }
            prev = Some(col);
            let (x0, y0, x1, y1) = (d.x0 as f64, d.y0 as f64, d.x1 as f64, d.y1 as f64);
            for (n, (x, y)) in [(x0, y0), (x1, y0), (x1, y1), (x0, y1)].into_iter().enumerate() {
                let (px, py) = run.m.apply(x, y);
                if n == 0 { self.path.move_to(px as f32, py as f32) } else { self.path.line_to(px as f32, py as f32) }
            }
            self.path.close();
        }
        if let Some(col) = prev { self.draw(&Paint { color: col, kind: PaintKind::Fill { rule: FillRule::NonZero } }); }
        true
    }
}
