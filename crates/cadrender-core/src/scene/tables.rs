//! Table entries: layers, linetypes, text styles, dimension styles,
//! multileader styles, and the few header variables we use.

use crate::color::EntColor;

/// A DXF/DWG object handle (hex in DXF). 0 = unknown.
pub type Handle = u64;

/// Index into `Scene::layers`.
pub type LayerId = u32;
/// Index into `Scene::blocks`.
pub type BlockId = u32;
/// Index into `Scene::linetypes`, or one of the special values below.
pub type LinetypeId = u32;
/// Index into `Scene::text_styles`.
pub type StyleId = u32;

pub const LT_BYLAYER: LinetypeId = u32::MAX;
pub const LT_BYBLOCK: LinetypeId = u32::MAX - 1;
/// Continuous (no dashes). Also used for unknown linetypes.
pub const LT_SOLID: LinetypeId = u32::MAX - 2;

/// Lineweight special values (entity group 370).
pub const LW_BYLAYER: i16 = -1;
pub const LW_BYBLOCK: i16 = -2;
pub const LW_DEFAULT: i16 = -3;

#[derive(Clone, Debug, Default)]
pub struct Header {
    /// `$ACADVER`, for example "AC1032".
    pub version: String,
    /// Global linetype scale `$LTSCALE`.
    pub ltscale: f64,
    /// `$PSLTSCALE`: if true, dashes in viewports keep their paper size.
    pub psltscale: bool,
    /// `$INSUNITS` (4 = mm).
    pub insunits: i32,
    /// `$MEASUREMENT` (0 = imperial, 1 = metric).
    pub measurement: i32,
    /// `$PDMODE`: how POINT entities look (0 = a dot).
    pub pdmode: i32,
    /// `$PDSIZE`: POINT size. > 0 drawing units, 0 = 5 % of the view height,
    /// < 0 = that percentage of the view height.
    pub pdsize: f64,
}

#[derive(Clone, Debug)]
pub struct Layer {
    pub name: String,
    pub handle: Handle,
    /// ACI colour, always positive (the sign in the file means "off").
    pub color: i32,
    pub true_color: Option<u32>,
    /// Lineweight in 1/100 mm, or `LW_DEFAULT`.
    pub lw: i16,
    pub ltype: LinetypeId,
    /// 255 = opaque.
    pub alpha: u8,
    pub off: bool,
    pub frozen: bool,
    /// "Plot" flag. Not used to hide (ezdxf draws no-plot layers too).
    pub plot: bool,
    /// Per-viewport overrides (VP layer properties).
    pub vp_overrides: Vec<LayerVpOverride>,
}

impl Layer {
    pub fn new(name: &str) -> Layer {
        Layer { name: name.to_string(), handle: 0, color: 7, true_color: None, lw: LW_DEFAULT, ltype: LT_SOLID, alpha: 255, off: false, frozen: false, plot: true, vp_overrides: vec![] }
    }
    /// The layer colour as an entity colour value.
    pub fn ent_color(&self) -> EntColor {
        match self.true_color { Some(rgb) => EntColor::true_color(rgb), None => EntColor::aci(self.color) }
    }
}

/// Layer properties changed for one viewport only.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayerVpOverride {
    /// Handle of the VIEWPORT entity.
    pub viewport: Handle,
    pub color: Option<EntColor>,
    pub ltype: Option<LinetypeId>,
    pub lw: Option<i16>,
    pub alpha: Option<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct Linetype {
    pub name: String,
    pub handle: Handle,
    /// Dash lengths in drawing units: > 0 dash, < 0 gap, 0 dot.
    pub pattern: Vec<f64>,
    /// The pattern has shapes or text in it.
    pub complex: bool,
    /// Text and shape elements of a complex linetype (see `crate::linetype`).
    pub parts: Vec<crate::linetype::LtPart>,
}

#[derive(Clone, Debug)]
pub struct TextStyle {
    pub name: String,
    pub handle: Handle,
    /// Primary font file as written in the file ("arial.ttf", "romans.shx").
    pub font: String,
    /// Big font file (SHX for Asian text), may be empty.
    pub bigfont: String,
    /// Fixed height, 0 = not fixed.
    pub height: f64,
    pub width_factor: f64,
    /// Oblique angle in degrees.
    pub oblique: f64,
    pub flags: i32,
    /// Font face used for this style (index into `Scene::fonts`), set when the scene is built.
    pub face: Option<u16>,
}

