//! LINE, CIRCLE, ARC, ELLIPSE, LWPOLYLINE, POLYLINE, SPLINE, LEADER.

use super::Common;
use crate::reader::Reader;
use crate::tags::{f, gf, gi, i, Tag};
use cadrender_core::entity::pline::PlineEnt;
use cadrender_core::entity::stroke::StrokeEnt;
use cadrender_core::entity::Kind;
use cadrender_core::geom::{Aff, PathData};
use cadrender_core::scene::tables::{LT_BYBLOCK, LT_BYLAYER, LT_SOLID};
use std::f64::consts::TAU;

fn g(body: &[Tag], code: i32) -> f64 { gf(body, code).unwrap_or(0.0) }

fn stroke(mut path: PathData, ocs: Option<Aff>, width: f64) -> Option<Kind> {
    if path.is_empty() { return None; }
    if let Some(o) = ocs { path.transform(&o); }
    Some(StrokeEnt { path, width: width.max(0.0) }.into())
}

pub fn line(body: &[Tag]) -> Option<Kind> {
    let mut p = PathData::default();
    p.move_to(g(body, 10), g(body, 20));
    p.line_to(g(body, 11), g(body, 21));
    stroke(p, None, 0.0)
}

pub fn circle(body: &[Tag], com: &Common) -> Option<Kind> {
    let r = g(body, 40);
    if r <= 0.0 { return None; }
    let mut p = PathData::default();
    p.arc(g(body, 10), g(body, 20), [r, 0.0], [0.0, r], 0.0, TAU, false);
    p.close();
    stroke(p, com.ocs(), 0.0)
}

pub fn arc(body: &[Tag], com: &Common) -> Option<Kind> {
    let r = g(body, 40);
    if r <= 0.0 { return None; }
    let a0 = g(body, 50).to_radians();
    let mut sw = (g(body, 51).to_radians() - a0).rem_euclid(TAU);
    if sw <= 1e-12 { sw = TAU; }
    let mut p = PathData::default();
    p.arc(g(body, 10), g(body, 20), [r, 0.0], [0.0, r], a0, sw, false);
    stroke(p, com.ocs(), 0.0)
}

pub fn ellipse(body: &[Tag], com: &Common) -> Option<Kind> {
    let (mx, my, ratio) = (g(body, 11), g(body, 21), g(body, 40));
    let p0 = g(body, 41);
    let mut p1 = gf(body, 42).unwrap_or(TAU);
    if (p1 - p0).abs() > 1e3 { return None; }
    while p1 <= p0 { p1 += TAU; }
    let sgn = if com.ez < 0.0 { -1.0 } else { 1.0 };
    let mut p = PathData::default();
    p.arc(g(body, 10), g(body, 20), [mx, my], [-my * ratio * sgn, mx * ratio * sgn], p0, p1 - p0, false);
    if (p1 - p0 - TAU).abs() < 1e-9 { p.close(); }
    stroke(p, None, 0.0)
}

pub fn lwpolyline(r: &Reader, body: &[Tag], com: &Common) -> Option<Kind> {
    let fl = gi(body, 70).unwrap_or(0);
    let closed = fl & 1 != 0;
    let width = g(body, 43);
    let mut pts: Vec<(f64, f64, f64)> = vec![];
    for &(c, v) in body {
        match c {
            10 => pts.push((f(v), 0.0, 0.0)),
            20 => if let Some(p) = pts.last_mut() { p.1 = f(v) },
            42 => if let Some(p) = pts.last_mut() { p.2 = f(v) },
            _ => {}
        }
    }
    if restarts_pattern(r, fl, com) { return pline(&pts, closed, com.ocs(), width); }
    let mut path = PathData::default();
    poly_path(&mut path, &pts, closed);
    stroke(path, com.ocs(), width)
}

/// A dashed polyline whose pattern restarts at every vertex.
fn pline(pts: &[(f64, f64, f64)], closed: bool, ocs: Option<Aff>, width: f64) -> Option<Kind> {
    let mut segs = PathData::default();
    poly_path_split(&mut segs, pts, closed);
    if segs.is_empty() { return None; }
    if let Some(o) = ocs { segs.transform(&o); }
    Some(PlineEnt { segs, width: width.max(0.0) }.into())
}

