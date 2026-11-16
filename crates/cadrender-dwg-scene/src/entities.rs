//! Entities: common properties and one function per entity type. The rules
//! (what is drawn, defaults, OCS mirror) follow `cadrender-dxf/src/entities/`.

use crate::kinds::{helix, solid, spline, stroke, wipeout};
use crate::spline::poly_path;
use crate::tables::Ctx;
use cadrender_core::color::EntColor;
use cadrender_core::entity::{Entity, Kind, Props};
use cadrender_core::geom::{Aff, PathData};
use cadrender_core::scene::tables::{BlockId, Handle, LT_BYBLOCK, LT_BYLAYER, LT_SOLID};
use cadrender_dwg::{Color, Entity as DEnt, EntityKind as K, LtypeRef, ObjectKind};
use std::f64::consts::TAU;

/// Common entity facts the kind functions need.
#[derive(Clone, Copy)]
pub struct Com {
    pub props: Props,
    /// Z of the extrusion direction (only the sign is used: < 0 = mirrored OCS).
    pub ez: f64,
}

impl Com {
    pub fn ocs(&self) -> Option<Aff> { if self.ez < 0.0 { Some(Aff::scale(-1.0, 1.0)) } else { None } }
}

/// DXF type name used for the statistics (`SceneStats::counts`).
pub fn dxf_name(type_name: &str) -> &str {
    match type_name {
        n if n.starts_with("DIMENSION_") => "DIMENSION",
        n if n.starts_with("POLYLINE_") => "POLYLINE",
        n if n.starts_with("VERTEX_") => "VERTEX",
        "MINSERT" => "INSERT",
        n => n,
    }
}

/// Extrusion Z of the kinds that have an OCS.
fn ez(k: &K) -> f64 {
    let e = match k {
        K::Circle(c) => c.extrusion, K::Arc(a) => a.extrusion, K::Ellipse(e) => e.extrusion,
        K::Solid(s) | K::Trace(s) => s.extrusion, K::LwPolyline(p) => p.extrusion, K::Polyline2d(p) => p.extrusion,
        K::Insert(i) => i.extrusion, K::Text(t) => t.extrusion, K::Attrib(a) => a.text.extrusion,
        K::MText(m) => m.extrusion, K::Hatch(h) => h.extrusion, K::Dimension(d) => d.extrusion,
        K::Line(l) => l.extrusion, K::Spline(_) => [0.0, 0.0, 1.0],
        K::Leader(l) => l.extrusion, K::Table(t) => t.insert.extrusion, K::Point(p) => p.extrusion,
        K::AttDef(a) => a.text.extrusion,
        _ => [0.0, 0.0, 1.0],
    };
    e[2]
}

