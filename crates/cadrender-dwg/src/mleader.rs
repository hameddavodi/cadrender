//! MULTILEADER (with its annotation context data) and MLEADERSTYLE, R2007+.
//!
//! Field order: ODA spec 20.4.86 / 20.4.87 (public), checked against
//! ACadSharp (MIT). DXF group codes are noted per field.

use crate::error::{DwgError, Result};
use crate::model::{Handle, P3};
use crate::streams::{Color, ObjReader};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LeaderLine {
    /// 10: points from the arrow head towards the landing.
    pub points: Vec<P3>,
    /// Break info: (segment index, (start, end) pairs).
    pub breaks: Vec<(i32, Vec<(P3, P3)>)>,
    pub index: i32,
    /// 170: 0 invisible, 1 straight, 2 spline.
    pub path_type: i16,
    pub color: Color,
    pub ltype: Handle,
    /// 171: lineweight in 1/100 mm (-1 by layer, -2 by block, -3 default).
    pub lineweight: i32,
    pub arrow_size: f64,
    pub arrow: Handle,
    /// 93: 1 type, 2 colour, 4 linetype, 8 lineweight, 16 arrow size, 32 arrow.
    pub override_flags: i32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LeaderRoot {
    pub content_valid: bool,
    pub unknown: bool,
    /// 10: where the leader meets the content (end of the landing).
    pub connection: P3,
    /// 11: dog-leg direction.
    pub direction: P3,
    pub breaks: Vec<(P3, P3)>,
    pub index: i32,
    /// 40: dog-leg (landing) length.
    pub landing_distance: f64,
    pub lines: Vec<LeaderLine>,
    /// 271: 0 horizontal, 1 vertical.
    pub attach_direction: i16,
}

/// The MTEXT content of a multileader.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MLeaderText {
    /// 304: the MTEXT string (format codes inside).
    pub label: String,
    pub normal: P3,
    pub style: Handle,
    /// 12: insertion point.
    pub location: P3,
    /// 13: text x direction.
    pub direction: P3,
    /// 42: radians.
    pub rotation: f64,
    /// 43, 44: MTEXT box width (wrap) and height.
    pub width: f64,
    pub height: f64,
    pub line_spacing_factor: f64,
    pub line_spacing_style: i16,
    pub color: Color,
    /// 171: MTEXT attachment 1..9.
    pub attachment: i16,
    pub flow: i16,
    pub bg_color: Color,
    pub bg_scale: f64,
    pub bg_transparency: i32,
    /// 291 / 292: background fill on, use page colour (mask).
    pub bg_fill: bool,
    pub bg_mask: bool,
    pub column_type: i16,
    pub auto_height: bool,
    pub column_width: f64,
    pub column_gutter: f64,
    pub column_flow_reversed: bool,
    pub column_sizes: Vec<f64>,
    pub word_break: bool,
}

/// The block content of a multileader.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MLeaderBlock {
    pub record: Handle,
    pub normal: P3,
    pub location: P3,
    pub scale: P3,
    /// 46: radians.
    pub rotation: f64,
    pub color: Color,
    /// 47: the full 4x4 transform, row by row as stored.
    pub matrix: [f64; 16],
}

