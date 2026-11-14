//! Entity types added for the Scene adapter: POLYLINE 3D / mesh /
//! pface and their vertices, LEADER, TOLERANCE, 3DFACE, MLINE and the
//! insert part of ACAD_TABLE. Kept apart from `objects.rs` to keep that file
//! short.
//!
//! Sources: ODA spec 20.4 (public), ACadSharp (MIT) as second reference.

use crate::error::{DwgError, Result};
use crate::model::{EntityKind, Handle, Insert, Spline, P3};
use crate::streams::ObjReader;

/// POLYLINE (3D).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Polyline3d {
    /// Curve flags: 1 = quadratic spline fit, 2 = cubic spline fit.
    pub spline_flags: u8,
    /// 1 = closed.
    pub closed_flags: u8,
    pub vertices: Vec<Handle>,
    pub seqend: Handle,
}

impl Polyline3d {
    pub fn closed(&self) -> bool { self.closed_flags & 1 != 0 }
}

/// VERTEX (3D, mesh or pface vertex): DXF 70 flags and the point.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Vertex3d {
    pub flags: u8,
    pub point: P3,
}

/// POLYLINE (polyface mesh or polygon mesh). Not drawn by the renderer yet.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PolyMesh {
    /// True for a polyface mesh (PFACE), false for a polygon mesh.
    pub pface: bool,
    /// PFACE: vertex and face counts. MESH: M and N vertex counts.
    pub counts: (i16, i16),
    /// MESH only: DXF 70 flags and curve type.
    pub flags: i16,
    pub curve_type: i16,
    pub vertices: Vec<Handle>,
    pub seqend: Handle,
}

/// VERTEX (pface face record): 1-based vertex indices, negative = invisible edge.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FaceRecord {
    pub index: [i16; 4],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Leader {
    /// 0 MTEXT, 1 TOLERANCE, 2 INSERT, 3 none.
    pub annot_type: i16,
    /// 0 straight, 1 spline.
    pub path_type: i16,
    pub points: Vec<P3>,
    pub origin: P3,
    pub extrusion: P3,
    pub x_direction: P3,
    pub block_offset: P3,
    pub annot_offset: P3,
    pub hookline_on_x: bool,
    pub arrowhead_on: bool,
    pub annotation: Handle,
    pub dimstyle: Handle,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tolerance {
    pub insertion: P3,
    pub x_direction: P3,
    pub extrusion: P3,
    pub text: String,
    pub dimstyle: Handle,
}

/// 3DFACE: four corners, invisible edge flags (1, 2, 4, 8).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Face3d {
    pub corners: [P3; 4],
    pub invisible_edges: i16,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MLineVertex {
    pub point: P3,
    pub direction: P3,
    pub miter: P3,
    /// Per line of the style: (segment parameters, area fill parameters).
    pub lines: Vec<(Vec<f64>, Vec<f64>)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MLine {
    pub scale: f64,
    /// 0 top, 1 zero, 2 bottom.
    pub justification: u8,
    pub base: P3,
    pub extrusion: P3,
    pub closed: bool,
    pub vertices: Vec<MLineVertex>,
    pub style: Handle,
}

/// RAY (`ray`) or XLINE: a point and a direction (WCS).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct XLine {
    pub ray: bool,
    pub base: P3,
    pub direction: P3,
}

/// ACAD_TABLE: only the INSERT part is decoded. The table's graphics are in
/// the anonymous block it refers to (`insert.block_header`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Table {
    pub insert: Insert,
}

/// MESH (subdivision mesh): vertices and faces (each a list of 0-based
/// vertex indices). Edges and creases are read but not kept.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Mesh {
    pub vertices: Vec<P3>,
    pub faces: Vec<Vec<i32>>,
}

