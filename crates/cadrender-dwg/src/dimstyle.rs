//! DIMSTYLE (R2004+). Every value is kept as a (DXF group code, text) pair,
//! the same form the DXF reader keeps (`DimStyle::raw` in the Scene), so
//! dimension code reads one format for both inputs.
//!
//! Field order: ODA spec 20.4.70, checked against ACadSharp (MIT).

use crate::error::Result;
use crate::model::Handle;
use crate::streams::ObjReader;
use crate::tables2::aci;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DimStyle {
    pub name: String,
    /// (DXF group code, value) pairs: numbers in Rust's shortest form,
    /// handles in upper-case hex.
    pub raw: Vec<(i32, String)>,
    pub text_style: Handle,
}

impl DimStyle {
    /// A value by DXF group code.
    pub fn get(&self, code: i32) -> Option<&str> {
        self.raw.iter().find(|r| r.0 == code).map(|r| r.1.as_str())
    }
}

struct Raw(Vec<(i32, String)>);

impl Raw {
    fn s(&mut self, c: i32, v: String) { self.0.push((c, v)); }
    fn f(&mut self, c: i32, v: f64) { self.0.push((c, format!("{v}"))); }
    fn i(&mut self, c: i32, v: i64) { self.0.push((c, v.to_string())); }
    fn h(&mut self, c: i32, v: Handle) { self.0.push((c, format!("{v:X}"))); }
}

pub(crate) fn dimstyle(r: &mut ObjReader<'_>) -> Result<DimStyle> {
    let name = r.tv()?;
    crate::objects::xref_dep(r)?;
    let mut x = Raw(vec![(2, name.clone())]);
    x.s(3, r.tv()?); // DIMPOST
    x.s(4, r.tv()?); // DIMAPOST
    for c in [40, 41, 42, 43, 44, 45, 46, 47, 48] { x.f(c, r.d.bd()?); } // DIMSCALE .. DIMTM
    let r2007 = r.ver.r2007_plus();
    if r2007 {
        x.f(49, r.d.bd()?); // DIMFXL
        x.f(50, r.d.bd()?); // DIMJOGANG (radians, as stored)
        x.i(69, r.d.bs()? as i64); // DIMTFILL
        x.i(70, aci(r.cmc()?) as i64); // DIMTFILLCLR
    }
    for c in [71, 72, 73, 74, 75, 76] { x.i(c, r.d.b()? as i64); } // DIMTOL DIMLIM DIMTIH DIMTOH DIMSE1 DIMSE2
    for c in [77, 78, 79] { x.i(c, r.d.bs()? as i64); } // DIMTAD DIMZIN DIMAZIN
    if r2007 { x.i(90, r.d.bs()? as i64); } // DIMARCSYM
    for c in [140, 141, 142, 143, 144, 145, 146, 147, 148] { x.f(c, r.d.bd()?); } // DIMTXT .. DIMALTRND
    x.i(170, r.d.b()? as i64); // DIMALT
    x.i(171, r.d.bs()? as i64); // DIMALTD
    for c in [172, 173, 174, 175] { x.i(c, r.d.b()? as i64); } // DIMTOFL DIMSAH DIMTIX DIMSOXD
    for c in [176, 177, 178] { x.i(c, aci(r.cmc()?) as i64); } // DIMCLRD DIMCLRE DIMCLRT
    for c in [179, 271, 272, 273, 274, 275, 276, 277, 278, 279, 280] { x.i(c, r.d.bs()? as i64); }
    for c in [281, 282] { x.i(c, r.d.b()? as i64); } // DIMSD1 DIMSD2
    for c in [283, 284, 285, 286] { x.i(c, r.d.bs()? as i64); } // DIMTOLJ DIMTZIN DIMALTZ DIMALTTZ
    x.i(288, r.d.b()? as i64); // DIMUPT
    x.i(289, r.d.bs()? as i64); // DIMATFIT
    if r2007 { x.i(290, r.d.b()? as i64); } // DIMFXLON
    if r.ver.r2010_plus() {
        x.i(294, r.d.b()? as i64); // DIMTXTDIRECTION
        x.f(10_001, r.d.bd()?); // DIMALTMZF (no DXF group; kept with a private code)
        x.s(10_002, r.tv()?); // DIMALTMZS
        x.f(10_003, r.d.bd()?); // DIMMZF
        x.s(10_004, r.tv()?); // DIMMZS
    }
    x.i(371, r.d.bs()? as i64); // DIMLWD
    x.i(372, r.d.bs()? as i64); // DIMLWE
    r.d.b()?; // unknown
    r.handle_ref()?; // xref block
    let text_style = r.handle_ref()?;
    x.h(340, text_style);
    for c in [341, 342, 343, 344] { x.h(c, r.handle_ref()?); } // DIMLDRBLK DIMBLK DIMBLK1 DIMBLK2
    if r2007 { for c in [345, 346, 347] { x.h(c, r.handle_ref()?); } } // DIMLTYPE DIMLTEX1 DIMLTEX2
    Ok(DimStyle { name, raw: x.0, text_style })
}