/// AcDbMLeaderObjectContextData: the geometry as last computed by AutoCAD.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MLeaderContext {
    pub roots: Vec<LeaderRoot>,
    /// 40: overall scale.
    pub scale: f64,
    pub content_base: P3,
    pub text_height: f64,
    pub arrow_size: f64,
    pub landing_gap: f64,
    pub text_left_attach: i16,
    pub text_right_attach: i16,
    pub text_align: i16,
    pub block_connection: i16,
    pub text: Option<MLeaderText>,
    pub block: Option<MLeaderBlock>,
    pub base_point: P3,
    pub base_direction: P3,
    pub base_vertical: P3,
    pub normal_reversed: bool,
    pub text_top_attach: i16,
    pub text_bottom_attach: i16,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MLeader {
    pub context: MLeaderContext,
    pub style: Handle,
    pub override_flags: i32,
    /// 170: 0 invisible, 1 straight, 2 spline.
    pub path_type: i16,
    pub line_color: Color,
    pub line_type: Handle,
    pub line_weight: i32,
    pub enable_landing: bool,
    pub enable_dogleg: bool,
    pub landing_distance: f64,
    pub arrow_head: Handle,
    pub arrow_size: f64,
    /// 172: 0 none, 1 block, 2 MTEXT, 3 tolerance.
    pub content_type: i16,
    pub text_style: Handle,
    pub text_left_attach: i16,
    pub text_right_attach: i16,
    pub text_angle_type: i16,
    pub text_alignment: i16,
    pub text_color: Color,
    pub text_frame: bool,
    pub block_content: Handle,
    pub block_color: Color,
    pub block_scale: P3,
    pub block_rotation: f64,
    pub block_connection: i16,
    pub annotative: bool,
    /// (ATTDEF handle, text, index, width).
    pub block_attribs: Vec<(Handle, String, i16, f64)>,
    pub text_direction_negative: bool,
    pub text_align_ipe: i16,
    pub text_attachment_point: i16,
    /// 45
    pub scale: f64,
    pub text_attach_direction: i16,
    pub text_bottom_attach: i16,
    pub text_top_attach: i16,
    pub extended_to_text: bool,
}

fn count(n: i32, max: i32, h: u64) -> Result<usize> {
    if !(0..=max).contains(&n) {
        return Err(DwgError::corrupt(format!("multileader {h:#x}: bad count {n}")));
    }
    Ok(n as usize)
}

fn pairs(r: &mut ObjReader<'_>) -> Result<Vec<(P3, P3)>> {
    let n = count(r.d.bl()?, 100_000, r.handle)?;
    (0..n).map(|_| Ok((r.d.bd3()?, r.d.bd3()?))).collect()
}

fn leader_line(r: &mut ObjReader<'_>) -> Result<LeaderLine> {
    let mut l = LeaderLine::default();
    let n = count(r.d.bl()?, 1_000_000, r.handle)?;
    for _ in 0..n {
        l.points.push(r.d.bd3()?);
    }
    let nb = count(r.d.bl()?, 100_000, r.handle)?;
    for _ in 0..nb {
        let seg = r.d.bl()?;
        l.breaks.push((seg, pairs(r)?));
    }
    l.index = r.d.bl()?;
    if r.ver.r2010_plus() {
        l.path_type = r.d.bs()?;
        l.color = r.cmc()?;
        l.ltype = r.handle_ref()?;
        l.lineweight = r.d.bl()?;
        l.arrow_size = r.d.bd()?;
        l.arrow = r.handle_ref()?;
        l.override_flags = r.d.bl()?;
    }
    Ok(l)
}

fn leader_root(r: &mut ObjReader<'_>) -> Result<LeaderRoot> {
    let mut l = LeaderRoot { content_valid: r.d.b()?, unknown: r.d.b()?, connection: r.d.bd3()?, direction: r.d.bd3()?, ..Default::default() };
    l.breaks = pairs(r)?;
    l.index = r.d.bl()?;
    l.landing_distance = r.d.bd()?;
    let n = count(r.d.bl()?, 100_000, r.handle)?;
    for _ in 0..n {
        l.lines.push(leader_line(r)?);
    }
    if r.ver.r2010_plus() {
        l.attach_direction = r.d.bs()?;
    }
    Ok(l)
}

fn text_content(r: &mut ObjReader<'_>) -> Result<MLeaderText> {
    let mut t = MLeaderText { label: r.tv()?, normal: r.d.bd3()?, style: r.handle_ref()?, ..Default::default() };
    t.location = r.d.bd3()?;
    t.direction = r.d.bd3()?;
    t.rotation = r.d.bd()?;
    t.width = r.d.bd()?;
    t.height = r.d.bd()?;
    t.line_spacing_factor = r.d.bd()?;
    t.line_spacing_style = r.d.bs()?;
    t.color = r.cmc()?;
    t.attachment = r.d.bs()?;
    t.flow = r.d.bs()?;
    t.bg_color = r.cmc()?;
    t.bg_scale = r.d.bd()?;
    t.bg_transparency = r.d.bl()?;
    t.bg_fill = r.d.b()?;
    t.bg_mask = r.d.b()?;
    t.column_type = r.d.bs()?;
    t.auto_height = r.d.b()?;
    t.column_width = r.d.bd()?;
    t.column_gutter = r.d.bd()?;
    t.column_flow_reversed = r.d.b()?;
    let n = count(r.d.bl()?, 10_000, r.handle)?;
    for _ in 0..n {
        t.column_sizes.push(r.d.bd()?);
    }
    t.word_break = r.d.b()?;
    r.d.b()?; // unknown
    Ok(t)
}

