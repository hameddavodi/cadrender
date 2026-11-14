//! Entities of the scene.
//!
//! An [`Entity`] is the common part (layer, colour, lineweight, linetype,
//! bounding box) plus a [`Kind`]: the shape-specific data. Every kind lives in
//! its own module and implements [`EntityKind`]. The list of kinds is written
//! once, in the `entity_kinds!` call below; the macro makes the `Kind` enum and
//! the dispatch functions, so there are no big `match` blocks anywhere else.
//!
//! **Adding a new kind** (see docs/architecture.md):
//! 1. new file `entity/<name>.rs` with a struct and `impl EntityKind for it`,
//! 2. `pub mod <name>;` and one line in `entity_kinds!` below,
//! 3. make a reader (cadrender-dxf) build it.

pub mod fill;
pub mod frame;
pub mod hatch;
pub mod image;
pub mod insert;
pub mod mleader;
pub mod pline;
pub mod point;
pub mod stroke;
pub mod text;
pub mod viewport;
pub mod xline;

use crate::color::EntColor;
use crate::geom::BBox;
use crate::render::walk::{Visit, Walker};
use crate::scene::tables::{BlockId, Handle, LayerId, LinetypeId, LT_BYLAYER, LW_BYLAYER};
use crate::sink::Sink;

/// Common properties of every entity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Props {
    pub handle: Handle,
    pub layer: LayerId,
    pub color: EntColor,
    /// Lineweight in 1/100 mm, or LW_BYLAYER / LW_BYBLOCK / LW_DEFAULT.
    pub lw: i16,
    pub ltype: LinetypeId,
    /// Entity linetype scale (group 48).
    pub ltscale: f32,
    /// 255 = opaque.
    pub alpha: u8,
}

impl Default for Props {
    fn default() -> Self { Props { handle: 0, layer: 0, color: EntColor::BYLAYER, lw: LW_BYLAYER, ltype: LT_BYLAYER, ltscale: 1.0, alpha: 255 } }
}

/// One entity in a block (model space and paper space are blocks too).
#[derive(Clone, Debug)]
pub struct Entity {
    pub props: Props,
    /// Bounding box in block coordinates. Set when the scene is built.
    pub bbox: BBox,
    pub kind: Kind,
}

impl Entity {
    pub fn new(props: Props, kind: impl Into<Kind>) -> Entity { Entity { props, bbox: BBox::EMPTY, kind: kind.into() } }
}

/// What a kind sees while the scene is built (bounding boxes, bottom-up).
pub struct PrepareCx<'a> {
    pub blocks: &'a [BlockInfo],
}

/// Facts about a block that inserts need. `bbox` is empty while the block is
/// still being prepared (a reference cycle), so a cycle cannot loop forever.
#[derive(Clone, Copy, Debug, Default)]
pub struct BlockInfo {
    pub bbox: BBox,
    pub base: [f64; 2],
    pub expanded: u64,
    pub depth: u32,
}

/// The behaviour of one entity kind.
pub trait EntityKind {
    /// Called once when the scene is built. Return the bounding box in block
    /// coordinates. May fix up own data (an INSERT applies the block base point here).
    fn prepare(&mut self, cx: &PrepareCx) -> BBox;

