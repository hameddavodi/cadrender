//! SOLID, TRACE and WIPEOUT.

use super::Common;
use crate::tags::{f, gf, gi, Tag};
use cadrender_core::entity::fill::FillEnt;
use cadrender_core::entity::Kind;
use cadrender_core::geom::PathData;

fn g(body: &[Tag], code: i32) -> f64 { gf(body, code).unwrap_or(0.0) }

pub fn solid(body: &[Tag], com: &Common) -> Option<Kind> {
    let mut path = PathData::default();
    path.move_to(g(body, 10), g(body, 20));
    path.line_to(g(body, 11), g(body, 21));
    // SOLID corner order is 1, 2, 4, 3
    if body.iter().any(|t| t.0 == 13) { path.line_to(g(body, 13), g(body, 23)); }
    path.line_to(g(body, 12), g(body, 22));
    path.close();
    if let Some(o) = com.ocs() { path.transform(&o); }
    Some(FillEnt { path, background: false }.into())
}

/// WIPEOUT: a mask in the page colour. Clip points are in image pixel space.
pub fn wipeout(body: &[Tag]) -> Option<Kind> {
    let (ox, oy, ux, uy, vx, vy, h) = (g(body, 10), g(body, 20), g(body, 11), g(body, 21), g(body, 12), g(body, 22), g(body, 23));
    let mut pts: Vec<(f64, f64)> = vec![];
    for &(c, v) in body { match c { 14 => pts.push((f(v), 0.0)), 24 => if let Some(p) = pts.last_mut() { p.1 = f(v) }, _ => {} } }
    // clip type 1 = rectangle given by two corners
    if gi(body, 71).unwrap_or(1) == 1 && pts.len() == 2 { let (a, b) = (pts[0], pts[1]); pts = vec![a, (b.0, a.1), b, (a.0, b.1)]; }
    if pts.len() < 3 { return None; }
    let (ox, oy) = (ox + ux * 0.5 - vx * 0.5, oy + uy * 0.5 - vy * 0.5);
    let mut path = PathData::default();
    for (k, p) in pts.iter().enumerate() {
        let (x, y) = (ox + ux * p.0 + vx * (h - p.1), oy + uy * p.0 + vy * (h - p.1));
        if k == 0 { path.move_to(x, y) } else { path.line_to(x, y) }
    }
    path.close();
    Some(FillEnt { path, background: true }.into())
}
