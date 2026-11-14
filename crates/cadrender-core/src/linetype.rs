//! Complex linetypes: text and shape elements placed along a line.
//!
//! Plain dash patterns are drawn by the sink (a dash array). A linetype with
//! text or shapes is drawn here instead: the path is flattened in device
//! pixels and walked once. Dashes become short segments, text becomes glyph
//! outlines (the one sans font of the font policy), and SHX shapes, which we
//! cannot ship, become one lobe of a wave each. The most common shape in our
//! files, the "BAT" shape of the insulation linetype, really looks like a wave.
//!
//! A text or shape sits at the *end* of the pattern element it belongs to,
//! moved by its x/y offset along / across the line (DXF LTYPE groups 44/45).

use crate::geom::{flatten, Aff, PathData};
use crate::render::batch::StrokeKey;
use crate::render::walk::Walker;
use crate::scene::tables::Linetype;
use crate::sink::Sink;
use crate::text::layout::Glyph;
use crate::text::FaceId;

/// What a complex linetype element draws.
#[derive(Clone, Debug, PartialEq)]
pub enum LtWhat {
    /// A text, laid out once in font units (baseline at y = 0).
    Text { face: FaceId, glyphs: Vec<Glyph>, cap: f64 },
    /// An SHX shape (shape number). Drawn as a wave lobe (fallback).
    Shape { code: u16 },
}

/// One text or shape element of a complex linetype.
#[derive(Clone, Debug, PartialEq)]
pub struct LtPart {
    /// Index of the pattern element it belongs to.
    pub elem: usize,
    pub what: LtWhat,
    /// Scale (group 46): text height, or shape size factor.
    pub scale: f64,
    /// Rotation in radians (group 50).
    pub rot: f64,
    /// Rotation is absolute (to the x axis), not relative to the line.
    pub absolute: bool,
    /// Offset along and across the line, in pattern units (groups 44, 45).
    pub off: [f64; 2],
}

/// Most pattern repeats drawn for one entity; above that the line is solid.
const MAX_REPEATS: f64 = 20_000.0;
/// Shorter periods (pixels) are drawn as a solid line.
const MIN_PERIOD_PX: f64 = 1.5;

/// The linetype `id`, if it has text or shape elements.
#[inline]
pub fn complex(lts: &[Linetype], id: u32) -> Option<&Linetype> {
    lts.get(id as usize).filter(|lt| !lt.parts.is_empty())
}

/// Half height of the wave drawn for a shape, in pattern units, for a shape
/// with scale `s`. Calibrated on the insulation linetype (BATTING: s = 0.1,
/// band about 1 pattern unit high).
fn wave_height(s: f64, period: f64) -> f64 { (5.0 * s.abs()).min(2.0 * period) }

/// Walk the polyline `poly` (device pixels) with a dash pattern `els` in
/// pixels (> 0 dash, < 0 gap, 0 dot), starting at the first element. Dashes
/// are added to `out`; `at_end(out, point, direction, k)` is called at the end
/// of every element `k`.
pub fn walk_pattern(poly: &[[f64; 2]], els: &[f64], out: &mut PathData, at_end: &mut impl FnMut(&mut PathData, [f64; 2], [f64; 2], usize)) {
    let period: f64 = els.iter().map(|d| d.abs()).sum();
    if els.is_empty() || period.is_nan() || period <= 0.0 { return; }
    let (mut k, mut left, mut pen) = (0usize, els[0].abs(), false);
    for seg in poly.windows(2) {
        let (a, b) = (seg[0], seg[1]);
        let sl = (b[0] - a[0]).hypot(b[1] - a[1]);
        if sl < 1e-9 { continue; }
        let dir = [(b[0] - a[0]) / sl, (b[1] - a[1]) / sl];
        let at = |t: f64| [a[0] + dir[0] * t, a[1] + dir[1] * t];
        let mut t = 0.0;
        loop {
            let step = left.min(sl - t);
            if els[k] > 0.0 && step > 0.0 {
                if !pen { let p = at(t); out.move_to(p[0], p[1]); pen = true; }
                let p = at(t + step);
                out.line_to(p[0], p[1]);
            }
            t += step;
            left -= step;
            if left > 1e-9 { break; }
            // end of element k
            let p = at(t);
            if els[k] == 0.0 { out.move_to(p[0], p[1]); out.line_to(p[0] + dir[0] * 0.5, p[1] + dir[1] * 0.5); }
            pen = false;
            at_end(out, p, dir, k);
            k = (k + 1) % els.len();
            left = els[k].abs();
            if t >= sl && left > 1e-9 { break; }
        }
    }
}

