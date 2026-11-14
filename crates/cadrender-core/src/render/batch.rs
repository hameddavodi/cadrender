//! Batching in front of a sink: consecutive shapes with the same style go into
//! one path, and the sink draws that path once. This is what keeps the number
//! of rasteriser calls small.

use crate::color::Rgba;
use crate::geom::{self, Aff, PathData};
use crate::sink::{FillRule, Paint, PaintKind, Sink};
use std::collections::HashMap;

/// No dash pattern.
pub const NO_DASH: u32 = u32::MAX;

/// Style of a stroke batch. Two strokes with the same key go into one path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StrokeKey {
    pub color: Rgba,
    /// Width in 1/16 pixel.
    pub width_q: u16,
    /// Index into the batcher's dash table, or `NO_DASH`.
    pub dash: u32,
}

impl StrokeKey {
    pub fn width_px(&self) -> f32 { self.width_q as f32 / 16.0 }
    pub fn solid(self) -> StrokeKey { StrokeKey { dash: NO_DASH, ..self } }
    pub fn with_width_px(self, px: f64) -> StrokeKey { StrokeKey { width_q: (px.clamp(0.0, 4000.0) * 16.0).round() as u16, ..self } }
}

#[derive(Clone, Copy, PartialEq)]
enum Mode { None, Stroke(StrokeKey), Fill(Rgba) }

/// Most points in one batch before it is flushed (keeps paths cache friendly).
const MAX_BATCH_POINTS: usize = 60_000;

pub struct Batcher<S: Sink> {
    pub sink: S,
    mode: Mode,
    npts: usize,
    dashes: Vec<Box<[f32]>>,
    dash_ids: HashMap<(u32, u32), u32>,
    /// Number of `draw` calls made on the sink.
    pub flushes: u64,
}

impl<S: Sink> Batcher<S> {
    pub fn new(sink: S) -> Self { Batcher { sink, mode: Mode::None, npts: 0, dashes: vec![], dash_ids: HashMap::new(), flushes: 0 } }

    /// Dash table id for linetype `ltype` at pixel scale `scale`. `pattern` in
    /// drawing units (> 0 dash, < 0 gap, 0 dot).
    pub fn dash_id(&mut self, ltype: u32, scale: f32, pattern: &[f64]) -> u32 {
        if let Some(&id) = self.dash_ids.get(&(ltype, scale.to_bits())) { return id; }
        let mut v: Vec<f32> = pattern.iter().map(|d| (d.abs() as f32 * scale).max(if *d >= 0.0 { 1.0 } else { 0.5 })).collect();
        if v.len() % 2 == 1 { v.push(0.5); }
        let id = self.dashes.len() as u32;
        self.dashes.push(v.into_boxed_slice());
        self.dash_ids.insert((ltype, scale.to_bits()), id);
        id
    }

    #[inline]
    pub fn set_stroke(&mut self, k: StrokeKey) {
        let m = Mode::Stroke(k);
        if self.mode != m || self.npts > MAX_BATCH_POINTS { self.flush(); self.mode = m; }
    }
    /// Non-zero fill, batched (glyphs).
    #[inline]
    pub fn set_fill(&mut self, color: Rgba) {
        let m = Mode::Fill(color);
        if self.mode != m || self.npts > MAX_BATCH_POINTS { self.flush(); self.mode = m; }
    }

    /// Draw what is batched.
    pub fn flush(&mut self) {
        if self.npts == 0 { return; }
        self.npts = 0;
        let paint = match self.mode {
            Mode::Stroke(k) => Paint { color: k.color, kind: PaintKind::Stroke { width: k.width_px(), dash: if k.dash == NO_DASH { None } else { self.dashes.get(k.dash as usize).map(|d| &d[..]) } } },
            Mode::Fill(c) => Paint { color: c, kind: PaintKind::Fill { rule: FillRule::NonZero } },
            Mode::None => { self.sink.discard(); return; }
        };
        self.sink.draw(&paint);
        self.flushes += 1;
    }

    /// Fill one path now with the even-odd rule (holes stay holes). Not merged
    /// with other shapes: overlapping even-odd fills would cancel out.
    pub fn fill_now(&mut self, p: &PathData, m: &Aff, color: Rgba) {
        self.flush();
        self.mode = Mode::None;
        self.push(p, m);
        self.npts = 0;
        self.sink.draw(&Paint { color, kind: PaintKind::Fill { rule: FillRule::EvenOdd } });
        self.flushes += 1;
    }

    /// Add a stored path, transformed to device pixels, to the current batch.
    #[inline]
    pub fn push(&mut self, p: &PathData, m: &Aff) {
        let t = |q: [f64; 2]| { let (x, y) = m.apply(q[0], q[1]); (x as f32, y as f32) };
        let mut i = 0;
        let s = &mut self.sink;
        for &v in &p.verbs {
            match v {
                geom::M => { let (x, y) = t(p.pts[i]); s.move_to(x, y); i += 1; }
                geom::L => { let (x, y) = t(p.pts[i]); s.line_to(x, y); i += 1; }
                geom::Q => { let (a, b) = (t(p.pts[i]), t(p.pts[i + 1])); s.quad_to(a.0, a.1, b.0, b.1); i += 2; }
                geom::C => { let (a, b, c) = (t(p.pts[i]), t(p.pts[i + 1]), t(p.pts[i + 2])); s.cubic_to(a.0, a.1, b.0, b.1, c.0, c.1); i += 3; }
                _ => s.close(),
            }
        }
        self.npts += p.pts.len();
    }

    /// Add one straight segment in device pixels.
    #[inline]
    pub fn seg(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        self.sink.move_to(x0, y0);
        self.sink.line_to(x1, y1);
        self.npts += 2;
    }

    /// Flush and give back the sink.
    pub fn finish(mut self) -> S { self.flush(); self.sink }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sink::RecordingSink;

    #[test]
    fn same_style_is_one_draw() {
        let mut b = Batcher::new(RecordingSink::page(&(), 10, 10, Rgba::WHITE));
        let k = StrokeKey { color: Rgba::BLACK, width_q: 16, dash: NO_DASH };
        b.set_stroke(k); b.seg(0.0, 0.0, 1.0, 1.0);
        b.set_stroke(k); b.seg(1.0, 0.0, 2.0, 1.0);
        b.set_stroke(StrokeKey { width_q: 32, ..k }); b.seg(0.0, 0.0, 5.0, 5.0);
        let s = b.finish();
        assert_eq!(s.draws.len(), 2);
        assert_eq!(s.draws[0].segments, 4);
        assert_eq!(s.draws[1].stroke_width, Some(2.0));
    }

    #[test]
    fn dash_table_reuses_ids() {
        let mut b = Batcher::new(RecordingSink::default());
        let a = b.dash_id(3, 2.0, &[5.0, -2.5, 0.0]);
        assert_eq!(a, b.dash_id(3, 2.0, &[5.0, -2.5, 0.0]));
        assert_ne!(a, b.dash_id(3, 4.0, &[5.0, -2.5, 0.0]));
        // odd length is made even, tiny parts get a minimum size
        assert_eq!(&b.dashes[a as usize][..], &[10.0, 5.0, 1.0, 0.5]);
    }
}
