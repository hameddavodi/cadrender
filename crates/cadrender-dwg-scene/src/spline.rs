//! Curve helpers: bulge polylines and B-spline sampling (same code as the
//! DXF reader, `cadrender-dxf/src/entities/curves.rs`), plus cubic
//! interpolation through fit points for SPLINEs that store only fit points.

use cadrender_core::geom::PathData;

/// Polyline with bulges (arcs) between vertices.
pub fn poly_path(path: &mut PathData, pts: &[(f64, f64, f64)], closed: bool) {
    if pts.len() < 2 { return; }
    path.move_to(pts[0].0, pts[0].1);
    for k in 1..pts.len() { path.bulge_to(pts[k].0, pts[k].1, pts[k - 1].2); }
    if closed { path.bulge_to(pts[0].0, pts[0].1, pts[pts.len() - 1].2); path.close(); }
}

pub fn polyline_pts(pts: &[[f64; 2]], path: &mut PathData, connect: bool) {
    for (k, c) in pts.iter().enumerate() { if k == 0 && !connect { path.move_to(c[0], c[1]) } else { path.line_to(c[0], c[1]) } }
}

/// B-spline -> polyline samples (de Boor). Weights are ignored.
pub fn bspline(degree: usize, knots: &[f64], ctrl: &[[f64; 2]], path: &mut PathData, connect: bool) {
    let n = ctrl.len();
    if n < 2 { return; }
    let p = degree.min(n - 1).max(1);
    if knots.len() < n + p + 1 { polyline_pts(ctrl, path, connect); return; }
    let (t0, t1) = (knots[p], knots[n]);
    let samples = (n * 8).clamp(16, 4000);
    let mut d = vec![[0.0f64; 2]; p + 1];
    let mut span = p;
    for sidx in 0..=samples {
        let t = t0 + (t1 - t0) * sidx as f64 / samples as f64;
        while span < n - 1 && t >= knots[span + 1] { span += 1; }
        d[..=p].copy_from_slice(&ctrl[span - p..=span]);
        for r in 1..=p {
            for j in (r..=p).rev() {
                let i0 = span - p + j;
                let den = knots[i0 + p - r + 1] - knots[i0];
                let a = if den.abs() < 1e-12 { 0.0 } else { (t - knots[i0]) / den };
                d[j] = [(1.0 - a) * d[j - 1][0] + a * d[j][0], (1.0 - a) * d[j - 1][1] + a * d[j][1]];
            }
        }
        if sidx == 0 && !connect { path.move_to(d[p][0], d[p][1]) } else { path.line_to(d[p][0], d[p][1]) }
    }
}

/// The 3 non-zero cubic basis functions at a simple interior knot
/// `u = knots[span]` (N[span-3], N[span-2], N[span-1]; N[span] is 0 there).
fn basis_at_knot(knots: &[f64], span: usize, u: f64) -> [f64; 3] {
    // Cox-de Boor (NURBS Book A2.2) on the span to the left of u.
    let s = span - 1;
    let (mut left, mut right, mut n) = ([0.0; 4], [0.0; 4], [0.0; 4]);
    n[0] = 1.0;
    for j in 1..=3 {
        left[j] = u - knots[s + 1 - j];
        right[j] = knots[s + j] - u;
        let mut saved = 0.0;
        for r in 0..j {
            let den = right[r + 1] + left[j - r];
            let tmp = if den.abs() < 1e-300 { 0.0 } else { n[r] / den };
            n[r] = saved + right[r + 1] * tmp;
            saved = left[j - r] * tmp;
        }
        n[j] = saved;
    }
    // functions N[s-3..=s]; at u = knots[s+1] the first one is 0
    [n[1], n[2], n[3]]
}

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] { [a[0] - b[0], a[1] - b[1]] }
fn len(a: [f64; 2]) -> f64 { a[0].hypot(a[1]) }

