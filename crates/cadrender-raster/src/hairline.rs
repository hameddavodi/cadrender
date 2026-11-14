//! Thin lines (up to `HAIR_MAX` pixels), drawn straight into the canvas.
//!
//! This is where most of the time of a CAD picture goes, so it is a small,
//! direct scanline drawer instead of a general path filler:
//!
//! - **Exact thickness.** For a line that is mostly horizontal, each pixel
//!   column gets the vertical cut through the line (`w / cos`), and each pixel
//!   in that column gets the part of the cut inside it. So a line puts `w`
//!   pixels of ink per pixel of length, at any angle.
//! - **Snapping.** A straight segment that is horizontal or vertical (within
//!   `SNAP_TOL`) is moved by at most half a pixel so that it covers whole
//!   pixel rows: centre of a pixel for odd widths, pixel edge for even widths.
//!   A 1 px line is then one dark row, not two half-grey rows. Curves are never
//!   moved, and a curve end that touches a snapped point moves with it, so
//!   joints stay closed.
//! - **Ink curve.** Partly covered pixels get a little more ink
//!   (`ink = 1 - (1 - coverage)^INK_GAMMA`), so lines at an angle look as dark
//!   as snapped ones and anti-aliased edges do not look washed out.
//! - **Joints.** Inside a polyline each pixel column (or row) belongs to one
//!   segment only, so joints are not drawn twice (no dark or light beads on
//!   flattened arcs). Open ends get a square cap of half the width (like
//!   matplotlib's default), so lines that meet at a corner close it.

use crate::canvas::{Blit, Px};
use crate::path::Poly;

/// Widest line drawn here. Wider lines go through the general stroker.
pub const HAIR_MAX: f32 = 2.0;
/// Largest offset (pixels) between the two ends of a segment that is still
/// treated as horizontal or vertical for snapping.
pub const SNAP_TOL: f32 = 0.25;
/// Exponent of the ink curve (1 = plain coverage).
pub const INK_GAMMA: f32 = 1.6;
/// Lines asked thinner than 1 px are drawn 1 px wide with this much ink at least.
pub const MIN_INK: f32 = 0.5;

/// Coverage (0..=255) to ink (0..=255).
pub fn ink_lut() -> &'static [u8; 256] {
    static LUT: std::sync::OnceLock<[u8; 256]> = std::sync::OnceLock::new();
    LUT.get_or_init(|| {
        let mut t = [0u8; 256];
        for (i, v) in t.iter_mut().enumerate() {
            let c = i as f32 / 255.0;
            *v = ((1.0 - (1.0 - c).powf(INK_GAMMA)) * 255.0).round().clamp(0.0, 255.0) as u8;
        }
        t
    })
}

/// Line settings for one draw call.
#[derive(Clone, Copy)]
pub struct Hair {
    /// Thickness in pixels (at least 1).
    pub w: f32,
    /// Snap to pixel centres (odd widths) or pixel edges (even widths).
    pub centre: bool,
    /// Ink scale 0..=256 (256 = full), for lines asked thinner than 1 px.
    pub ink: u32,
}

impl Hair {
    pub fn new(width: f32) -> Hair {
        let w = width.max(1.0);
        let ink = if width >= 1.0 { 256 } else { (width.max(MIN_INK) * 256.0) as u32 };
        Hair { w, centre: (w.round() as i32) % 2 == 1, ink }
    }
    #[inline]
    fn snap(&self, v: f32) -> f32 { if self.centre { v.floor() + 0.5 } else { v.round() } }
}

