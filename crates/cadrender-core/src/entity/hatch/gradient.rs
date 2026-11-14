//! Gradient fills: the 9 AutoCAD gradient types. A sink with
//! `Sink::GRADIENTS` (SVG) gets one smooth gradient (`Sink::fill_gradient`).
//! The others (PNG) get many thin bands of solid colour, found by clipping
//! the boundary polygons with straight strips (linear kinds) or with circles
//! (spherical kinds).
//!
//! AutoCAD does not publish its gradient formulas. The shapes here follow the
//! pictures in the AutoCAD hatch dialog: colour 1 at the "dark" side, colour 2
//! at the light centre or side; "shifted" moves the light towards the upper
//! left (a quarter of the size).

use crate::color::{EntColor, Rgba};
use crate::geom::{self, Aff, BBox, PathData};
use crate::render::walk::Walker;
use crate::sink::{GradientPaint, GradientShape, Sink};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GradientKind { Linear, Cylinder, InvCylinder, Spherical, InvSpherical, Hemispherical, InvHemispherical, Curved, InvCurved }

impl GradientKind {
    /// From the DXF name (group 470), with or without the `GR_` prefix.
    pub fn from_name(s: &str) -> Option<GradientKind> {
        let s = s.trim().to_ascii_uppercase();
        let s = s.strip_prefix("GR_").unwrap_or(&s);
        Some(match s {
            "LINEAR" => GradientKind::Linear,
            "CYLINDER" => GradientKind::Cylinder,
            "INVCYLINDER" => GradientKind::InvCylinder,
            "SPHERICAL" => GradientKind::Spherical,
            "INVSPHERICAL" => GradientKind::InvSpherical,
            "HEMISPHERICAL" => GradientKind::Hemispherical,
            "INVHEMISPHERICAL" => GradientKind::InvHemispherical,
            "CURVED" => GradientKind::Curved,
            "INVCURVED" => GradientKind::InvCurved,
            _ => return None,
        })
    }
    fn radial(self) -> bool { matches!(self, GradientKind::Spherical | GradientKind::InvSpherical | GradientKind::Hemispherical | GradientKind::InvHemispherical) }
}

/// A gradient fill (DXF groups 450-470).
#[derive(Clone, Debug, PartialEq)]
pub struct Gradient {
    pub kind: GradientKind,
    /// Colour 1 and colour 2 (ACI or true colour).
    pub colors: [EntColor; 2],
    /// `false` if the file gave only one colour: colour 2 then comes from `tint`.
    pub two_colors: bool,
    /// Rotation in radians.
    pub angle: f64,
    /// 0 = centred, 1 = shifted.
    pub shift: f64,
    /// One-colour gradients: 0 = towards black, 0.5 = the colour, 1 = towards white.
    pub tint: f64,
}

fn mix(a: Rgba, b: Rgba, t: f64) -> Rgba {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f64 + (y as f64 - x as f64) * t).round() as u8;
    Rgba { r: m(a.r, b.r), g: m(a.g, b.g), b: m(a.b, b.b), a: m(a.a, b.a) }
}

/// Colour 2 of a one-colour gradient.
pub fn tint_color(c: Rgba, tint: f64) -> Rgba {
    if tint < 0.5 { mix(c, Rgba::BLACK.with_alpha(c.a), 1.0 - 2.0 * tint) } else { mix(c, Rgba::WHITE.with_alpha(c.a), 2.0 * tint - 1.0) }
}

/// Mix factor (0 = colour 1, 1 = colour 2) along the band coordinate `t` in 0..1
/// (linear kinds) or the distance `t` in 0..1 from the light centre (radial kinds).
fn profile(kind: GradientKind, t: f64, c: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    match kind {
        GradientKind::Linear => if t < c { 0.5 * t / c } else { 0.5 + 0.5 * (t - c) / (1.0 - c) },
        GradientKind::Cylinder => 1.0 - (t - c).abs() / c.max(1.0 - c),
        GradientKind::InvCylinder => (t - c).abs() / c.max(1.0 - c),
        GradientKind::Curved => 1.0 - (1.0 - t) * (1.0 - t),
        GradientKind::InvCurved => (1.0 - t) * (1.0 - t),
        GradientKind::Spherical | GradientKind::Hemispherical => 1.0 - t,
        GradientKind::InvSpherical | GradientKind::InvHemispherical => t,
    }
}

/// Keep the part of `poly` with `n . p >= c` (one Sutherland-Hodgman step).
fn clip_half(poly: &[[f64; 2]], n: [f64; 2], c: f64, out: &mut Vec<[f64; 2]>) {
    out.clear();
    let k = poly.len();
    if k == 0 { return; }
    let side = |p: [f64; 2]| n[0] * p[0] + n[1] * p[1] - c;
    let mut a = poly[k - 1];
    let mut sa = side(a);
    for &b in poly {
        let sb = side(b);
        if (sa >= 0.0) != (sb >= 0.0) {
            let t = sa / (sa - sb);
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
        }
        if sb >= 0.0 { out.push(b); }
        a = b;
        sa = sb;
    }
}