impl TextStyle {
    pub fn new(name: &str) -> TextStyle {
        TextStyle { name: name.to_string(), handle: 0, font: String::new(), bigfont: String::new(), height: 0.0, width_factor: 1.0, oblique: 0.0, flags: 0, face: None }
    }
}

/// Dimension style. The few values we read now, plus every raw group so the
/// dimension work can read more without changing the readers.
#[derive(Clone, Debug, Default)]
pub struct DimStyle {
    pub name: String,
    pub handle: Handle,
    /// DIMSCALE (40)
    pub scale: f64,
    /// DIMASZ (41): arrow size
    pub arrow_size: f64,
    /// DIMTXT (140): text height
    pub text_height: f64,
    /// All (group code, value) pairs of the table entry.
    pub raw: Vec<(i32, String)>,
}

impl DimStyle {
    /// A number from the raw groups (for example 42 = DIMEXO, 271 = DIMDEC, 69 = DIMTFILL).
    pub fn num(&self, code: i32) -> Option<f64> { self.raw.iter().find(|r| r.0 == code).and_then(|r| r.1.trim().parse().ok()) }
    /// A handle from the raw groups (for example 340 = DIMTXSTY).
    pub fn handle_of(&self, code: i32) -> Option<Handle> { self.raw.iter().find(|r| r.0 == code).and_then(|r| u64::from_str_radix(r.1.trim(), 16).ok()) }
}

/// Multileader style (MLEADERSTYLE object). MULTILEADER entities written by
/// AutoCAD and ODA repeat every value they use, so the reader takes them from
/// the entity; the style is kept for callers and for files that leave values out.
#[derive(Clone, Debug, Default)]
pub struct MLeaderStyle {
    pub name: String,
    pub handle: Handle,
    /// 170: 0 none, 1 block, 2 MTEXT.
    pub content: i32,
    /// 173: 0 invisible, 1 straight, 2 spline.
    pub leader_type: i32,
    /// 91: leader line colour.
    pub line_color: Option<EntColor>,
    /// 340: leader linetype handle.
    pub ltype: Handle,
    /// 92: leader lineweight.
    pub lw: i16,
    /// 290 / 291: landing and dogleg on.
    pub landing: bool,
    pub dogleg: bool,
    /// 42: landing gap, 43: dogleg length.
    pub landing_gap: f64,
    pub dogleg_len: f64,
    /// 341: arrowhead block record handle (0 = closed filled).
    pub arrow_block: Handle,
    /// 44: arrowhead size.
    pub arrow_size: f64,
    /// 342: text style handle, 45: text height, 93: text colour.
    pub text_style: Handle,
    pub text_height: f64,
    pub text_color: Option<EntColor>,
    /// 292: frame around the text.
    pub frame: bool,
    /// 343: block content record handle, 94: its colour.
    pub block: Handle,
    pub block_color: Option<EntColor>,
    /// 142: overall scale.
    pub scale: f64,
    pub raw: Vec<(i32, String)>,
}

impl MLeaderStyle {
    /// Fill the typed fields from `raw`.
    pub fn parse(&mut self) {
        let num = |v: &str| v.trim().parse::<f64>().unwrap_or(0.0);
        let int = |v: &str| v.trim().parse::<i64>().unwrap_or(0);
        let hex = |v: &str| u64::from_str_radix(v.trim(), 16).unwrap_or(0);
        let col = |v: &str| EntColor::from_raw(int(v) as i32);
        self.scale = 1.0;
        for (c, v) in &self.raw {
            match c {
                170 => self.content = int(v) as i32,
                173 => self.leader_type = int(v) as i32,
                91 => self.line_color = col(v),
                340 => self.ltype = hex(v),
                92 => self.lw = int(v).clamp(-3, 211) as i16,
                290 => self.landing = int(v) != 0,
                291 => self.dogleg = int(v) != 0,
                42 => self.landing_gap = num(v),
                43 => self.dogleg_len = num(v),
                341 => self.arrow_block = hex(v),
                44 => self.arrow_size = num(v),
                342 => self.text_style = hex(v),
                45 => self.text_height = num(v),
                93 => self.text_color = col(v),
                292 => self.frame = int(v) != 0,
                343 => self.block = hex(v),
                94 => self.block_color = col(v),
                142 => self.scale = num(v),
                _ => {}
            }
        }
    }
}
