//! Decoding of single objects from AcDb:AcDbObjects (ODA spec chapter 20).
//!
//! Every object starts with its size, so an object we do not understand can
//! be skipped safely. [`decode_object`] reads the frame (size, handle stream
//! size, type, handle, EED), sets up the three streams of R2007+ objects,
//! reads the common entity / non-entity data and then the type-specific part.
//!
//! As a self-check it also reports how many bits of the data stream and the
//! handle stream were left unread. For a correctly decoded object the data
//! stream is used exactly, and fewer than 8 padding bits stay in the handle
//! stream. The validation tool counts objects that break this rule.

use crate::bits::BitReader;
use crate::classes::DwgClass;
use crate::container::Version;
use crate::crc::crc16;
use crate::error::{DwgError, Result};
use crate::model::*;
use crate::streams::{string_stream, Color, ObjReader};

/// Fixed DWG object type numbers (spec 20.3).
pub mod types {
    pub const TEXT: u16 = 1;
    pub const ATTRIB: u16 = 2;
    pub const ATTDEF: u16 = 3;
    pub const BLOCK: u16 = 4;
    pub const ENDBLK: u16 = 5;
    pub const SEQEND: u16 = 6;
    pub const INSERT: u16 = 7;
    pub const MINSERT: u16 = 8;
    pub const VERTEX_2D: u16 = 0x0A;
    pub const POLYLINE_2D: u16 = 0x0F;
    pub const ARC: u16 = 0x11;
    pub const CIRCLE: u16 = 0x12;
    pub const LINE: u16 = 0x13;
    pub const DIM_ORDINATE: u16 = 0x14;
    pub const DIM_LINEAR: u16 = 0x15;
    pub const DIM_ALIGNED: u16 = 0x16;
    pub const DIM_ANG3PT: u16 = 0x17;
    pub const DIM_ANG2LN: u16 = 0x18;
    pub const DIM_RADIUS: u16 = 0x19;
    pub const DIM_DIAMETER: u16 = 0x1A;
    pub const POINT: u16 = 0x1B;
    pub const SOLID: u16 = 0x1F;
    pub const TRACE: u16 = 0x20;
    pub const VIEWPORT: u16 = 0x22;
    pub const ELLIPSE: u16 = 0x23;
    pub const SPLINE: u16 = 0x24;
    pub const DICTIONARY: u16 = 0x2A;
    pub const MTEXT: u16 = 0x2C;
    pub const BLOCK_CONTROL: u16 = 0x30;
    pub const BLOCK_HEADER: u16 = 0x31;
    pub const LAYER_CONTROL: u16 = 0x32;
    pub const LAYER: u16 = 0x33;
    pub const STYLE_CONTROL: u16 = 0x34;
    pub const STYLE: u16 = 0x35;
    pub const LTYPE_CONTROL: u16 = 0x38;
    pub const LTYPE: u16 = 0x39;
    pub const VIEW_CONTROL: u16 = 0x3C;
    pub const UCS_CONTROL: u16 = 0x3E;
    pub const VPORT_CONTROL: u16 = 0x40;
    pub const APPID_CONTROL: u16 = 0x42;
    pub const DIMSTYLE_CONTROL: u16 = 0x44;
    pub const VX_CONTROL: u16 = 0x46;
    pub const LWPOLYLINE: u16 = 0x4D;
    pub const HATCH: u16 = 0x4E;
    pub const LAYOUT: u16 = 0x52;
    pub const PROXY_ENTITY: u16 = 0x1F2;
}

use types as t;

/// Name of a fixed type number, as LibreDWG/ODA print it.
pub fn fixed_type_name(code: u16) -> Option<&'static str> {
    Some(match code {
        0 => "UNUSED",
        1 => "TEXT",
        2 => "ATTRIB",
        3 => "ATTDEF",
        4 => "BLOCK",
        5 => "ENDBLK",
        6 => "SEQEND",
        7 => "INSERT",
        8 => "MINSERT",
        0x0A => "VERTEX_2D",
        0x0B => "VERTEX_3D",
        0x0C => "VERTEX_MESH",
        0x0D => "VERTEX_PFACE",
        0x0E => "VERTEX_PFACE_FACE",
        0x0F => "POLYLINE_2D",
        0x10 => "POLYLINE_3D",
        0x11 => "ARC",
        0x12 => "CIRCLE",
        0x13 => "LINE",
        0x14 => "DIMENSION_ORDINATE",
        0x15 => "DIMENSION_LINEAR",
        0x16 => "DIMENSION_ALIGNED",
        0x17 => "DIMENSION_ANG3PT",
        0x18 => "DIMENSION_ANG2LN",
        0x19 => "DIMENSION_RADIUS",
        0x1A => "DIMENSION_DIAMETER",
        0x1B => "POINT",
        0x1C => "3DFACE",
        0x1D => "POLYLINE_PFACE",
        0x1E => "POLYLINE_MESH",
        0x1F => "SOLID",
        0x20 => "TRACE",
        0x21 => "SHAPE",
        0x22 => "VIEWPORT",
        0x23 => "ELLIPSE",
        0x24 => "SPLINE",
        0x25 => "REGION",
        0x26 => "3DSOLID",
        0x27 => "BODY",
        0x28 => "RAY",
        0x29 => "XLINE",
        0x2A => "DICTIONARY",
        0x2B => "OLEFRAME",
        0x2C => "MTEXT",
        0x2D => "LEADER",
        0x2E => "TOLERANCE",
        0x2F => "MLINE",
        0x30 => "BLOCK_CONTROL",
        0x31 => "BLOCK_HEADER",
        0x32 => "LAYER_CONTROL",
        0x33 => "LAYER",
        0x34 => "STYLE_CONTROL",
        0x35 => "STYLE",
        0x38 => "LTYPE_CONTROL",
        0x39 => "LTYPE",
        0x3C => "VIEW_CONTROL",
        0x3D => "VIEW",
        0x3E => "UCS_CONTROL",
        0x3F => "UCS",
        0x40 => "VPORT_CONTROL",
        0x41 => "VPORT",
        0x42 => "APPID_CONTROL",
        0x43 => "APPID",
        0x44 => "DIMSTYLE_CONTROL",
        0x45 => "DIMSTYLE",
        0x46 => "VX_CONTROL",
        0x47 => "VX_TABLE_RECORD",
        0x48 => "GROUP",
        0x49 => "MLINESTYLE",
        0x4A => "OLE2FRAME",
        0x4B => "DUMMY",
        0x4C => "LONG_TRANSACTION",
        0x4D => "LWPOLYLINE",
        0x4E => "HATCH",
        0x4F => "XRECORD",
        0x50 => "PLACEHOLDER",
        0x51 => "VBA_PROJECT",
        0x52 => "LAYOUT",
        0x1F2 => "ACAD_PROXY_ENTITY",
        0x1F3 => "ACAD_PROXY_OBJECT",
        _ => return None,
    })
}

/// Is a fixed type number an entity?
pub fn fixed_is_entity(code: u16) -> bool {
    matches!(code, 1..=0x29 | 0x2B..=0x2F | 0x4A | 0x4D | 0x4E | 0x1F2)
}

/// Class lookup by type number.
pub struct ClassTable<'a> {
    classes: &'a [DwgClass],
}

impl<'a> ClassTable<'a> {
    pub fn new(classes: &'a [DwgClass]) -> Self {
        ClassTable { classes }
    }

