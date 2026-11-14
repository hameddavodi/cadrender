//! The drawing backend interface.
//!
//! The walk turns the scene into device-space paths (pixels, y down) with a
//! style, already batched: many shapes with the same style come as one path.
//! A `Sink` only has to draw what it gets. There is one sink per output kind:
//! PNG (tiny-skia, in `cadrender-raster`) now; SVG and a custom grey
//! rasteriser can be added later without touching the walk.
//!
//! Protocol: the walk builds a path with `move_to`/`line_to`/`quad_to`/
//! `cubic_to`/`close`, then calls `draw` once with the paint for the whole
//! path. After `draw` the sink starts a new, empty path.
//!
//! Viewports: the walk asks for a `region` (a new empty sink for a part of the
//! page), draws the viewport into it (maybe on another thread), then gives it
//! back with `composite`, which puts it on the page, clipped.

use crate::color::Rgba;
use crate::geom::Aff;
use crate::text::layout::Glyph;
use crate::text::mtext::Rich;
use crate::text::Face;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FillRule { NonZero, EvenOdd }

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PaintKind<'a> {
    /// Stroke with butt caps and round joins. `width` in pixels. `dash` is an
    /// even-length list of on/off lengths in pixels.
    Stroke { width: f32, dash: Option<&'a [f32]> },
    Fill { rule: FillRule },
}

/// What to do with the current path.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Paint<'a> {
    pub color: Rgba,
    pub kind: PaintKind<'a>,
}

/// One laid-out text, for sinks that write real text (SVG) instead of glyph outlines.
pub struct TextRun<'a> {
    pub face: &'a Face,
    /// Glyph ids and positions in font units (`face.chars` gives the characters).
    pub glyphs: &'a [Glyph],
    /// Font units (y up, first baseline at y = 0) -> device pixels.
    pub m: Aff,
    pub color: Rgba,
}

/// Formatted MTEXT for sinks that write real text: the formatting of a
/// [`TextRun`], with the colours already resolved.
pub struct RichText<'a> {
    pub rich: &'a Rich,
    /// Colour of each `rich.runs[i]`.
    pub run_colors: &'a [Rgba],
    /// Colour of each `rich.deco[i]`.
    pub deco_colors: &'a [Rgba],
}

/// Shape of a smooth gradient, in device pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradientShape {
    /// The colour changes along the line from `p0` (offset 0) to `p1`
    /// (offset 1) and stays the same across it.
    Linear { p0: [f32; 2], p1: [f32; 2] },
    /// The colour changes with the distance from `c`: offset 0 at `c`, 1 at radius `r`.
    Radial { c: [f32; 2], r: f32 },
}

/// A smooth gradient fill (hatch gradients). `stops` are (offset 0..1,
/// colour), sorted by offset; outside them the end colours go on.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientPaint<'a> {
    pub shape: GradientShape,
    pub stops: &'a [(f32, Rgba)],
}

/// A raster picture (IMAGE entity) to draw, in device pixels.
pub struct ImageDraw<'a> {
    pub pic: &'a crate::entity::image::Picture,
    /// Picture pixels (x right, y down, (0,0) = top-left corner) -> device pixels.
    pub m: Aff,
    /// Clip polygon in device pixels: draw only inside it.
    pub clip: &'a [[f32; 2]],
    /// 0..=100 like AutoCAD (50, 50, 0 = unchanged). Fade blends toward `background`.
    pub brightness: u8,
    pub contrast: u8,
    pub fade: u8,
    /// Draw in grey (monochrome colour policy).
    pub gray: bool,
    pub background: Rgba,
}

impl ImageDraw<'_> {
    /// Brightness and contrast as `v' = slope * v + intercept` (values 0..1).
    pub fn linear(&self) -> (f32, f32) {
        let slope = self.contrast.min(100) as f32 / 50.0;
        let intercept = 0.5 - 0.5 * slope + (self.brightness.min(100) as f32 - 50.0) / 100.0;
        (slope, intercept)
    }
    /// The same as a table for 8-bit values.
    pub fn lut(&self) -> [u8; 256] {
        let (k, d) = self.linear();
        let mut t = [0u8; 256];
        for (i, v) in t.iter_mut().enumerate() { *v = ((k * i as f32 / 255.0 + d).clamp(0.0, 1.0) * 255.0).round() as u8; }
        t
    }
}

/// Integer rectangle in page pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IRect { pub x: i32, pub y: i32, pub w: u32, pub h: u32 }

