//! Entity kinds that need more than a few lines: ELLIPSE, POLYLINE (2D/3D),
//! SPLINE, SOLID, WIPEOUT, INSERT (+ ATTRIBs), VIEWPORT.

use crate::entities::Com;
use crate::spline::{bspline, interpolate, poly_path, polyline_pts};
use crate::tables::{deg, Ctx};
use cadrender_core::entity::fill::FillEnt;
use cadrender_core::entity::insert::InsertEnt;
use cadrender_core::entity::pline::PlineEnt;
use cadrender_core::scene::tables::{LT_BYBLOCK, LT_BYLAYER, LT_SOLID};
use cadrender_core::entity::stroke::StrokeEnt;
use cadrender_core::entity::viewport::ViewportEnt;
use cadrender_core::entity::{Entity, Kind};
use cadrender_core::geom::{Aff, PathData};
use cadrender_core::scene::tables::Handle;
use cadrender_dwg::{EntityKind as K, Insert, Polyline2d, Polyline3d, Solid, Spline, Viewport, Wipeout, ObjectKind};
use std::f64::consts::TAU;

fn finite(p: &PathData) -> bool { p.pts.iter().all(|q| q[0].is_finite() && q[1].is_finite()) }

pub fn stroke(mut path: PathData, ocs: Option<Aff>, width: f64) -> Option<Kind> {
    if path.is_empty() || !finite(&path) { return None; }
    if let Some(o) = ocs { path.transform(&o); }
    Some(StrokeEnt { path, width: if width.is_finite() { width.max(0.0) } else { 0.0 } }.into())
}

pub fn spline(s: &Spline) -> Option<Kind> {
    let deg = s.degree.clamp(1, 32) as usize;
    let ctrl: Vec<[f64; 2]> = s.control_points.iter().map(|p| [p[0], p[1]]).collect();
    let mut path = PathData::default();
    if ctrl.len() >= 2 {
        bspline(deg, &s.knots, &ctrl, &mut path, false);
    } else {
        // Only fit points stored: build the curve through them (ODA computes
        // control points for its DXF; the DXF reader then draws that curve).
        let fit: Vec<[f64; 2]> = s.fit_points.iter().map(|p| [p[0], p[1]]).collect();
        let t0 = [s.start_tangent[0], s.start_tangent[1]];
        let t1 = [s.end_tangent[0], s.end_tangent[1]];
        match interpolate(&fit, t0, t1) {
            Some((knots, cps)) => bspline(3, &knots, &cps, &mut path, false),
            None => polyline_pts(&fit, &mut path, false),
        }
    }
    stroke(path, None, 0.0)
}

/// HELIX: drawn from its helix values like ODA's DXF (see [`crate::spline::helix`]),
/// else from the stored spline.
pub fn helix(h: &cadrender_dwg::Helix) -> Option<Kind> {
    let Some((knots, ctrl)) = crate::spline::helix(h) else { return spline(&h.spline) };
    let mut path = PathData::default();
    bspline(3, &knots, &ctrl, &mut path, false);
    stroke(path, None, 0.0)
}

pub fn solid(s: &Solid, com: &Com) -> Option<Kind> {
    let c = &s.corners;
    let mut path = PathData::default();
    // SOLID corner order is 1, 2, 4, 3
    path.move_to(c[0][0], c[0][1]);
    path.line_to(c[1][0], c[1][1]);
    path.line_to(c[3][0], c[3][1]);
    path.line_to(c[2][0], c[2][1]);
    path.close();
    if !finite(&path) { return None; }
    if let Some(o) = com.ocs() { path.transform(&o); }
    Some(FillEnt { path, background: false }.into())
}