impl Ctx<'_> {
    /// Common properties, like the DXF reader's `common()`.
    pub fn com(&mut self, handle: Handle, e: &DEnt) -> Com {
        let c = &e.common;
        let mut p = Props { handle, ..Props::default() };
        p.layer = self.layer_of.get(&c.layer).copied().unwrap_or(0);
        p.color = match c.color.color {
            Color::ByBlock => EntColor::BYBLOCK,
            Color::Index(i) => EntColor::aci(i as i32),
            Color::Rgb(rgb) => EntColor::true_color(rgb),
            Color::ByLayer | Color::None => EntColor::BYLAYER,
        };
        p.ltype = match c.ltype {
            LtypeRef::ByLayer => LT_BYLAYER,
            LtypeRef::ByBlock => LT_BYBLOCK,
            LtypeRef::Continuous => LT_SOLID,
            LtypeRef::Handle(h) => self.ltype_of.get(&h).copied().unwrap_or(LT_SOLID),
        };
        p.lw = c.lineweight_mm100().clamp(-3, 211);
        p.ltscale = if c.ltype_scale > 0.0 && c.ltype_scale.is_finite() { c.ltype_scale as f32 } else { 1.0 };
        if let Some(t) = c.color.transparency { if t & 0x0200_0000 != 0 { p.alpha = (t & 0xff) as u8; } }
        Com { props: p, ez: ez(&e.kind) }
    }

    /// Every block's entities, in the block's own order.
    pub fn entities(&mut self) {
        let d = self.d;
        let blocks: Vec<(BlockId, &[Handle])> = d.objects.iter().filter_map(|o| match &o.kind {
            ObjectKind::BlockHeader(bh) => Some((*self.block_of.get(&o.handle)?, bh.entities.as_slice())),
            _ => None,
        }).collect();
        for (block, hs) in blocks {
            // ATTDEF rules: is this a block definition (not model / paper space)?
            let n = self.b.block_name(block).to_ascii_lowercase();
            self.block_def = !(n.starts_with("*model_space") || n.starts_with("*paper_space"));
            for &h in hs {
                self.record_meta(block, h);
                if let Some(e) = self.entity(h) { self.b.push(block, e); }
            }
        }
    }

    /// One entity by handle, or `None` if it is not drawn.
    pub fn entity(&mut self, h: Handle) -> Option<Entity> {
        let d = self.d;
        let o = d.get(h)?;
        let e = o.entity()?;
        let name = dxf_name(d.type_name(o.type_code));
        if matches!(e.kind, K::Block { .. } | K::EndBlk | K::SeqEnd) { return None; }
        self.b.count(name);
        if let K::Insert(i) = &e.kind { for &a in &i.attribs { if d.get(a).and_then(|x| x.entity()).is_some() { self.b.count("ATTRIB"); } } }
        if e.common.invisible { return None; }
        let com = self.com(h, e);
        let k: Option<Kind> = match &e.kind {
            K::Line(l) => { let mut p = PathData::default(); p.move_to(l.start[0], l.start[1]); p.line_to(l.end[0], l.end[1]); stroke(p, None, 0.0) }
            K::Circle(c) => {
                if c.radius.is_nan() || c.radius <= 0.0 { return None; }
                let mut p = PathData::default();
                p.arc(c.center[0], c.center[1], [c.radius, 0.0], [0.0, c.radius], 0.0, TAU, false);
                p.close();
                stroke(p, com.ocs(), 0.0)
            }
            K::Arc(a) => {
                if a.radius.is_nan() || a.radius <= 0.0 || !a.start_angle.is_finite() || !a.end_angle.is_finite() { return None; }
                let mut sw = (a.end_angle - a.start_angle).rem_euclid(TAU);
                if sw <= 1e-12 { sw = TAU; }
                let mut p = PathData::default();
                p.arc(a.center[0], a.center[1], [a.radius, 0.0], [0.0, a.radius], a.start_angle, sw, false);
                stroke(p, com.ocs(), 0.0)
            }
            K::Ellipse(el) => self.ellipse(el, &com),
            K::LwPolyline(pl) => {
                let pts: Vec<(f64, f64, f64)> = pl.vertices.iter().map(|v| (v.point[0], v.point[1], v.bulge)).collect();
                // DXF flag 70: 1 closed, 128 linetype generation (DWG 0x200 / 0x100)
                let fl = pl.closed() as i32 | if pl.flags & 0x100 != 0 { 128 } else { 0 };
                if self.restarts_pattern(fl, &com) { return Some(Entity::new(com.props, crate::kinds::pline(&pts, pl.closed(), com.ocs(), lw_width(pl))?)); }
                let mut path = PathData::default();
                poly_path(&mut path, &pts, pl.closed());
                stroke(path, com.ocs(), lw_width(pl))
            }
            K::Polyline2d(pl) => self.polyline2d(pl, &com),
            K::Polyline3d(pl) => self.polyline3d(pl, &com),
            K::PolyMesh(pm) => self.polymesh(pm),
            K::Point(pt) => self.point(pt, &com),
            K::XLine(x) => crate::shapes::xline(x),
            K::Face3d(f) => crate::shapes::face3d(f),
            K::Mesh(m) => crate::shapes::mesh(m),
            K::Wipeout(w) if name == "IMAGE" => self.image(w),
            K::AttDef(a) => self.attdef(a, &com),
            K::Spline(s) => spline(s),
            K::Helix(h) => helix(h),
            K::Leader(l) => {
                let pts: Vec<(f64, f64, f64)> = l.points.iter().map(|p| (p[0], p[1], 0.0)).collect();
                let mut path = PathData::default();
                poly_path(&mut path, &pts, false);
                stroke(path, None, 0.0)
            }
            K::Solid(s) | K::Trace(s) => solid(s, &com),
            K::Wipeout(w) if name == "WIPEOUT" => wipeout(w),
            K::Hatch(hatch) => crate::hatch::hatch(hatch, com.ocs()),
            K::Insert(i) => self.insert(i, &com),
            K::Dimension(dm) if dm.kind >= 0x14 => self.block_ref(dm.block, Aff::ID),
            K::Table(t) => {
                let ins = &t.insert;
                let m = Aff::translate(ins.insertion[0], ins.insertion[1]).mul(&Aff::rotate(ins.rotation));
                self.block_ref(ins.block_header, match com.ocs() { Some(o) => o.mul(&m), None => m })
            }
            K::Text(_) => self.text_kind(&e.kind, &com).map(Kind::from),
            K::MText(m) => { let rt = self.rich_text(o.xdict); self.mtext_kind(m, rt.as_deref(), &com).map(Kind::from) }
            K::Viewport(v) => Some(self.viewport(h, v)),
            K::MLeader(m) => self.mleader(m, &com),
            K::Acis(a) => self.acis(a, name),
            _ => { self.b.skip(name); return None; }
        };
        Some(Entity::new(com.props, k?))
    }
}

/// Constant width of a LWPOLYLINE as ODA writes it (DXF 43): the stored
/// constant width, or the common width when every vertex has the same start
/// and end width. Varying widths: 0 (the DXF reader reads only group 43).
fn lw_width(pl: &cadrender_dwg::LwPolyline) -> f64 {
    if pl.const_width != 0.0 { return pl.const_width; }
    let Some(first) = pl.vertices.first() else { return 0.0 };
    let w = first.start_width;
    if pl.vertices.iter().all(|v| v.start_width == w && v.end_width == w) { w } else { 0.0 }
}