fn block_content(r: &mut ObjReader<'_>) -> Result<MLeaderBlock> {
    let mut b = MLeaderBlock { record: r.handle_ref()?, normal: r.d.bd3()?, location: r.d.bd3()?, scale: r.d.bd3()?, ..Default::default() };
    b.rotation = r.d.bd()?;
    b.color = r.cmc()?;
    for m in b.matrix.iter_mut() {
        *m = r.d.bd()?;
    }
    Ok(b)
}

fn context(r: &mut ObjReader<'_>) -> Result<MLeaderContext> {
    let mut c = MLeaderContext::default();
    let n = count(r.d.bl()?, 100_000, r.handle)?;
    for _ in 0..n {
        c.roots.push(leader_root(r)?);
    }
    c.scale = r.d.bd()?;
    c.content_base = r.d.bd3()?;
    c.text_height = r.d.bd()?;
    c.arrow_size = r.d.bd()?;
    c.landing_gap = r.d.bd()?;
    c.text_left_attach = r.d.bs()?;
    c.text_right_attach = r.d.bs()?;
    c.text_align = r.d.bs()?;
    c.block_connection = r.d.bs()?;
    if r.d.b()? {
        c.text = Some(text_content(r)?);
    } else if r.d.b()? {
        c.block = Some(block_content(r)?);
    }
    c.base_point = r.d.bd3()?;
    c.base_direction = r.d.bd3()?;
    c.base_vertical = r.d.bd3()?;
    c.normal_reversed = r.d.b()?;
    if r.ver.r2010_plus() {
        c.text_top_attach = r.d.bs()?;
        c.text_bottom_attach = r.d.bs()?;
    }
    Ok(c)
}

pub(crate) fn mleader(r: &mut ObjReader<'_>) -> Result<MLeader> {
    if r.ver.r2010_plus() {
        r.d.bs()?; // 270 version (2)
    }
    let mut m = MLeader { context: context(r)?, style: r.handle_ref()?, ..Default::default() };
    m.override_flags = r.d.bl()?;
    m.path_type = r.d.bs()?;
    m.line_color = r.cmc()?;
    m.line_type = r.handle_ref()?;
    m.line_weight = r.d.bl()?;
    m.enable_landing = r.d.b()?;
    m.enable_dogleg = r.d.b()?;
    m.landing_distance = r.d.bd()?;
    m.arrow_head = r.handle_ref()?;
    m.arrow_size = r.d.bd()?;
    m.content_type = r.d.bs()?;
    m.text_style = r.handle_ref()?;
    m.text_left_attach = r.d.bs()?;
    m.text_right_attach = r.d.bs()?;
    m.text_angle_type = r.d.bs()?;
    m.text_alignment = r.d.bs()?;
    m.text_color = r.cmc()?;
    m.text_frame = r.d.b()?;
    m.block_content = r.handle_ref()?;
    m.block_color = r.cmc()?;
    m.block_scale = r.d.bd3()?;
    m.block_rotation = r.d.bd()?;
    m.block_connection = r.d.bs()?;
    m.annotative = r.d.b()?;
    if !r.ver.r2010_plus() {
        // R2007: arrow head list (BL count, then B is-default + H each).
        // ACadSharp gates it as "before R2007"; a real AC1021 sample
        // only reads to its exact end with the list present.
        let n = count(r.d.bl()?, 100_000, r.handle)?;
        for _ in 0..n {
            r.d.b()?;
            r.handle_ref()?;
        }
    }
    let n = count(r.d.bl()?, 100_000, r.handle)?;
    for _ in 0..n {
        let h = r.handle_ref()?;
        m.block_attribs.push((h, r.tv()?, r.d.bs()?, r.d.bd()?));
    }
    m.text_direction_negative = r.d.b()?;
    m.text_align_ipe = r.d.bs()?;
    m.text_attachment_point = r.d.bs()?;
    m.scale = r.d.bd()?;
    if r.ver.r2010_plus() {
        m.text_attach_direction = r.d.bs()?;
        m.text_bottom_attach = r.d.bs()?;
        m.text_top_attach = r.d.bs()?;
    }
    if r.ver.r2013_plus() {
        m.extended_to_text = r.d.b()?;
    }
    Ok(m)
}