    pub fn get(&self, code: u16) -> Option<&'a DwgClass> {
        if code < 500 {
            return None;
        }
        // Usually class number = 500 + index; fall back to a search.
        let i = (code - 500) as usize;
        match self.classes.get(i) {
            Some(c) if c.number as u16 == code => Some(c),
            _ => self.classes.iter().find(|c| c.number as u16 == code),
        }
    }

    /// The type name (fixed name, or the class DXF name).
    pub fn name(&self, code: u16) -> &'a str {
        if let Some(n) = fixed_type_name(code) {
            return n;
        }
        self.get(code).map(|c| c.dxf_name.as_str()).unwrap_or("UNKNOWN")
    }

    pub fn is_entity(&self, code: u16) -> bool {
        if code < 500 {
            fixed_is_entity(code)
        } else {
            self.get(code).map(|c| c.is_entity()).unwrap_or(false)
        }
    }
}

/// What [`decode_object`] returns besides the object itself.
#[derive(Clone, Copy, Debug, Default)]
pub struct DecodeInfo {
    /// Object size in bytes (from the MS field).
    pub size: u32,
    /// Bits of the data stream left unread (or over-read: negative never
    /// happens, an over-read is an error).
    pub data_left: usize,
    /// Bits of the handle stream left unread.
    pub handles_left: usize,
    /// True when the type-specific part was decoded (not skipped).
    pub decoded: bool,
    /// True when the object CRC matched.
    pub crc_ok: bool,
}

/// The object frame: everything up to the type-specific data.
struct Frame<'a> {
    r: ObjReader<'a>,
    type_code: u16,
    size: u32,
    crc_ok: bool,
    /// EED blocks (app handle, raw bytes), kept for LAYER only (transparency).
    eed: Vec<(u64, Vec<u8>)>,
}

fn read_frame<'a>(buf: &'a [u8], offset: usize, ver: Version, check_crc: bool) -> Result<Frame<'a>> {
    let start_bit = offset.checked_mul(8).ok_or_else(|| DwgError::corrupt("offset overflow"))?;
    let mut r = BitReader::with_range(buf, start_bit, buf.len() * 8);
    let size = r.ms()?;
    // R2010+: MC handle stream size here. R2007: none (see below).
    let mc = if ver.r2010_plus() { Some(r.mc_unsigned()? as usize) } else { None };
    let data_start = r.pos();
    let obj_end = data_start + size as usize * 8;
    if obj_end + 16 > buf.len() * 8 {
        return Err(DwgError::corrupt(format!("object at {offset:#x}: size {size} past section end")));
    }
    // R2004/R2007 (version check): the data starts with the type (BS) and an RL
    // with the size of the data part in bits; the handle stream follows it.
    let hsize = match mc {
        Some(h) => h,
        None => {
            let mut t = BitReader::with_range(buf, data_start, obj_end);
            t.bs()?;
            let bits = t.rl()? as usize;
            if bits > size as usize * 8 {
                return Err(DwgError::corrupt(format!("object at {offset:#x}: bit size {bits} past object end")));
            }
            size as usize * 8 - bits
        }
    };
    if hsize > size as usize * 8 {
        return Err(DwgError::corrupt(format!("object at {offset:#x}: handle stream larger than object")));
    }
    let crc_ok = if check_crc {
        let end_byte = obj_end / 8;
        let stored = u16::from_le_bytes([buf[end_byte], buf[end_byte + 1]]);
        crc16(0xC0C1, &buf[offset..end_byte]) == stored
    } else {
        true
    };
    let handle_start = obj_end - hsize;
    if handle_start <= data_start {
        return Err(DwgError::corrupt(format!("object at {offset:#x}: no data stream")));
    }
    let mut d = BitReader::with_range(buf, data_start, handle_start);
    let type_code = if ver.r2010_plus() {
        d.ot_r2010()?
    } else {
        // R2007: BS type, then the RL bit size (already used above).
        let t = d.bs()? as u16;
        d.rl()?;
        t
    };
    let handle = d.h()?.value;
    // EED: |size BS|app handle|size bytes|, until size 0.
    let mut eed = Vec::new();
    loop {
        let sz = d.bs()?;
        if sz == 0 {
            break;
        }
        if sz < 0 {
            return Err(DwgError::corrupt(format!("negative EED size in object {handle:#x}")));
        }
        let app = d.h()?.value;
        if type_code == t::LAYER {
            eed.push((app, d.bytes(sz as usize)?));
        } else {
            d.skip(sz as usize * 8)?;
        }
    }
    // R2004: no string stream, texts are 8-bit in the data stream, which
    // ends where the handle stream starts.
    let s = if ver.r2007_plus() { string_stream(buf, data_start, handle_start - 1)? } else { None };
    let data_end = if ver.r2007_plus() { s.map(|s| s.pos()).unwrap_or(handle_start - 1) } else { handle_start };
    if data_end < d.pos() {
        return Err(DwgError::corrupt(format!("object {handle:#x}: string stream overlaps the header")));
    }
    d.set_end(data_end);
    let h = BitReader::with_range(buf, handle_start, obj_end);
    Ok(Frame { r: ObjReader { d, s, h, ver, handle }, type_code, size, crc_ok, eed })
}

/// Read just the type and handle of the object at `offset` (cheap).
pub fn peek_type(buf: &[u8], offset: usize, ver: Version) -> Result<(u16, u64)> {
    let f = read_frame(buf, offset, ver, false)?;
    Ok((f.type_code, f.r.handle))
}

