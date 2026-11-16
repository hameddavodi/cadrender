//! Entities: the common part here, one module per group of entity types.
//!
//! To read a new entity type: add a function in a module (or a new module),
//! and one line in the `match` in [`read`].

mod acis;
pub use acis::fill_pending as acis_fill_pending;
mod curves;
mod image;
pub use image::fill_files as image_fill_files;
mod dimension;
mod fills;
mod hatch;
mod insert;
mod shapes;
mod masks;
pub use masks::wipeout_frames;
mod mleader;
mod text;
mod viewport;

use crate::reader::Reader;
use crate::tags::{f, gi, hex, i, trim, Tag, Tags};
use cadrender_core::color::EntColor;
use cadrender_core::entity::{Entity, Kind, Props};
use cadrender_core::geom::Aff;

/// Common entity properties plus reader-only facts.
#[derive(Clone, Copy, Debug)]
pub struct Common {
    pub props: Props,
    /// Owner (BLOCK_RECORD) handle.
    pub owner: u64,
    pub invisible: bool,
    /// Group 67: in paper space.
    pub paper: bool,
    /// Z of the extrusion direction (only the sign is used: < 0 = mirrored OCS).
    pub ez: f64,
}

impl Common {
    /// Mirror for entities whose extrusion points down (-Z).
    pub fn ocs(&self) -> Option<Aff> { if self.ez < 0.0 { Some(Aff::scale(-1.0, 1.0)) } else { None } }
}

/// Read the common groups. Groups inside `{...}` 102 blocks and XDATA are ignored.
/// Only the first value of each group counts.
pub fn common(r: &mut Reader, body: &[Tag]) -> Common {
    let mut c = Common { props: Props::default(), owner: 0, invisible: false, paper: false, ez: 1.0 };
    let mut seen = 0u32;
    let mut in_group = false;
    for &(code, v) in body {
        if code == 102 { in_group = trim(v).first() == Some(&b'{'); continue; }
        if in_group { continue; }
        if code >= 1000 { break; }
        let bit = match code { 8 => 1, 62 => 2, 6 => 4, 370 => 8, 48 => 16, 60 => 32, 330 => 64, 420 => 128, 440 => 256, 67 => 512, 230 => 1024, 5 => 2048, _ => 0 };
        if bit == 0 || seen & bit != 0 { continue; }
        seen |= bit;
        let p = &mut c.props;
        match code {
            8 => p.layer = r.b.layer(&crate::tags::s(v)),
            62 if seen & 128 == 0 => p.color = EntColor::aci(i(v).abs()),
            6 => p.ltype = r.b.linetype_id(&crate::tags::s(v)),
            370 => p.lw = i(v).clamp(-3, 211) as i16,
            48 => { let s = f(v); p.ltscale = if s > 0.0 { s as f32 } else { 1.0 } }
            60 => c.invisible = i(v) != 0,
            330 => c.owner = hex(v),
            420 => p.color = EntColor::true_color(crate::tags::long(v) as u32 & 0xff_ffff),
            440 => { let t = crate::tags::long(v); if t & 0x0200_0000 != 0 { p.alpha = (t & 0xff) as u8; } }
            67 => c.paper = i(v) != 0,
            230 => c.ez = f(v),
            5 => p.handle = hex(v),
            _ => {}
        }
    }
    c
}