/// Cubic B-spline through `fit` points (chord-length knots, AutoCAD's default),
/// with end tangents `t0`/`t1` (zero = not given: taken from the parabola
/// through the first / last three points). Returns (knots, control points).
pub fn interpolate(fit: &[[f64; 2]], t0: [f64; 2], t1: [f64; 2]) -> Option<(Vec<f64>, Vec<[f64; 2]>)> {
    let mut q: Vec<[f64; 2]> = vec![];
    for p in fit { if !(p[0].is_finite() && p[1].is_finite()) { return None; }
        if q.last().is_none_or(|l| len(sub(*p, *l)) > 1e-12) { q.push(*p); } }
    if q.len() < 2 || q.len() > 100_000 { return None; }
    let n = q.len() - 1;
    let mut u = vec![0.0; n + 1];
    for k in 1..=n { u[k] = u[k - 1] + len(sub(q[k], q[k - 1])); }
    let total = u[n];
    if total.is_nan() || total <= 0.0 { return None; }
    let unit = |t: [f64; 2]| { let l = len(t); if l > 1e-12 && l.is_finite() { Some([t[0] / l, t[1] / l]) } else { None } };
    // end derivatives with respect to the chord-length parameter (|d| ~ 1)
    let d0 = unit(t0).unwrap_or_else(|| end_dir(&q, &u, false));
    let d1 = unit(t1).unwrap_or_else(|| end_dir(&q, &u, true));
    solve(&q, &u, d0, d1)
}

/// Cubic B-spline through points `q[k]` at parameters `u[k]` (`u[0] = 0`,
/// increasing), with end derivatives `d0`/`d1` with respect to `u`.
/// Clamped knots at the `u` values. Returns (knots, control points).
fn solve(q: &[[f64; 2]], u: &[f64], d0: [f64; 2], d1: [f64; 2]) -> Option<(Vec<f64>, Vec<[f64; 2]>)> {
    let n = q.len() - 1;
    let total = u[n];
    let mut knots = vec![0.0; 4];
    knots.extend_from_slice(&u[1..n]);
    knots.extend_from_slice(&[total; 4]);
    let mut p = vec![[0.0; 2]; n + 3];
    p[0] = q[0];
    p[1] = [q[0][0] + u[1] / 3.0 * d0[0], q[0][1] + u[1] / 3.0 * d0[1]];
    p[n + 1] = [q[n][0] - (total - u[n - 1]) / 3.0 * d1[0], q[n][1] - (total - u[n - 1]) / 3.0 * d1[1]];
    p[n + 2] = q[n];
    if n >= 2 {
        // rows k = 1..n-1: a P[k] + b P[k+1] + c P[k+2] = Q[k]; unknowns P[2..=n]
        let m = n - 1;
        let (mut cp, mut dp) = (vec![0.0; m], vec![[0.0; 2]; m]);
        for i in 0..m {
            let k = i + 1;
            let [a, b, c] = basis_at_knot(&knots, k + 3, u[k]);
            let mut rhs = q[k];
            if i == 0 { rhs = [rhs[0] - a * p[1][0], rhs[1] - a * p[1][1]]; }
            if i == m - 1 { rhs = [rhs[0] - c * p[n + 1][0], rhs[1] - c * p[n + 1][1]]; }
            let (a, c) = (if i == 0 { 0.0 } else { a }, if i == m - 1 { 0.0 } else { c });
            let den = b - a * if i == 0 { 0.0 } else { cp[i - 1] };
            if den.abs() < 1e-300 { return None; }
            cp[i] = c / den;
            let prev = if i == 0 { [0.0; 2] } else { dp[i - 1] };
            dp[i] = [(rhs[0] - a * prev[0]) / den, (rhs[1] - a * prev[1]) / den];
        }
        for i in (0..m).rev() {
            let next = if i == m - 1 { [0.0; 2] } else { p[i + 3] };
            p[i + 2] = [dp[i][0] - cp[i] * next[0], dp[i][1] - cp[i] * next[1]];
        }
    }
    Some((knots, p))
}