/// Decode the object that starts at byte `offset` of the objects section.
pub fn decode_object(
    buf: &[u8],
    offset: usize,
    ver: Version,
    classes: &ClassTable<'_>,
    check_crc: bool,
) -> Result<(Object, DecodeInfo)> {
    let Frame { mut r, type_code, size, crc_ok, eed } = read_frame(buf, offset, ver, check_crc)?;
    let mut obj = Object {
        handle: r.handle,
        type_code,
        owner: 0,
        xdict: None,
        reactors: Vec::new(),
        kind: ObjectKind::Unknown,
    };
    let mut decoded = true;
    let name = classes.name(type_code);
    if classes.is_entity(type_code) {
        let common = entity_common(&mut r, &mut obj, false)?;
        let kind = match (type_code, name) {
            (t::LINE, _) => EntityKind::Line(line(&mut r)?),
            (t::CIRCLE, _) => EntityKind::Circle(circle(&mut r)?),
            (t::ARC, _) => EntityKind::Arc(arc(&mut r)?),
            (t::POINT, _) => EntityKind::Point(point(&mut r)?),
            (t::ELLIPSE, _) => EntityKind::Ellipse(ellipse(&mut r)?),
            (t::SOLID, _) => EntityKind::Solid(solid(&mut r)?),
            (t::TRACE, _) => EntityKind::Trace(solid(&mut r)?),
            (t::TEXT, _) => EntityKind::Text(text(&mut r)?),
            (t::ATTRIB, _) => EntityKind::Attrib(attrib(&mut r, false)?),
            (t::ATTDEF, _) => EntityKind::AttDef(attrib(&mut r, true)?),
            (t::BLOCK, _) => EntityKind::Block { name: r.tv()? },
            (t::ENDBLK, _) => EntityKind::EndBlk,
            (t::SEQEND, _) => EntityKind::SeqEnd,
            (t::INSERT, _) => EntityKind::Insert(insert(&mut r, false)?),
            (t::MINSERT, _) => EntityKind::Insert(insert(&mut r, true)?),
            (t::VERTEX_2D, _) => EntityKind::Vertex2d(vertex2d(&mut r)?),
            (t::POLYLINE_2D, _) => EntityKind::Polyline2d(polyline2d(&mut r)?),
            (t::MTEXT, _) => EntityKind::MText(mtext(&mut r)?),
            (t::SPLINE, _) => EntityKind::Spline(spline(&mut r)?),
            (t::VIEWPORT, _) => EntityKind::Viewport(viewport(&mut r)?),
            (t::DIM_ORDINATE..=t::DIM_DIAMETER, _) => EntityKind::Dimension(dimension(&mut r, type_code)?),
            (t::LWPOLYLINE, _) | (_, "LWPOLYLINE") => EntityKind::LwPolyline(lwpolyline(&mut r)?),
            (t::HATCH, _) | (_, "HATCH") => EntityKind::Hatch(hatch(&mut r)?),
            (_, "WIPEOUT") | (_, "IMAGE") => EntityKind::Wipeout(wipeout(&mut r)?),
            (_, "ARC_DIMENSION") => EntityKind::Dimension(dimension(&mut r, 0)?),
            (_, "LARGE_RADIAL_DIMENSION") => EntityKind::Dimension(dimension(&mut r, 1)?),
            // more types: see extra.rs
            _ => match crate::extra::entity(&mut r, type_code, name)? {
                Some((k, full)) => {
                    decoded = full;
                    k
                }
                None => {
                    decoded = false;
                    EntityKind::Other
                }
            },
        };
        obj.kind = ObjectKind::Entity(Box::new(Entity { common, kind }));
    } else {
        nonentity_common(&mut r, &mut obj)?;
        obj.kind = match (type_code, name) {
            (t::BLOCK_HEADER, _) => ObjectKind::BlockHeader(block_header(&mut r)?),
            (t::LAYER, _) => ObjectKind::Layer(Layer { eed, ..layer(&mut r)? }),
            (t::LTYPE, _) => ObjectKind::Ltype(ltype(&mut r)?),
            (t::STYLE, _) => ObjectKind::Style(text_style(&mut r)?),
            (t::LAYOUT, _) | (_, "LAYOUT") => ObjectKind::Layout(Box::new(layout(&mut r)?)),
            (t::DICTIONARY, _) => ObjectKind::Dictionary(dictionary(&mut r)?),
            (_, "ACDBDICTIONARYWDFLT") => {
                let d = dictionary(&mut r)?;
                r.handle_ref()?; // default entry
                ObjectKind::Dictionary(d)
            }
            (_, "DBCOLOR") => {
                r.d.bs()?; // colour index
                let rgb = r.d.bl()? as u32;
                let flags = r.d.rc()?;
                let name = if flags & 1 != 0 { r.tv()? } else { String::new() };
                let book = if flags & 2 != 0 { r.tv()? } else { String::new() };
                ObjectKind::DbColor { color: Color::from_cmc_rgb(rgb), name, book }
            }
            (_, "SORTENTSTABLE") => ObjectKind::SortEnts(sortents(&mut r)?),
            (t::BLOCK_CONTROL, _) => ObjectKind::Control(control(&mut r, 2)?),
            // LTYPE_CONTROL ends with the BYLAYER and BYBLOCK linetypes.
            (t::LTYPE_CONTROL, _) => ObjectKind::Control(control(&mut r, 2)?),
            (t::LAYER_CONTROL | t::STYLE_CONTROL, _)
            | (t::VIEW_CONTROL | t::UCS_CONTROL | t::VPORT_CONTROL | t::APPID_CONTROL, _)
            | (t::VX_CONTROL, _) => ObjectKind::Control(control(&mut r, 0)?),
            // DIMSTYLE_CONTROL: entries, then the handles of the
            // dimension-style-like entries (one count, read like extras).
            (t::DIMSTYLE_CONTROL, _) => ObjectKind::Control(crate::tables2::dimstyle_control(&mut r)?),
            // more types: see tables2.rs
            _ => match crate::tables2::object(&mut r, type_code, name)? {
                Some(k) => k,
                None => {
                    decoded = false;
                    ObjectKind::Unknown
                }
            },
        };
    }
    let info = DecodeInfo {
        size,
        data_left: r.d.remaining(),
        handles_left: r.h.remaining(),
        decoded,
        crc_ok,
    };
    Ok((obj, info))
}

/// Common non-entity data (spec 20.1): reactors, xdictionary, owner.
fn nonentity_common(r: &mut ObjReader<'_>, obj: &mut Object) -> Result<()> {
    let nreact = r.d.bl()?;
    let xdic_missing = r.d.b()?;
    if r.ver.r2013_plus() {
        r.d.b()?; // has DS binary data
    }
    obj.owner = r.handle_ref()?;
    reactors_and_xdict(r, obj, nreact, xdic_missing)
}

fn reactors_and_xdict(r: &mut ObjReader<'_>, obj: &mut Object, nreact: i32, xdic_missing: bool) -> Result<()> {
    if !(0..=100_000).contains(&nreact) {
        return Err(DwgError::corrupt(format!("object {:#x}: {nreact} reactors", obj.handle)));
    }
    for _ in 0..nreact {
        obj.reactors.push(r.handle_ref()?);
    }
    if !xdic_missing {
        obj.xdict = Some(r.handle_ref()?);
    }
    Ok(())
}

/// Common entity data (spec 20.4.1, 20.4.2). With `embedded` (the MTEXT
/// inside a multi-line ATTRIB) reading starts at the entity mode.
fn entity_common(r: &mut ObjReader<'_>, obj: &mut Object, embedded: bool) -> Result<EntityCommon> {
    if !embedded && r.d.b()? {
        // graphic (proxy) data present; size is an RL up to R2007 (version check)
        let n = if r.ver.r2010_plus() { r.d.bll()? } else { r.d.rl()? as u64 };
        let bits = usize::try_from(n).ok().and_then(|n| n.checked_mul(8));
        match bits {
            Some(b) => r.d.skip(b)?,
            None => return Err(DwgError::corrupt("graphic size too large")),
        }
    }
    let mut c = EntityCommon { entmode: r.d.bb()?, ..Default::default() };
    let nreact = r.d.bl()?;
    let xdic_missing = r.d.b()?;
    if r.ver.r2013_plus() {
        r.d.b()?; // has DS binary data
    }
    if c.entmode == 0 {
        obj.owner = r.handle_ref()?;
    }
    reactors_and_xdict(r, obj, nreact, xdic_missing)?;
    c.color = r.enc()?;
    if c.color.has_book_handle {
        c.color_book = Some(r.handle_ref()?);
    }
    c.ltype_scale = r.d.bd()?;
    c.layer = r.handle_ref()?;
    let ltf = r.d.bb()?;
    c.ltype = match ltf {
        0 => LtypeRef::ByLayer,
        1 => LtypeRef::ByBlock,
        2 => LtypeRef::Continuous,
        _ => LtypeRef::Handle(r.handle_ref()?),
    };
    if r.ver.r2007_plus() {
        if r.d.bb()? == 3 {
            c.material = Some(r.handle_ref()?);
        }
        r.d.rc()?; // shadow flags
    }
    if r.d.bb()? == 3 {
        c.plotstyle = Some(r.handle_ref()?);
    }
    if r.ver.r2010_plus() {
        for _ in 0..3 {
            if r.d.b()? {
                r.handle_ref()?; // full / face / edge visual style
            }
        }
    }
    c.invisible = r.d.bs()? & 1 != 0;
    c.lineweight_raw = r.d.rc()?;
    Ok(c)
}

