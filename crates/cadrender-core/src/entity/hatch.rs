//! HATCH: solid fill, gradient fill, or a real line pattern.
//!
//! The pattern is the exact definition from the file (angle, base point,
//! offset, dash list; the file already has the pattern scale and angle in
//! it). Pattern lines are made only for the visible part of the hatch, with a
//! sweep over the boundary edges, and drawn with the hatch's own pen width.
//! A dash of length 0 is a dot: AutoCAD plots it as a zero-length line, so it
//! is a dot as big as the pen (not a circle).
//!
//! Level of detail (`Lod`, off in `Lod::full()`): a line family whose lines are
//! closer than `Lod::hatch_tint_px` becomes a tint with the same mean ink
//! (pen width x ink part of the dash pattern / line distance).
//!
//! Boundaries (edges, islands, hatch style) are in `boundary`, gradients in
//! `gradient`.

pub mod boundary;
pub mod gradient;

pub use boundary::{BoundaryLoop, Edge, HatchStyle};
pub use gradient::{Gradient, GradientKind};

use super::{Entity, EntityKind, PrepareCx};
use crate::geom::{self, Aff, BBox, PathData};
use crate::render::batch::StrokeKey;
use crate::render::walk::{Visit, Walker};
use crate::sink::Sink;

/// One line family of a hatch pattern (pattern units = drawing units).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PatLine {
    /// Angle in radians.
    pub angle: f64,
    pub base: [f64; 2],
    pub offset: [f64; 2],
    /// Dash lengths: > 0 dash, < 0 gap, 0 dot. Empty = continuous.
    pub dashes: Vec<f64>,
}

impl PatLine {
    /// The same family after an affine map (used for a mirrored OCS). Lengths
    /// scale with the map along the line direction.
    pub fn transformed(&self, m: &Aff) -> PatLine {
        let [a, b, c, d, ..] = m.0;
        let lin = |x: f64, y: f64| [a * x + c * y, b * x + d * y];
        let (sa, ca) = self.angle.sin_cos();
        let dir = lin(ca, sa);
        let k = (dir[0] * dir[0] + dir[1] * dir[1]).sqrt();
        let (bx, by) = m.apply(self.base[0], self.base[1]);
        PatLine { angle: dir[1].atan2(dir[0]), base: [bx, by], offset: lin(self.offset[0], self.offset[1]), dashes: self.dashes.iter().map(|x| x * k).collect() }
    }
}

#[derive(Clone, Debug, Default)]
pub struct HatchEnt {
    /// The boundary loops the hatch style keeps, as one path (even-odd).
    pub path: PathData,
    pub solid: bool,
    pub pattern: Vec<PatLine>,
    /// Gradient fill (boxed: rare).
    pub gradient: Option<Box<Gradient>>,
}

/// Most pattern lines of one family drawn in one view; more becomes a tint.
const MAX_LINES: i64 = 40_000;

/// With level of detail on, pattern pens thinner than this are drawn 1 px wide.
const HAIRLINE_SNAP_PX: f64 = 1.5;

impl EntityKind for HatchEnt {
    fn prepare(&mut self, _: &PrepareCx) -> BBox { self.path.bbox() }

    fn draw<S: Sink>(&self, e: &Entity, v: &Visit, w: &mut Walker<'_, S>) {
        let (st, _) = w.style(e, v.layer, &v.inh, 0.0);
        if v.tiny {
            w.out.set_stroke(st.solid());
            w.dot(&v.db);
            return;
        }
        if let Some(g) = &self.gradient {
            let c1 = w.color(g.colors[0], v.layer, &v.inh).with_alpha(st.color.a);
            let c2 = if g.two_colors { w.color(g.colors[1], v.layer, &v.inh).with_alpha(st.color.a) } else { gradient::tint_color(c1, g.tint) };
            gradient::draw(g, &self.path, &v.m, c1, c2, w);
            return;
        }
        if self.solid || self.pattern.is_empty() {
            w.out.fill_now(&self.path, &v.m, st.color);
        } else {
            self.draw_pattern(w, &v.m, st, &e.bbox);
        }
        w.stats.drawn += 1;
    }
}

/// A pattern family in the pattern frame: u along the lines, v across.
struct Family<'a> {
    pl: &'a PatLine,
    sa: f64,
    ca: f64,
    /// Signed distance between lines.
    s: f64,
    /// Length of one dash period (0 = continuous).
    period: f64,
    /// Part of a line that has ink (dots count as one pen width).
    duty: f64,
}