/// Snap the points of `p` that touch a horizontal or vertical straight segment.
pub fn snap_poly(p: &mut Poly, h: &Hair) {
    let n = p.pts.len();
    if n < 2 { return; }
    // which coordinate of each point to snap: bit 0 = y (horizontal segment), bit 1 = x
    let mut flags = [0u8; 64];
    let mut big: Vec<u8>;
    let f: &mut [u8] = if n <= 64 { &mut flags[..n] } else { big = vec![0; n]; &mut big };
    for i in 0..n - 1 {
        if !p.line[i] { continue; }
        let (a, b) = (p.pts[i], p.pts[i + 1]);
        let (dx, dy) = ((b[0] - a[0]).abs(), (b[1] - a[1]).abs());
        let bit = if dy <= SNAP_TOL && dx > dy { 1 } else if dx <= SNAP_TOL && dy > dx { 2 } else { 0 };
        f[i] |= bit;
        f[i + 1] |= bit;
    }
    if p.closed && p.pts[0] == p.pts[n - 1] { let b = f[0] | f[n - 1]; f[0] = b; f[n - 1] = b; }
    for (q, &b) in p.pts.iter_mut().zip(f.iter()) {
        if b & 1 != 0 { q[1] = h.snap(q[1]); }
        if b & 2 != 0 { q[0] = h.snap(q[0]); }
    }
}

/// Draw one segment. `open0` / `open1`: that end is a real line end (its
/// end pixel gets the covered part only); otherwise it is a joint and the
/// pixel columns are split between the two segments by their centres.
#[inline]
pub fn segment<P: Px>(b: &mut Blit<P>, p0: [f32; 2], p1: [f32; 2], h: &Hair, open0: bool, open1: bool) {
    let (dx, dy) = ((p1[0] - p0[0]).abs(), (p1[1] - p0[1]).abs());
    if dx >= dy {
        major::<P, false>(b, p0[0], p0[1], p1[0], p1[1], h, open0, open1);
    } else {
        major::<P, true>(b, p0[1], p0[0], p1[1], p1[0], h, open0, open1);
    }
}

/// `u` is the main direction of the segment, `v` the other one. With
/// `T = true`, `u` is y (a mostly vertical segment).
#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn major<P: Px, const T: bool>(b: &mut Blit<P>, mut u0: f32, mut v0: f32, mut u1: f32, mut v1: f32, h: &Hair, mut open0: bool, mut open1: bool) {
    if u0 > u1 {
        std::mem::swap(&mut u0, &mut u1);
        std::mem::swap(&mut v0, &mut v1);
        std::mem::swap(&mut open0, &mut open1);
    }
    let du = u1 - u0;
    if du <= 1e-6 || !du.is_finite() || !v0.is_finite() || !v1.is_finite() { return; }
    let k = (v1 - v0) / du;
    let half = 0.5 * h.w * (1.0 + k * k).sqrt();
    let (umax, vmax) = if T { (b.height(), b.width()) } else { (b.width(), b.height()) };
    // pixel columns: open ends take every column they touch, joints take the
    // columns whose centre is on this side
    let lo = if open0 { u0.floor() } else { (u0 - 0.5).ceil() };
    let hi = if open1 { u1.ceil() - 1.0 } else { (u1 - 0.5).ceil() - 1.0 };
    let i0 = lo.max(0.0) as i32;
    let i1 = hi.min(umax as f32 - 1.0);
    if i1 < i0 as f32 { return; }
    let i1 = i1 as i32;
    let lut = ink_lut();
    let ink = |c: f32| -> u32 {
        let c8 = lut[((c * 255.0 + 0.5) as usize).min(255)] as u32;
        if h.ink == 256 { c8 } else { (c8 * h.ink) >> 8 }
    };
    if k == 0.0 {
        // exactly horizontal (or vertical): every column is the same, so draw
        // runs: the end columns, then one run per pixel row in between
        let (top, bot) = (v0 - half, v0 + half);
        let fend = |i: i32| -> f32 {
            let fi = i as f32;
            let mut f = 1.0f32;
            if open0 && fi < u0 { f -= u0 - fi; }
            if open1 && fi + 1.0 > u1 { f -= fi + 1.0 - u1; }
            f
        };
        let (f0, f1) = (fend(i0), fend(i1));
        let (m0, m1) = (if f0 < 1.0 { i0 + 1 } else { i0 }, if f1 < 1.0 && i1 > i0 { i1 } else { i1 + 1 });
        for r in top.floor() as i32..bot.ceil() as i32 {
            let fr = r as f32;
            let c = bot.min(fr + 1.0) - top.max(fr);
            if c <= 0.0 { continue; }
            if m0 > i0 { let e = ink(c * f0); if T { b.px(r, i0, e) } else { b.px(i0, r, e) } }
            if m1 <= i1 && i1 > i0 { let e = ink(c * f1); if T { b.px(r, i1, e) } else { b.px(i1, r, e) } }
            if m1 > m0 { let e = ink(c); if T { b.vspan(r, m0, m1, e) } else { b.span(m0, m1, r, e) } }
        }
        return;
    }
    // skip columns where the line is above or below the canvas
    let vlim = vmax as f32 + half + 1.0;
    for i in i0..=i1 {
        let fi = i as f32;
        let mut f = 1.0f32;
        if open0 && fi < u0 { f -= u0 - fi; }
        if open1 && fi + 1.0 > u1 { f -= fi + 1.0 - u1; }
        let uc = (fi + 0.5).clamp(u0, u1);
        let vc = v0 + (uc - u0) * k;
        if vc < -half - 1.0 || vc > vlim { continue; }
        let (top, bot) = (vc - half, vc + half);
        let r0 = top.floor() as i32;
        let r1 = bot.ceil() as i32;
        for r in r0..r1 {
            let fr = r as f32;
            let c = (bot.min(fr + 1.0) - top.max(fr)) * f;
            if c <= 0.0 { continue; }
            let c8 = ink(c);
            if T { b.px(r, i, c8) } else { b.px(i, r, c8) }
        }
    }
}