/// HELIX as ODA's DXF draws it. The DWG stores AutoCAD's own spline (knots
/// of double multiplicity); ODA writes a different one for the DXF: a cubic
/// spline through the helix points every 1/8 turn (knots at the turn angle,
/// k * pi/4), with the helix's own derivatives at both ends. Checked against
/// ODA's control points on our sample: the curve hits the helix at every
/// knot within 3e-12. Seen from above both splines look the same, but the
/// linetype dashes run along the curve length, so only the same curve gives
/// the same dashes. Returns (knots, control points) in x/y (WCS), or None
/// when the helix values are unusable (the caller then draws the stored spline).
pub fn helix(h: &cadrender_dwg::Helix) -> Option<(Vec<f64>, Vec<[f64; 2]>)> {
    type V = [f64; 3];
    let dot = |a: V, b: V| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let cross = |a: V, b: V| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
    let al = dot(h.axis, h.axis).sqrt();
    if !(al > 1e-12 && al.is_finite() && h.turns > 0.0 && h.turns <= 10_000.0 && h.radius.is_finite() && h.turn_height.is_finite()) { return None; }
    let ax = [h.axis[0] / al, h.axis[1] / al, h.axis[2] / al];
    let v0 = [h.start[0] - h.axis_base[0], h.start[1] - h.axis_base[1], h.start[2] - h.axis_base[2]];
    let along = dot(v0, ax);
    let radial = [v0[0] - along * ax[0], v0[1] - along * ax[1], v0[2] - along * ax[2]];
    let r0 = dot(radial, radial).sqrt();
    if !(r0 > 1e-12 && r0.is_finite()) { return None; }
    let xd = [radial[0] / r0, radial[1] / r0, radial[2] / r0];
    let yd = cross(ax, xd);
    let s = if h.right_handed { 1.0 } else { -1.0 };
    let total = std::f64::consts::TAU * h.turns;
    let (dr, dz) = ((h.radius - r0) / total, h.turns * h.turn_height / total);
    // point and derivative at turn angle t, projected to x/y
    let at = |t: f64| -> ([f64; 2], [f64; 2]) {
        let (sn, cs) = t.sin_cos();
        let r = r0 + dr * t;
        let p = |k: usize| h.start[k] - radial[k] + r * (cs * xd[k] + s * sn * yd[k]) + dz * t * ax[k];
        let d = |k: usize| dr * (cs * xd[k] + s * sn * yd[k]) + r * (-sn * xd[k] + s * cs * yd[k]) + dz * ax[k];
        ([p(0), p(1)], [d(0), d(1)])
    };
    let n = ((h.turns * 8.0 - 1e-9).ceil() as usize).max(1);
    let u: Vec<f64> = (0..=n).map(|k| total * k as f64 / n as f64).collect();
    let q: Vec<[f64; 2]> = u.iter().map(|&t| at(t).0).collect();
    solve(&q, &u, at(0.0).1, at(total).1)
}