/// MESH, field order as in ACadSharp (MIT): BS version, B blend crease,
/// BL level, vertices, face list (size, indices...), edges, creases, BL.
pub(crate) fn mesh(r: &mut ObjReader<'_>) -> Result<Mesh> {
    let mut m = Mesh::default();
    r.d.bs()?; // version
    r.d.b()?; // blend crease
    r.d.bl()?; // subdivision level
    let lim = (r.d.remaining() / 2) as i32;
    let nv = count(r.d.bl()?, lim, "mesh vertices", r.handle)?;
    for _ in 0..nv {
        m.vertices.push(r.d.bd3()?);
    }
    // the face list size counts the size entries too
    let nlist = count(r.d.bl()?, lim, "mesh faces", r.handle)?;
    let mut k = 0;
    while k < nlist {
        let n = count(r.d.bl()?, lim, "mesh face", r.handle)?;
        m.faces.push((0..n).map(|_| r.d.bl()).collect::<Result<Vec<i32>>>()?);
        k += n + 1;
    }
    let ne = count(r.d.bl()?, lim, "mesh edges", r.handle)?;
    for _ in 0..ne * 2 {
        r.d.bl()?;
    }
    let nc = count(r.d.bl()?, lim, "mesh creases", r.handle)?;
    for _ in 0..nc {
        r.d.bd()?;
    }
    r.d.bl()?; // unknown
    Ok(m)
}

/// HELIX: a SPLINE (AutoCAD's own approximation) plus the helix values
/// (DXF AcDbHelix: 10, 11, 12, 40, 41, 42, 290, 280).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Helix {
    pub spline: Spline,
    pub axis_base: P3,
    /// Start point; its distance from the axis is the base radius.
    pub start: P3,
    pub axis: P3,
    /// Top radius.
    pub radius: f64,
    pub turns: f64,
    pub turn_height: f64,
    /// Right-handed = counter-clockwise when seen from the axis tip.
    pub right_handed: bool,
    /// 0 = turn height, 1 = turns, 2 = height is constrained.
    pub constraint: u8,
}

/// HELIX data after the SPLINE part. Field order checked with the bit
/// self-check (the data stream is used to the exact bit) on our samples.
pub(crate) fn helix(r: &mut ObjReader<'_>) -> Result<Helix> {
    let spline = crate::objects::spline(r)?;
    r.d.bl()?; // major version (DXF 90)
    r.d.bl()?; // maintenance version (DXF 91)
    Ok(Helix {
        spline,
        axis_base: r.d.bd3()?,
        start: r.d.bd3()?,
        axis: r.d.bd3()?,
        radius: r.d.bd()?,
        turns: r.d.bd()?,
        turn_height: r.d.bd()?,
        right_handed: r.d.b()?,
        constraint: r.d.rc()?,
    })
}

fn count(n: i32, max: i32, what: &str, h: u64) -> Result<usize> {
    if !(0..=max).contains(&n) {
        return Err(DwgError::corrupt(format!("{what} {h:#x}: bad count {n}")));
    }
    Ok(n as usize)
}

fn handles(r: &mut ObjReader<'_>, n: usize) -> Result<Vec<Handle>> {
    let mut v = Vec::with_capacity(n.min(4096));
    for _ in 0..n {
        v.push(r.handle_ref()?);
    }
    Ok(v)
}

pub(crate) fn polyline3d(r: &mut ObjReader<'_>) -> Result<Polyline3d> {
    let spline_flags = r.d.rc()?;
    let closed_flags = r.d.rc()?;
    let n = count(r.d.bl()?, 10_000_000, "polyline", r.handle)?;
    let vertices = handles(r, n)?;
    Ok(Polyline3d { spline_flags, closed_flags, vertices, seqend: r.handle_ref()? })
}

/// VERTEX_3D, VERTEX_MESH and VERTEX_PFACE share this layout.
pub(crate) fn vertex3d(r: &mut ObjReader<'_>) -> Result<Vertex3d> {
    Ok(Vertex3d { flags: r.d.rc()?, point: r.d.bd3()? })
}

pub(crate) fn face_record(r: &mut ObjReader<'_>) -> Result<FaceRecord> {
    Ok(FaceRecord { index: [r.d.bs()?, r.d.bs()?, r.d.bs()?, r.d.bs()?] })
}

pub(crate) fn polyface(r: &mut ObjReader<'_>) -> Result<PolyMesh> {
    let counts = (r.d.bs()?, r.d.bs()?);
    let n = count(r.d.bl()?, 10_000_000, "pface", r.handle)?;
    let vertices = handles(r, n)?;
    Ok(PolyMesh { pface: true, counts, flags: 64, curve_type: 0, vertices, seqend: r.handle_ref()? })
}

pub(crate) fn polymesh(r: &mut ObjReader<'_>) -> Result<PolyMesh> {
    let flags = r.d.bs()?;
    let curve_type = r.d.bs()?;
    let counts = (r.d.bs()?, r.d.bs()?);
    r.d.bs()?; // M density
    r.d.bs()?; // N density
    let n = count(r.d.bl()?, 10_000_000, "mesh", r.handle)?;
    let vertices = handles(r, n)?;
    Ok(PolyMesh { pface: false, counts, flags, curve_type, vertices, seqend: r.handle_ref()? })
}