/// Without "linetype generation" (polyline flag 128, off by default) AutoCAD
/// starts the dash pattern again at every vertex. The sink restarts dashes at
/// every sub-path, so such polylines are stored one sub-path per segment, but
/// only when they really have a dash pattern (BYBLOCK is not known here).
fn restarts_pattern(r: &Reader, flags: i32, com: &Common) -> bool {
    if flags & 128 != 0 { return false; }
    let lt = match com.props.ltype { LT_BYLAYER => r.b.layers()[com.props.layer as usize].ltype, x => x };
    lt < LT_BYBLOCK && lt != LT_SOLID && r.b.linetype_has_dashes(lt)
}

/// Like `poly_path`, one sub-path per segment.
pub fn poly_path_split(path: &mut PathData, pts: &[(f64, f64, f64)], closed: bool) {
    if pts.len() < 2 { return; }
    let n = pts.len();
    for k in 0..if closed { n } else { n - 1 } {
        let (a, b) = (pts[k], pts[(k + 1) % n]);
        path.move_to(a.0, a.1);
        path.bulge_to(b.0, b.1, a.2);
    }
}

pub fn polyline(r: &mut Reader, body: &[Tag], com: &Common, verts: &[(f64, f64, f64)]) -> Option<Kind> {
    let fl = gi(body, 70).unwrap_or(0);
    // 3D polylines (flag 8) are in world coordinates, not OCS
    let ocs = if fl & 8 != 0 { None } else { com.ocs() };
    if restarts_pattern(r, fl, com) { return pline(verts, fl & 1 != 0, ocs, g(body, 40)); }
    let mut path = PathData::default();
    poly_path(&mut path, verts, fl & 1 != 0);
    stroke(path, ocs, g(body, 40))
}

pub fn spline(body: &[Tag]) -> Option<Kind> {
    let (mut deg, mut knots, mut ctrl, mut fit) = (3usize, vec![], Vec::<[f64; 2]>::new(), Vec::<[f64; 2]>::new());
    for &(c, v) in body {
        // HELIX: its own axis point (10) and radius (40) follow; they are not spline data.
        if c == 100 && v.trim_ascii() == b"AcDbHelix" { break; }
        match c {
            71 => deg = i(v).clamp(1, 32) as usize,
            40 => knots.push(f(v)),
            10 => ctrl.push([f(v), 0.0]),
            20 => if let Some(p) = ctrl.last_mut() { p[1] = f(v) },
            11 => fit.push([f(v), 0.0]),
            21 => if let Some(p) = fit.last_mut() { p[1] = f(v) },
            _ => {}
        }
    }
    let mut path = PathData::default();
    if ctrl.len() >= 2 { bspline(deg, &knots, &ctrl, &mut path, false); } else { polyline_pts(&fit, &mut path, false); }
    stroke(path, None, 0.0)
}

pub fn leader(body: &[Tag]) -> Option<Kind> {
    let mut pts: Vec<(f64, f64, f64)> = vec![];
    for &(c, v) in body {
        match c { 10 => pts.push((f(v), 0.0, 0.0)), 20 => if let Some(p) = pts.last_mut() { p.1 = f(v) }, _ => {} }
    }
    let mut path = PathData::default();
    poly_path(&mut path, &pts, false);
    stroke(path, None, 0.0)
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn path_of(body: &[Tag]) -> Vec<[f64; 2]> {
        match spline(body) { Some(Kind::Stroke(s)) => s.path.pts, _ => panic!("no stroke") }
    }

    #[test]
    fn helix_axis_and_radius_are_not_spline_data() {
        let spline_part: Vec<Tag> = vec![
            (100, b"AcDbSpline"), (71, b"1"),
            (40, b"0"), (40, b"0"), (40, b"1"), (40, b"1"),
            (10, b"0"), (20, b"0"), (10, b"10"), (20, b"0"),
        ];
        let mut helix = spline_part.clone();
        helix.extend_from_slice(&[(100, b"AcDbHelix" as &[u8]), (10, b"5"), (20, b"5"), (40, b"3")]);
        assert_eq!(path_of(&spline_part), path_of(&helix));
    }
}
