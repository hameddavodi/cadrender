//! POINT, RAY, XLINE, 3DFACE, polyface and polygon meshes (wireframe, seen
//! from above: z is dropped) and ATTDEF. Same rules as
//! `cadrender-dxf/src/entities/shapes.rs`.

use crate::entities::Com;
use crate::tables::Ctx;
use cadrender_core::entity::point::PointEnt;
use cadrender_core::entity::stroke::StrokeEnt;
use cadrender_core::entity::xline::XLineEnt;
use cadrender_core::entity::Kind;
use cadrender_core::geom::PathData;
use cadrender_dwg::{Attrib, EntityKind as K, Face3d, PolyMesh, XLine};
use std::collections::HashSet;

fn stroke(path: PathData) -> Option<Kind> {
    if path.is_empty() || !path.pts.iter().all(|p| p[0].is_finite() && p[1].is_finite()) { return None; }
    Some(StrokeEnt { path, width: 0.0 }.into())
}

/// RAY and XLINE (world coordinates).
pub fn xline(x: &XLine) -> Option<Kind> {
    let dir = [x.direction[0], x.direction[1]];
    if dir[0].hypot(dir[1]).is_nan() || dir[0].hypot(dir[1]) < 1e-12 || !x.base[0].is_finite() || !x.base[1].is_finite() { return None; }
    Some(XLineEnt { base: [x.base[0], x.base[1]], dir, ray: x.ray }.into())
}

/// 3DFACE: 3 or 4 corners; bits 1..8 hide edges 1..4.
pub fn face3d(f: &Face3d) -> Option<Kind> {
    let c: Vec<[f64; 2]> = f.corners.iter().map(|p| [p[0], p[1]]).collect();
    let n = if c[3] == c[2] { 3 } else { 4 };
    let mut p = PathData::default();
    let mut pen = false;
    for e in 0..n {
        let (a, b) = (c[e], c[(e + 1) % n]);
        if f.invisible_edges & (1 << e) != 0 { pen = false; continue; }
        if !pen { p.move_to(a[0], a[1]); }
        p.line_to(b[0], b[1]);
        pen = true;
    }
    stroke(p)
}

/// MESH: the edges of its faces, seen from above (like the DXF reader).
pub fn mesh(m: &cadrender_dwg::Mesh) -> Option<Kind> {
    let verts: Vec<[f64; 2]> = m.vertices.iter().map(|p| [p[0], p[1]]).collect();
    let faces: Vec<Vec<i64>> = m.faces.iter().map(|f| f.iter().map(|&i| i as i64 + 1).collect()).collect();
    stroke(edges(&verts, &faces))
}

/// Edges of polygons given as 1-based vertex numbers, each edge once. A
/// negative number hides the edge that starts at that vertex.
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

impl Ctx<'_> {
    /// POINT. Points on "Defpoints" are never plotted: left out and counted.
    pub fn point(&mut self, pt: &cadrender_dwg::Point, com: &Com) -> Option<Kind> {
        if self.b.layers()[com.props.layer as usize].name.eq_ignore_ascii_case("defpoints") {
            self.b.skip("POINT(defpoints)");
            return None;
        }
        let (mut x, y) = (pt.position[0], pt.position[1]);
        if !(x.is_finite() && y.is_finite()) { return None; }
        if com.ez < 0.0 { x = -x; }
        Some(PointEnt { pos: [x, y] }.into())
    }

    /// Polyface mesh (vertex records, then face records) or polygon mesh.
    pub fn polymesh(&self, pm: &PolyMesh) -> Option<Kind> {
        let d = self.d;
        let (mut verts, mut faces): (Vec<[f64; 2]>, Vec<Vec<i64>>) = (vec![], vec![]);
        for &h in &pm.vertices {
            match d.get(h).and_then(|o| o.entity()).map(|e| &e.kind) {
                Some(K::Vertex3d(v)) if v.flags & 16 == 0 => verts.push([v.point[0], v.point[1]]),
                Some(K::FaceRecord(f)) => faces.push(f.index.iter().filter(|&&x| x != 0).map(|&x| x as i64).collect()),
                _ => {}
            }
        }
        if pm.pface { return stroke(edges(&verts, &faces)); }
        let (m, n) = (pm.counts.0.max(0) as usize, pm.counts.1.max(0) as usize);
        if m == 0 || n == 0 || verts.len() < m * n { return None; }
        let at = |a: usize, b: usize| verts[a * n + b];
        let mut p = PathData::default();
        let mut line = |pts: &mut dyn Iterator<Item = [f64; 2]>| {
            for (k, q) in pts.enumerate() { if k == 0 { p.move_to(q[0], q[1]) } else { p.line_to(q[0], q[1]) } }
        };
        for a in 0..m { line(&mut (0..n).chain((pm.flags & 32 != 0).then_some(0)).map(|b| at(a, b))); }
        for b in 0..n { line(&mut (0..m).chain((pm.flags & 1 != 0).then_some(0)).map(|a| at(a, b))); }
        stroke(p)
    }

    /// ATTDEF. In a block definition only a constant attribute (flag 2) shows
    /// its value; outside a block AutoCAD shows the tag.
    pub fn attdef(&mut self, a: &Attrib, com: &Com) -> Option<Kind> {
        let mut t = a.text.clone();
        if self.block_def {
            if a.flags & 2 == 0 || a.flags & 1 != 0 { return None; }
        } else {
            if a.tag.trim().is_empty() { return None; }
            t.value = a.tag.clone();
        }
        self.text_kind(&K::Text(t), com).map(Kind::from)
    }
}