    /// Draw the entity. The walk has already checked layer visibility and culled
    /// by bounding box. `v` has the matrix to device pixels and the style inherited
    /// from the parent INSERT.
    fn draw<S: Sink>(&self, e: &Entity, v: &Visit, w: &mut Walker<'_, S>);

    /// Blocks this entity refers to (to prepare them first).
    fn block_refs(&self, _out: &mut Vec<BlockId>) {}

    /// How many plain entities this one stands for when fully expanded (statistics).
    fn expanded(&self, _cx: &PrepareCx) -> u64 { 1 }

    /// Nesting depth below this entity (0 for plain entities).
    fn depth(&self, _cx: &PrepareCx) -> u32 { 0 }

    /// Bounding box after the clean rules removed things (used only to frame the
    /// sheet). `clean_bbox(b)` gives the cleaned box of block `b`.
    fn clean_bbox(&self, e: &Entity, _dropped: &dyn Fn(LayerId) -> bool, _clean_bbox: &dyn Fn(BlockId) -> Option<BBox>) -> Option<BBox> { Some(e.bbox) }
}

macro_rules! entity_kinds {
    ($($variant:ident($ty:ty)),* $(,)?) => {
        /// The shape-specific part of an entity. One variant per module.
        #[derive(Clone, Debug)]
        pub enum Kind { $($variant($ty)),* }

        $(impl From<$ty> for Kind { fn from(k: $ty) -> Kind { Kind::$variant(k) } })*

        impl Kind {
            /// Short name for statistics and errors.
            pub fn name(&self) -> &'static str { match self { $(Kind::$variant(_) => stringify!($variant)),* } }
            pub(crate) fn prepare(&mut self, cx: &PrepareCx) -> BBox { match self { $(Kind::$variant(k) => k.prepare(cx)),* } }
            #[inline]
            pub(crate) fn draw<S: Sink>(&self, e: &Entity, v: &Visit, w: &mut Walker<'_, S>) { match self { $(Kind::$variant(k) => k.draw(e, v, w)),* } }
            pub(crate) fn block_refs(&self, out: &mut Vec<BlockId>) { match self { $(Kind::$variant(k) => k.block_refs(out)),* } }
            pub(crate) fn expanded(&self, cx: &PrepareCx) -> u64 { match self { $(Kind::$variant(k) => k.expanded(cx)),* } }
            pub(crate) fn depth(&self, cx: &PrepareCx) -> u32 { match self { $(Kind::$variant(k) => k.depth(cx)),* } }
            pub(crate) fn clean_bbox(&self, e: &Entity, dropped: &dyn Fn(LayerId) -> bool, cb: &dyn Fn(BlockId) -> Option<BBox>) -> Option<BBox> { match self { $(Kind::$variant(k) => k.clean_bbox(e, dropped, cb)),* } }
        }
    };
}

// Keep `Kind` small (it is walked millions of times per sheet): big, rarely
// drawn kinds are boxed. Write `Name(Box<module::Type>)` for those and add a
// `From<module::Type>` impl below.
entity_kinds! {
    Stroke(stroke::StrokeEnt),
    Fill(fill::FillEnt),
    Hatch(hatch::HatchEnt),
    Text(Box<text::TextEnt>),
    Insert(insert::InsertEnt),
    Viewport(Box<viewport::ViewportEnt>),
    Point(point::PointEnt),
    Pline(pline::PlineEnt),
    XLine(xline::XLineEnt),
    Frame(frame::FrameEnt),
    Image(Box<image::ImageEnt>),
}

impl From<text::TextEnt> for Kind { fn from(k: text::TextEnt) -> Kind { Kind::Text(Box::new(k)) } }
impl From<image::ImageEnt> for Kind { fn from(k: image::ImageEnt) -> Kind { Kind::Image(Box::new(k)) } }
impl From<viewport::ViewportEnt> for Kind { fn from(k: viewport::ViewportEnt) -> Kind { Kind::Viewport(Box::new(k)) } }

/// A boxed kind behaves like the kind.
impl<T: EntityKind> EntityKind for Box<T> {
    fn prepare(&mut self, cx: &PrepareCx) -> BBox { (**self).prepare(cx) }
    #[inline]
    fn draw<S: Sink>(&self, e: &Entity, v: &Visit, w: &mut Walker<'_, S>) { (**self).draw(e, v, w) }
    fn block_refs(&self, out: &mut Vec<BlockId>) { (**self).block_refs(out) }
    fn expanded(&self, cx: &PrepareCx) -> u64 { (**self).expanded(cx) }
    fn depth(&self, cx: &PrepareCx) -> u32 { (**self).depth(cx) }
    fn clean_bbox(&self, e: &Entity, d: &dyn Fn(LayerId) -> bool, cb: &dyn Fn(BlockId) -> Option<BBox>) -> Option<BBox> { (**self).clean_bbox(e, d, cb) }
}

#[cfg(test)]
mod size_tests {
    /// The walk touches every entity of a visible block: keep them small.
    #[test]
    fn entity_stays_small() {
        assert!(std::mem::size_of::<super::Entity>() <= 160, "Entity is {} bytes", std::mem::size_of::<super::Entity>());
    }
}
