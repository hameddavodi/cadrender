//! Hatch boundaries: edges -> closed loops -> the loops that the hatch style
//! keeps. Readers (DXF, DWG) give the raw loops; this module does the rest,
//! so both readers get the same result.
//!
//! Words: a **loop** is one closed boundary. An **island** is a loop inside
//! another loop. The **depth** of a loop is how many other loops are around it.

use crate::geom::{self, BBox, PathData};

/// Loop flag bits (DXF group 92 / DWG): external, polyline, derived, text box, outermost.
pub const LOOP_EXTERNAL: u32 = 1;
pub const LOOP_TEXTBOX: u32 = 8;
pub const LOOP_OUTERMOST: u32 = 16;

/// Island detection style (DXF group 75).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HatchStyle {
    /// Fill every other level: outer area, not the island, the island in the island, ...
    #[default]
    Normal,
    /// Fill only the outer level; islands stay empty, also what is inside them.
    Outer,
    /// Fill the outer loops and ignore all islands.
    Ignore,
}

impl HatchStyle {
    pub fn from_code(c: i32) -> HatchStyle { match c { 1 => HatchStyle::Outer, 2 => HatchStyle::Ignore, _ => HatchStyle::Normal } }
}

/// One edge of an edge-type boundary loop. Angles in radians.
#[derive(Clone, Debug, PartialEq)]
pub enum Edge {
    Line { a: [f64; 2], b: [f64; 2] },
    /// Circle arc. `a0`/`a1` as stored: for clockwise arcs (`ccw == false`)
    /// the file stores the angles mirrored (360 - angle), like AutoCAD does.
    Arc { c: [f64; 2], r: f64, a0: f64, a1: f64, ccw: bool },
    /// Elliptic arc: `major` = end of the major axis relative to the centre,
    /// `ratio` = minor / major. `a0`/`a1` are real angles (not parameters),
    /// stored like the circle arc angles.
    Ellipse { c: [f64; 2], major: [f64; 2], ratio: f64, a0: f64, a1: f64, ccw: bool },
    /// B-spline (rational if `weights` is not empty). If there are no control
    /// points, the fit points are joined with straight lines.
    Spline { degree: usize, knots: Vec<f64>, ctrl: Vec<[f64; 2]>, weights: Vec<f64>, fit: Vec<[f64; 2]> },
}

/// One raw boundary loop as read from the file.
#[derive(Clone, Debug, Default)]
pub struct BoundaryLoop {
    pub flags: u32,
    /// The closed loop (`close()` at the end).
    pub path: PathData,
}

fn d2(a: [f64; 2], b: [f64; 2]) -> f64 { (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) }

/// Normalise a positive sweep into (0, 2 pi]; 0 means a full turn.
fn sweep_of(a0: f64, a1: f64) -> f64 { let s = (a1 - a0).rem_euclid(std::f64::consts::TAU); if s <= 1e-12 { std::f64::consts::TAU } else { s } }

/// Real angle on an ellipse -> ellipse parameter.
fn angle_to_param(ratio: f64, a: f64) -> f64 { if ratio.abs() < 1e-12 { a } else { (a.sin() / ratio).atan2(a.cos()) } }

/// An edge flattened to its geometry: start point, signed arc data or points.
enum Piece {
    Line([f64; 2], [f64; 2]),
    /// centre, half axes, start parameter, signed sweep
    Arc([f64; 2], [f64; 2], [f64; 2], f64, f64),
    Points(Vec<[f64; 2]>),
}