fn line(r: &mut ObjReader<'_>) -> Result<Line> {
    let z_zero = r.d.b()?;
    let sx = r.d.rd()?;
    let ex = r.d.dd(sx)?;
    let sy = r.d.rd()?;
    let ey = r.d.dd(sy)?;
    let (sz, ez) = if z_zero {
        (0.0, 0.0)
    } else {
        let sz = r.d.rd()?;
        (sz, r.d.dd(sz)?)
    };
    Ok(Line { start: [sx, sy, sz], end: [ex, ey, ez], thickness: r.d.bt()?, extrusion: r.d.be()? })
}

fn circle(r: &mut ObjReader<'_>) -> Result<Circle> {
    Ok(Circle { center: r.d.bd3()?, radius: r.d.bd()?, thickness: r.d.bt()?, extrusion: r.d.be()? })
}

fn arc(r: &mut ObjReader<'_>) -> Result<Arc> {
    Ok(Arc {
        center: r.d.bd3()?,
        radius: r.d.bd()?,
        thickness: r.d.bt()?,
        extrusion: r.d.be()?,
        start_angle: r.d.bd()?,
        end_angle: r.d.bd()?,
    })
}

fn point(r: &mut ObjReader<'_>) -> Result<Point> {
    Ok(Point { position: r.d.bd3()?, thickness: r.d.bt()?, extrusion: r.d.be()?, x_axis_angle: r.d.bd()? })
}

fn ellipse(r: &mut ObjReader<'_>) -> Result<Ellipse> {
    Ok(Ellipse {
        center: r.d.bd3()?,
        major_axis: r.d.bd3()?,
        extrusion: r.d.bd3()?,
        axis_ratio: r.d.bd()?,
        start_param: r.d.bd()?,
        end_param: r.d.bd()?,
    })
}

fn solid(r: &mut ObjReader<'_>) -> Result<Solid> {
    let thickness = r.d.bt()?;
    let elevation = r.d.bd()?;
    let corners = [r.d.rd2()?, r.d.rd2()?, r.d.rd2()?, r.d.rd2()?];
    Ok(Solid { thickness, elevation, corners, extrusion: r.d.be()? })
}

/// Common TEXT data (R2000+ form, spec 20.4.3).
fn text(r: &mut ObjReader<'_>) -> Result<Text> {
    let flags = r.d.rc()?;
    let mut t = Text { width_factor: 1.0, ..Default::default() };
    if flags & 0x01 == 0 {
        t.elevation = r.d.rd()?;
    }
    t.insertion = r.d.rd2()?;
    if flags & 0x02 == 0 {
        t.alignment = Some(r.d.dd2(t.insertion)?);
    }
    t.extrusion = r.d.be()?;
    t.thickness = r.d.bt()?;
    if flags & 0x04 == 0 {
        t.oblique = r.d.rd()?;
    }
    if flags & 0x08 == 0 {
        t.rotation = r.d.rd()?;
    }
    t.height = r.d.rd()?;
    if flags & 0x10 == 0 {
        t.width_factor = r.d.rd()?;
    }
    t.value = r.tv()?;
    if flags & 0x20 == 0 {
        t.generation = r.d.bs()?;
    }
    if flags & 0x40 == 0 {
        t.halign = r.d.bs()?;
    }
    if flags & 0x80 == 0 {
        t.valign = r.d.bs()?;
    }
    t.style = r.handle_ref()?;
    Ok(t)
}

/// ATTRIB / ATTDEF (spec 20.4.4, 20.4.5).
fn attrib(r: &mut ObjReader<'_>, is_def: bool) -> Result<Attrib> {
    let text = text(r)?;
    let mut a = Attrib { text, ..Default::default() };
    if r.ver.r2010_plus() {
        r.d.rc()?; // version (R2010+; version check for R2007)
    }
    let att_type = if r.ver.r2018_plus() { r.d.rc()? } else { 1 };
    if att_type == 2 || att_type == 4 {
        // Multi-line: an embedded MTEXT, starting at its entity mode.
        let mut dummy = Object {
            handle: r.handle,
            type_code: t::MTEXT,
            owner: 0,
            xdict: None,
            reactors: Vec::new(),
            kind: ObjectKind::Unknown,
        };
        entity_common(r, &mut dummy, true)?;
        a.mtext = Some(Box::new(mtext(r)?));
        let annot = r.d.bs()?;
        if annot > 0 {
            r.d.skip(annot as usize * 8)?;
            r.handle_ref()?; // registered application
            r.d.bs()?;
        }
    }
    a.tag = r.tv()?;
    r.d.bs()?; // field length
    a.flags = r.d.rc()?;
    if r.ver.r2007_plus() {
        a.lock_position = r.d.b()?;
    }
    if is_def {
        if r.ver.r2010_plus() {
            r.d.rc()?; // version again (R2010+; version check for R2007)
        }
        a.prompt = Some(r.tv()?);
    }
    Ok(a)
}

pub(crate) fn insert(r: &mut ObjReader<'_>, minsert: bool) -> Result<Insert> {
    let insertion = r.d.bd3()?;
    let scale = match r.d.bb()? {
        0 => {
            let x = r.d.rd()?;
            [x, r.d.dd(x)?, r.d.dd(x)?]
        }
        1 => [1.0, r.d.dd(1.0)?, r.d.dd(1.0)?],
        2 => {
            let x = r.d.rd()?;
            [x, x, x]
        }
        _ => [1.0, 1.0, 1.0],
    };
    let rotation = r.d.bd()?;
    let extrusion = r.d.bd3()?;
    let has_attribs = r.d.b()?;
    let owned = if has_attribs { r.d.bl()? } else { 0 };
    if !(0..=1_000_000).contains(&owned) {
        return Err(DwgError::corrupt(format!("insert {:#x}: {owned} attribs", r.handle)));
    }
    let grid = if minsert { Some((r.d.bs()?, r.d.bs()?, r.d.bd()?, r.d.bd()?)) } else { None };
    let block_header = r.handle_ref()?;
    let mut attribs = Vec::with_capacity(owned as usize);
    for _ in 0..owned {
        attribs.push(r.handle_ref()?);
    }
    let seqend = if has_attribs { Some(r.handle_ref()?) } else { None };
    Ok(Insert { block_header, insertion, scale, rotation, extrusion, attribs, seqend, grid })
}

fn vertex2d(r: &mut ObjReader<'_>) -> Result<Vertex2d> {
    let flags = r.d.rc()?;
    let point = r.d.bd3()?;
    let w = r.d.bd()?;
    let (start_width, end_width) = if w < 0.0 { (-w, -w) } else { (w, r.d.bd()?) };
    let bulge = r.d.bd()?;
    if r.ver.r2010_plus() {
        r.d.bl()?; // vertex id (R2010+; version check for R2007)
    }
    let tangent_dir = r.d.bd()?;
    Ok(Vertex2d { flags, point, start_width, end_width, bulge, tangent_dir })
}

