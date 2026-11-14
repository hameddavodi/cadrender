//! POINT, RAY, XLINE, 3DFACE, MESH, polyface and polygon meshes (wireframe,
//! seen from above: z is dropped), and ATTDEF.

use super::Common;
use crate::reader::Reader;
use crate::tags::{f, find, gf, gi, i, trim, Tag};
use cadrender_core::entity::point::PointEnt;
use cadrender_core::entity::stroke::StrokeEnt;
use cadrender_core::entity::xline::XLineEnt;
use cadrender_core::entity::Kind;
use cadrender_core::geom::PathData;
use std::collections::HashSet;

fn g(body: &[Tag], code: i32) -> f64 { gf(body, code).unwrap_or(0.0) }

fn stroke(path: PathData) -> Option<Kind> { if path.is_empty() { None } else { Some(StrokeEnt { path, width: 0.0 }.into()) } }

/// POINT. Points on the "Defpoints" layer are never plotted by AutoCAD (they
/// are the definition points of dimensions): left out and counted.
pub fn point(r: &mut Reader, body: &[Tag], com: &Common) -> Option<Kind> {
    if r.b.layers()[com.props.layer as usize].name.eq_ignore_ascii_case("defpoints") {
        r.b.skip("POINT(defpoints)");
        return None;
    }
    let (mut x, y) = (g(body, 10), g(body, 20));
    if com.ez < 0.0 { x = -x; }
    Some(PointEnt { pos: [x, y] }.into())
}

/// RAY and XLINE (start point 10, direction 11, both in world coordinates).
pub fn xline(kind: &[u8], body: &[Tag]) -> Option<Kind> {
    let dir = [g(body, 11), g(body, 21)];
    if dir[0].hypot(dir[1]) < 1e-12 { return None; } // seen along the line: a point, nothing to draw
    Some(XLineEnt { base: [g(body, 10), g(body, 20)], dir, ray: kind == b"RAY" }.into())
}

/// 3DFACE: 3 or 4 corners; bits of group 70 hide edges 1..4.
pub fn face3d(body: &[Tag]) -> Option<Kind> {
    let c: Vec<[f64; 2]> = (0..4).map(|k| [g(body, 10 + k), g(body, 20 + k)]).collect();
    let hidden = gi(body, 70).unwrap_or(0);
    let n = if c[3] == c[2] { 3 } else { 4 };
    let mut p = PathData::default();
    let mut pen = false;
    for e in 0..n {
        let (a, b) = (c[e], c[(e + 1) % n]);
        if hidden & (1 << e) != 0 { pen = false; continue; }
        if !pen { p.move_to(a[0], a[1]); }
        p.line_to(b[0], b[1]);
        pen = true;
    }
    stroke(p)
}

/// Edges of polygons given as 1-based vertex numbers, each edge once. A
/// negative number hides the edge that starts at that vertex (polyface meshes).
fn edges(verts: &[[f64; 2]], faces: &[Vec<i64>]) -> PathData {
    let mut seen: HashSet<(usize, usize)> = HashSet::new();
    let mut p = PathData::default();
    for face in faces {
        let n = face.len();
        for k in 0..n {
            let (a, b) = (face[k], face[(k + 1) % n]);
            if a <= 0 || b == 0 { continue; }
            let (a, b) = (a as usize - 1, b.unsigned_abs() as usize - 1);
            let (Some(pa), Some(pb)) = (verts.get(a), verts.get(b)) else { continue };
            if a == b || !seen.insert((a.min(b), a.max(b))) { continue; }
            p.move_to(pa[0], pa[1]);
            p.line_to(pb[0], pb[1]);
        }
    }
    p
}

/// MESH (subdivision mesh): vertices (92 + 10/20/30), faces (93 + 90 lists).
pub fn mesh(body: &[Tag]) -> Option<Kind> {
    let mut verts: Vec<[f64; 2]> = vec![];
    let mut list: Vec<i64> = vec![];
    let mut stage = 0; // 1 = vertices, 2 = face list, 3 = past it
    let mut left = 0i64;
    for &(c, v) in body {
        match (stage, c) {
            (_, 92) if stage == 0 => stage = 1,
            (1, 10) => verts.push([f(v), 0.0]),
            (1, 20) => if let Some(p) = verts.last_mut() { p[1] = f(v) },
            (1 | 0, 93) => { stage = 2; left = i(v).max(0) as i64; }
            (2, 90) if left > 0 => { list.push(i(v) as i64); left -= 1; if left == 0 { stage = 3; } }
            (2, _) => stage = 3,
            _ => {}
        }
    }
    let mut faces = vec![];
    let mut k = 0;
    while k < list.len() {
        let n = list[k].max(0) as usize;
        let end = (k + 1 + n).min(list.len());
        faces.push(list[k + 1..end].iter().map(|&x| x + 1).collect());
        k = end;
    }
    stroke(edges(&verts, &faces))
}

/// POLYLINE polyface mesh (flag 64): vertex records, then face records with
/// 1-based indices (negative = invisible edge).
pub fn polyface(verts: &[(f64, f64, f64)], faces: &[[i32; 4]]) -> Option<Kind> {
    let v: Vec<[f64; 2]> = verts.iter().map(|p| [p.0, p.1]).collect();
    let f: Vec<Vec<i64>> = faces.iter().map(|fc| fc.iter().filter(|&&x| x != 0).map(|&x| x as i64).collect()).collect();
    stroke(edges(&v, &f))
}

/// POLYLINE polygon mesh (flag 16): M x N vertices; flag 1 / 32 close M / N.
pub fn polygon_mesh(body: &[Tag], verts: &[(f64, f64, f64)]) -> Option<Kind> {
    let fl = gi(body, 70).unwrap_or(0);
    let (m, n) = (gi(body, 71).unwrap_or(0).max(0) as usize, gi(body, 72).unwrap_or(0).max(0) as usize);
    if m == 0 || n == 0 || verts.len() < m * n { return None; }
    let at = |a: usize, b: usize| { let p = verts[a * n + b]; [p.0, p.1] };
    let mut p = PathData::default();
    let line = |p: &mut PathData, pts: &mut dyn Iterator<Item = [f64; 2]>| {
        for (k, q) in pts.enumerate() { if k == 0 { p.move_to(q[0], q[1]) } else { p.line_to(q[0], q[1]) } }
    };
    for a in 0..m {
        let close = if fl & 32 != 0 { Some(0) } else { None };
        line(&mut p, &mut (0..n).chain(close).map(|b| at(a, b)));
    }
    for b in 0..n {
        let close = if fl & 1 != 0 { Some(0) } else { None };
        line(&mut p, &mut (0..m).chain(close).map(|a| at(a, b)));
    }
    stroke(p)
}

/// ATTDEF. In a block definition it is a template for the ATTRIBs of each
/// INSERT and is not drawn, except a constant attribute (flag 2), which has no
/// ATTRIB and shows its value. Outside a block (model or paper space) AutoCAD
/// shows the tag.
pub fn attdef(r: &mut Reader, body: &[Tag], com: &Common) -> Option<Kind> {
    let flags = gi(body, 70).unwrap_or(0);
    let mut b: Vec<Tag> = body.iter().filter(|t| t.0 != 70).copied().collect();
    if r.block_def {
        if flags & 2 == 0 || flags & 1 != 0 { return None; }
    } else {
        let tag = find(body, 2).unwrap_or(b"");
        if trim(tag).is_empty() { return None; }
        b.retain(|t| t.0 != 1);
        b.push((1, tag));
    }
    super::text::text(r, b"ATTRIB", &b, com).map(Kind::from)
}