impl Piece {
    fn of(e: &Edge) -> Option<Piece> {
        Some(match e {
            Edge::Line { a, b } => Piece::Line(*a, *b),
            Edge::Arc { c, r, a0, a1, ccw } => {
                if r.is_nan() || *r <= 0.0 { return None; }
                let (s0, sw) = if *ccw { (*a0, sweep_of(*a0, *a1)) } else { (-*a0, -sweep_of(*a0, *a1)) };
                Piece::Arc(*c, [*r, 0.0], [0.0, *r], s0, sw)
            }
            Edge::Ellipse { c, major, ratio, a0, a1, ccw } => {
                let (mx, my) = (major[0], major[1]);
                if mx == 0.0 && my == 0.0 { return None; }
                let (p0, p1) = (angle_to_param(*ratio, *a0), angle_to_param(*ratio, *a1));
                let full = (sweep_of(*a0, *a1) - std::f64::consts::TAU).abs() < 1e-9;
                let sw = if full { std::f64::consts::TAU } else { sweep_of(p0, p1) };
                let (s0, sw) = if *ccw { (p0, sw) } else { (-p0, -sw) };
                Piece::Arc(*c, [mx, my], [-my * ratio, mx * ratio], s0, sw)
            }
            Edge::Spline { degree, knots, ctrl, weights, fit } => {
                let pts = if ctrl.len() >= 2 { spline_points(*degree, knots, ctrl, weights) } else { fit.clone() };
                if pts.len() < 2 { return None; }
                Piece::Points(pts)
            }
        })
    }
    fn ends(&self) -> ([f64; 2], [f64; 2]) {
        match self {
            Piece::Line(a, b) => (*a, *b),
            Piece::Arc(c, ax, ay, s0, sw) => {
                let p = |t: f64| { let (s, co) = t.sin_cos(); [c[0] + ax[0] * co + ay[0] * s, c[1] + ax[1] * co + ay[1] * s] };
                (p(*s0), p(s0 + sw))
            }
            Piece::Points(v) => (v[0], v[v.len() - 1]),
        }
    }
    fn reverse(&mut self) {
        match self {
            Piece::Line(a, b) => std::mem::swap(a, b),
            Piece::Arc(_, _, _, s0, sw) => { *s0 += *sw; *sw = -*sw; }
            Piece::Points(v) => v.reverse(),
        }
    }
    /// Append to `path`; the first piece of a loop starts with move_to.
    fn emit(&self, path: &mut PathData, first: bool) {
        let start = self.ends().0;
        if first { path.move_to(start[0], start[1]); }
        else if let Some(p) = path.last() { if d2(p, start) > 1e-18 { path.line_to(start[0], start[1]); } }
        match self {
            Piece::Line(_, b) => path.line_to(b[0], b[1]),
            Piece::Arc(c, ax, ay, s0, sw) => path.arc(c[0], c[1], *ax, *ay, *s0, *sw, true),
            Piece::Points(v) => for q in &v[1..] { path.line_to(q[0], q[1]) },
        }
    }
}

/// Build one closed loop from edges. Edges are chained by nearest end: after
/// each edge, the next one is the edge (of those not used yet) with an end
/// nearest to the current point, turned if it is stored the wrong way round.
/// The next edge in the file is tried first, so a loop in order costs one
/// check per edge. Small gaps between edges are closed with a straight line
/// (AutoCAD accepts both). If a part closes on itself and no edge touches
/// it, the rest starts a new part (a new sub-path, not a connecting line).
pub fn loop_from_edges(edges: &[Edge]) -> PathData {
    let mut pieces: Vec<Piece> = edges.iter().filter_map(Piece::of).collect();
    let mut path = PathData::default();
    if pieces.is_empty() { return path; }
    let mut ends: Vec<([f64; 2], [f64; 2])> = pieces.iter().map(Piece::ends).collect();
    let tol2 = touch_tolerance2(&ends);
    // the first edge: turn it if its start (not its end) touches the second edge
    if pieces.len() > 1 {
        let ((s0, e0), (s1, e1)) = (ends[0], ends[1]);
        let gap_end = d2(e0, s1).min(d2(e0, e1));
        let gap_start = d2(s0, s1).min(d2(s0, e1));
        if gap_start < gap_end { pieces[0].reverse(); ends[0] = (e0, s0); }
    }
    let mut part_start = ends[0].0;
    let mut cur = part_start;
    for k in 0..pieces.len() {
        let mut first = k == 0;
        if k > 0 {
            let near = |(s, e): ([f64; 2], [f64; 2])| d2(cur, s).min(d2(cur, e));
            let gap = near(ends[k]);
            if gap > tol2 {
                // out of order: take the unused edge with the nearest end
                let (mut best, mut bd) = (k, gap);
                for (j, &en) in ends.iter().enumerate().skip(k + 1) {
                    let d = near(en);
                    if d < bd { best = j; bd = d; }
                }
                pieces.swap(k, best);
                ends.swap(k, best);
                if bd > tol2 && d2(cur, part_start) <= tol2 { path.close(); first = true; }
            }
            let (s, e) = ends[k];
            if d2(cur, e) < d2(cur, s) { pieces[k].reverse(); ends[k] = (e, s); }
            if first { part_start = ends[k].0; }
        }
        pieces[k].emit(&mut path, first);
        cur = ends[k].1;
    }
    path.close();
    path
}