/// Clip every ring with a list of half planes (a convex region). Clipping
/// each ring on its own keeps the even-odd result right inside the region.
fn clip_rings(rings: &[Vec<[f64; 2]>], planes: &[([f64; 2], f64)], path: &mut PathData) {
    let (mut a, mut b) = (Vec::new(), Vec::new());
    for r in rings {
        a.clear();
        a.extend_from_slice(r);
        for &(n, c) in planes {
            clip_half(&a, n, c, &mut b);
            std::mem::swap(&mut a, &mut b);
            if a.len() < 3 { break; }
        }
        if a.len() < 3 { continue; }
        path.move_to(a[0][0], a[0][1]);
        for p in &a[1..] { path.line_to(p[0], p[1]); }
        path.close();
    }
}

/// Light centre of a radial kind in the (axis, up) frame of the boundary box
/// `fb`, and the distance from it to the farthest corner.
fn light_centre(kind: GradientKind, fb: &BBox, shift: f64) -> (f64, f64, f64) {
    let hemi = matches!(kind, GradientKind::Hemispherical | GradientKind::InvHemispherical);
    let cu = fb.x0 + fb.w() * (0.5 - shift);
    let cv = if hemi { fb.y0 } else { fb.y0 + fb.h() * (0.5 + shift) };
    let rmax = [(fb.x0, fb.y0), (fb.x1, fb.y0), (fb.x0, fb.y1), (fb.x1, fb.y1)].iter()
        .map(|&(x, y)| ((x - cu).powi(2) + (y - cv).powi(2)).sqrt()).fold(0.0, f64::max).max(1e-9);
    (cu, cv, rmax)
}

/// Give the gradient to a sink that draws smooth gradients (SVG). Shape and
/// stops follow the same profile as the bands, so both outputs look alike:
/// linear and cylinder profiles are exact with 3 stops, spheres with 2, the
/// curved ones get 9. `cut` is the boundary in device pixels; `a`/`b` are
/// the axis and "up" directions of the gradient, `fb` the boundary box in
/// that frame. Returns `false` if the sink said no (then nothing was drawn).
#[allow(clippy::too_many_arguments)]
fn smooth<S: Sink>(g: &Gradient, cut: &PathData, a: [f64; 2], b: [f64; 2], fb: &BBox, shift: f64, c1: Rgba, c2: Rgba, w: &mut Walker<'_, S>) -> bool {
    let pt = |u: f64, v: f64| [(u * a[0] + v * b[0]) as f32, (u * a[1] + v * b[1]) as f32];
    let (shape, offsets, c) = if g.kind.radial() {
        let (cu, cv, r) = light_centre(g.kind, fb, shift);
        (GradientShape::Radial { c: pt(cu, cv), r: r as f32 }, vec![0.0, 1.0], 0.5)
    } else if matches!(g.kind, GradientKind::Curved | GradientKind::InvCurved) {
        // curved: along "up"; a point on the "up" line at height v is v * b
        let shape = GradientShape::Linear { p0: pt(0.0, fb.y0), p1: pt(0.0, fb.y1) };
        (shape, (0..=8).map(|i| i as f64 / 8.0).collect(), 0.5)
    } else {
        let c = 0.5 - shift;
        (GradientShape::Linear { p0: pt(fb.x0, 0.0), p1: pt(fb.x1, 0.0) }, vec![0.0, c, 1.0], c)
    };
    let stops: Vec<(f32, Rgba)> = offsets.iter().map(|&t| (t as f32, mix(c1, c2, profile(g.kind, t, c)))).collect();
    w.out.flush(); // keep the draw order
    let s = &mut w.out.sink;
    let mut i = 0;
    for &verb in &cut.verbs {
        match verb {
            geom::M => { s.move_to(cut.pts[i][0] as f32, cut.pts[i][1] as f32); i += 1; }
            geom::L => { s.line_to(cut.pts[i][0] as f32, cut.pts[i][1] as f32); i += 1; }
            _ => s.close(), // `clip_rings` makes only moves, lines and closes
        }
    }
    s.fill_gradient(&GradientPaint { shape, stops: &stops })
}