/// WIPEOUT: a mask in the page colour. Clip points are in image pixel space.
pub fn wipeout(w: &Wipeout) -> Option<Kind> {
    let (ox, oy) = (w.insertion[0], w.insertion[1]);
    let (ux, uy, vx, vy, h) = (w.u_vector[0], w.u_vector[1], w.v_vector[0], w.v_vector[1], w.size[1]);
    let mut pts: Vec<(f64, f64)> = w.clip_vertices.iter().map(|p| (p[0], p[1])).collect();
    if w.clip_type == 1 && pts.len() == 2 { let (a, b) = (pts[0], pts[1]); pts = vec![a, (b.0, a.1), b, (a.0, b.1)]; }
    if pts.len() < 3 { return None; }
    let (ox, oy) = (ox + ux * 0.5 - vx * 0.5, oy + uy * 0.5 - vy * 0.5);
    let mut path = PathData::default();
    for (k, p) in pts.iter().enumerate() {
        let (x, y) = (ox + ux * p.0 + vx * (h - p.1), oy + uy * p.0 + vy * (h - p.1));
        if k == 0 { path.move_to(x, y) } else { path.line_to(x, y) }
    }
    path.close();
    if !finite(&path) { return None; }
    Some(FillEnt { path, background: true }.into())
}

impl Ctx<'_> {
    pub fn ellipse(&self, e: &cadrender_dwg::Ellipse, com: &Com) -> Option<Kind> {
        let (mx, my, ratio) = (e.major_axis[0], e.major_axis[1], e.axis_ratio);
        let p0 = e.start_param;
        let mut p1 = e.end_param;
        if !p0.is_finite() || !p1.is_finite() || (p1 - p0).abs() > 1e3 { return None; }
        while p1 <= p0 { p1 += TAU; }
        let sgn = if com.ez < 0.0 { -1.0 } else { 1.0 };
        let mut p = PathData::default();
        p.arc(e.center[0], e.center[1], [mx, my], [-my * ratio * sgn, mx * ratio * sgn], p0, p1 - p0, false);
        if (p1 - p0 - TAU).abs() < 1e-9 { p.close(); }
        stroke(p, None, 0.0)
    }

    pub fn polyline2d(&mut self, pl: &Polyline2d, com: &Com) -> Option<Kind> {
        let d = self.d;
        let mut verts = vec![];
        for &h in &pl.vertices {
            let Some(K::Vertex2d(v)) = d.get(h).and_then(|o| o.entity()).map(|e| &e.kind) else { continue };
            let fl = v.flags as i32;
            // skip spline frame points and face records
            if fl & 16 == 0 && !(fl & 128 != 0 && fl & 64 == 0) { verts.push((v.point[0], v.point[1], v.bulge)); }
        }
        if self.restarts_pattern(pl.flags as i32, com) { return pline(&verts, pl.flags & 1 != 0, com.ocs(), pl.start_width); }
        let mut path = PathData::default();
        poly_path(&mut path, &verts, pl.flags & 1 != 0);
        stroke(path, com.ocs(), pl.start_width)
    }

    /// 3D polyline: world coordinates, no OCS, no width.
    pub fn polyline3d(&mut self, pl: &Polyline3d, com: &Com) -> Option<Kind> {
        let d = self.d;
        let mut verts = vec![];
        for &h in &pl.vertices {
            let Some(K::Vertex3d(v)) = d.get(h).and_then(|o| o.entity()).map(|e| &e.kind) else { continue };
            if v.flags & 16 == 0 { verts.push((v.point[0], v.point[1], 0.0)); }
        }
        // DXF flag 70 of a 3D polyline never has bit 128
        if self.restarts_pattern(8, com) { return pline(&verts, pl.closed(), None, 0.0); }
        let mut path = PathData::default();
        poly_path(&mut path, &verts, pl.closed());
        stroke(path, None, 0.0)
    }

    pub fn block_ref(&mut self, rec: Handle, m: Aff) -> Option<Kind> {
        let block = *self.block_of.get(&rec)?;
        if !m.0.iter().all(|v| v.is_finite()) { return None; }
        Some(InsertEnt::new(block, m).into())
    }

    pub fn insert(&mut self, i: &Insert, com: &Com) -> Option<Kind> {
        let (mut sx, mut sy) = (i.scale[0], i.scale[1]);
        if sx == 0.0 { sx = 1.0; }
        if sy == 0.0 { sy = 1.0; }
        let m = Aff::translate(i.insertion[0], i.insertion[1]).mul(&Aff::rotate(i.rotation)).mul(&Aff::scale(sx, sy));
        let m = match com.ocs() { Some(o) => o.mul(&m), None => m };
        let Some(Kind::Insert(mut ins)) = self.block_ref(i.block_header, m) else { return None };
        if let Some((cols, rows, dx, dy)) = i.grid {
            ins.set_grid((cols as u32).clamp(1, 10_000), (rows as u32).clamp(1, 10_000), dx, dy);
        }
        let d = self.d;
        for &a in &i.attribs {
            let Some(o) = d.get(a) else { continue };
            let Some(e) = o.entity() else { continue };
            if e.common.invisible || !matches!(e.kind, K::Attrib(_)) { continue; }
            let ac = self.com(a, e);
            if let Some(te) = self.text_kind(&e.kind, &ac) { ins.attribs.push(Entity::new(ac.props, te)); }
        }
        Some(Kind::Insert(ins))
    }

    /// VIEWPORT. DWG has no status (DXF 68): ODA writes the 1-based position
    /// among the viewports of the layout block (entity order), 0 when off.
    /// (The LAYOUT's own viewport list is empty in our R2010+ files.)
    pub fn viewport(&mut self, h: Handle, v: &Viewport) -> Kind {
        let pos = self.vp_pos.get(&h).copied().unwrap_or(0);
        let status = if v.status & 0x2_0000 != 0 { 0 } else { pos };
        let dir = v.view_direction;
        let top_view = dir[0].abs() < 1e-9 && dir[1].abs() < 1e-9 && dir[2] >= 0.0;
        ViewportEnt {
            handle: h,
            center: [v.center[0], v.center[1]], w: v.width, h: v.height,
            view_center: v.view_center, target: [v.view_target[0], v.view_target[1]], view_h: v.view_height, twist: deg(v.twist_angle),
            status, id: pos, flags: v.status,
            frozen_handles: v.frozen_layers.clone(), frozen_layers: vec![], top_view, view_dir: dir, target_z: v.view_target[2],
            clip_handle: v.clip_boundary, clip: None,
        }.into()
    }

    /// 1-based position of every viewport among the viewports of its layout block.
    pub fn index_viewports(&mut self) {
        let d = self.d;
        for o in &d.objects {
            let ObjectKind::BlockHeader(bh) = &o.kind else { continue };
            if bh.layout == 0 { continue; }
            let mut n = 0;
            for &h in &bh.entities {
                if let Some(K::Viewport(_)) = d.get(h).and_then(|x| x.entity()).map(|e| &e.kind) {
                    n += 1;
                    self.vp_pos.insert(h, n);
                }
            }
        }
    }
}