/// MLEADERSTYLE. All values are kept as (DXF group code, text) pairs, the
/// form the Scene keeps for both readers (`MLeaderStyle::raw`). Colours are
/// written as DXF raw colours (0xC0.. by layer, 0xC1.. by block, 0xC2 RGB,
/// 0xC3 index), as signed 32-bit numbers like in DXF.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MLeaderStyle {
    pub raw: Vec<(i32, String)>,
}

impl MLeaderStyle {
    pub fn raw_groups(&self) -> Vec<(i32, String)> { self.raw.clone() }
    /// A value by DXF group code.
    pub fn get(&self, code: i32) -> Option<&str> {
        self.raw.iter().find(|r| r.0 == code).map(|r| r.1.as_str())
    }
}

/// DXF raw colour value of a colour.
pub fn raw_color(c: Color) -> i32 {
    let v: u32 = match c {
        Color::ByLayer | Color::None => 0xC000_0000,
        Color::ByBlock => 0xC100_0000,
        Color::Rgb(rgb) => 0xC200_0000 | (rgb & 0xff_ffff),
        Color::Index(i) => 0xC300_0000 | i as u32,
    };
    v as i32
}

pub(crate) fn mleader_style(r: &mut ObjReader<'_>) -> Result<MLeaderStyle> {
    let mut v: Vec<(i32, String)> = vec![];
    let mut put = |c: i32, s: String| v.push((c, s));
    if r.ver.r2010_plus() {
        put(179, r.d.bs()?.to_string());
    }
    for c in [170, 171, 172] { put(c, r.d.bs()?.to_string()); }
    put(90, r.d.bl()?.to_string());
    put(40, format!("{}", r.d.bd()?));
    put(41, format!("{}", r.d.bd()?));
    put(173, r.d.bs()?.to_string());
    put(91, raw_color(r.cmc()?).to_string());
    put(340, format!("{:X}", r.handle_ref()?));
    put(92, r.d.bl()?.to_string());
    put(290, (r.d.b()? as i32).to_string());
    put(42, format!("{}", r.d.bd()?));
    put(291, (r.d.b()? as i32).to_string());
    put(43, format!("{}", r.d.bd()?));
    put(3, r.tv()?);
    put(341, format!("{:X}", r.handle_ref()?));
    put(44, format!("{}", r.d.bd()?));
    put(300, r.tv()?);
    put(342, format!("{:X}", r.handle_ref()?));
    for c in [174, 178, 175, 176] { put(c, r.d.bs()?.to_string()); }
    put(93, raw_color(r.cmc()?).to_string());
    put(45, format!("{}", r.d.bd()?));
    put(292, (r.d.b()? as i32).to_string());
    put(297, (r.d.b()? as i32).to_string());
    put(46, format!("{}", r.d.bd()?));
    put(343, format!("{:X}", r.handle_ref()?));
    put(94, raw_color(r.cmc()?).to_string());
    let s = r.d.bd3()?;
    put(47, format!("{}", s[0]));
    put(49, format!("{}", s[1]));
    put(140, format!("{}", s[2]));
    put(293, (r.d.b()? as i32).to_string());
    put(141, format!("{}", r.d.bd()?));
    put(294, (r.d.b()? as i32).to_string());
    put(177, r.d.bs()?.to_string());
    put(142, format!("{}", r.d.bd()?));
    put(295, (r.d.b()? as i32).to_string());
    put(296, (r.d.b()? as i32).to_string());
    put(143, format!("{}", r.d.bd()?));
    if r.ver.r2010_plus() {
        put(271, r.d.bs()?.to_string());
        put(273, r.d.bs()?.to_string());
        put(272, r.d.bs()?.to_string());
    }
    if r.ver.r2013_plus() {
        put(298, (r.d.b()? as i32).to_string());
    }
    Ok(MLeaderStyle { raw: v })
}
