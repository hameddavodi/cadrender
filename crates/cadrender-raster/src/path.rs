//! The path the walk builds between two `draw` calls, and curve flattening.
//! Stored as plain verbs and points (device pixels), reused from draw to draw,
//! so a draw allocates nothing once the buffers have grown.

pub const M: u8 = 0;
pub const L: u8 = 1;
pub const Q: u8 = 2;
pub const C: u8 = 3;
pub const Z: u8 = 4;

/// Largest distance (pixels) between a curve and its flattened lines.
pub const FLATTEN_TOL: f32 = 0.2;

#[derive(Default)]
pub struct Path {
    pub verbs: Vec<u8>,
    pub pts: Vec<[f32; 2]>,
}

impl Path {
    pub fn clear(&mut self) { self.verbs.clear(); self.pts.clear(); }
    pub fn is_empty(&self) -> bool { self.verbs.is_empty() }

    /// Every subpath as a polyline. `f(points, line, closed)`: `line[i]` is
    /// true when segment `i` (from point `i` to `i + 1`) is a straight segment
    /// of the path, false when it is a piece of a flattened curve.
    pub fn for_each_poly(&self, poly: &mut Poly, mut f: impl FnMut(&mut Poly)) {
        poly.clear();
        let mut i = 0;
        for &v in &self.verbs {
            match v {
                M => {
                    if poly.pts.len() > 1 { f(poly); }
                    poly.clear();
                    poly.pts.push(self.pts[i]);
                    i += 1;
                }
                L => { poly.start_if_empty(self.pts[i]); poly.push(self.pts[i], true); i += 1; }
                Q => {
                    let p0 = poly.last_or(self.pts[i]);
                    poly.start_if_empty(p0);
                    flatten_quad(p0, self.pts[i], self.pts[i + 1], poly);
                    i += 2;
                }
                C => {
                    let p0 = poly.last_or(self.pts[i]);
                    poly.start_if_empty(p0);
                    flatten_cubic(p0, self.pts[i], self.pts[i + 1], self.pts[i + 2], poly);
                    i += 3;
                }
                _ => {
                    if poly.pts.len() > 1 {
                        let (a, b) = (poly.pts[0], *poly.pts.last().unwrap());
                        if a != b { poly.push(a, true); }
                        poly.closed = true;
                        f(poly);
                    }
                    // a new subpath starts at the same first point
                    let first = poly.pts.first().copied();
                    poly.clear();
                    if let Some(p) = first { poly.pts.push(p); }
                }
            }
        }
        if poly.pts.len() > 1 { f(poly); }
    }

    /// Bounding box `[x0, y0, x1, y1]` of all points (control points included).
    pub fn bounds(&self) -> [f32; 4] {
        let mut b = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
        for p in &self.pts { b[0] = b[0].min(p[0]); b[1] = b[1].min(p[1]); b[2] = b[2].max(p[0]); b[3] = b[3].max(p[1]); }
        b
    }
}

#[derive(Default)]
pub struct Poly {
    pub pts: Vec<[f32; 2]>,
    pub line: Vec<bool>,
    pub closed: bool,
}

impl Poly {
    fn clear(&mut self) { self.pts.clear(); self.line.clear(); self.closed = false; }
    fn start_if_empty(&mut self, p: [f32; 2]) { if self.pts.is_empty() { self.pts.push(p); } }
    fn last_or(&self, p: [f32; 2]) -> [f32; 2] { self.pts.last().copied().unwrap_or(p) }
    #[inline]
    fn push(&mut self, p: [f32; 2], line: bool) { self.pts.push(p); self.line.push(line); }
}

#[inline]
fn steps(dd: f32) -> usize { if dd.is_nan() || dd <= 0.0 { 1 } else { (dd.sqrt().ceil() as usize).clamp(1, 500) } }

fn flatten_quad(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], out: &mut Poly) {
    let dx = p0[0] - 2.0 * p1[0] + p2[0];
    let dy = p0[1] - 2.0 * p1[1] + p2[1];
    // error of n chords <= |p0 - 2 p1 + p2| / (4 n^2)
    let n = steps((dx * dx + dy * dy).sqrt() / (4.0 * FLATTEN_TOL));
    for k in 1..=n {
        let t = k as f32 / n as f32;
        let u = 1.0 - t;
        let (a, b, c) = (u * u, 2.0 * u * t, t * t);
        out.push([a * p0[0] + b * p1[0] + c * p2[0], a * p0[1] + b * p1[1] + c * p2[1]], false);
    }
}

fn flatten_cubic(p0: [f32; 2], p1: [f32; 2], p2: [f32; 2], p3: [f32; 2], out: &mut Poly) {
    let d = |a: [f32; 2], b: [f32; 2], c: [f32; 2]| { let (x, y) = (a[0] - 2.0 * b[0] + c[0], a[1] - 2.0 * b[1] + c[1]); (x * x + y * y).sqrt() };
    // error of n chords <= 3/4 * max|second difference| / n^2
    let n = steps(0.75 * d(p0, p1, p2).max(d(p1, p2, p3)) / FLATTEN_TOL);
    for k in 1..=n {
        let t = k as f32 / n as f32;
        let u = 1.0 - t;
        let (a, b, c, e) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
        out.push([a * p0[0] + b * p1[0] + c * p2[0] + e * p3[0], a * p0[1] + b * p1[1] + c * p2[1] + e * p3[1]], false);
    }
}
