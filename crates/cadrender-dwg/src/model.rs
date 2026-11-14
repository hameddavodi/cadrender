//! The typed object model this crate produces.
//!
//! It stays close to the DWG data (one Rust type per DWG object type) and
//! does not depend on cadrender-core. An adapter to the core Scene comes
//! later. Handles are plain `u64` values; [`Drawing::get`] finds an object
//! by handle.

use crate::streams::{Color, EntityColor};

/// A DWG handle (unique object id inside one file).
pub type Handle = u64;
pub type P2 = [f64; 2];
pub type P3 = [f64; 3];

/// How an entity's linetype is chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LtypeRef {
    #[default]
    ByLayer,
    ByBlock,
    Continuous,
    Handle(Handle),
}

/// Data every entity has (ODA spec 20.4.1 / 20.4.2).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EntityCommon {
    /// 0 = owner handle stored, 1 = paper space, 2 = model space.
    pub entmode: u8,
    pub layer: Handle,
    pub ltype: LtypeRef,
    pub color: EntityColor,
    /// DBCOLOR object (colour book), when the colour has one.
    pub color_book: Option<Handle>,
    pub ltype_scale: f64,
    /// Lineweight code as stored (RC). See [`EntityCommon::lineweight_mm100`].
    pub lineweight_raw: u8,
    pub invisible: bool,
    /// Plot style handle when the plotstyle flags say "handle".
    pub plotstyle: Option<Handle>,
    pub material: Option<Handle>,
}

/// DWG lineweight index (0..=23) to 1/100 mm. Negative values are
/// -1 by layer, -2 by block, -3 default.
pub const LINEWEIGHTS: [i16; 24] =
    [0, 5, 9, 13, 15, 18, 20, 25, 30, 35, 40, 50, 53, 60, 70, 80, 90, 100, 106, 120, 140, 158, 200, 211];

impl EntityCommon {
    /// Lineweight in 1/100 mm, or -1 (by layer), -2 (by block), -3 (default).
    pub fn lineweight_mm100(&self) -> i16 {
        lineweight_from_raw(self.lineweight_raw)
    }
}