/// Draw `path` (local units, matrix `m`) with gradient `g` from `c1` to `c2`.
pub(super) fn draw<S: Sink>(g: &Gradient, path: &PathData, m: &Aff, c1: Rgba, c2: Rgba, w: &mut Walker<'_, S>) {
    // boundary in device pixels, cut to the visible window
    let mut rings = vec![];
    geom::flatten(path, 0.25 / m.scale_factor().max(1e-300), &mut rings);
    for r in &mut rings { for p in r.iter_mut() { let (x, y) = m.apply(p[0], p[1]); *p = [x, y]; } }
    let v = w.clip.grow(2.0);
    let win = [([1.0, 0.0], v.x0), ([-1.0, 0.0], -v.x1), ([0.0, 1.0], v.y0), ([0.0, -1.0], -v.y1)];
    let mut cut = PathData::default();
    clip_rings(&rings, &win, &mut cut);
    if cut.is_empty() { return; }
    let mut vis = vec![];
    geom::flatten(&cut, 1.0, &mut vis);
    // gradient frame: the angle turns with the entity; device y points down
    // axis and "up" of the gradient, as device directions
    let (sa, ca) = g.angle.sin_cos();
    let unit = |x: f64, y: f64| { let [a, b, ..] = m.0; let [_, _, c, d, ..] = m.0; let (dx, dy) = (a * x + c * y, b * x + d * y); let l = (dx * dx + dy * dy).sqrt().max(1e-300); (dx / l, dy / l) };
    let (ax, ay) = unit(ca, sa);
    let (bx, by) = unit(-sa, ca);
    let mut fb = BBox::EMPTY; // whole boundary in the (axis, up) frame
    for r in &rings { for p in r { fb.add(p[0] * ax + p[1] * ay, p[0] * bx + p[1] * by); } }
    if fb.is_empty() { return; }
    let shift = g.shift.clamp(0.0, 1.0) * 0.25;
    if S::GRADIENTS && smooth(g, &cut, [ax, ay], [bx, by], &fb, shift, c1, c2, w) {
        w.stats.drawn += 1;
        return;
    }
    let opaque = c1.a == 255 && c2.a == 255;
    let ext = fb.w().max(fb.h());
    let diff = [c1.r.abs_diff(c2.r), c1.g.abs_diff(c2.g), c1.b.abs_diff(c2.b), c1.a.abs_diff(c2.a)].into_iter().max().unwrap_or(0) as f64;
    let n = ((ext / 3.0).ceil().min(diff + 1.0)).clamp(1.0, 96.0) as usize;
    let mut band = PathData::default();
    if !g.kind.radial() {
        // straight bands along the axis (linear, cylinder, curved: curved runs along "up")
        let curved = matches!(g.kind, GradientKind::Curved | GradientKind::InvCurved);
        let (dir, lo, hi) = if curved { ([bx, by], fb.y0, fb.y1) } else { ([ax, ay], fb.x0, fb.x1) };
        let len = (hi - lo).max(1e-9);
        let centre = 0.5 - shift;
        for i in 0..n {
            let (t0, t1) = (i as f64 / n as f64, (i + 1) as f64 / n as f64);
            let col = mix(c1, c2, profile(g.kind, (t0 + t1) * 0.5, centre));
            band.verbs.clear(); band.pts.clear();
            // opaque: each band covers everything after it (no seams); else exact strips
            let mut planes = vec![(dir, lo + t0 * len - if i == 0 { 1e9 } else { 0.0 })];
            if !opaque && i + 1 < n { planes.push(([-dir[0], -dir[1]], -(lo + t1 * len))); }
            clip_rings(&vis, &planes, &mut band);
            if !band.is_empty() { w.out.fill_now(&band, &Aff::ID, col); }
        }
    } else {
        // circles around the light centre, drawn from the outside in
        let (cu, cv, rmax) = light_centre(g.kind, &fb, shift);
        let (cx, cy) = (cu * ax + cv * bx, cu * ay + cv * by);
        let sides = 72;
        for i in (0..n).rev() {
            let (t0, t1) = (i as f64 / n as f64, (i + 1) as f64 / n as f64);
            let col = mix(c1, c2, profile(g.kind, (t0 + t1) * 0.5, 0.5));
            band.verbs.clear(); band.pts.clear();
            let disc = |r: f64| -> Vec<([f64; 2], f64)> {
                (0..sides).map(|k| {
                    let a = std::f64::consts::TAU * (k as f64 + 0.5) / sides as f64;
                    let nrm = [-a.cos(), -a.sin()];
                    (nrm, nrm[0] * cx + nrm[1] * cy - r)
                }).collect()
            };
            if i + 1 == n { clip_rings(&vis, &[], &mut band); } else { clip_rings(&vis, &disc(t1 * rmax), &mut band); }
            if !opaque && i > 0 {
                // exact ring: the inner disc again, so even-odd cuts it out
                clip_rings(&vis, &disc(t0 * rmax), &mut band);
            }
            if !band.is_empty() { w.out.fill_now(&band, &Aff::ID, col); }
        }
    }
    w.stats.drawn += 1;
}
