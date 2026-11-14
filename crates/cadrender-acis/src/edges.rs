//! Edges of the records -> one 2D path (seen from above: z is dropped).
//!
//! Every "edge" record is drawn once, so we do not need to walk the
//! topology (body -> lump -> shell -> face -> loop -> coedge -> edge).
//! An edge points to two vertices (-> points) and to a curve:
//! "straight-curve" is the line between the two points, "ellipse-curve" is
//! an arc (centre, normal, major axis, ratio) from one point to the other.
//! Other curves (splines: "intcurve-curve", ...) are counted and skipped.

use crate::tokens::{File, Record, Tok};
use cadrender_core::geom::PathData;

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 { [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }
fn dot(a: V3, b: V3) -> f64 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }
fn cross(a: V3, b: V3) -> V3 { [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]] }
fn scale(a: V3, s: f64) -> V3 { [a[0] * s, a[1] * s, a[2] * s] }

/// Affine 3D transform: `p' = s * (p x A) + t` (ACIS "transform" record, row vectors).
#[derive(Clone, Copy, Debug)]
struct Xf { a: [f64; 9], t: V3, s: f64 }

impl Xf {
    fn point(&self, p: V3) -> V3 { let v = self.dir(p); [v[0] + self.t[0], v[1] + self.t[1], v[2] + self.t[2]] }
    fn dir(&self, p: V3) -> V3 {
        let a = &self.a;
        scale([p[0] * a[0] + p[1] * a[3] + p[2] * a[6], p[0] * a[1] + p[1] * a[4] + p[2] * a[7], p[0] * a[2] + p[1] * a[5] + p[2] * a[8]], self.s)
    }
}

/// Values of a record after the common part (attribute pointer, id from
/// version 7.0, pattern pointer from version 7.0).
fn data(r: &Record, version: i64) -> &[Tok] {
    let skip = if version >= 700 { 3 } else { 1 };
    r.toks.get(skip..).unwrap_or(&[])
}

fn nums(t: &[Tok], n: usize) -> Option<Vec<f64>> {
    let v: Vec<f64> = t.iter().filter_map(|t| if let Tok::Num(x) = t { Some(*x) } else { None }).take(n).collect();
    (v.len() == n).then_some(v)
}

fn ptrs(t: &[Tok]) -> impl Iterator<Item = i64> + '_ { t.iter().filter_map(|t| if let Tok::Ptr(p) = t { Some(*p) } else { None }) }

/// What came out of one ACIS file.
#[derive(Clone, Debug, Default)]
pub struct Outline {
    pub path: PathData,
    /// Edges drawn.
    pub edges: u32,
    /// Edges skipped: spline or other curve types we do not read.
    pub skipped: u32,
}

pub fn outline(f: &File) -> Outline {
    let v = f.version;
    let rec = |p: i64| if p >= 0 { f.records.get(p as usize) } else { None };
    // a body's transform (the first one found applies to all edges)
    let xf = f.records.iter().filter(|r| r.name == "body").find_map(|b| ptrs(data(b, v)).nth(2).and_then(rec)).filter(|r| r.name == "transform").and_then(|r| transform(r, v));
    let point = |vertex: i64| -> Option<V3> {
        let vx = rec(vertex).filter(|r| is(r, "vertex"))?;
        let pt = rec(ptrs(data(vx, v)).last()?).filter(|r| r.name == "point")?;
        let p = nums(data(pt, v), 3)?;
        let p = [p[0], p[1], p[2]];
        Some(xf.map_or(p, |x| x.point(p)))
    };
    let mut out = Outline::default();
    for e in f.records.iter().filter(|r| is(r, "edge")) {
        let d = data(e, v);
        let mut it = d.iter();
        let mut next_ptr = || it.by_ref().find_map(|t| if let Tok::Ptr(p) = t { Some(*p) } else { None });
        let (Some(sv), Some(ev), Some(_coedge), Some(cv)) = (next_ptr(), next_ptr(), next_ptr(), next_ptr()) else { out.skipped += 1; continue };
        // the sense comes right after the curve pointer
        let reversed = match it.next() { Some(Tok::Bool(b)) => *b, Some(Tok::Str(s)) => s == "reversed", _ => false };
        let (Some(a), Some(b)) = (point(sv), point(ev)) else { out.skipped += 1; continue };
        let Some(curve) = rec(cv) else { out.skipped += 1; continue };
        let ok = if is(curve, "straight-curve") {
            line(&mut out.path, a, b);
            true
        } else if is(curve, "ellipse-curve") {
            ellipse(&mut out.path, data(curve, v), xf, a, b, sv == ev, reversed)
        } else {
            false
        };
        if ok { out.edges += 1 } else { out.skipped += 1 }
    }
    out
}

/// Record type `kind` or a sub-type of it ("tvertex-vertex" is a vertex).
fn is(r: &Record, kind: &str) -> bool {
    r.name == kind || (r.name.len() > kind.len() && r.name.ends_with(kind) && r.name.as_bytes()[r.name.len() - kind.len() - 1] == b'-')
}

fn transform(r: &Record, v: i64) -> Option<Xf> {
    // no pattern pointer here: attribute pointer and id only
    let d = r.toks.get(if v >= 700 { 2 } else { 1 }..).unwrap_or(&[]);
    // SAB may hold the numbers as one literal string
    let x: Vec<f64> = match nums(d, 13) {
        Some(x) => x,
        None => d.iter().find_map(|t| if let Tok::Str(s) = t { Some(s.split_whitespace().filter_map(|w| w.parse().ok()).collect::<Vec<f64>>()) } else { None }).filter(|x| x.len() >= 12)?,
    };
    let s = x.get(12).copied().filter(|s| *s > 0.0).unwrap_or(1.0);
    Some(Xf { a: x[..9].try_into().ok()?, t: [x[9], x[10], x[11]], s })
}

fn line(p: &mut PathData, a: V3, b: V3) {
    p.move_to(a[0], a[1]);
    p.line_to(b[0], b[1]);
}

/// Arc of an ellipse from point `a` to point `b` (whole ellipse if `closed`).
/// The curve runs counter-clockwise around its normal; a reversed edge runs
/// the other way, which draws the same arc from `b` to `a`.
#[allow(clippy::too_many_arguments)]
fn ellipse(p: &mut PathData, d: &[Tok], xf: Option<Xf>, a: V3, b: V3, closed: bool, reversed: bool) -> bool {
    let Some(x) = nums(d, 10) else { return false };
    let (mut c, mut n, mut maj) = ([x[0], x[1], x[2]], [x[3], x[4], x[5]], [x[6], x[7], x[8]]);
    if let Some(t) = xf { c = t.point(c); n = t.dir(n); maj = t.dir(maj); }
    let nl = dot(n, n).sqrt();
    let ml2 = dot(maj, maj);
    if nl < 1e-12 || ml2 < 1e-24 || !x[9].is_finite() { return false; }
    let minor = scale(cross(scale(n, 1.0 / nl), maj), x[9]);
    let mi2 = dot(minor, minor).max(1e-300);
    let ang = |q: V3| { let r = sub(q, c); (dot(r, minor) / mi2).atan2(dot(r, maj) / ml2) };
    let tau = std::f64::consts::TAU;
    let (from, to) = if reversed { (b, a) } else { (a, b) };
    let t0 = ang(from);
    let mut sweep = if closed { tau } else { (ang(to) - t0).rem_euclid(tau) };
    if sweep < 1e-9 { sweep = tau; }
    p.arc(c[0], c[1], [maj[0], maj[1]], [minor[0], minor[1]], t0, sweep, false);
    true
}