fn polyline2d(r: &mut ObjReader<'_>) -> Result<Polyline2d> {
    let mut p = Polyline2d {
        flags: r.d.bs()?,
        curve_type: r.d.bs()?,
        start_width: r.d.bd()?,
        end_width: r.d.bd()?,
        thickness: r.d.bt()?,
        elevation: r.d.bd()?,
        extrusion: r.d.be()?,
        ..Default::default()
    };
    let owned = r.d.bl()?;
    if !(0..=10_000_000).contains(&owned) {
        return Err(DwgError::corrupt(format!("polyline {:#x}: {owned} vertices", r.handle)));
    }
    for _ in 0..owned {
        p.vertices.push(r.handle_ref()?);
    }
    p.seqend = r.handle_ref()?;
    Ok(p)
}

fn lwpolyline(r: &mut ObjReader<'_>) -> Result<LwPolyline> {
    let flags = r.d.bs()?;
    let mut p = LwPolyline { flags, extrusion: [0.0, 0.0, 1.0], ..Default::default() };
    if flags & 4 != 0 {
        p.const_width = r.d.bd()?;
    }
    if flags & 8 != 0 {
        p.elevation = r.d.bd()?;
    }
    if flags & 2 != 0 {
        p.thickness = r.d.bd()?;
    }
    if flags & 1 != 0 {
        p.extrusion = r.d.bd3()?;
    }
    let n = r.d.bl()?;
    let nbulges = if flags & 16 != 0 { r.d.bl()? } else { 0 };
    let nids = if flags & 1024 != 0 { r.d.bl()? } else { 0 };
    let nwidths = if flags & 32 != 0 { r.d.bl()? } else { 0 };
    // Each vertex needs at least 4 bits; refuse counts the data cannot hold.
    let limit = (r.d.remaining() / 4) as i32;
    for c in [n, nbulges, nids, nwidths] {
        if c < 0 || c > limit.max(1) {
            return Err(DwgError::corrupt(format!("lwpolyline {:#x}: count {c} too large", r.handle)));
        }
    }
    p.vertices.reserve(n as usize);
    if n > 0 {
        let mut pt = r.d.rd2()?;
        p.vertices.push(LwVertex { point: pt, ..Default::default() });
        for _ in 1..n {
            pt = r.d.dd2(pt)?;
            p.vertices.push(LwVertex { point: pt, ..Default::default() });
        }
    }
    for i in 0..nbulges as usize {
        let b = r.d.bd()?;
        if let Some(v) = p.vertices.get_mut(i) {
            v.bulge = b;
        }
    }
    for _ in 0..nids {
        r.d.bl()?;
    }
    for i in 0..nwidths as usize {
        let (s, e) = (r.d.bd()?, r.d.bd()?);
        if let Some(v) = p.vertices.get_mut(i) {
            v.start_width = s;
            v.end_width = e;
        }
    }
    Ok(p)
}

/// MTEXT body (spec 20.4.46), after the common entity data.
fn mtext(r: &mut ObjReader<'_>) -> Result<MText> {
    let mut m = MText {
        insertion: r.d.bd3()?,
        extrusion: r.d.bd3()?,
        x_axis: r.d.bd3()?,
        rect_width: r.d.bd()?,
        ..Default::default()
    };
    if r.ver.r2007_plus() {
        m.rect_height = r.d.bd()?;
    }
    m.text_height = r.d.bd()?;
    m.attachment = r.d.bs()?;
    m.drawing_dir = r.d.bs()?;
    r.d.bd()?; // extents height
    r.d.bd()?; // extents width
    m.value = r.tv()?;
    m.style = r.handle_ref()?;
    m.linespacing_style = r.d.bs()?;
    m.linespacing_factor = r.d.bd()?;
    r.d.b()?; // unknown bit
    m.background_flags = r.d.bl()?;
    if m.background_flags & 0x01 != 0 || (r.ver.r2018_plus() && m.background_flags & 0x10 != 0) {
        m.background_scale = r.d.bd()?;
        m.background_color = Some(r.cmc()?);
        m.background_transparency = r.d.bl()?;
    }
    if r.ver.r2018_plus() {
        let not_annotative = r.d.b()?;
        if not_annotative {
            r.d.bs()?; // version
            r.d.b()?; // default flag
            r.handle_ref()?; // registered application
            r.d.bl()?; // attachment point
            r.d.bd3()?; // x-axis dir
            r.d.bd3()?; // insertion point
            r.d.bd()?; // rect width
            r.d.bd()?; // rect height
            r.d.bd()?; // extents width
            r.d.bd()?; // extents height
            let col_type = r.d.bs()?;
            if col_type != 0 {
                let count = r.d.bl()?;
                r.d.bd()?; // column width
                r.d.bd()?; // gutter
                let auto_height = r.d.b()?;
                r.d.b()?; // flow reversed
                if !auto_height && col_type == 2 {
                    if !(0..=10_000).contains(&count) {
                        return Err(DwgError::corrupt("mtext column count"));
                    }
                    for _ in 0..count {
                        r.d.bd()?;
                    }
                }
            }
        }
    }
    Ok(m)
}

pub(crate) fn spline(r: &mut ObjReader<'_>) -> Result<Spline> {
    let mut s = Spline { scenario: r.d.bl()?, ..Default::default() };
    if r.ver.r2013_plus() {
        let flags1 = r.d.bl()?;
        let knot_param = r.d.bl()?;
        s.scenario = if knot_param == 15 || flags1 & 8 == 0 { 1 } else { 2 };
        s.closed = flags1 & 4 != 0;
    }
    s.degree = r.d.bl()?;
    let (mut nknots, mut nctrl, mut nfit, mut weighted) = (0, 0, 0, false);
    match s.scenario {
        1 => {
            s.rational = r.d.b()?;
            s.closed |= r.d.b()?;
            s.periodic = r.d.b()?;
            r.d.bd()?; // knot tolerance
            r.d.bd()?; // control tolerance
            nknots = r.d.bl()?;
            nctrl = r.d.bl()?;
            weighted = r.d.b()?;
        }
        2 => {
            r.d.bd()?; // fit tolerance
            s.start_tangent = r.d.bd3()?;
            s.end_tangent = r.d.bd3()?;
            nfit = r.d.bl()?;
        }
        x => return Err(DwgError::corrupt(format!("spline scenario {x}"))),
    }
    let limit = (r.d.remaining() / 2) as i32;
    for c in [nknots, nctrl, nfit] {
        if c < 0 || c > limit.max(1) {
            return Err(DwgError::corrupt(format!("spline {:#x}: count {c} too large", r.handle)));
        }
    }
    for _ in 0..nknots {
        s.knots.push(r.d.bd()?);
    }
    for _ in 0..nctrl {
        s.control_points.push(r.d.bd3()?);
        if weighted {
            s.weights.push(r.d.bd()?);
        }
    }
    for _ in 0..nfit {
        s.fit_points.push(r.d.bd3()?);
    }
    Ok(s)
}

