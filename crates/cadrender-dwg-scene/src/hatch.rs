//! HATCH: boundary loops, hatch style, pattern definition and gradient.
//! Same rules as `cadrender-dxf/src/entities/hatch.rs`; the loop geometry
//! (edge chaining, islands, style) is shared in
//! `cadrender_core::entity::hatch::boundary`. DWG angles are already radians.

use cadrender_core::color::EntColor;
use cadrender_core::entity::hatch::boundary::{join_loops, loop_from_edges, loop_from_polyline};
use cadrender_core::entity::hatch::{BoundaryLoop, Edge, Gradient, GradientKind, HatchEnt, HatchStyle, PatLine};
use cadrender_core::entity::Kind;
use cadrender_core::geom::Aff;
use cadrender_dwg::{Color, Hatch, HatchEdge};

pub(crate) fn ent_color(c: Color) -> EntColor {
    match c {
        Color::Rgb(rgb) => EntColor::true_color(rgb),
        Color::Index(i) => EntColor::aci(i as i32),
        Color::ByBlock => EntColor::BYBLOCK,
        Color::ByLayer | Color::None => EntColor::BYLAYER,
    }
}

fn edge(e: &HatchEdge) -> Edge {
    match e {
        HatchEdge::Line { start, end } => Edge::Line { a: *start, b: *end },
        HatchEdge::Arc { center, radius, start_angle, end_angle, ccw } => Edge::Arc { c: *center, r: *radius, a0: *start_angle, a1: *end_angle, ccw: *ccw },
        HatchEdge::Ellipse { center, major_end, ratio, start_angle, end_angle, ccw } => {
            Edge::Ellipse { c: *center, major: *major_end, ratio: *ratio, a0: *start_angle, a1: *end_angle, ccw: *ccw }
        }
        HatchEdge::Spline { degree, rational, knots, control, fit, .. } => Edge::Spline {
            degree: (*degree).clamp(1, 32) as usize,
            knots: knots.clone(),
            ctrl: control.iter().map(|c| [c[0], c[1]]).collect(),
            weights: if *rational { control.iter().map(|c| c[2]).collect() } else { vec![] },
            fit: fit.clone(),
        },
    }
}

/// Start and end point of an edge as it is drawn.
fn ends(e: &Edge) -> ([f64; 2], [f64; 2]) {
    let at = |c: [f64; 2], ax: [f64; 2], ay: [f64; 2], t: f64| [c[0] + ax[0] * t.cos() + ay[0] * t.sin(), c[1] + ax[1] * t.cos() + ay[1] * t.sin()];
    match e {
        Edge::Line { a, b } => (*a, *b),
        Edge::Arc { c, r, a0, a1, ccw } => {
            let s = if *ccw { 1.0 } else { -1.0 };
            (at(*c, [*r, 0.0], [0.0, *r], s * a0), at(*c, [*r, 0.0], [0.0, *r], s * a1))
        }
        Edge::Ellipse { c, major, ratio, a0, a1, ccw } => {
            let s = if *ccw { 1.0 } else { -1.0 };
            let ay = [-major[1] * ratio, major[0] * ratio];
            let p = |a: f64| (a.sin() / ratio).atan2(a.cos());
            (at(*c, *major, ay, s * p(*a0)), at(*c, *major, ay, s * p(*a1)))
        }
        Edge::Spline { ctrl, fit, .. } => {
            let v = if ctrl.len() >= 2 { ctrl } else { fit };
            (v.first().copied().unwrap_or_default(), v.last().copied().unwrap_or_default())
        }
    }
}

/// Edges in chain order: each next edge is the one with an end nearest to
/// the current end (`loop_from_edges` then turns edges that run backwards).
fn chain(mut rest: Vec<Edge>) -> Vec<Edge> {
    let d2 = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2);
    let mut out = Vec::with_capacity(rest.len());
    if rest.is_empty() || rest.len() > 5000 { return rest; }
    let first = rest.remove(0);
    let mut cur = ends(&first).1;
    out.push(first);
    while !rest.is_empty() {
        let (k, flip) = rest.iter().enumerate().map(|(k, e)| { let (s, t) = ends(e); (k, d2(cur, s), d2(cur, t)) })
            .map(|(k, ds, dt)| (k, ds.min(dt), dt < ds))
            .min_by(|a, b| a.1.total_cmp(&b.1)).map(|(k, _, f)| (k, f)).unwrap_or((0, false));
        let e = rest.remove(k);
        let (s, t) = ends(&e);
        cur = if flip { s } else { t };
        out.push(e);
    }
    out
}