/// Squared distance under which two edge ends touch: a millionth of the loop size.
fn touch_tolerance2(ends: &[([f64; 2], [f64; 2])]) -> f64 {
    let mut b = BBox::EMPTY;
    for &(s, e) in ends { b.add(s[0], s[1]); b.add(e[0], e[1]); }
    let t = ((b.x1 - b.x0).hypot(b.y1 - b.y0) * 1e-6).max(1e-9);
    if t.is_finite() { t * t } else { 1e-18 }
}

/// Polyline loop with bulges: `(x, y, bulge)`.
pub fn loop_from_polyline(pts: &[(f64, f64, f64)]) -> PathData {
    let mut path = PathData::default();
    if pts.len() < 2 { return path; }
    path.move_to(pts[0].0, pts[0].1);
    for k in 1..pts.len() { path.bulge_to(pts[k].0, pts[k].1, pts[k - 1].2); }
    let l = pts[pts.len() - 1];
    if d2([l.0, l.1], [pts[0].0, pts[0].1]) > 1e-18 || l.2.abs() > 1e-9 { path.bulge_to(pts[0].0, pts[0].1, l.2); }
    path.close();
    path
}

/// Points along a (rational) B-spline, de Boor in homogeneous coordinates.
pub fn spline_points(degree: usize, knots: &[f64], ctrl: &[[f64; 2]], weights: &[f64]) -> Vec<[f64; 2]> {
    let n = ctrl.len();
    if n < 2 { return ctrl.to_vec(); }
    let p = degree.clamp(1, 32).min(n - 1);
    if knots.len() < n + p + 1 { return ctrl.to_vec(); }
    let w = |i: usize| -> f64 { match weights.get(i) { Some(&x) if weights.len() == n && x > 0.0 => x, _ => 1.0 } };
    let (t0, t1) = (knots[p], knots[n]);
    if t1.is_nan() || t0.is_nan() || t1 <= t0 { return ctrl.to_vec(); }
    let samples = (n * 8).clamp(16, 4000);
    let mut out = Vec::with_capacity(samples + 1);
    let mut d = vec![[0.0f64; 3]; p + 1];
    let mut span = p;
    for s in 0..=samples {
        let t = t0 + (t1 - t0) * s as f64 / samples as f64;
        while span < n - 1 && t >= knots[span + 1] { span += 1; }
        for (j, dj) in d.iter_mut().enumerate() { let i = span - p + j; let wi = w(i); *dj = [ctrl[i][0] * wi, ctrl[i][1] * wi, wi]; }
        for r in 1..=p {
            for j in (r..=p).rev() {
                let i0 = span - p + j;
                let den = knots[i0 + p - r + 1] - knots[i0];
                let a = if den.abs() < 1e-12 { 0.0 } else { (t - knots[i0]) / den };
                let prev = d[j - 1];
                for (c, x) in d[j].iter_mut().enumerate() { *x = (1.0 - a) * prev[c] + a * *x; }
            }
        }
        let h = if d[p][2].abs() < 1e-300 { 1.0 } else { d[p][2] };
        out.push([d[p][0] / h, d[p][1] / h]);
    }
    out
}

/// Loops above this count skip the nesting test (it is O(n^2)); all are kept.
const MAX_NEST_LOOPS: usize = 400;

/// Point inside a closed polygon (even-odd).
fn inside(poly: &[[f64; 2]], p: [f64; 2]) -> bool {
    let mut c = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0] { c = !c; }
        j = i;
    }
    c
}

fn append(out: &mut PathData, p: &PathData) { out.verbs.extend_from_slice(&p.verbs); out.pts.extend_from_slice(&p.pts); }

