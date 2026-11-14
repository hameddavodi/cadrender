//! AcDb:Classes (ODA spec 10.2): the custom classes of the drawing.
//!
//! Objects with a type number of 500 or more are instances of a class. The
//! class's DXF name ("LWPOLYLINE" is fixed, but "HATCH", "LAYOUT",
//! "SORTENTSTABLE", "WIPEOUT", "MULTILEADER", ... are classes) tells us
//! what the object is.

use crate::bits::BitReader;
use crate::container::Version;
use crate::error::{DwgError, Result};
use crate::streams::{string_stream, ObjReader};

const START_SENTINEL: [u8; 16] =
    [0x8D, 0xA1, 0xC4, 0xB8, 0xC4, 0xA9, 0xF8, 0xC5, 0xC0, 0xDC, 0xF4, 0x5F, 0xE7, 0xCF, 0xB6, 0x8A];

/// One custom class.
#[derive(Clone, Debug, Default)]
pub struct DwgClass {
    /// Class number (object type number), 500 and up.
    pub number: i16,
    pub proxy_flags: i16,
    pub app_name: String,
    pub cpp_name: String,
    pub dxf_name: String,
    pub was_zombie: bool,
    /// 0x1F2 for entity classes, 0x1F3 for object classes.
    pub item_class_id: i16,
    pub instance_count: i32,
}

impl DwgClass {
    /// True when objects of this class are entities (graphical).
    pub fn is_entity(&self) -> bool {
        self.item_class_id == 0x1F2
    }
}

/// Parse the decompressed AcDb:Classes section (R2004+).
pub fn parse(buf: &[u8], ver: Version, maint_release: u8) -> Result<Vec<DwgClass>> {
    if buf.len() < 24 || buf[..16] != START_SENTINEL {
        return Err(DwgError::corrupt("classes section start sentinel not found"));
    }
    let mut d = BitReader::new(buf);
    d.skip(16 * 8)?;
    let size = d.rl()? as usize;
    if (ver.r2010_plus() && maint_release > 3) || ver.r2018_plus() {
        d.rl()?;
    }
    let initial = d.pos();
    let data_end = (initial + size * 8).min(buf.len() * 8);
    let s = if ver.r2007_plus() {
        let size_bits = d.rl()? as usize;
        let flag_pos = initial + size_bits - 1;
        if size_bits == 0 || flag_pos >= data_end {
            return Err(DwgError::corrupt("classes bit size out of range"));
        }
        let s = string_stream(buf, initial, flag_pos)?;
        d.set_end(s.map(|s| s.pos()).unwrap_or(flag_pos));
        s
    } else {
        // R2004: no bit size and no string stream; texts are 8-bit in the data
        d.set_end(data_end);
        None
    };
    let mut r = ObjReader { d, s, h: BitReader::new(&[]), ver, handle: 0 };

    r.d.bs()?; // maximum class number
    r.d.rc()?; // 0x00
    r.d.rc()?; // 0x00
    r.d.b()?; // true

    let mut out = Vec::new();
    // Each class needs well over 32 bits; the last few bits are padding.
    while r.d.remaining() > 32 {
        let mut c = DwgClass { number: r.d.bs()?, proxy_flags: r.d.bs()?, ..Default::default() };
        c.app_name = r.tv()?;
        c.cpp_name = r.tv()?;
        c.dxf_name = r.tv()?;
        c.was_zombie = r.d.b()?;
        c.item_class_id = r.d.bs()?;
        c.instance_count = r.d.bl()?;
        r.d.bl()?; // dwg version
        r.d.bl()?; // maintenance version
        r.d.bl()?; // unknown
        r.d.bl()?; // unknown
        out.push(c);
    }
    Ok(out)
}