pub fn hatch(h: &Hatch, ocs: Option<Aff>) -> Option<Kind> {
    // Loops flagged "not closed" (32) are pieces of one boundary. When every
    // loop is such a piece, ODA's DXF replaces the first one by all their
    // edges chained into one closed loop and keeps the other pieces; do the
    // same. When the hatch also has a closed loop, ODA keeps the pieces as
    // they are (checked on all hatches with open loops in our 8 files).
    let is_open = |lp: &&cadrender_dwg::HatchPath| lp.flags & 32 != 0 && lp.polyline.is_none();
    let all_open = h.paths.iter().all(|lp| is_open(&lp));
    let first_open = if all_open { h.paths.iter().position(|lp| is_open(&lp)) } else { None };
    let loops: Vec<BoundaryLoop> = h.paths.iter().enumerate().map(|(k, lp)| {
        let path = match &lp.polyline {
            Some((_, pts)) => loop_from_polyline(&pts.iter().map(|p| (p[0], p[1], p[2])).collect::<Vec<_>>()),
            None if Some(k) == first_open => loop_from_edges(&chain(h.paths.iter().filter(is_open).flat_map(|p| p.edges.iter().map(edge)).collect())),
            None => loop_from_edges(&lp.edges.iter().map(edge).collect::<Vec<_>>()),
        };
        BoundaryLoop { flags: lp.flags as u32, path }
    }).collect();
    let mut path = join_loops(loops, HatchStyle::from_code(h.style as i32));
    if path.is_empty() || !path.pts.iter().all(|p| p[0].is_finite() && p[1].is_finite()) { return None; }
    let mut pattern: Vec<PatLine> = h.pattern_lines.iter().map(|l| PatLine { angle: l.angle, base: l.base, offset: l.offset, dashes: l.dashes.clone() }).collect();
    // a double user hatch stored with one line family: add the crossing family
    if h.double && pattern.len() == 1 {
        let p = pattern[0].clone();
        pattern.push(PatLine { angle: p.angle + std::f64::consts::FRAC_PI_2, offset: [-p.offset[1], p.offset[0]], ..p });
    }
    let mut gradient = h.gradient.then(|| {
        let mut g = Gradient { kind: GradientKind::from_name(&h.gradient_name).unwrap_or(GradientKind::Linear), colors: [EntColor::aci(5), EntColor::aci(2)], two_colors: true, angle: h.gradient_angle, shift: h.gradient_shift, tint: h.gradient_tint };
        for (k, c) in h.gradient_colors.iter().take(2).enumerate() { g.colors[k] = ent_color(*c); }
        g.two_colors = !(h.gradient_one_color && h.gradient_colors.len() < 2);
        Box::new(g)
    });
    if let Some(o) = ocs {
        path.transform(&o);
        for p in &mut pattern { *p = p.transformed(&o); }
        if let Some(g) = &mut gradient { g.angle = std::f64::consts::PI - g.angle; }
    }
    Some(HatchEnt { path, solid: h.solid, pattern, gradient }.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn piece(a: [f64; 2], b: [f64; 2]) -> cadrender_dwg::HatchPath {
        cadrender_dwg::HatchPath { flags: 33, edges: vec![cadrender_dwg::HatchEdge::Line { start: a, end: b }], ..Default::default() }
    }

    fn area(k: &Kind) -> f64 {
        let Kind::Hatch(h) = k else { panic!("not a hatch") };
        let mut v = vec![];
        cadrender_core::geom::flatten(&h.path, 1e-6, &mut v);
        v.iter().map(|p| (0..p.len()).map(|i| { let (a, b) = (p[i], p[(i + 1) % p.len()]); a[0] * b[1] - b[0] * a[1] }).sum::<f64>().abs() / 2.0).sum()
    }

    #[test]
    fn open_pieces_join_only_when_no_loop_is_closed() {
        // a unit square as 4 open one-edge loops: joined into one closed loop
        let sq = [([0.0, 0.0], [1.0, 0.0]), ([1.0, 1.0], [1.0, 0.0]), ([1.0, 1.0], [0.0, 1.0]), ([0.0, 1.0], [0.0, 0.0])];
        let mut h = Hatch { solid: true, paths: sq.iter().map(|&(a, b)| piece(a, b)).collect(), ..Default::default() };
        assert!((area(&hatch(&h, None).expect("hatch")) - 1.0).abs() < 1e-9);
        // with a closed loop (a square at x 5..7) the pieces stay apart, as in ODA's DXF:
        // only the closed loop has area
        let closed = cadrender_dwg::HatchPath { flags: 1, polyline: Some((true, vec![[5.0, 0.0, 0.0], [7.0, 0.0, 0.0], [7.0, 2.0, 0.0], [5.0, 2.0, 0.0]])), ..Default::default() };
        h.paths.insert(0, closed);
        assert!((area(&hatch(&h, None).expect("hatch")) - 4.0).abs() < 1e-9);
    }

    #[test]
    fn chain_orders_unordered_edges_by_nearest_end() {
        let l = |a: [f64; 2], b: [f64; 2]| Edge::Line { a, b };
        // a unit square, edges shuffled and one reversed
        let out = chain(vec![l([0.0, 0.0], [1.0, 0.0]), l([0.0, 1.0], [1.0, 1.0]), l([0.0, 1.0], [0.0, 0.0]), l([1.0, 0.0], [1.0, 1.0])]);
        let starts: Vec<([f64; 2], [f64; 2])> = out.iter().map(ends).collect();
        assert_eq!(starts[0], ([0.0, 0.0], [1.0, 0.0]));
        assert_eq!(starts[1], ([1.0, 0.0], [1.0, 1.0]));
        assert_eq!(starts[2], ([0.0, 1.0], [1.0, 1.0])); // runs backwards; loop_from_edges turns it
        assert_eq!(starts[3], ([0.0, 1.0], [0.0, 0.0]));
    }
}