pub(crate) fn leader(r: &mut ObjReader<'_>) -> Result<Leader> {
    let mut l = Leader::default();
    r.d.b()?; // unknown, always 0
    l.annot_type = r.d.bs()?;
    l.path_type = r.d.bs()?;
    let n = count(r.d.bl()?, 1_000_000, "leader", r.handle)?;
    for _ in 0..n {
        l.points.push(r.d.bd3()?);
    }
    l.origin = r.d.bd3()?;
    l.extrusion = r.d.bd3()?;
    l.x_direction = r.d.bd3()?;
    l.block_offset = r.d.bd3()?;
    l.annot_offset = r.d.bd3()?;
    if !r.ver.r2007_plus() {
        // R2004: MTEXT box height and width (the bit self-check on a real
        // R2007 sample shows R2007 files do not have them)
        r.d.bd()?;
        r.d.bd()?;
    }
    l.hookline_on_x = r.d.b()?;
    l.arrowhead_on = r.d.b()?;
    r.d.bs()?; // unknown
    r.d.b()?; // unknown
    r.d.b()?; // unknown
    l.annotation = r.handle_ref()?;
    l.dimstyle = r.handle_ref()?;
    Ok(l)
}

pub(crate) fn tolerance(r: &mut ObjReader<'_>) -> Result<Tolerance> {
    let insertion = r.d.bd3()?;
    let x_direction = r.d.bd3()?;
    let extrusion = r.d.bd3()?;
    let text = r.tv()?;
    Ok(Tolerance { insertion, x_direction, extrusion, text, dimstyle: r.handle_ref()? })
}

pub(crate) fn face3d(r: &mut ObjReader<'_>) -> Result<Face3d> {
    let no_flags = r.d.b()?;
    let z_zero = r.d.b()?;
    let (x, y) = (r.d.rd()?, r.d.rd()?);
    let z = if z_zero { 0.0 } else { r.d.rd()? };
    let c0 = [x, y, z];
    let c1 = dd3(r, c0)?;
    let c2 = dd3(r, c1)?;
    let c3 = dd3(r, c2)?;
    let invisible_edges = if no_flags { 0 } else { r.d.bs()? };
    Ok(Face3d { corners: [c0, c1, c2, c3], invisible_edges })
}

fn dd3(r: &mut ObjReader<'_>, d: P3) -> Result<P3> {
    Ok([r.d.dd(d[0])?, r.d.dd(d[1])?, r.d.dd(d[2])?])
}

pub(crate) fn mline(r: &mut ObjReader<'_>) -> Result<MLine> {
    let mut m = MLine { scale: r.d.bd()?, justification: r.d.rc()?, base: r.d.bd3()?, extrusion: r.d.bd3()?, ..Default::default() };
    m.closed = r.d.bs()? == 3;
    let nlines = r.d.rc()? as usize;
    let nverts = count(r.d.bs()? as i32, 32767, "mline", r.handle)?;
    for _ in 0..nverts {
        let mut v = MLineVertex { point: r.d.bd3()?, direction: r.d.bd3()?, miter: r.d.bd3()?, lines: vec![] };
        for _ in 0..nlines {
            let ns = count(r.d.bs()? as i32, 32767, "mline", r.handle)?;
            let seg = (0..ns).map(|_| r.d.bd()).collect::<Result<Vec<f64>>>()?;
            let nf = count(r.d.bs()? as i32, 32767, "mline", r.handle)?;
            let fill = (0..nf).map(|_| r.d.bd()).collect::<Result<Vec<f64>>>()?;
            v.lines.push((seg, fill));
        }
        m.vertices.push(v);
    }
    m.style = r.handle_ref()?;
    Ok(m)
}

/// ACAD_TABLE: the INSERT part only (block, point, scale, rotation). The
/// table content that follows is not read; the caller marks the object as
/// not fully decoded, so the bit self-checks do not count it.
pub(crate) fn table(r: &mut ObjReader<'_>) -> Result<Table> {
    Ok(Table { insert: crate::objects::insert(r, false)? })
}