/// VPORT table entry (model space window setup). Field order: ODA spec
/// 20.4.66, checked against ACadSharp.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct VPort {
    pub name: String,
    pub view_height: f64,
    /// Width / height (the file stores width; divided here like DXF 41).
    pub aspect_ratio: f64,
    /// View centre in DCS (DXF 12).
    pub center: [f64; 2],
    pub target: [f64; 3],
    pub direction: [f64; 3],
    /// Radians.
    pub twist: f64,
    pub lens_length: f64,
    /// Lower-left and upper-right corner in screen fractions (DXF 10, 11).
    pub lower_left: [f64; 2],
    pub upper_right: [f64; 2],
}

pub(crate) fn vport(r: &mut ObjReader<'_>) -> Result<VPort> {
    let mut v = VPort { name: r.tv()?, ..Default::default() };
    crate::objects::xref_dep(r)?;
    v.view_height = r.d.bd()?;
    let w = r.d.bd()?;
    v.aspect_ratio = if v.view_height != 0.0 { w / v.view_height } else { 0.0 };
    v.center = r.d.rd2()?;
    v.target = r.d.bd3()?;
    v.direction = r.d.bd3()?;
    v.twist = r.d.bd()?;
    v.lens_length = r.d.bd()?;
    r.d.bd()?; // front clip
    r.d.bd()?; // back clip
    for _ in 0..4 {
        r.d.b()?; // view mode bits
    }
    r.d.rc()?; // render mode
    let r2007 = r.ver.r2007_plus();
    if r2007 {
        r.d.b()?; // default lights
        r.d.rc()?; // default lighting type
        r.d.bd()?; // brightness
        r.d.bd()?; // contrast
        r.cmc()?; // ambient colour
    }
    v.lower_left = r.d.rd2()?;
    v.upper_right = r.d.rd2()?;
    r.d.b()?; // UCSFOLLOW
    r.d.bs()?; // circle zoom
    r.d.b()?; // fast zoom
    r.d.b()?; // UCSICON bit 0
    r.d.b()?; // UCSICON bit 1
    r.d.b()?; // grid on
    r.d.rd2()?; // grid spacing
    r.d.b()?; // snap on
    r.d.b()?; // snap style
    r.d.bs()?; // snap isopair
    r.d.bd()?; // snap rotation
    r.d.rd2()?; // snap base
    r.d.rd2()?; // snap spacing
    r.d.b()?; // unknown
    r.d.b()?; // UCS per viewport
    r.d.bd3()?; // UCS origin
    r.d.bd3()?; // UCS x axis
    r.d.bd3()?; // UCS y axis
    r.d.bd()?; // UCS elevation
    r.d.bs()?; // UCS orthographic type
    if r2007 {
        r.d.bs()?; // grid flags
        r.d.bs()?; // grid major
    }
    // xref block, (R2007+: background, visual style, sun), named UCS, base UCS
    for _ in 0..if r2007 { 6 } else { 3 } {
        r.handle_ref()?;
    }
    Ok(v)
}