/// Stroke one polyline (already snapped), solid or dashed. `dash` is an
/// even-length list of on/off lengths in pixels.
pub fn stroke_poly<P: Px>(b: &mut Blit<P>, p: &Poly, h: &Hair, dash: Option<&[f32]>) {
    let n = p.pts.len();
    if n < 2 { return; }
    if let Some(d) = dash { if d.len() >= 2 && d.iter().all(|x| *x > 0.0 && x.is_finite()) { return dashed(b, p, h, d); } }
    let last = n - 2;
    let cap = 0.5 * h.w;
    for i in 0..=last {
        let (mut a, mut c) = (p.pts[i], p.pts[i + 1]);
        let (open0, open1) = (!p.closed && i == 0, !p.closed && i == last);
        if open0 || open1 {
            let (dx, dy) = (c[0] - a[0], c[1] - a[1]);
            let len = (dx * dx + dy * dy).sqrt();
            if len > 1e-6 {
                let (ex, ey) = (dx / len * cap, dy / len * cap);
                if open0 { a = [a[0] - ex, a[1] - ey]; }
                if open1 { c = [c[0] + ex, c[1] + ey]; }
            }
        }
        segment(b, a, c, h, open0, open1);
    }
}

/// Dashes: walk along the polyline, draw the "on" parts with plain ends.
fn dashed<P: Px>(b: &mut Blit<P>, p: &Poly, h: &Hair, d: &[f32]) {
    let (mut k, mut left) = (0usize, d[0]);
    for i in 0..p.pts.len() - 1 {
        let (a, c) = (p.pts[i], p.pts[i + 1]);
        let (dx, dy) = (c[0] - a[0], c[1] - a[1]);
        let len = (dx * dx + dy * dy).sqrt();
        if len <= 1e-6 || !len.is_finite() { continue; }
        let mut t = 0.0f32;
        while t < len {
            let step = left.min(len - t);
            if k % 2 == 0 {
                let (s0, s1) = (t / len, (t + step) / len);
                let q0 = [a[0] + dx * s0, a[1] + dy * s0];
                let q1 = [a[0] + dx * s1, a[1] + dy * s1];
                // a dash that goes on over a joint has no end there
                segment(b, q0, q1, h, t > 0.0 || left >= d[k], t + step < len || step >= left);
            }
            t += step;
            left -= step;
            if left <= 1e-6 { k = (k + 1) % d.len(); left = d[k]; }
        }
    }
}