/// Decode the entity types of this module. Returns the kind and whether the
/// whole object was read (false: only a leading part, see [`table`]), or
/// `None` for a type this module does not know.
pub(crate) fn entity(r: &mut ObjReader<'_>, code: u16, name: &str) -> Result<Option<(EntityKind, bool)>> {
    let k = match (code, name) {
        (0x10, _) => EntityKind::Polyline3d(polyline3d(r)?),
        (0x0B..=0x0D, _) => EntityKind::Vertex3d(vertex3d(r)?),
        (0x0E, _) => EntityKind::FaceRecord(face_record(r)?),
        (0x1D, _) => EntityKind::PolyMesh(polyface(r)?),
        (0x1E, _) => EntityKind::PolyMesh(polymesh(r)?),
        (0x1C, _) => EntityKind::Face3d(face3d(r)?),
        (0x2D, _) => EntityKind::Leader(Box::new(leader(r)?)),
        (0x2E, _) => EntityKind::Tolerance(Box::new(tolerance(r)?)),
        (0x2F, _) => EntityKind::MLine(Box::new(mline(r)?)),
        (0x28 | 0x29, _) => EntityKind::XLine(XLine { ray: code == 0x28, base: r.d.bd3()?, direction: r.d.bd3()? }),
        (0x25..=0x27, _) => return Ok(Some((EntityKind::Acis(Box::new(crate::acis::acis(r)?)), false))),
        (_, "ACAD_TABLE") => return Ok(Some((EntityKind::Table(Box::new(table(r)?)), false))),
        (_, "MULTILEADER") => EntityKind::MLeader(Box::new(crate::mleader::mleader(r)?)),
        (_, "HELIX") => EntityKind::Helix(Box::new(helix(r)?)),
        (_, "MESH") => EntityKind::Mesh(Box::new(mesh(r)?)),
        _ => return Ok(None),
    };
    Ok(Some((k, true)))
}

/// Items of one raw EED block (R2007+ layout, ODA spec 20.2.12) as
/// (DXF group code 1000-1071, value). Stops quietly at bad data.
pub fn eed_items(b: &[u8]) -> Vec<(i32, crate::tables2::XValue)> {
    use crate::tables2::XValue;
    let mut out = vec![];
    let mut p = 0usize;
    let rd = |p: usize| b.get(p..p + 8).map(|x| f64::from_le_bytes(x.try_into().unwrap_or([0; 8])));
    while p < b.len() {
        let code = b[p] as i32;
        p += 1;
        let v = match code {
            0 => {
                let Some(n) = b.get(p..p + 2).map(|x| u16::from_le_bytes([x[0], x[1]]) as usize) else { break };
                let Some(raw) = b.get(p + 2..p + 2 + n * 2) else { break };
                p += 2 + n * 2;
                let units: Vec<u16> = raw.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                XValue::Str(String::from_utf16_lossy(&units))
            }
            2 => { let Some(&x) = b.get(p) else { break }; p += 1; XValue::Int(x as i64) }
            3 | 5 => { let Some(x) = b.get(p..p + 8) else { break }; p += 8; XValue::Handle(u64::from_be_bytes(x.try_into().unwrap_or([0; 8]))) }
            4 => { let Some(&n) = b.get(p) else { break }; let Some(x) = b.get(p + 1..p + 1 + n as usize) else { break }; p += 1 + n as usize; XValue::Bytes(x.to_vec()) }
            10..=13 => { let (Some(x), Some(y), Some(z)) = (rd(p), rd(p + 8), rd(p + 16)) else { break }; p += 24; XValue::Point([x, y, z]) }
            40..=42 => { let Some(x) = rd(p) else { break }; p += 8; XValue::Real(x) }
            70 => { let Some(x) = b.get(p..p + 2) else { break }; p += 2; XValue::Int(i16::from_le_bytes([x[0], x[1]]) as i64) }
            71 => { let Some(x) = b.get(p..p + 4) else { break }; p += 4; XValue::Int(i32::from_le_bytes([x[0], x[1], x[2], x[3]]) as i64) }
            _ => break,
        };
        out.push((1000 + code, v));
    }
    out
}

/// APPID: the registered application name.
pub(crate) fn appid(r: &mut ObjReader<'_>) -> Result<String> {
    let name = r.tv()?;
    crate::objects::xref_dep(r)?;
    r.d.rc()?; // unknown
    r.handle_ref()?; // xref block
    Ok(name)
}