/// A dashed polyline whose pattern restarts at every vertex: one sub-path per
/// segment (`PlineEnt`), like the DXF reader.
pub fn pline(pts: &[(f64, f64, f64)], closed: bool, ocs: Option<Aff>, width: f64) -> Option<Kind> {
    let mut segs = PathData::default();
    if pts.len() >= 2 {
        let n = pts.len();
        for k in 0..if closed { n } else { n - 1 } {
            let (a, b) = (pts[k], pts[(k + 1) % n]);
            segs.move_to(a.0, a.1);
            segs.bulge_to(b.0, b.1, a.2);
        }
    }
    if segs.is_empty() || !finite(&segs) { return None; }
    if let Some(o) = ocs { segs.transform(&o); }
    Some(PlineEnt { segs, width: if width.is_finite() { width.max(0.0) } else { 0.0 } }.into())
}

impl Ctx<'_> {
    /// Without "linetype generation" (DXF polyline flag 128) AutoCAD starts
    /// the dash pattern again at every vertex (same test as the DXF reader).
    pub fn restarts_pattern(&self, dxf_flags: i32, com: &Com) -> bool {
        if dxf_flags & 128 != 0 { return false; }
        let lt = match com.props.ltype { LT_BYLAYER => self.b.layers()[com.props.layer as usize].ltype, x => x };
        lt < LT_BYBLOCK && lt != LT_SOLID && self.b.linetype_has_dashes(lt)
    }
}
