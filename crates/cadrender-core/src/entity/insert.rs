//! INSERT: a placed block, also with rows and columns (MINSERT) and ATTRIBs.
//! DIMENSION and ACAD_TABLE use this too (through their ready-made blocks).
//! The block is never copied: the walk goes into it with a new matrix.

use super::{Entity, EntityKind, PrepareCx};
use crate::geom::{Aff, BBox};
use crate::render::walk::{Visit, Walker};
use crate::scene::tables::{BlockId, LayerId};
use crate::sink::Sink;

#[derive(Clone, Debug)]
pub struct InsertEnt {
    pub block: BlockId,
    /// Block coordinates -> parent coordinates. The block base point is
    /// applied when the scene is built.
    pub m: Aff,
    /// Rows and columns (MINSERT), boxed because it is rare.
    pub grid: Option<Box<Grid>>,
    /// ATTRIB texts, in parent coordinates.
    pub attribs: Vec<Entity>,
}

/// MINSERT grid: `cols` x `rows` copies, spacing in the insert's rotated frame, before scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Grid { pub cols: u32, pub rows: u32, pub dx: f64, pub dy: f64 }

impl InsertEnt {
    pub fn new(block: BlockId, m: Aff) -> InsertEnt { InsertEnt { block, m, grid: None, attribs: vec![] } }
    /// Set a MINSERT grid (1 x 1 removes it).
    pub fn set_grid(&mut self, cols: u32, rows: u32, dx: f64, dy: f64) {
        self.grid = if cols.max(1) * rows.max(1) > 1 { Some(Box::new(Grid { cols: cols.max(1), rows: rows.max(1), dx, dy })) } else { None };
    }
    /// (cols, rows)
    pub fn size(&self) -> (u32, u32) { self.grid.as_ref().map_or((1, 1), |g| (g.cols, g.rows)) }

    fn cell(&self, c: u32, r: u32) -> Aff {
        match &self.grid { Some(g) if c + r > 0 => self.m.mul(&Aff::translate(c as f64 * g.dx, r as f64 * g.dy)), _ => self.m }
    }
    fn grid_bbox(&self, bb: &BBox) -> BBox {
        let mut b = BBox::EMPTY;
        if bb.is_empty() { return b; }
        let (c, r) = self.size();
        let (c, r) = (c - 1, r - 1);
        for (cx, cy) in [(0, 0), (c, 0), (0, r), (c, r)] { b.union(&bb.transformed(&self.cell(cx, cy))); }
        b
    }
}

impl EntityKind for InsertEnt {
    fn prepare(&mut self, cx: &PrepareCx) -> BBox {
        let info = cx.blocks.get(self.block as usize).copied().unwrap_or_default();
        self.m = self.m.mul(&Aff::translate(-info.base[0], -info.base[1]));
        let mut b = self.grid_bbox(&info.bbox);
        for a in self.attribs.iter_mut() { a.bbox = a.kind.prepare(cx); b.union(&a.bbox); }
        b
    }

    fn draw<S: Sink>(&self, e: &Entity, v: &Visit, w: &mut Walker<'_, S>) {
        if w.prep.dropped_block.get(self.block as usize).copied().unwrap_or(false) { return; }
        let (st, ninh) = w.style(e, v.layer, &v.inh, 0.0);
        if v.tiny {
            w.out.set_stroke(st.solid());
            w.dot(&v.db);
            return;
        }
        if v.depth < w.opts.limits.max_depth {
            let scene = w.scene;
            if let Some(b) = scene.blocks.get(self.block as usize) {
                let order = b.file_order.as_deref();
                let (cols, rows) = self.size();
                for r in 0..rows {
                    for c in 0..cols {
                        let mm = v.m.mul(&self.cell(c, r));
                        w.walk(&b.ents, order, &mm, &ninh, v.depth + 1);
                    }
                }
            }
        } else {
            w.warnings.depth_limit = true;
        }
        if !self.attribs.is_empty() { w.walk(&self.attribs, None, &v.m, &v.inh, v.depth); }
    }

    fn block_refs(&self, out: &mut Vec<BlockId>) { out.push(self.block); }
    fn expanded(&self, cx: &PrepareCx) -> u64 {
        let (c, r) = self.size();
        cx.blocks.get(self.block as usize).map_or(0, |b| b.expanded) * c as u64 * r as u64 + self.attribs.len() as u64
    }
    fn depth(&self, cx: &PrepareCx) -> u32 { cx.blocks.get(self.block as usize).map_or(0, |b| b.depth) + 1 }

    fn clean_bbox(&self, _: &Entity, dropped: &dyn Fn(LayerId) -> bool, cb: &dyn Fn(BlockId) -> Option<BBox>) -> Option<BBox> {
        let bb = cb(self.block)?;
        let mut b = self.grid_bbox(&bb);
        for a in &self.attribs { if !dropped(a.props.layer) { b.union(&a.bbox); } }
        Some(b)
    }
}