fn viewport(r: &mut ObjReader<'_>) -> Result<Viewport> {
    let mut v = Viewport {
        center: r.d.bd3()?,
        width: r.d.bd()?,
        height: r.d.bd()?,
        view_target: r.d.bd3()?,
        view_direction: r.d.bd3()?,
        twist_angle: r.d.bd()?,
        view_height: r.d.bd()?,
        lens_length: r.d.bd()?,
        front_clip: r.d.bd()?,
        back_clip: r.d.bd()?,
        ..Default::default()
    };
    r.d.bd()?; // snap angle
    v.view_center = r.d.rd2()?;
    r.d.rd2()?; // snap base
    r.d.rd2()?; // snap spacing
    r.d.rd2()?; // grid spacing
    r.d.bs()?; // circle zoom
    if r.ver.r2007_plus() {
        r.d.bs()?; // grid major
    }
    let nfrozen = r.d.bl()?;
    v.status = r.d.bl()?;
    r.tv()?; // style sheet
    r.d.rc()?; // render mode
    r.d.b()?; // UCS at origin
    r.d.b()?; // UCS per viewport
    r.d.bd3()?; // UCS origin
    r.d.bd3()?; // UCS x axis
    r.d.bd3()?; // UCS y axis
    r.d.bd()?; // UCS elevation
    r.d.bs()?; // UCS ortho type
    v.shade_plot_mode = r.d.bs()?;
    if r.ver.r2007_plus() {
        r.d.b()?; // use default lights
        r.d.rc()?; // default lighting type
        r.d.bd()?; // brightness
        r.d.bd()?; // contrast
        r.cmc()?; // ambient light colour
    }
    if !(0..=100_000).contains(&nfrozen) {
        return Err(DwgError::corrupt(format!("viewport {:#x}: {nfrozen} frozen layers", r.handle)));
    }
    for _ in 0..nfrozen {
        v.frozen_layers.push(r.handle_ref()?);
    }
    v.clip_boundary = r.handle_ref()?;
    r.handle_ref()?; // named UCS
    r.handle_ref()?; // base UCS
    if r.ver.r2007_plus() {
        r.handle_ref()?; // background
        r.handle_ref()?; // visual style
        r.handle_ref()?; // shade plot id
        r.handle_ref()?; // sun
    }
    Ok(v)
}

fn dimension(r: &mut ObjReader<'_>, kind: u16) -> Result<Dimension> {
    if r.ver.r2010_plus() {
        r.d.rc()?; // version (R2010+; version check for R2007)
    }
    let extrusion = r.d.bd3()?;
    let mid = r.d.rd2()?;
    let elevation = r.d.bd()?;
    r.d.rc()?; // flags 1
    let user_text = r.tv()?;
    let text_rotation = r.d.bd()?;
    r.d.bd()?; // horizontal direction
    r.d.bd3()?; // insertion scale
    r.d.bd()?; // insertion rotation
    r.d.bs()?; // attachment point
    r.d.bs()?; // linespacing style
    r.d.bd()?; // linespacing factor
    r.d.bd()?; // actual measurement
    if r.ver.r2007_plus() {
        r.d.b()?; // unknown (73)
        r.d.b()?; // flip arrow 1
        r.d.b()?; // flip arrow 2
    }
    let p12 = r.d.rd2()?;
    match kind {
        t::DIM_ORDINATE => {
            r.d.bd3()?;
            r.d.bd3()?;
            r.d.bd3()?;
            r.d.rc()?;
        }
        t::DIM_LINEAR | t::DIM_ALIGNED => {
            r.d.bd3()?;
            r.d.bd3()?;
            r.d.bd3()?;
            r.d.bd()?; // ext line rotation
            if kind == t::DIM_LINEAR {
                r.d.bd()?; // dim rotation
            }
        }
        t::DIM_ANG3PT => {
            for _ in 0..4 {
                r.d.bd3()?;
            }
        }
        t::DIM_ANG2LN => {
            r.d.rd2()?;
            for _ in 0..4 {
                r.d.bd3()?;
            }
        }
        t::DIM_RADIUS | t::DIM_DIAMETER => {
            r.d.bd3()?;
            r.d.bd3()?;
            r.d.bd()?;
        }
        0 => {
            // ARC_DIMENSION
            for _ in 0..4 {
                r.d.bd3()?;
            }
            r.d.b()?;
            r.d.bd()?;
            r.d.bd()?;
            r.d.b()?;
            r.d.bd3()?;
            r.d.bd3()?;
        }
        _ => {
            // LARGE_RADIAL_DIMENSION
            r.d.bd3()?;
            r.d.bd3()?;
            r.d.bd()?;
            r.d.bd3()?;
            r.d.bd3()?;
        }
    }
    let dimstyle = r.handle_ref()?;
    let block = r.handle_ref()?;
    Ok(Dimension {
        kind,
        extrusion,
        text_mid: [mid[0], mid[1], elevation],
        user_text,
        text_rotation,
        insertion: [p12[0], p12[1], elevation],
        dimstyle,
        block,
    })
}

