//! The Scene: one read-only picture of a drawing file.
//!
//! Built once per file by a reader (DXF now, DWG later) through
//! [`SceneBuilder`]. After `build()` nothing changes, so all sheets and
//! viewports can render from it at the same time on many threads.

pub mod builder;
pub mod meta;
pub mod tables;

pub use builder::SceneBuilder;
pub use meta::{InsertRec, Meta, TextKind, TextRec};
pub use tables::*;

use crate::entity::viewport::ViewportEnt;
use crate::entity::{Entity, Kind};
use crate::geom::BBox;
use crate::text::Fonts;
use std::borrow::Cow;
use std::collections::BTreeMap;

/// A block definition. Model space and each paper-space layout are blocks too.
/// Stored once; INSERTs refer to it by id.
#[derive(Clone, Debug, Default)]
pub struct Block {
    pub name: String,
    /// Handle of the BLOCK_RECORD.
    pub handle: Handle,
    pub base: [f64; 2],
    pub ents: Vec<Entity>,
    /// Box of all entities (block coordinates).
    pub bbox: BBox,
    /// Number of plain entities when fully expanded (statistics).
    pub expanded: u64,
    /// Deepest INSERT nesting inside.
    pub depth: u32,
    /// `ents` are stored in draw order (SORTENTS applied when the scene is
    /// built, so the walk reads them in memory order). If that changed the
    /// order, this lists the entities in file order (indices into `ents`).
    pub file_order: Option<Box<[u32]>>,
}

impl Block {
    /// The name metadata uses. Anonymous blocks (`*U`, `*D`, `*T` ...) are
    /// numbered differently by each writer (and by the DWG reader), so they
    /// are named by their BLOCK_RECORD handle: `*U#2F1A`. The same block then
    /// has the same name in a DWG and in its DXF.
    pub fn meta_name(&self) -> Cow<'_, str> {
        let n = &self.name;
        let lower = n.to_ascii_lowercase();
        if !n.starts_with('*') || self.handle == 0 || lower.starts_with("*model_space") || lower.starts_with("*paper_space") { return Cow::Borrowed(n); }
        let prefix: String = n.chars().take(1).chain(n.chars().skip(1).take_while(|c| c.is_ascii_alphabetic())).collect();
        Cow::Owned(format!("{prefix}#{:X}", self.handle))
    }
}

/// One paper-space tab.
#[derive(Clone, Debug)]
pub struct Layout {
    pub name: String,
    /// Tab order (group 71).
    pub order: i32,
    pub block: BlockId,
    pub handle: Handle,
    /// Viewports to draw, as indices into the block's entities, in drawing order.
    /// The paper-space "overall" viewport is not in this list.
    pub viewports: Vec<u32>,
    /// Paper and plot settings of the tab.
    pub paper: PaperSetup,
}

/// Paper and plot settings of a layout (LAYOUT object). Readers fill what they know.
#[derive(Clone, Debug, PartialEq)]
pub struct PaperSetup {
    /// The layout's own PSLTSCALE flag; `None` = use the header value.
    pub psltscale: Option<bool>,
    /// Paper units per mm (1 for mm layouts, 1/25.4 for inch layouts).
    pub units_per_mm: f64,
    /// Layout limits (the paper outline in paper units), if valid.
    pub limits: Option<BBox>,
    /// Paper size in mm (width, height), if known.
    pub size_mm: Option<(f64, f64)>,
    /// Unprintable margins in mm: left, bottom, right, top.
    pub margins_mm: [f64; 4],
}

impl Default for PaperSetup {
    fn default() -> Self { PaperSetup { psltscale: None, units_per_mm: 1.0, limits: None, size_mm: None, margins_mm: [0.0; 4] } }
}

/// What the reader saw: entity counts and types it did not draw.
#[derive(Clone, Debug, Default)]
pub struct SceneStats {
    /// Entity type name -> count, for every entity read.
    pub counts: BTreeMap<String, u64>,
    /// Entity type name -> count, for entities not turned into scene entities.
    pub skipped: BTreeMap<String, u64>,
    /// Problems the reader worked around.
    pub warnings: Vec<String>,
}

#[derive(Debug)]
pub struct Scene {
    pub header: Header,
    pub layers: Vec<Layer>,
    pub linetypes: Vec<Linetype>,
    pub text_styles: Vec<TextStyle>,
    pub dim_styles: Vec<DimStyle>,
    pub mleader_styles: Vec<MLeaderStyle>,
    pub blocks: Vec<Block>,
    pub model: BlockId,
    /// Paper-space tabs in tab order. Model space is not in this list.
    pub layouts: Vec<Layout>,
    pub fonts: Fonts,
    /// Blocks in bottom-up order (a block comes after all blocks it inserts).
    pub prepare_order: Vec<BlockId>,
    pub stats: SceneStats,
    /// Block attributes and text content (not drawn; see `meta.rs`).
    pub meta: Meta,
}

impl Scene {
    pub fn layout(&self, name: &str) -> Option<&Layout> { self.layouts.iter().find(|l| l.name == name) }
    pub fn sheet_names(&self) -> Vec<&str> { self.layouts.iter().map(|l| l.name.as_str()).collect() }
    pub fn model_block(&self) -> &Block { &self.blocks[self.model as usize] }
    /// The viewports a layout draws.
    pub fn viewports<'a>(&'a self, layout: &'a Layout) -> impl Iterator<Item = &'a ViewportEnt> + 'a {
        let ents = &self.blocks[layout.block as usize].ents;
        layout.viewports.iter().filter_map(move |&i| match ents.get(i as usize).map(|e| &e.kind) { Some(Kind::Viewport(v)) => Some(&**v), _ => None })
    }
    /// Name of the place a block's entities are on: the sheet name for a
    /// sheet's block, "Model" for model space, else [`Scene::block_name`].
    pub fn owner_name(&self, block: BlockId) -> Cow<'_, str> {
        if block == self.model { return Cow::Borrowed("Model"); }
        match self.layouts.iter().find(|l| l.block == block) {
            Some(l) => Cow::Borrowed(&l.name),
            None => self.block_name(block),
        }
    }
    /// A block's name for metadata (see [`Block::meta_name`]).
    pub fn block_name(&self, block: BlockId) -> Cow<'_, str> { self.blocks.get(block as usize).map_or(Cow::Borrowed(""), |b| b.meta_name()) }
    /// Number of scene entities (all blocks, attributes included).
    pub fn entity_count(&self) -> u64 {
        self.blocks.iter().map(|b| b.ents.iter().map(|e| 1 + if let Kind::Insert(i) = &e.kind { i.attribs.len() as u64 } else { 0 }).sum::<u64>()).sum()
    }
}