/// Unit end direction from the parabola through three end points (Bessel).
fn end_dir(q: &[[f64; 2]], u: &[f64], at_end: bool) -> [f64; 2] {
    let n = q.len() - 1;
    let d = if n < 2 {
        sub(q[n], q[0])
    } else if !at_end {
        let (h1, h2) = (u[1] - u[0], u[2] - u[1]);
        let (s1, s2) = (sub(q[1], q[0]), sub(q[2], q[1]));
        // derivative of the parabola at its start: (1 + w) s1/h1 - w s2/h2
        let w = h1 / (h1 + h2);
        [s1[0] / h1 * (1.0 + w) - w * s2[0] / h2, s1[1] / h1 * (1.0 + w) - w * s2[1] / h2]
    } else {
        let (h1, h2) = (u[n - 1] - u[n - 2], u[n] - u[n - 1]);
        let (s1, s2) = (sub(q[n - 1], q[n - 2]), sub(q[n], q[n - 1]));
        let w = h2 / (h1 + h2);
        [s2[0] / h2 * (1.0 + w) - w * s1[0] / h1, s2[1] / h2 * (1.0 + w) - w * s1[1] / h1]
    };
    let l = len(d);
    if l > 1e-12 { [d[0] / l, d[1] / l] } else { [1.0, 0.0] }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// de Boor: point of a B-spline at parameter t.
    fn eval(deg: usize, knots: &[f64], ctrl: &[[f64; 2]], t: f64) -> [f64; 2] {
        let n = ctrl.len();
        let mut k = deg;
        while k + 1 < n && knots[k + 1] <= t { k += 1; }
        let mut d: Vec<[f64; 2]> = (0..=deg).map(|j| ctrl[j + k - deg]).collect();
        for r in 1..=deg {
            for j in (r..=deg).rev() {
                let i = j + k - deg;
                let den = knots[i + deg + 1 - r] - knots[i];
                let a = if den > 0.0 { (t - knots[i]) / den } else { 0.0 };
                d[j] = [(1.0 - a) * d[j - 1][0] + a * d[j][0], (1.0 - a) * d[j - 1][1] + a * d[j][1]];
            }
        }
        d[deg]
    }

    #[test]
    fn interpolation_passes_through_every_fit_point() {
        let fit = [[0.0, 0.0], [10.0, 5.0], [20.0, -3.0], [35.0, 4.0], [40.0, 0.0]];
        let (knots, ctrl) = interpolate(&fit, [0.0, 0.0], [0.0, 0.0]).expect("curve");
        assert_eq!(ctrl.len(), fit.len() + 2);
        // the fit points sit at the inner knots (chord-length parameters)
        let inner: Vec<f64> = knots[3..knots.len() - 3].to_vec();
        assert_eq!(inner.len(), fit.len());
        let t_end = knots[knots.len() - 4] - 1e-12;
        for (p, &t) in fit.iter().zip(&inner) {
            let q = eval(3, &knots, &ctrl, t.min(t_end));
            assert!((p[0] - q[0]).abs() < 1e-6 && (p[1] - q[1]).abs() < 1e-6, "{p:?} vs {q:?}");
        }
    }

    fn cone_helix(right_handed: bool) -> cadrender_dwg::Helix {
        cadrender_dwg::Helix {
            axis_base: [10.0, 20.0, 0.0], start: [-25.0, 20.0, 0.0], axis: [0.0, 0.0, 2.0],
            radius: 115.0, turns: 7.0, turn_height: 11.0, right_handed, ..Default::default()
        }
    }

    #[test]
    fn helix_spline_hits_the_helix_every_eighth_turn() {
        // a cone helix: base radius 35 (start point), top radius 115, 7 turns
        let h = cone_helix(true);
        let (knots, ctrl) = helix(&h).expect("helix");
        // like ODA's DXF: 8 spans per turn, knots k * pi/4, 59 control points
        assert_eq!(ctrl.len(), 59);
        assert_eq!(knots.len(), 63);
        assert!((knots[4] - std::f64::consts::FRAC_PI_4).abs() < 1e-12);
        let total = 14.0 * std::f64::consts::PI;
        for k in 0..=56 {
            let t = (total * k as f64 / 56.0).min(total - 1e-12);
            let r = 35.0 + 80.0 * t / total;
            // start at angle pi around (10, 20), counter-clockwise
            let want = [10.0 + r * (std::f64::consts::PI + t).cos(), 20.0 + r * (std::f64::consts::PI + t).sin()];
            let got = eval(3, &knots, &ctrl, t);
            assert!((want[0] - got[0]).abs() < 1e-6 && (want[1] - got[1]).abs() < 1e-6, "k={k}: {want:?} vs {got:?}");
        }
        // end derivative: ctrl[1] - ctrl[0] = helix'(0) * (pi/4) / 3
        let d = [ctrl[1][0] - ctrl[0][0], ctrl[1][1] - ctrl[0][1]];
        let span = std::f64::consts::FRAC_PI_4 / 3.0;
        assert!((d[0] - (-80.0 / total) * span).abs() < 1e-9 && (d[1] - (-35.0) * span).abs() < 1e-9, "{d:?}");
    }

    #[test]
    fn helix_left_handed_turns_clockwise_and_bad_values_fall_back() {
        let (_, ctrl) = helix(&cone_helix(false)).expect("helix");
        assert!(ctrl[1][1] > 20.0, "clockwise from angle pi goes up: {:?}", ctrl[1]);
        let mut h = cone_helix(true);
        h.start = h.axis_base; // start on the axis: no base radius direction
        assert!(helix(&h).is_none());
        let mut h = cone_helix(true);
        h.turns = f64::NAN;
        assert!(helix(&h).is_none());
    }

    #[test]
    fn interpolation_rejects_bad_input() {
        assert!(interpolate(&[[0.0, 0.0]], [0.0, 0.0], [0.0, 0.0]).is_none());
        assert!(interpolate(&[[0.0, 0.0], [f64::NAN, 1.0], [2.0, 0.0]], [0.0, 0.0], [0.0, 0.0]).is_none());
    }
}