fn hatch(r: &mut ObjReader<'_>) -> Result<Hatch> {
    let mut h = Hatch { gradient: r.d.bl()? != 0, ..Default::default() };
    r.d.bl()?; // reserved
    h.gradient_angle = r.d.bd()?;
    h.gradient_shift = r.d.bd()?;
    h.gradient_one_color = r.d.bl()? != 0;
    h.gradient_tint = r.d.bd()?;
    let ncolors = r.d.bl()?;
    if !(0..=1000).contains(&ncolors) {
        return Err(DwgError::corrupt(format!("hatch {:#x}: {ncolors} gradient colours", r.handle)));
    }
    for _ in 0..ncolors {
        r.d.bd()?;
        h.gradient_colors.push(r.cmc()?);
    }
    h.gradient_name = r.tv()?;
    h.elevation = r.d.bd()?;
    h.extrusion = r.d.bd3()?;
    h.pattern_name = r.tv()?;
    h.solid = r.d.b()?;
    h.associative = r.d.b()?;
    let npaths = r.d.bl()?;
    let sane = |n: i32, r: &ObjReader<'_>| n >= 0 && (n as usize) <= r.d.remaining().max(1);
    if !sane(npaths, r) {
        return Err(DwgError::corrupt(format!("hatch {:#x}: {npaths} paths", r.handle)));
    }
    let mut derived = false;
    for _ in 0..npaths {
        let mut p = HatchPath { flags: r.d.bl()?, ..Default::default() };
        derived |= p.flags & 4 != 0;
        if p.flags & 2 == 0 {
            let nseg = r.d.bl()?;
            if !sane(nseg, r) {
                return Err(DwgError::corrupt(format!("hatch {:#x}: {nseg} edges", r.handle)));
            }
            for _ in 0..nseg {
                let et = r.d.rc()?;
                let e = match et {
                    1 => HatchEdge::Line { start: r.d.rd2()?, end: r.d.rd2()? },
                    2 => HatchEdge::Arc {
                        center: r.d.rd2()?,
                        radius: r.d.bd()?,
                        start_angle: r.d.bd()?,
                        end_angle: r.d.bd()?,
                        ccw: r.d.b()?,
                    },
                    3 => HatchEdge::Ellipse {
                        center: r.d.rd2()?,
                        major_end: r.d.rd2()?,
                        ratio: r.d.bd()?,
                        start_angle: r.d.bd()?,
                        end_angle: r.d.bd()?,
                        ccw: r.d.b()?,
                    },
                    4 => {
                        let degree = r.d.bl()?;
                        let rational = r.d.b()?;
                        let periodic = r.d.b()?;
                        let nk = r.d.bl()?;
                        let nc = r.d.bl()?;
                        if !sane(nk, r) || !sane(nc, r) {
                            return Err(DwgError::corrupt("hatch spline counts"));
                        }
                        let mut knots = Vec::with_capacity(nk as usize);
                        for _ in 0..nk {
                            knots.push(r.d.bd()?);
                        }
                        let mut control = Vec::with_capacity(nc as usize);
                        for _ in 0..nc {
                            let p = r.d.rd2()?;
                            let w = if rational { r.d.bd()? } else { 1.0 };
                            control.push([p[0], p[1], w]);
                        }
                        let nfit = r.d.bl()?;
                        if !sane(nfit, r) {
                            return Err(DwgError::corrupt("hatch spline fit count"));
                        }
                        let mut fit = Vec::with_capacity(nfit as usize);
                        if nfit > 0 {
                            for _ in 0..nfit {
                                fit.push(r.d.rd2()?);
                            }
                            r.d.rd2()?; // start tangent
                            r.d.rd2()?; // end tangent
                        }
                        HatchEdge::Spline { degree, rational, periodic, knots, control, fit }
                    }
                    x => return Err(DwgError::corrupt(format!("hatch {:#x}: edge type {x}", r.handle))),
                };
                p.edges.push(e);
            }
        } else {
            let bulges = r.d.b()?;
            let closed = r.d.b()?;
            let n = r.d.bl()?;
            if !sane(n, r) {
                return Err(DwgError::corrupt(format!("hatch {:#x}: {n} polyline points", r.handle)));
            }
            let mut pts = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let pt = r.d.rd2()?;
                let b = if bulges { r.d.bd()? } else { 0.0 };
                pts.push([pt[0], pt[1], b]);
            }
            p.polyline = Some((closed, pts));
        }
        let nb = r.d.bl()?;
        if nb < 0 || nb as usize > r.h.remaining() / 8 {
            return Err(DwgError::corrupt(format!("hatch {:#x}: {nb} boundary handles", r.handle)));
        }
        for _ in 0..nb {
            p.boundary_handles.push(r.handle_ref()?);
        }
        h.paths.push(p);
    }
    h.style = r.d.bs()?;
    h.pattern_type = r.d.bs()?;
    if !h.solid {
        h.pattern_angle = r.d.bd()?;
        h.pattern_scale = r.d.bd()?;
        h.double = r.d.b()?;
        let nlines = r.d.bs()?;
        if nlines < 0 {
            return Err(DwgError::corrupt("hatch pattern line count"));
        }
        for _ in 0..nlines {
            let mut l = PatternLine { angle: r.d.bd()?, base: r.d.bd2()?, offset: r.d.bd2()?, dashes: Vec::new() };
            let nd = r.d.bs()?;
            if nd < 0 {
                return Err(DwgError::corrupt("hatch dash count"));
            }
            for _ in 0..nd {
                l.dashes.push(r.d.bd()?);
            }
            h.pattern_lines.push(l);
        }
    }
    if derived {
        r.d.bd()?; // pixel size
    }
    let nseeds = r.d.bl()?;
    if !sane(nseeds, r) {
        return Err(DwgError::corrupt("hatch seed count"));
    }
    for _ in 0..nseeds {
        h.seed_points.push(r.d.rd2()?);
    }
    Ok(h)
}

fn wipeout(r: &mut ObjReader<'_>) -> Result<Wipeout> {
    r.d.bl()?; // class version
    let mut w = Wipeout {
        insertion: r.d.bd3()?,
        u_vector: r.d.bd3()?,
        v_vector: r.d.bd3()?,
        size: r.d.rd2()?,
        display_flags: r.d.bs()?,
        clipping: r.d.b()?,
        ..Default::default()
    };
    w.brightness = r.d.rc()?;
    w.contrast = r.d.rc()?;
    w.fade = r.d.rc()?;
    if r.ver.r2010_plus() {
        w.clip_inside = r.d.b()?; // clip mode (R2010+; version check for R2007)
    }
    w.clip_type = r.d.bs()?;
    match w.clip_type {
        1 => {
            w.clip_vertices.push(r.d.rd2()?);
            w.clip_vertices.push(r.d.rd2()?);
        }
        2 => {
            let n = r.d.bl()?;
            if n < 0 || n as usize > r.d.remaining() / 128 + 1 {
                return Err(DwgError::corrupt(format!("wipeout {:#x}: {n} clip points", r.handle)));
            }
            for _ in 0..n {
                w.clip_vertices.push(r.d.rd2()?);
            }
        }
        _ => {}
    }
    w.imagedef = r.handle_ref()?;
    r.handle_ref()?; // image def reactor
    Ok(w)
}

/// R2007+ "xref dependent" field of table entries.
/// Table entry xref data: R2007+ one BS (flags combined); before R2007
/// B 64-flag, BS xref index + 1, B xref dependent.
pub(crate) fn xref_dep(r: &mut ObjReader<'_>) -> Result<()> {
    if r.ver.r2007_plus() {
        r.d.bs()?;
    } else {
        r.d.b()?;
        r.d.bs()?;
        r.d.b()?;
    }
    Ok(())
}

fn block_header(r: &mut ObjReader<'_>) -> Result<BlockHeader> {
    let mut b = BlockHeader { name: r.tv()?, ..Default::default() };
    xref_dep(r)?;
    b.anonymous = r.d.b()?;
    b.has_attribs = r.d.b()?;
    b.is_xref = r.d.b()?;
    b.is_overlay = r.d.b()?;
    r.d.b()?; // loaded bit
    let owned = if !b.is_xref && !b.is_overlay { r.d.bl()? } else { 0 };
    if owned < 0 || owned as usize > r.h.remaining() / 8 {
        return Err(DwgError::corrupt(format!("block header {:#x}: {owned} entities", r.handle)));
    }
    b.base_point = r.d.bd3()?;
    b.xref_path = r.tv()?;
    let mut ninserts = 0usize;
    while r.d.rc()? != 0 {
        ninserts += 1;
    }
    b.description = r.tv()?;
    let npreview = r.d.bl()?;
    if npreview < 0 {
        return Err(DwgError::corrupt("negative preview size"));
    }
    r.d.skip(npreview as usize * 8)?;
    if r.ver.r2007_plus() {
        b.insert_units = r.d.bs()?;
        b.explodable = r.d.b()?;
        r.d.rc()?; // block scaling
    }
    r.handle_ref()?; // NULL / xref block
    b.block_entity = r.handle_ref()?;
    b.entities.reserve(owned as usize);
    for _ in 0..owned {
        b.entities.push(r.handle_ref()?);
    }
    b.endblk = r.handle_ref()?;
    for _ in 0..ninserts {
        r.handle_ref()?;
    }
    b.layout = r.handle_ref()?;
    Ok(b)
}

fn layer(r: &mut ObjReader<'_>) -> Result<Layer> {
    let mut l = Layer { name: r.tv()?, ..Default::default() };
    xref_dep(r)?;
    let v = r.d.bs()?;
    l.frozen = v & 1 != 0;
    l.on = v & 2 == 0;
    l.frozen_new_vp = v & 4 != 0;
    l.locked = v & 8 != 0;
    l.plot = v & 16 != 0;
    l.lineweight_raw = ((v & 0x3E0) >> 5) as u8;
    l.color = r.cmc()?;
    r.handle_ref()?; // xref block
    l.plotstyle = r.handle_ref()?;
    if r.ver.r2007_plus() {
        l.material = r.handle_ref()?;
    }
    l.ltype = r.handle_ref()?;
    if r.ver.r2013_plus() {
        r.handle_ref()?; // unknown, always NULL
    }
    Ok(l)
}

