//! Small 2D helpers: affine matrix, bounding box, path storage.
//!
//! Everything here is `f64`. Coordinates stay 64-bit until the very last
//! step (the sink), so far-away drawings do not jitter.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aff(pub [f64; 6]); // x' = a x + c y + e ; y' = b x + d y + f

impl Aff {
    pub const ID: Aff = Aff([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    #[inline]
    pub fn translate(x: f64, y: f64) -> Aff { Aff([1.0, 0.0, 0.0, 1.0, x, y]) }
    pub fn scale(sx: f64, sy: f64) -> Aff { Aff([sx, 0.0, 0.0, sy, 0.0, 0.0]) }
    pub fn rotate(rad: f64) -> Aff {
        let (s, c) = rad.sin_cos();
        Aff([c, s, -s, c, 0.0, 0.0])
    }
    /// Shear along x (oblique text): x' = x + tan(angle) * y.
    pub fn shear_x(rad: f64) -> Aff { Aff([1.0, 0.0, rad.tan(), 1.0, 0.0, 0.0]) }
    /// self * o : apply `o` first, then `self`.
    #[inline]
    pub fn mul(&self, o: &Aff) -> Aff {
        let [a, b, c, d, e, f] = self.0;
        let [a2, b2, c2, d2, e2, f2] = o.0;
        Aff([
            a * a2 + c * b2, b * a2 + d * b2,
            a * c2 + c * d2, b * c2 + d * d2,
            a * e2 + c * f2 + e, b * e2 + d * f2 + f,
        ])
    }
    #[inline]
    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        let [a, b, c, d, e, f] = self.0;
        (a * x + c * y + e, b * x + d * y + f)
    }
    #[inline]
    pub fn det(&self) -> f64 { self.0[0] * self.0[3] - self.0[1] * self.0[2] }
    /// mean scale factor
    #[inline]
    pub fn scale_factor(&self) -> f64 { self.det().abs().sqrt() }
    pub fn inverse(&self) -> Aff {
        let [a, b, c, d, e, f] = self.0;
        let det = a * d - b * c;
        let id = if det.abs() < 1e-300 { 0.0 } else { 1.0 / det };
        let (ia, ib, ic, idd) = (d * id, -b * id, -c * id, a * id);
        Aff([ia, ib, ic, idd, -(ia * e + ic * f), -(ib * e + idd * f)])
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BBox { pub x0: f64, pub y0: f64, pub x1: f64, pub y1: f64 }

impl Default for BBox { fn default() -> Self { BBox::EMPTY } }

impl BBox {
    pub fn new(x0: f64, y0: f64, x1: f64, y1: f64) -> BBox { BBox { x0, y0, x1, y1 } }
    /// Box around a centre point with full width `w` and height `h`.
    pub fn centered(c: [f64; 2], w: f64, h: f64) -> BBox { BBox { x0: c[0] - w / 2.0, y0: c[1] - h / 2.0, x1: c[0] + w / 2.0, y1: c[1] + h / 2.0 } }
    #[inline]
    pub fn grow(&self, d: f64) -> BBox { if self.is_empty() { *self } else { BBox { x0: self.x0 - d, y0: self.y0 - d, x1: self.x1 + d, y1: self.y1 + d } } }
    pub const EMPTY: BBox = BBox { x0: f64::INFINITY, y0: f64::INFINITY, x1: f64::NEG_INFINITY, y1: f64::NEG_INFINITY };
    #[inline]
    pub fn is_empty(&self) -> bool { !(self.x0 <= self.x1 && self.y0 <= self.y1) }
    #[inline]
    pub fn add(&mut self, x: f64, y: f64) {
        if x.is_finite() && y.is_finite() {
            if x < self.x0 { self.x0 = x }
            if x > self.x1 { self.x1 = x }
            if y < self.y0 { self.y0 = y }
            if y > self.y1 { self.y1 = y }
        }
    }
    #[inline]
    pub fn union(&mut self, o: &BBox) {
        if !o.is_empty() { self.add(o.x0, o.y0); self.add(o.x1, o.y1); }
    }
    #[inline]
    pub fn transformed(&self, m: &Aff) -> BBox {
        if self.is_empty() { return *self; }
        // centre and half size: exact box of the transformed rectangle, no branches
        let [a, b, c, d, e, f] = m.0;
        let (cx, cy) = ((self.x0 + self.x1) * 0.5, (self.y0 + self.y1) * 0.5);
        let (hx, hy) = ((self.x1 - self.x0) * 0.5, (self.y1 - self.y0) * 0.5);
        let (nx, ny) = (a * cx + c * cy + e, b * cx + d * cy + f);
        let (ex, ey) = (a.abs() * hx + c.abs() * hy, b.abs() * hx + d.abs() * hy);
        BBox { x0: nx - ex, y0: ny - ey, x1: nx + ex, y1: ny + ey }
    }
    /// `o` lies fully inside this box.
    #[inline]
    pub fn contains(&self, o: &BBox) -> bool { self.x0 <= o.x0 && self.y0 <= o.y0 && self.x1 >= o.x1 && self.y1 >= o.y1 }
    #[inline]
    pub fn intersect(&self, o: &BBox) -> BBox {
        BBox { x0: self.x0.max(o.x0), y0: self.y0.max(o.y0), x1: self.x1.min(o.x1), y1: self.y1.min(o.y1) }
    }
    #[inline]
    pub fn w(&self) -> f64 { self.x1 - self.x0 }
    #[inline]
    pub fn h(&self) -> f64 { self.y1 - self.y0 }
}

pub const M: u8 = 0;
pub const L: u8 = 1;
pub const Q: u8 = 2;
pub const C: u8 = 3;
pub const Z: u8 = 4;

/// A path in local (entity) coordinates, stored once and re-used for every draw.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct PathData { pub verbs: Vec<u8>, pub pts: Vec<[f64; 2]> }

impl PathData {
    pub fn move_to(&mut self, x: f64, y: f64) { self.verbs.push(M); self.pts.push([x, y]); }
    pub fn line_to(&mut self, x: f64, y: f64) { self.verbs.push(L); self.pts.push([x, y]); }
    pub fn quad_to(&mut self, x1: f64, y1: f64, x: f64, y: f64) { self.verbs.push(Q); self.pts.push([x1, y1]); self.pts.push([x, y]); }
    pub fn cubic_to(&mut self, x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64) {
        self.verbs.push(C); self.pts.push([x1, y1]); self.pts.push([x2, y2]); self.pts.push([x, y]);
    }
    pub fn close(&mut self) { self.verbs.push(Z); }
    pub fn is_empty(&self) -> bool { self.pts.len() < 2 }
    pub fn last(&self) -> Option<[f64; 2]> { self.pts.last().copied() }
    pub fn bbox(&self) -> BBox {
        let mut b = BBox::EMPTY;
        for p in &self.pts { b.add(p[0], p[1]); }
        b
    }
    /// Arc around (cx,cy), from angle a0 with signed sweep (radians), as cubic
    /// Bezier curves. `rx_axis`/`ry_axis` are the two half axes (ellipse support).
    /// If `connect` the first point is reached with line_to, else move_to.
    #[allow(clippy::too_many_arguments)]
    pub fn arc(&mut self, cx: f64, cy: f64, ax: [f64; 2], ay: [f64; 2], a0: f64, sweep: f64, connect: bool) {
        let n = ((sweep.abs() / std::f64::consts::FRAC_PI_2).ceil() as usize).clamp(1, 64);
        let d = sweep / n as f64;
        let k = 4.0 / 3.0 * (d / 4.0).tan();
        let pt = |c: f64, s: f64| (cx + ax[0] * c + ay[0] * s, cy + ax[1] * c + ay[1] * s);
        let (s0, c0) = a0.sin_cos();
        let (x, y) = pt(c0, s0);
        if connect {
            match self.last() {
                Some(p) if (p[0] - x).abs() + (p[1] - y).abs() < 1e-9 => {}
                Some(_) => self.line_to(x, y),
                None => self.move_to(x, y),
            }
        } else { self.move_to(x, y); }
        let mut a = a0;
        for _ in 0..n {
            let (s1, c1) = a.sin_cos();
            let (s2, c2) = (a + d).sin_cos();
            let (x1, y1) = pt(c1 - k * s1, s1 + k * c1);
            let (x2, y2) = pt(c2 + k * s2, s2 - k * c2);
            let (x3, y3) = pt(c2, s2);
            self.cubic_to(x1, y1, x2, y2, x3, y3);
            a += d;
        }
    }
    /// Polyline segment with a DXF bulge from the current point to (x,y).
    pub fn bulge_to(&mut self, x: f64, y: f64, bulge: f64) {
        let Some([x0, y0]) = self.last() else { self.move_to(x, y); return; };
        if bulge.abs() < 1e-9 { self.line_to(x, y); return; }
        let (dx, dy) = (x - x0, y - y0);
        let chord = (dx * dx + dy * dy).sqrt();
        if chord < 1e-12 { return; }
        let theta = 4.0 * bulge.atan();
        let r = chord / (2.0 * (theta / 2.0).sin().abs());
        // centre: left of the chord for positive bulge
        let h = (r * r - chord * chord / 4.0).max(0.0).sqrt();
        let (mx, my) = ((x0 + x) / 2.0, (y0 + y) / 2.0);
        let (nx, ny) = (-dy / chord, dx / chord);
        let sgn = if (bulge > 0.0) == (bulge.abs() <= 1.0) { 1.0 } else { -1.0 };
        let (cx, cy) = (mx + nx * h * sgn, my + ny * h * sgn);
        let a0 = (y0 - cy).atan2(x0 - cx);
        self.arc(cx, cy, [r, 0.0], [0.0, r], a0, theta, true);
    }
    pub fn transform(&mut self, m: &Aff) {
        for p in &mut self.pts { let (x, y) = m.apply(p[0], p[1]); *p = [x, y]; }
    }
}

/// Flatten a path to closed polygons (for hatch pattern clipping). `tol` in local units.
pub fn flatten(p: &PathData, tol: f64, out: &mut Vec<Vec<[f64; 2]>>) {
    let mut cur: Vec<[f64; 2]> = Vec::new();
    let mut i = 0;
    for &v in &p.verbs {
        match v {
            M => { if cur.len() > 1 { out.push(std::mem::take(&mut cur)); } cur.clear(); cur.push(p.pts[i]); i += 1; }
            L => { cur.push(p.pts[i]); i += 1; }
            Q => {
                let p0 = *cur.last().unwrap_or(&p.pts[i]);
                let (p1, p2) = (p.pts[i], p.pts[i + 1]);
                let n = segs(&[p0, p1, p2], tol);
                for k in 1..=n {
                    let t = k as f64 / n as f64; let u = 1.0 - t;
                    cur.push([u * u * p0[0] + 2.0 * u * t * p1[0] + t * t * p2[0], u * u * p0[1] + 2.0 * u * t * p1[1] + t * t * p2[1]]);
                }
                i += 2;
            }
            C => {
                let p0 = *cur.last().unwrap_or(&p.pts[i]);
                let (p1, p2, p3) = (p.pts[i], p.pts[i + 1], p.pts[i + 2]);
                let n = segs(&[p0, p1, p2, p3], tol);
                for k in 1..=n {
                    let t = k as f64 / n as f64; let u = 1.0 - t;
                    let (b0, b1, b2, b3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                    cur.push([b0 * p0[0] + b1 * p1[0] + b2 * p2[0] + b3 * p3[0], b0 * p0[1] + b1 * p1[1] + b2 * p2[1] + b3 * p3[1]]);
                }
                i += 3;
            }
            _ => { if cur.len() > 1 { out.push(std::mem::take(&mut cur)); } }
        }
    }
    if cur.len() > 1 { out.push(cur); }
}

fn segs(ctrl: &[[f64; 2]], tol: f64) -> usize {
    // control polygon length based estimate
    let mut len = 0.0;
    for w in ctrl.windows(2) { len += ((w[1][0] - w[0][0]).powi(2) + (w[1][1] - w[0][1]).powi(2)).sqrt(); }
    ((len / tol.max(1e-12)).sqrt() * 0.6).ceil().clamp(2.0, 64.0) as usize
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: (f64, f64), b: (f64, f64)) -> bool { (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9 }

    #[test]
    fn matrix_order_and_inverse() {
        let m = Aff::translate(10.0, 0.0).mul(&Aff::scale(2.0, 2.0));
        assert!(close(m.apply(1.0, 1.0), (12.0, 2.0)));
        let back = m.inverse().apply(12.0, 2.0);
        assert!(close(back, (1.0, 1.0)));
        assert!((m.scale_factor() - 2.0).abs() < 1e-12);
    }

    #[test]
    fn large_coordinates_keep_precision() {
        // 5 km away in mm, a 0.01 mm detail must survive the round trip
        let m = Aff::translate(-5_000_000.0, -5_000_000.0);
        let (x, _) = m.apply(5_000_000.01, 5_000_000.0);
        assert!((x - 0.01).abs() < 1e-6);
    }

    #[test]
    fn bbox_empty_and_union() {
        let mut b = BBox::EMPTY;
        assert!(b.is_empty());
        b.add(1.0, 2.0);
        b.add(f64::NAN, 5.0);
        b.add(3.0, -1.0);
        assert_eq!((b.x0, b.y0, b.x1, b.y1), (1.0, -1.0, 3.0, 2.0));
        let r = b.transformed(&Aff::rotate(std::f64::consts::FRAC_PI_2));
        assert!((r.w() - 3.0).abs() < 1e-9 && (r.h() - 2.0).abs() < 1e-9);
    }

    #[test]
    fn bulge_half_circle() {
        let mut p = PathData::default();
        p.move_to(0.0, 0.0);
        p.bulge_to(2.0, 0.0, 1.0); // half circle, radius 1, centre (1,0), goes below (ccw)
        let b = p.bbox();
        assert!((b.y0 + 1.0).abs() < 0.01, "{b:?}");
        assert!(b.y1.abs() < 1e-9);
    }

    #[test]
    fn flatten_circle() {
        let mut p = PathData::default();
        p.arc(0.0, 0.0, [1.0, 0.0], [0.0, 1.0], 0.0, std::f64::consts::TAU, false);
        p.close();
        let mut out = vec![];
        flatten(&p, 0.01, &mut out);
        assert_eq!(out.len(), 1);
        for q in &out[0] { assert!(((q[0] * q[0] + q[1] * q[1]).sqrt() - 1.0).abs() < 0.01); }
    }
}