pub trait Sink: Send + Sync + Sized {
    /// Settings for this sink kind (for example SVG precision). `()` if none.
    type Config: Sync;

    /// A new page `w` x `h` pixels filled with `background`.
    fn page(cfg: &Self::Config, w: u32, h: u32, background: Rgba) -> Self;
    /// A new empty, transparent sink for the page rectangle `rect` (one viewport).
    /// Coordinates in the region start at its top-left corner.
    fn region(&self, rect: IRect) -> Self;
    /// Put a finished region on this sink at `rect`. If `clip` is given (a
    /// polygon in region pixels), only the inside is kept.
    fn composite(&mut self, region: Self, rect: IRect, clip: Option<&[[f32; 2]]>);

    fn move_to(&mut self, x: f32, y: f32);
    fn line_to(&mut self, x: f32, y: f32);
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32);
    fn cubic_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32);
    fn close(&mut self);
    /// Draw the path built since the last `draw`, then start a new one.
    fn draw(&mut self, paint: &Paint<'_>);
    /// Forget the path built since the last `draw` without drawing it.
    fn discard(&mut self);

    /// `true` if this sink wants texts as [`TextRun`]s. Checked at compile
    /// time, so sinks that do not (PNG) pay nothing.
    const TEXT_RUNS: bool = false;
    /// Draw one text as text. Return `false` to get the glyph outlines instead.
    fn text(&mut self, _run: &TextRun<'_>) -> bool { false }
    /// Draw one formatted MTEXT as text (asked only when `TEXT_RUNS`).
    /// Return `false` to get the glyph outlines instead.
    fn rich_text(&mut self, _run: &TextRun<'_>, _rich: &RichText<'_>) -> bool { false }

    /// `true` if this sink draws smooth gradients with [`Sink::fill_gradient`].
    /// Checked at compile time; the others (PNG) get bands of solid colour.
    const GRADIENTS: bool = false;
    /// Fill the path built since the last `draw` with a gradient (even-odd
    /// rule), then start a new path. Return `false` to forget the path and
    /// get bands of solid colour instead.
    fn fill_gradient(&mut self, _g: &GradientPaint<'_>) -> bool { self.discard(); false }

    /// Draw a raster picture. Return `false` if this sink cannot (then only
    /// the IMAGE frame is drawn).
    fn image(&mut self, _img: &ImageDraw<'_>) -> bool { false }
}

/// A sink that records what it gets. Used by tests, and handy for debugging.
#[derive(Debug, Default)]
pub struct RecordingSink {
    pub w: u32,
    pub h: u32,
    /// Number of path segments since the last draw.
    pending: usize,
    pub draws: Vec<RecordedDraw>,
    pub regions: Vec<(IRect, RecordingSink)>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecordedDraw {
    pub color: Rgba,
    pub stroke_width: Option<f32>,
    pub dashed: bool,
    pub fill_rule: Option<FillRule>,
    pub segments: usize,
}

impl RecordingSink {
    /// All draws of this sink and its regions.
    pub fn all_draws(&self) -> Vec<RecordedDraw> {
        let mut v = self.draws.clone();
        for (_, r) in &self.regions { v.extend(r.all_draws()); }
        v
    }
}

impl Sink for RecordingSink {
    type Config = ();
    fn page(_: &(), w: u32, h: u32, _bg: Rgba) -> Self { RecordingSink { w, h, ..Default::default() } }
    fn region(&self, rect: IRect) -> Self { RecordingSink { w: rect.w, h: rect.h, ..Default::default() } }
    fn composite(&mut self, region: Self, rect: IRect, _clip: Option<&[[f32; 2]]>) { self.regions.push((rect, region)); }
    fn move_to(&mut self, _: f32, _: f32) { self.pending += 1; }
    fn line_to(&mut self, _: f32, _: f32) { self.pending += 1; }
    fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) { self.pending += 1; }
    fn cubic_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) { self.pending += 1; }
    fn close(&mut self) {}
    fn draw(&mut self, paint: &Paint<'_>) {
        let (stroke_width, dashed, fill_rule) = match paint.kind {
            PaintKind::Stroke { width, dash } => (Some(width), dash.is_some(), None),
            PaintKind::Fill { rule } => (None, false, Some(rule)),
        };
        self.draws.push(RecordedDraw { color: paint.color, stroke_width, dashed, fill_rule, segments: self.pending });
        self.pending = 0;
    }
    fn discard(&mut self) { self.pending = 0; }
}