fn ltype(r: &mut ObjReader<'_>) -> Result<Ltype> {
    let mut l = Ltype { name: r.tv()?, ..Default::default() };
    xref_dep(r)?;
    l.description = r.tv()?;
    l.pattern_length = r.d.bd()?;
    r.d.rc()?; // alignment 'A'
    let n = r.d.rc()?;
    let mut has_text = false;
    for _ in 0..n {
        let d = LtypeDash {
            length: r.d.bd()?,
            shape_number: r.d.bs()?,
            offset: r.d.rd2()?,
            scale: r.d.bd()?,
            rotation: r.d.bd()?,
            flags: r.d.bs()?,
            ..Default::default()
        };
        has_text |= d.flags & 2 != 0;
        l.dashes.push(d);
    }
    if !r.ver.r2007_plus() {
        // R2004: always 256 bytes of 8-bit, zero-ended texts (ACadSharp;
        // the spec calls it the strings area); a text starts at the shape number.
        let area = r.d.bytes(256)?;
        for d in l.dashes.iter_mut().filter(|d| d.flags & 2 != 0) {
            let start = (d.shape_number.max(0) as usize).min(area.len());
            let end = area[start..].iter().position(|&b| b == 0).map_or(area.len(), |k| start + k);
            d.text = Some(area[start..end].iter().map(|&b| b as char).collect());
        }
    } else if has_text {
        // 512 bytes of UTF-16 text area; dash texts start at the shape number
        // (a byte offset into the area).
        let area = r.d.bytes(512)?;
        for d in l.dashes.iter_mut().filter(|d| d.flags & 2 != 0) {
            let start = d.shape_number.max(0) as usize;
            let mut units = Vec::new();
            let mut i = start;
            while i + 1 < area.len() {
                let u = u16::from_le_bytes([area[i], area[i + 1]]);
                if u == 0 {
                    break;
                }
                units.push(u);
                i += 2;
            }
            d.text = Some(String::from_utf16_lossy(&units));
        }
    }
    r.handle_ref()?; // xref block
    for d in l.dashes.iter_mut() {
        d.style = r.handle_ref()?;
    }
    Ok(l)
}

fn text_style(r: &mut ObjReader<'_>) -> Result<TextStyle> {
    let mut s = TextStyle { name: r.tv()?, ..Default::default() };
    xref_dep(r)?;
    s.is_shape_file = r.d.b()?;
    s.vertical = r.d.b()?;
    s.fixed_height = r.d.bd()?;
    s.width_factor = r.d.bd()?;
    s.oblique = r.d.bd()?;
    s.generation = r.d.rc()?;
    s.last_height = r.d.bd()?;
    s.font_file = r.tv()?;
    s.bigfont_file = r.tv()?;
    r.handle_ref()?; // xref block
    Ok(s)
}

fn layout(r: &mut ObjReader<'_>) -> Result<Layout> {
    let mut l = Layout::default();
    // Plot settings part
    r.tv()?; // page setup name
    r.tv()?; // printer / config
    r.d.bs()?; // plot layout flags
    for m in l.margins.iter_mut() {
        *m = r.d.bd()?;
    }
    l.paper_width = r.d.bd()?;
    l.paper_height = r.d.bd()?;
    r.tv()?; // paper size
    l.plot_origin = r.d.bd2()?;
    l.plot_paper_units = r.d.bs()?;
    l.plot_rotation = r.d.bs()?;
    l.plot_type = r.d.bs()?;
    l.window_min = r.d.bd2()?;
    l.window_max = r.d.bd2()?;
    l.scale_numerator = r.d.bd()?;
    l.scale_divisor = r.d.bd()?;
    r.tv()?; // style sheet
    r.d.bs()?; // scale type
    r.d.bd()?; // scale factor
    r.d.bd2()?; // paper image origin
    r.d.bs()?; // shade plot mode
    r.d.bs()?; // shade plot resolution
    r.d.bs()?; // shade plot dpi
    r.handle_ref()?; // plot view
    if r.ver.r2007_plus() {
        r.handle_ref()?; // visual style
    }
    // Layout part
    l.name = r.tv()?;
    l.tab_order = r.d.bl()?;
    l.flags = r.d.bs()?;
    r.d.bd3()?; // UCS origin
    l.limmin = r.d.rd2()?;
    l.limmax = r.d.rd2()?;
    l.insbase = r.d.bd3()?;
    r.d.bd3()?; // UCS x axis
    r.d.bd3()?; // UCS y axis
    r.d.bd()?; // elevation
    r.d.bs()?; // ortho view type
    l.extmin = r.d.bd3()?;
    l.extmax = r.d.bd3()?;
    let nvp = r.d.bl()?;
    if nvp < 0 || nvp as usize > r.h.remaining() / 8 {
        return Err(DwgError::corrupt(format!("layout {:#x}: {nvp} viewports", r.handle)));
    }
    l.block_record = r.handle_ref()?;
    l.active_viewport = r.handle_ref()?;
    r.handle_ref()?; // base UCS
    r.handle_ref()?; // named UCS
    for _ in 0..nvp {
        l.viewports.push(r.handle_ref()?);
    }
    Ok(l)
}

fn sortents(r: &mut ObjReader<'_>) -> Result<SortEntsTable> {
    let block_owner = r.handle_ref()?;
    let n = r.d.bl()?;
    if n < 0 || n as usize > r.h.remaining() / 8 {
        return Err(DwgError::corrupt(format!("sortentstable {:#x}: {n} entries", r.handle)));
    }
    let mut entries = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let sort = r.d.h()?.value; // in the data stream
        let ent = r.handle_ref()?;
        entries.push((sort, ent));
    }
    Ok(SortEntsTable { block_owner, entries })
}

fn dictionary(r: &mut ObjReader<'_>) -> Result<Dictionary> {
    let n = r.d.bl()?;
    r.d.bs()?; // cloning flag
    let hard_owner = r.d.rc()? != 0;
    if n < 0 || n as usize > r.h.remaining() / 8 {
        return Err(DwgError::corrupt(format!("dictionary {:#x}: {n} entries", r.handle)));
    }
    let mut entries = Vec::with_capacity(n as usize);
    for _ in 0..n {
        let name = r.tv()?;
        let h = r.handle_ref()?;
        entries.push((name, h));
    }
    Ok(Dictionary { hard_owner, entries })
}

/// Table control objects. `extra` handles follow the entries
/// (BLOCK_CONTROL: *MODEL_SPACE and *PAPER_SPACE).
fn control(r: &mut ObjReader<'_>, extra: usize) -> Result<TableControl> {
    let n = r.d.bl()?;
    if n < 0 || n as usize > r.h.remaining() / 8 {
        return Err(DwgError::corrupt(format!("control {:#x}: {n} entries", r.handle)));
    }
    let mut c = TableControl::default();
    for _ in 0..n {
        c.entries.push(r.handle_ref()?);
    }
    for _ in 0..extra {
        c.extra.push(r.handle_ref()?);
    }
    Ok(c)
}