/// Decode a raw lineweight byte (entities and layers).
pub fn lineweight_from_raw(v: u8) -> i16 {
    match v {
        0..=23 => LINEWEIGHTS[v as usize],
        // 28 is by layer too (seen in real files; ODA writes -1 for it)
        28 | 29 => -1,
        30 => -2,
        31 => -3,
        _ => -3,
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Line {
    pub start: P3,
    pub end: P3,
    pub thickness: f64,
    pub extrusion: P3,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Circle {
    pub center: P3,
    pub radius: f64,
    pub thickness: f64,
    pub extrusion: P3,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Arc {
    pub center: P3,
    pub radius: f64,
    pub thickness: f64,
    pub extrusion: P3,
    pub start_angle: f64,
    pub end_angle: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Point {
    pub position: P3,
    pub thickness: f64,
    pub extrusion: P3,
    pub x_axis_angle: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ellipse {
    pub center: P3,
    pub major_axis: P3,
    pub extrusion: P3,
    pub axis_ratio: f64,
    pub start_param: f64,
    pub end_param: f64,
}

/// SOLID (and TRACE): four corners at one elevation.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Solid {
    pub thickness: f64,
    pub elevation: f64,
    pub corners: [P2; 4],
    pub extrusion: P3,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LwVertex {
    pub point: P2,
    pub bulge: f64,
    pub start_width: f64,
    pub end_width: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LwPolyline {
    /// Raw flag bits (0x200 closed, 0x100 plinegen).
    pub flags: i16,
    pub const_width: f64,
    pub elevation: f64,
    pub thickness: f64,
    pub extrusion: P3,
    pub vertices: Vec<LwVertex>,
}

impl LwPolyline {
    pub fn closed(&self) -> bool {
        self.flags & 0x200 != 0
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Polyline2d {
    /// DXF 70 flags (1 = closed, 2 = curve fit, 4 = spline fit, 128 = plinegen).
    pub flags: i16,
    pub curve_type: i16,
    pub start_width: f64,
    pub end_width: f64,
    pub thickness: f64,
    pub elevation: f64,
    pub extrusion: P3,
    /// VERTEX objects, in order.
    pub vertices: Vec<Handle>,
    pub seqend: Handle,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Vertex2d {
    pub flags: u8,
    pub point: P3,
    pub start_width: f64,
    pub end_width: f64,
    pub bulge: f64,
    pub tangent_dir: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Insert {
    pub block_header: Handle,
    pub insertion: P3,
    pub scale: P3,
    pub rotation: f64,
    pub extrusion: P3,
    /// ATTRIB objects owned by this INSERT.
    pub attribs: Vec<Handle>,
    pub seqend: Option<Handle>,
    /// MINSERT grid: (columns, rows, column spacing, row spacing).
    pub grid: Option<(i16, i16, f64, f64)>,
}

/// TEXT, and the text part of ATTRIB / ATTDEF.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Text {
    pub elevation: f64,
    pub insertion: P2,
    pub alignment: Option<P2>,
    pub extrusion: P3,
    pub thickness: f64,
    pub oblique: f64,
    pub rotation: f64,
    pub height: f64,
    pub width_factor: f64,
    pub value: String,
    pub generation: i16,
    pub halign: i16,
    pub valign: i16,
    pub style: Handle,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Attrib {
    pub text: Text,
    pub tag: String,
    pub flags: u8,
    pub lock_position: bool,
    /// ATTDEF only.
    pub prompt: Option<String>,
    /// R2018 multi-line attribute: the embedded MTEXT.
    pub mtext: Option<Box<MText>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct MText {
    pub insertion: P3,
    pub extrusion: P3,
    pub x_axis: P3,
    pub rect_width: f64,
    pub rect_height: f64,
    pub text_height: f64,
    pub attachment: i16,
    pub drawing_dir: i16,
    pub value: String,
    pub style: Handle,
    pub linespacing_style: i16,
    pub linespacing_factor: f64,
    /// 0 none, 1 fill colour, 2 window colour, 0x10 text frame.
    pub background_flags: i32,
    pub background_scale: f64,
    pub background_color: Option<Color>,
    pub background_transparency: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum HatchEdge {
    Line { start: P2, end: P2 },
    Arc { center: P2, radius: f64, start_angle: f64, end_angle: f64, ccw: bool },
    Ellipse { center: P2, major_end: P2, ratio: f64, start_angle: f64, end_angle: f64, ccw: bool },
    Spline { degree: i32, rational: bool, periodic: bool, knots: Vec<f64>, control: Vec<[f64; 3]>, fit: Vec<P2> },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct HatchPath {
    /// 1 external, 2 polyline, 4 derived, 8 textbox, 16 outermost.
    pub flags: i32,
    /// Polyline path: (x, y, bulge).
    pub polyline: Option<(bool, Vec<[f64; 3]>)>,
    pub edges: Vec<HatchEdge>,
    pub boundary_handles: Vec<Handle>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PatternLine {
    pub angle: f64,
    pub base: P2,
    pub offset: P2,
    pub dashes: Vec<f64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hatch {
    pub gradient: bool,
    pub gradient_name: String,
    /// Gradient angle (radians), shift, one-colour flag, tint, colours.
    pub gradient_angle: f64,
    pub gradient_shift: f64,
    pub gradient_one_color: bool,
    pub gradient_tint: f64,
    pub gradient_colors: Vec<Color>,
    pub elevation: f64,
    pub extrusion: P3,
    pub pattern_name: String,
    pub solid: bool,
    pub associative: bool,
    pub paths: Vec<HatchPath>,
    pub style: i16,
    pub pattern_type: i16,
    pub pattern_angle: f64,
    pub pattern_scale: f64,
    pub double: bool,
    pub pattern_lines: Vec<PatternLine>,
    pub seed_points: Vec<P2>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spline {
    pub scenario: i32,
    pub degree: i32,
    pub rational: bool,
    pub closed: bool,
    pub periodic: bool,
    pub knots: Vec<f64>,
    pub control_points: Vec<P3>,
    pub weights: Vec<f64>,
    pub fit_points: Vec<P3>,
    pub start_tangent: P3,
    pub end_tangent: P3,
}

/// WIPEOUT (and IMAGE): a clipped parallelogram.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Wipeout {
    pub insertion: P3,
    pub u_vector: P3,
    pub v_vector: P3,
    pub size: P2,
    pub display_flags: i16,
    pub clipping: bool,
    /// 1 rectangle (2 corners), 2 polygon.
    pub clip_type: i16,
    pub clip_inside: bool,
    pub clip_vertices: Vec<P2>,
    /// IMAGE only: 0..=100, and the IMAGEDEF handle.
    pub brightness: u8,
    pub contrast: u8,
    pub fade: u8,
    pub imagedef: Handle,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dimension {
    /// DWG type number (20..=26) or 0 for ARC_DIMENSION / LARGE_RADIAL.
    pub kind: u16,
    pub extrusion: P3,
    pub text_mid: P3,
    pub user_text: String,
    pub text_rotation: f64,
    pub insertion: P3,
    pub dimstyle: Handle,
    /// The anonymous block with the dimension's graphics.
    pub block: Handle,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Viewport {
    pub center: P3,
    pub width: f64,
    pub height: f64,
    pub view_target: P3,
    pub view_direction: P3,
    pub twist_angle: f64,
    pub view_height: f64,
    pub lens_length: f64,
    pub front_clip: f64,
    pub back_clip: f64,
    pub view_center: P2,
    /// DXF 90 status flags.
    pub status: i32,
    pub frozen_layers: Vec<Handle>,
    pub clip_boundary: Handle,
    pub shade_plot_mode: i16,
}

/// The geometry part of an entity.
#[derive(Clone, Debug, PartialEq)]
pub enum EntityKind {
    Line(Line),
    Circle(Circle),
    Arc(Arc),
    Point(Point),
    Ellipse(Ellipse),
    Solid(Solid),
    Trace(Solid),
    LwPolyline(LwPolyline),
    Polyline2d(Polyline2d),
    Vertex2d(Vertex2d),
    Insert(Insert),
    Text(Text),
    Attrib(Attrib),
    AttDef(Attrib),
    MText(MText),
    Hatch(Hatch),
    Spline(Spline),
    Wipeout(Wipeout),
    Dimension(Dimension),
    Viewport(Viewport),
    Block { name: String },
    EndBlk,
    SeqEnd,
    // Added for the Scene adapter (types in extra.rs / mleader.rs).
    Polyline3d(crate::extra::Polyline3d),
    /// VERTEX_3D, VERTEX_MESH and VERTEX_PFACE.
    Vertex3d(crate::extra::Vertex3d),
    /// POLYLINE_PFACE and POLYLINE_MESH.
    PolyMesh(crate::extra::PolyMesh),
    FaceRecord(crate::extra::FaceRecord),
    Leader(Box<crate::extra::Leader>),
    Tolerance(Box<crate::extra::Tolerance>),
    Face3d(crate::extra::Face3d),
    MLine(Box<crate::extra::MLine>),
    Table(Box<crate::extra::Table>),
    MLeader(Box<crate::mleader::MLeader>),
    Helix(Box<crate::extra::Helix>),
    Mesh(Box<crate::extra::Mesh>),
    /// RAY and XLINE.
    XLine(crate::extra::XLine),
    /// REGION, 3DSOLID and BODY: raw ACIS data.
    Acis(Box<crate::acis::Acis>),
    /// An entity type this crate does not decode (yet). The common data is
    /// still read.
    Other,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    pub common: EntityCommon,
    pub kind: EntityKind,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockHeader {
    pub name: String,
    pub anonymous: bool,
    pub has_attribs: bool,
    pub is_xref: bool,
    pub is_overlay: bool,
    pub base_point: P3,
    pub xref_path: String,
    pub description: String,
    pub insert_units: i16,
    pub explodable: bool,
    pub block_entity: Handle,
    pub entities: Vec<Handle>,
    pub endblk: Handle,
    pub layout: Handle,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layer {
    pub name: String,
    /// DXF 70-style bits: 1 frozen, 2 frozen in new viewports, 4 locked.
    pub frozen: bool,
    pub on: bool,
    pub frozen_new_vp: bool,
    pub locked: bool,
    pub plot: bool,
    pub lineweight_raw: u8,
    pub color: Color,
    pub ltype: Handle,
    pub plotstyle: Handle,
    pub material: Handle,
    /// Extended data blocks: (APPID handle, raw EED bytes). Layer
    /// transparency is stored here (app "AcCmTransparency", group 1071).
    pub eed: Vec<(Handle, Vec<u8>)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LtypeDash {
    pub length: f64,
    pub shape_number: i16,
    pub offset: P2,
    pub scale: f64,
    pub rotation: f64,
    /// 1 = rotation absolute, 2 = text, 4 = shape.
    pub flags: i16,
    pub style: Handle,
    pub text: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ltype {
    pub name: String,
    pub description: String,
    pub pattern_length: f64,
    pub dashes: Vec<LtypeDash>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextStyle {
    pub name: String,
    pub is_shape_file: bool,
    pub vertical: bool,
    pub fixed_height: f64,
    pub width_factor: f64,
    pub oblique: f64,
    pub generation: u8,
    pub last_height: f64,
    pub font_file: String,
    pub bigfont_file: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub name: String,
    /// Unprintable margins in mm: left, bottom, right, top (DXF 40-43).
    pub margins: [f64; 4],
    pub plot_paper_units: i16,
    pub paper_width: f64,
    pub paper_height: f64,
    pub plot_rotation: i16,
    pub plot_origin: P2,
    pub plot_type: i16,
    pub window_min: P2,
    pub window_max: P2,
    pub scale_numerator: f64,
    pub scale_divisor: f64,
    pub tab_order: i32,
    pub flags: i16,
    pub limmin: P2,
    pub limmax: P2,
    pub insbase: P3,
    pub extmin: P3,
    pub extmax: P3,
    pub block_record: Handle,
    pub active_viewport: Handle,
    pub viewports: Vec<Handle>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SortEntsTable {
    pub block_owner: Handle,
    /// (sort handle, entity handle) pairs.
    pub entries: Vec<(Handle, Handle)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Dictionary {
    pub hard_owner: bool,
    pub entries: Vec<(String, Handle)>,
}

/// LAYER_CONTROL, BLOCK_CONTROL, ...: list of table entries.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TableControl {
    pub entries: Vec<Handle>,
    /// BLOCK_CONTROL only: *MODEL_SPACE and *PAPER_SPACE.
    pub extra: Vec<Handle>,
}

/// The decoded payload of one object.
#[derive(Clone, Debug, PartialEq)]
pub enum ObjectKind {
    Entity(Box<Entity>),
    BlockHeader(BlockHeader),
    Layer(Layer),
    Ltype(Ltype),
    Style(TextStyle),
    Layout(Box<Layout>),
    SortEnts(SortEntsTable),
    Dictionary(Dictionary),
    Control(TableControl),
    /// DBCOLOR: a named colour from a colour book.
    DbColor { color: Color, name: String, book: String },
    // Added for the Scene adapter (types in tables2.rs, dimstyle.rs, mleader.rs).
    XRecord(crate::tables2::XRecord),
    /// APPID: registered application name.
    AppId(String),
    DimStyle(Box<crate::dimstyle::DimStyle>),
    VPort(Box<crate::dimstyle::VPort>),
    MLeaderStyle(Box<crate::mleader::MLeaderStyle>),
    /// WIPEOUTVARIABLES: 70 display frame (1 = frames are drawn and plotted).
    WipeoutVariables { frame: i16 },
    /// IMAGEDEF: the picture file name.
    ImageDef { file: String },
    /// A non-entity type this crate does not decode; skipped by its size.
    Unknown,
}

/// One object of the file.
#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    pub handle: Handle,
    /// DWG type number (classes are 500+).
    pub type_code: u16,
    pub owner: Handle,
    pub xdict: Option<Handle>,
    pub reactors: Vec<Handle>,
    pub kind: ObjectKind,
}

impl Object {
    pub fn entity(&self) -> Option<&Entity> {
        match &self.kind {
            ObjectKind::Entity(e) => Some(e),
            _ => None,
        }
    }
}