impl<'a> Family<'a> {
    fn new(pl: &'a PatLine, dot: f64) -> Option<Family<'a>> {
        let (sa, ca) = pl.angle.sin_cos();
        let s = -pl.offset[0] * sa + pl.offset[1] * ca;
        if !s.is_finite() || s.abs() <= 1e-12 { return None; }
        let period: f64 = pl.dashes.iter().map(|d| d.abs()).sum();
        let duty = if period > 0.0 {
            (pl.dashes.iter().map(|&d| if d > 0.0 { d } else if d == 0.0 { dot } else { 0.0 }).sum::<f64>() / period).min(1.0)
        } else { 1.0 };
        Some(Family { pl, sa, ca, s, period, duty })
    }
}

impl HatchEnt {
    fn draw_pattern<S: Sink>(&self, w: &mut Walker<'_, S>, m: &Aff, st: StrokeKey, bbox: &BBox) {
        let scale = m.scale_factor();
        if scale.is_nan() || scale <= 0.0 { return; }
        let lod = w.opts.lod;
        let mut pen_px = (st.width_px() as f64).max(w.opts.lineweight.min_px).max(0.1);
        // a pen a bit wider than 1 px looks the same, but costs much more to
        // rasterise than a 1 px hairline; full detail keeps the true width
        if lod.enabled && pen_px < HAIRLINE_SNAP_PX { pen_px = pen_px.min(1.0); }
        let dot = pen_px / scale; // a dot is one pen width long
        // visible window in local coordinates
        let inv = m.inverse();
        let vis = w.clip.grow(pen_px + 1.0);
        let corners = [inv.apply(vis.x0, vis.y0), inv.apply(vis.x1, vis.y0), inv.apply(vis.x1, vis.y1), inv.apply(vis.x0, vis.y1)];
        let fams: Vec<Family> = self.pattern.iter().filter_map(|pl| Family::new(pl, dot)).collect();
        // pass 1: which families become a tint
        let mut clear = 1.0f64; // part of the area left without ink by the tinted families
        let mut lines = Vec::with_capacity(fams.len());
        for f in &fams {
            let spacing_px = f.s.abs() * scale;
            let ink = (pen_px * f.duty / spacing_px).min(1.0);
            let too_dense = lod.enabled && spacing_px < lod.hatch_tint_px;
            let (ka, kb) = line_range(f, &corners, bbox);
            if too_dense || kb - ka > MAX_LINES {
                clear *= 1.0 - ink;
                w.stats.hatch_dense += 1;
            } else if kb >= ka {
                lines.push((f, ka, kb));
            }
        }
        if clear < 1.0 {
            let a = ((1.0 - clear) * st.color.a as f64).round() as u8;
            if a > 0 { w.out.fill_now(&self.path, m, st.color.with_alpha(a)); }
        }
        if lines.is_empty() { return; }
        // pass 2: real lines, clipped by a sweep over the boundary edges
        let mut polys = vec![];
        geom::flatten(&self.path, 0.3 / scale, &mut polys);
        let mut sweep = Sweep::default();
        for (f, k0, k1) in lines {
            sweep.edges(&polys, f, &corners);
            let period_px = f.period * scale;
            // dashes below the pixel: a continuous line with the same mean ink
            let min_period = if lod.enabled { 1.5 } else { 0.25 };
            let (dashed, style) = if f.period > 0.0 && period_px < min_period {
                let mut k = st.solid().with_width_px(pen_px);
                k.color = k.color.with_alpha((k.color.a as f64 * f.duty).round().max(1.0) as u8);
                (false, k)
            } else {
                (f.period > 0.0, st.solid().with_width_px(pen_px))
            };
            w.out.set_stroke(style);
            sweep.run(w, f, k0, k1, dashed, dot, m);
        }
    }
}

/// Range of line numbers k that cross the visible window and the boundary box `b`.
fn line_range(f: &Family, corners: &[(f64, f64); 4], b: &BBox) -> (i64, i64) {
    let (mut vlo, mut vhi) = (f64::INFINITY, f64::NEG_INFINITY);
    for &(x, y) in corners { let v = -x * f.sa + y * f.ca; vlo = vlo.min(v); vhi = vhi.max(v); }
    let (mut blo, mut bhi) = (f64::INFINITY, f64::NEG_INFINITY);
    for (x, y) in [(b.x0, b.y0), (b.x1, b.y0), (b.x1, b.y1), (b.x0, b.y1)] { let v = -x * f.sa + y * f.ca; blo = blo.min(v); bhi = bhi.max(v); }
    let (vlo, vhi) = (vlo.max(blo), vhi.min(bhi));
    if vhi.is_nan() || vlo.is_nan() || vhi < vlo { return (0, -1); }
    let v0 = -f.pl.base[0] * f.sa + f.pl.base[1] * f.ca;
    let (ka, kb) = ((vlo - v0) / f.s, (vhi - v0) / f.s);
    let (lo, hi) = (ka.min(kb).ceil(), ka.max(kb).floor());
    if !(lo.abs() < 1e15 && hi.abs() < 1e15) { return (0, -1); }
    (lo as i64, hi as i64)
}

/// Scan-line clipper for one family: boundary edges sorted by v, the active
/// edges at each pattern line, and the crossings. Buffers are reused.
#[derive(Default)]
struct Sweep {
    /// v_lo, v_hi, u at v_lo, du/dv
    edges: Vec<(f64, f64, f64, f64)>,
    active: Vec<usize>,
    xs: Vec<f64>,
    /// visible u range
    ulo: f64,
    uhi: f64,
}

impl Sweep {
    fn edges(&mut self, polys: &[Vec<[f64; 2]>], f: &Family, corners: &[(f64, f64); 4]) {
        let (sa, ca) = (f.sa, f.ca);
        let (mut ulo, mut uhi, mut vlo, mut vhi) = (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
        for &(x, y) in corners {
            let (u, v) = (x * ca + y * sa, -x * sa + y * ca);
            ulo = ulo.min(u); uhi = uhi.max(u); vlo = vlo.min(v); vhi = vhi.max(v);
        }
        self.ulo = ulo;
        self.uhi = uhi;
        self.edges.clear();
        for poly in polys {
            let n = poly.len();
            for k in 0..n {
                let (a, b) = (poly[k], poly[(k + 1) % n]);
                let (ua, va, ub, vb) = (a[0] * ca + a[1] * sa, -a[0] * sa + a[1] * ca, b[0] * ca + b[1] * sa, -b[0] * sa + b[1] * ca);
                if va == vb { continue; }
                let (v0, v1, u0, u1) = if va < vb { (va, vb, ua, ub) } else { (vb, va, ub, ua) };
                if v1 < vlo || v0 > vhi { continue; }
                self.edges.push((v0, v1, u0, (u1 - u0) / (v1 - v0)));
            }
        }
        self.edges.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    }

    #[allow(clippy::too_many_arguments)]
    fn run<S: Sink>(&mut self, w: &mut Walker<'_, S>, f: &Family, k0: i64, k1: i64, dashed: bool, dot: f64, m: &Aff) {
        if self.edges.is_empty() { return; }
        let (sa, ca, s, pl) = (f.sa, f.ca, f.s, f.pl);
        let v0 = -pl.base[0] * sa + pl.base[1] * ca;
        let u0 = pl.base[0] * ca + pl.base[1] * sa;
        let du = pl.offset[0] * ca + pl.offset[1] * sa; // shift of the dash start per line
        self.active.clear();
        let mut ptr = 0;
        // sweep in increasing v, whatever the sign of the spacing
        let (kstart, kstep) = if s > 0.0 { (k0, 1i64) } else { (k1, -1i64) };
        for step in 0..=(k1 - k0) {
            let k = kstart + step * kstep;
            let v = v0 + k as f64 * s;
            while ptr < self.edges.len() && self.edges[ptr].0 <= v { self.active.push(ptr); ptr += 1; }
            let edges = &self.edges;
            self.active.retain(|&i| edges[i].1 > v);
            if self.active.len() < 2 { continue; }
            self.xs.clear();
            for &i in &self.active { let e = edges[i]; self.xs.push(e.2 + (v - e.0) * e.3); }
            self.xs.sort_unstable_by(|a, b| a.total_cmp(b));
            let org = u0 + k as f64 * du;
            for pair in self.xs.chunks_exact(2) {
                let (a, b) = (pair[0].max(self.ulo), pair[1].min(self.uhi));
                if b <= a { continue; }
                if !dashed { seg(w, a, b, v, sa, ca, m); continue; }
                dashes(w, &pl.dashes, f.period, org, a, b, v, sa, ca, dot, m);
            }
        }
    }
}

/// The dashes of one pattern line between u = a and u = b.
#[allow(clippy::too_many_arguments)]
fn dashes<S: Sink>(w: &mut Walker<'_, S>, list: &[f64], period: f64, org: f64, a: f64, b: f64, v: f64, sa: f64, ca: f64, dot: f64, m: &Aff) {
    let mut pos = a - (a - org).rem_euclid(period); // start of the period that holds a
    let mut guard = 0;
    while pos < b {
        for &d in list {
            if d > 0.0 {
                let (x0, x1) = (pos.max(a), (pos + d).min(b));
                if x1 > x0 { seg(w, x0, x1, v, sa, ca, m); }
            } else if d == 0.0 && pos >= a && pos <= b {
                seg(w, pos - dot * 0.5, pos + dot * 0.5, v, sa, ca, m);
            }
            pos += d.abs();
            if pos >= b { break; }
        }
        guard += 1;
        if guard > 200_000 { break; }
    }
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn seg<S: Sink>(w: &mut Walker<'_, S>, ua: f64, ub: f64, v: f64, sa: f64, ca: f64, m: &Aff) {
    let (x0, y0) = m.apply(ua * ca - v * sa, ua * sa + v * ca);
    let (x1, y1) = m.apply(ub * ca - v * sa, ub * sa + v * ca);
    w.out.seg(x0 as f32, y0 as f32, x1 as f32, y1 as f32);
    w.stats.hatch_lines += 1;
}