/// Join the loops the hatch style keeps into one path, filled with the
/// even-odd rule. AutoCAD decides islands by geometry, not by the order or
/// direction of the loops, so the nesting depth of each loop is measured:
/// `Normal` keeps all loops (even-odd = every other level), `Outer` keeps
/// depth 0 and 1, `Ignore` keeps depth 0 only. Text box loops are islands
/// too (the hatch leaves room for the text), except with `Ignore`.
pub fn join_loops(loops: Vec<BoundaryLoop>, style: HatchStyle) -> PathData {
    let loops: Vec<BoundaryLoop> = loops.into_iter().filter(|l| !l.path.is_empty()).collect();
    let mut out = PathData::default();
    if loops.len() <= 1 || style == HatchStyle::Normal || loops.len() > MAX_NEST_LOOPS {
        for l in &loops { append(&mut out, &l.path); }
        return out;
    }
    // coarse polygons and boxes for the nesting test
    let polys: Vec<(Vec<[f64; 2]>, BBox)> = loops.iter().map(|l| {
        let b = l.path.bbox();
        let mut v = vec![];
        geom::flatten(&l.path, (b.w().max(b.h()) * 1e-3).max(1e-9), &mut v);
        (v.into_iter().next().unwrap_or_default(), b)
    }).collect();
    let max_depth = if style == HatchStyle::Outer { 1 } else { 0 };
    for (i, l) in loops.iter().enumerate() {
        let (pi, bi) = &polys[i];
        if pi.is_empty() { continue; }
        let mut depth = 0;
        for (j, (pj, bj)) in polys.iter().enumerate() {
            if i == j || pj.len() < 3 || !bj.contains(bi) { continue; }
            // vote over a few vertices: a self-touching loop may share some points
            let step = (pi.len() / 3).max(1);
            let votes = (0..3).map(|k| pi[(k * step) % pi.len()]).filter(|&q| inside(pj, q)).count();
            if votes >= 2 { depth += 1; }
        }
        if depth <= max_depth { append(&mut out, &l.path); }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sq(x: f64, y: f64, s: f64) -> BoundaryLoop {
        BoundaryLoop { flags: 1, path: loop_from_polyline(&[(x, y, 0.0), (x + s, y, 0.0), (x + s, y + s, 0.0), (x, y + s, 0.0)]) }
    }
    fn count_loops(p: &PathData) -> usize { p.verbs.iter().filter(|&&v| v == geom::M).count() }

    #[test]
    fn styles_keep_the_right_islands() {
        let nest = || vec![sq(0.0, 0.0, 100.0), sq(10.0, 10.0, 80.0), sq(20.0, 20.0, 60.0), sq(30.0, 30.0, 40.0)];
        assert_eq!(count_loops(&join_loops(nest(), HatchStyle::Normal)), 4);
        assert_eq!(count_loops(&join_loops(nest(), HatchStyle::Outer)), 2);
        assert_eq!(count_loops(&join_loops(nest(), HatchStyle::Ignore)), 1);
        // the order of the loops does not matter
        let mut r = nest();
        r.reverse();
        let p = join_loops(r, HatchStyle::Outer);
        assert_eq!(count_loops(&p), 2);
        let b = p.bbox();
        assert_eq!((b.x0, b.x1), (0.0, 100.0));
    }

    #[test]
    fn reversed_edges_and_gaps_are_chained() {
        let e = vec![
            Edge::Line { a: [0.0, 0.0], b: [10.0, 0.0] },
            Edge::Line { a: [10.0, 10.0], b: [10.0, 0.0] }, // stored backwards
            Edge::Line { a: [10.0, 10.0], b: [0.001, 10.0] }, // tiny gap to the start
        ];
        let p = loop_from_edges(&e);
        assert_eq!(p.pts, vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.001, 10.0]]);
        assert_eq!(*p.verbs.last().unwrap(), geom::Z);
    }

    fn line(a: [f64; 2], b: [f64; 2]) -> Edge { Edge::Line { a, b } }

    #[test]
    fn edges_out_of_order_are_chained_by_nearest_end() {
        // square 0..10, edges shuffled and two of them stored backwards
        let e = vec![
            line([0.0, 0.0], [10.0, 0.0]),
            line([0.0, 10.0], [10.0, 10.0]), // top, backwards, out of order
            line([10.0, 10.0], [10.0, 0.0]), // right, backwards
            line([0.0, 10.0], [0.0, 0.0]),   // left
        ];
        let p = loop_from_edges(&e);
        assert_eq!(p.pts, vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0], [0.0, 0.0]]);
        assert_eq!(count_loops(&p), 1);
        assert!((p.bbox().x1 - 10.0).abs() < 1e-12);
    }

    #[test]
    fn arc_out_of_order_joins_at_its_nearer_end() {
        // D shape: bottom line, half circle on the right stored clockwise, top line,
        // closing line; file order: bottom, top, left, arc
        let e = vec![
            line([0.0, 0.0], [10.0, 0.0]),
            line([10.0, 10.0], [0.0, 10.0]),
            line([0.0, 10.0], [0.0, 0.0]),
            Edge::Arc { c: [10.0, 5.0], r: 5.0, a0: 270f64.to_radians(), a1: 90f64.to_radians(), ccw: true },
        ];
        let p = loop_from_edges(&e);
        // after the bottom line the arc comes, then the top and the left line
        assert_eq!(p.pts[..2], [[0.0, 0.0], [10.0, 0.0]]);
        let b = p.bbox();
        assert!((b.x1 - 15.0).abs() < 1e-6, "{b:?}");
        // no straight jump: every point is on the D outline
        let top = p.pts.iter().position(|q| d2(*q, [0.0, 10.0]) < 1e-12).unwrap();
        assert_eq!(p.pts[top + 1], [0.0, 0.0]);
        assert_eq!(count_loops(&p), 1);
    }

    #[test]
    fn two_closed_parts_in_one_loop_become_two_sub_paths() {
        let sq = |x: f64| vec![line([x, 0.0], [x + 1.0, 0.0]), line([x + 1.0, 0.0], [x + 1.0, 1.0]), line([x + 1.0, 1.0], [x, 1.0]), line([x, 1.0], [x, 0.0])];
        let mut e = sq(0.0);
        e.extend(sq(5.0));
        let p = loop_from_edges(&e);
        assert_eq!(count_loops(&p), 2);
        assert_eq!(p.verbs.iter().filter(|&&v| v == geom::Z).count(), 2);
    }

    #[test]
    fn clockwise_arc_uses_mirrored_angles() {
        // AutoCAD stores a clockwise arc from 90 to 0 degrees as 270 .. 360
        let e = Edge::Arc { c: [0.0, 0.0], r: 1.0, a0: 270f64.to_radians(), a1: 360f64.to_radians(), ccw: false };
        let (s, t) = Piece::of(&e).unwrap().ends();
        assert!(d2(s, [0.0, 1.0]) < 1e-12 && d2(t, [1.0, 0.0]) < 1e-12, "{s:?} {t:?}");
    }

    #[test]
    fn ellipse_angles_are_real_angles() {
        // ellipse 2 x 1, real angle 45 degrees: the end point is on the line y = x
        let e = Edge::Ellipse { c: [0.0, 0.0], major: [2.0, 0.0], ratio: 0.5, a0: 0.0, a1: 45f64.to_radians(), ccw: true };
        let (_, t) = Piece::of(&e).unwrap().ends();
        assert!((t[0] - t[1]).abs() < 1e-9 && (t[0] * t[0] / 4.0 + t[1] * t[1] - 1.0).abs() < 1e-9, "{t:?}");
    }

    #[test]
    fn rational_spline_is_a_circle_quadrant() {
        let w = std::f64::consts::FRAC_1_SQRT_2;
        let pts = spline_points(2, &[0.0, 0.0, 0.0, 1.0, 1.0, 1.0], &[[1.0, 0.0], [1.0, 1.0], [0.0, 1.0]], &[1.0, w, 1.0]);
        for q in &pts { assert!(((q[0] * q[0] + q[1] * q[1]).sqrt() - 1.0).abs() < 1e-9, "{q:?}"); }
    }
}