/// Read one entity whose type is `kind` and whose groups are `body`. Sub-records
/// that follow (VERTEX, ATTRIB, SEQEND) are taken from `t`.
pub fn read<'b>(r: &mut Reader, kind: &[u8], body: &[Tag<'b>], t: &mut Tags<'b>) -> Option<(Entity, Common)> {
    // Stop at an embedded object (101): ODA repeats codes like 10/11 there
    // (in MTEXT the second 11 would be read as the text direction).
    let body = match body.iter().position(|t| t.0 == 101) { Some(p) => &body[..p], None => body };
    let com = common(r, body);
    let name = std::str::from_utf8(kind).unwrap_or("?");
    r.b.count(name);
    let mut verts: Vec<(f64, f64, f64)> = vec![];
    let mut faces: Vec<[i32; 4]> = vec![];
    let mut attribs: Vec<Entity> = vec![];
    let mut values: Vec<(String, String)> = vec![];
    if kind == b"POLYLINE" || kind == b"INSERT" { followers(r, kind, body, t, &mut verts, &mut faces, &mut attribs, &mut values); }
    if !values.is_empty() { insert::record(r, body, &com, values); }
    if com.invisible { return None; }
    if kind == b"TEXT" || kind == b"MTEXT" { text::record(r, kind, body, &com); }
    let k: Option<Kind> = match kind {
        b"LINE" => curves::line(body),
        b"CIRCLE" => curves::circle(body, &com),
        b"ARC" => curves::arc(body, &com),
        b"ELLIPSE" => curves::ellipse(body, &com),
        b"LWPOLYLINE" => curves::lwpolyline(r, body, &com),
        b"POLYLINE" => match gi(body, 70).unwrap_or(0) {
            fl if fl & 64 != 0 => shapes::polyface(&verts, &faces),
            fl if fl & 16 != 0 => shapes::polygon_mesh(body, &verts),
            _ => curves::polyline(r, body, &com, &verts),
        },
        b"SPLINE" | b"HELIX" => curves::spline(body),
        b"LEADER" => curves::leader(body),
        b"SOLID" | b"TRACE" => fills::solid(body, &com),
        b"WIPEOUT" => fills::wipeout(body),
        b"HATCH" => hatch::hatch(body, &com),
        b"DIMENSION" => dimension::dimension(r, body, &com),
        b"INSERT" | b"ACAD_TABLE" => insert::insert(r, kind, body, &com, attribs),
        b"TEXT" | b"MTEXT" => text::text(r, kind, body, &com).map(Kind::from),
        b"VIEWPORT" => viewport::viewport(body, &com),
        b"POINT" => shapes::point(r, body, &com),
        b"RAY" | b"XLINE" => shapes::xline(kind, body),
        b"3DFACE" => shapes::face3d(body),
        b"MESH" => shapes::mesh(body),
        b"IMAGE" => image::image(r, body, &com),
        b"REGION" | b"3DSOLID" | b"BODY" => acis::body(r, body, &com),
        b"ATTDEF" => { mleader::keep_attdef(r, body, &com); shapes::attdef(r, body, &com) }
        b"MULTILEADER" => mleader::mleader(r, body, &com),
        _ => { r.b.skip(name); return None; }
    };
    let k = k?;
    Some((Entity::new(com.props, k), com))
}

/// VERTEX records after a POLYLINE, ATTRIB records after an INSERT, and the
/// closing SEQEND. Tolerant: ATTRIBs without the "attributes follow" flag are
/// taken too, and a missing SEQEND is fine.
/// `values` gets the (tag, value) of every ATTRIB, hidden ones too.
#[allow(clippy::too_many_arguments)]
fn followers<'b>(r: &mut Reader, kind: &[u8], body: &[Tag<'b>], t: &mut Tags<'b>, verts: &mut Vec<(f64, f64, f64)>, faces: &mut Vec<[i32; 4]>, attribs: &mut Vec<Entity>, values: &mut Vec<(String, String)>) {
    let mut sub: Vec<Tag<'b>> = Vec::with_capacity(32);
    let has66 = body.iter().any(|&(c, v)| c == 66 && i(v) != 0);
    while let Some((0, k)) = t.peek() {
        let k = trim(k);
        if kind == b"POLYLINE" && k == b"VERTEX" {
            t.next();
            t.body(&mut sub);
            let (mut x, mut y, mut b, mut fl, mut fc) = (0.0, 0.0, 0.0, 0, [0i32; 4]);
            for &(c, v) in &sub { match c { 10 => x = f(v), 20 => y = f(v), 42 => b = f(v), 70 => fl = i(v), 71..=74 => fc[(c - 71) as usize] = i(v), _ => {} } }
            // skip spline frame points; polyface face records go apart
            if fl & 128 != 0 && fl & 64 == 0 { faces.push(fc); } else if fl & 16 == 0 { verts.push((x, y, b)); }
        } else if kind == b"INSERT" && k == b"ATTRIB" {
            t.next();
            t.body(&mut sub);
            r.b.count("ATTRIB");
            if let Some(v) = text::attrib_value(&sub) { values.push(v); }
            let ac = common(r, &sub);
            if ac.invisible { continue; }
            if let Some(te) = text::text(r, b"ATTRIB", &sub, &ac) { attribs.push(Entity::new(ac.props, te)); }
        } else if k == b"SEQEND" && (kind == b"POLYLINE" || !attribs.is_empty() || has66) {
            t.next();
            t.body(&mut sub);
            break;
        } else {
            break;
        }
    }
}