/// Polyline length.
pub fn len(p: &[[f64; 2]]) -> f64 { p.windows(2).map(|w| (w[1][0] - w[0][0]).hypot(w[1][1] - w[0][1])).sum() }

/// Draw `path` (local coordinates, `m` to device pixels) with the complex
/// linetype `lt`. `sc` = pixels per pattern unit. The dash of `key` is ignored.
pub fn draw<S: Sink>(w: &mut Walker<'_, S>, path: &PathData, m: &Aff, key: StrokeKey, lt: &Linetype, sc: f64) {
    let solid = key.solid();
    let units: Vec<f64> = lt.pattern.iter().map(|d| d.abs()).collect();
    let period_u: f64 = units.iter().sum();
    let period = period_u * sc;
    let mut dev = path.clone();
    dev.transform(m);
    let mut polys = vec![];
    flatten(&dev, 0.3, &mut polys);
    let total: f64 = polys.iter().map(|p| len(p)).sum();
    if !(period >= MIN_PERIOD_PX && period.is_finite()) || units.is_empty() || total / period > MAX_REPEATS {
        w.stroke(path, m, solid);
        return;
    }
    // "left of the line" in device pixels: y is down when the matrix mirrors
    let side = if m.det() < 0.0 { -1.0 } else { 1.0 };
    let norm = |d: [f64; 2]| [-d[1] * side, d[0] * side];
    let xa = { let (a, b) = (m.0[0], m.0[1]); let l = a.hypot(b).max(1e-300); [a / l, b / l] };
    let n_shapes = lt.parts.iter().filter(|p| matches!(p.what, LtWhat::Shape { .. })).count().max(1);
    let lobe_w = period_u / n_shapes as f64;

    let mut lines = PathData::default();
    let mut texts: Vec<(Aff, &LtPart)> = vec![];
    let mut place = |lines: &mut PathData, at: [f64; 2], dir: [f64; 2], k: usize| {
        for part in lt.parts.iter().filter(|p| p.elem == k) {
            let nv = norm(dir);
            let o = [at[0] + (part.off[0] * dir[0] + part.off[1] * nv[0]) * sc, at[1] + (part.off[0] * dir[1] + part.off[1] * nv[1]) * sc];
            let base = if part.absolute { xa } else { dir };
            let (s, c) = part.rot.sin_cos();
            let u = [base[0] * c + norm(base)[0] * s, base[1] * c + norm(base)[1] * s];
            let v = norm(u);
            match &part.what {
                LtWhat::Shape { .. } => {
                    let h = wave_height(part.scale, period_u);
                    for i in 0..=8 {
                        let t = i as f64 / 8.0;
                        let (x, y) = (lobe_w * t * sc, h * (std::f64::consts::PI * t).sin() * sc);
                        let p = [o[0] + u[0] * x + v[0] * y, o[1] + u[1] * x + v[1] * y];
                        if i == 0 { lines.move_to(p[0], p[1]) } else { lines.line_to(p[0], p[1]) }
                    }
                }
                LtWhat::Text { cap, .. } => {
                    let k = part.scale.abs() * sc / cap.max(1e-9);
                    texts.push((Aff([u[0] * k, u[1] * k, v[0] * k, v[1] * k, o[0], o[1]]), part));
                }
            }
        }
    };
    let px: Vec<f64> = lt.pattern.iter().map(|d| d * sc).collect();
    for poly in &polys { walk_pattern(poly, &px, &mut lines, &mut place); }
    w.out.set_stroke(solid);
    w.out.push(&lines, &Aff::ID);
    w.stats.drawn += 1;
    if texts.is_empty() { return; }
    let fonts = &w.scene.fonts;
    w.out.set_fill(key.color);
    for (am, part) in texts {
        let LtWhat::Text { face, glyphs, .. } = &part.what else { continue };
        let Some(face) = fonts.faces.get(*face as usize) else { continue };
        if !face.filled { w.out.set_stroke(solid); }
        for g in glyphs {
            if let Some(gd) = face.glyphs.get(g.id as usize) {
                if !gd.path.is_empty() { w.out.push(&gd.path, &am.mul(&Aff::translate(g.x as f64, g.y as f64))); }
            }
        }
        w.stats.glyphs += glyphs.len() as u64;
    }
}
