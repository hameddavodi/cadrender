//! Non-graphic objects added for the Scene adapter: XRECORD,
//! DIMSTYLE (+ its control object) and VPORT.
//!
//! Sources: ODA spec 20.4 (public), ACadSharp (MIT) as second reference.

use crate::error::{DwgError, Result};
use crate::model::{Handle, ObjectKind, TableControl};
use crate::streams::{Color, ObjReader};

/// One XRECORD value. The group code says the type (DXF rules).
#[derive(Clone, Debug, PartialEq)]
pub enum XValue {
    Str(String),
    Point([f64; 3]),
    Real(f64),
    Int(i64),
    Bool(bool),
    Bytes(Vec<u8>),
    Handle(Handle),
}

impl XValue {
    pub fn int(&self) -> Option<i64> {
        match self { XValue::Int(i) => Some(*i), XValue::Bool(b) => Some(*b as i64), _ => None }
    }
    pub fn handle(&self) -> Option<Handle> {
        match self { XValue::Handle(h) => Some(*h), _ => None }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct XRecord {
    /// (DXF group code, value) pairs in file order.
    pub items: Vec<(i32, XValue)>,
    pub cloning: i16,
}

/// The value type of a DXF group code in XRECORD data (DXF reference).
fn xvalue(r: &mut ObjReader<'_>, code: i32) -> Result<Option<XValue>> {
    let r2007 = r.ver.r2007_plus();
    let d = &mut r.d;
    let utf16 = |d: &mut crate::bits::BitReader<'_>| -> Result<String> {
        let n = d.rs()? as usize;
        if !r2007 {
            // R2004: RS length, RC code page, then 8-bit bytes
            d.rc()?;
            if n * 8 > d.remaining() { return Err(DwgError::corrupt("xrecord string past the end")); }
            let b = d.bytes(n)?;
            return Ok(b.iter().take_while(|&&c| c != 0).map(|&c| c as char).collect());
        }
        if n * 16 > d.remaining() { return Err(DwgError::corrupt("xrecord string past the end")); }
        let units = (0..n).map(|_| d.rs()).collect::<Result<Vec<u16>>>()?;
        Ok(String::from_utf16_lossy(&units).trim_end_matches('\0').to_string())
    };
    Ok(Some(match code {
        0..=4 | 6..=9 | 100..=102 | 300..=309 | 410..=419 | 430..=439 | 470..=479 | 999 | 1000..=1003 => XValue::Str(utf16(d)?),
        5 | 105 | 320..=329 | 480..=481 => {
            let s = utf16(d)?;
            XValue::Handle(u64::from_str_radix(s.trim(), 16).unwrap_or(0))
        }
        10..=39 | 1010..=1013 => XValue::Point([d.rd()?, d.rd()?, d.rd()?]),
        40..=59 | 110..=149 | 210..=239 | 460..=469 | 1014..=1059 => XValue::Real(d.rd()?),
        60..=79 | 170..=179 | 270..=279 | 370..=389 | 400..=409 | 1060..=1070 => XValue::Int(d.rs()? as i16 as i64),
        90..=99 | 420..=429 | 440..=459 | 1071 => XValue::Int(d.rl()? as i32 as i64),
        160..=169 => XValue::Int(d.rll()? as i64),
        280..=289 => XValue::Int(d.rc()? as i64),
        290..=299 => XValue::Bool(d.rc()? != 0),
        310..=319 | 1004 => { let n = d.rc()? as usize; XValue::Bytes(d.bytes(n)?) }
        330..=369 | 390..=399 | 1005..=1009 => XValue::Handle(d.rll()?),
        _ => return Ok(None),
    }))
}

pub(crate) fn xrecord(r: &mut ObjReader<'_>) -> Result<XRecord> {
    let n = r.d.bl()?;
    if n < 0 || n as usize * 8 > r.d.remaining() {
        return Err(DwgError::corrupt(format!("xrecord {:#x}: {n} data bytes", r.handle)));
    }
    let end = r.d.pos() + n as usize * 8;
    let mut x = XRecord::default();
    while r.d.pos() < end {
        let code = r.d.rs()? as i16 as i32;
        match xvalue(r, code)? {
            Some(v) => x.items.push((code, v)),
            None => return Err(DwgError::corrupt(format!("xrecord {:#x}: unknown group code {code}", r.handle))),
        }
    }
    if r.d.pos() != end {
        return Err(DwgError::corrupt(format!("xrecord {:#x}: data overrun", r.handle)));
    }
    x.cloning = r.d.bs()?;
    // the rest of the handle stream: object ids, as many as there are
    while r.h.remaining() >= 8 {
        r.handle_ref()?;
    }
    Ok(x)
}

/// DIMSTYLE_CONTROL: entries, then (R2000+) an RC count of extra handles.
pub(crate) fn dimstyle_control(r: &mut ObjReader<'_>) -> Result<TableControl> {
    let n = r.d.bl()?;
    if n < 0 || n as usize > r.h.remaining() / 8 {
        return Err(DwgError::corrupt(format!("control {:#x}: {n} entries", r.handle)));
    }
    let extra = r.d.rc()? as usize;
    let mut c = TableControl::default();
    for _ in 0..n {
        c.entries.push(r.handle_ref()?);
    }
    for _ in 0..extra {
        c.extra.push(r.handle_ref()?);
    }
    Ok(c)
}

/// Non-entity types of this module (and of mleader.rs).
pub(crate) fn object(r: &mut ObjReader<'_>, code: u16, name: &str) -> Result<Option<ObjectKind>> {
    Ok(Some(match (code, name) {
        (0x4F, _) => ObjectKind::XRecord(xrecord(r)?),
        (0x43, _) => ObjectKind::AppId(crate::extra::appid(r)?),
        (0x45, _) => ObjectKind::DimStyle(Box::new(crate::dimstyle::dimstyle(r)?)),
        (0x41, _) => ObjectKind::VPort(Box::new(crate::dimstyle::vport(r)?)),
        (_, "MLEADERSTYLE") => ObjectKind::MLeaderStyle(Box::new(crate::mleader::mleader_style(r)?)),
        (_, "IMAGEDEF") => crate::image::imagedef(r)?,
        (_, "WIPEOUTVARIABLES") => ObjectKind::WipeoutVariables { frame: r.d.bs()? },
        _ => return Ok(None),
    }))
}

/// ACI number of a colour for DXF groups that only take an index.
pub fn aci(c: Color) -> i32 {
    match c { Color::ByBlock => 0, Color::Index(i) => i as i32, Color::Rgb(_) => 7, Color::ByLayer | Color::None => 256 }
}
