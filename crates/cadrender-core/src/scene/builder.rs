//! `SceneBuilder`: the one way readers fill a Scene. It owns the name ->
//! id maps and does the reader-independent work in `build()`: bounding boxes
//! bottom-up, draw order, viewport selection, tab order.

use super::tables::*;
use super::{Block, Layout, Scene, SceneStats};
use crate::entity::{BlockInfo, Entity, Kind, PrepareCx};
use crate::geom::BBox;
use crate::text::Fonts;
use std::collections::HashMap;

pub struct SceneBuilder {
    pub header: Header,
    layers: Vec<Layer>,
    layer_idx: HashMap<String, LayerId>,
    linetypes: Vec<Linetype>,
    ltype_idx: HashMap<String, LinetypeId>,
    ltype_by_handle: HashMap<Handle, LinetypeId>,
    text_styles: Vec<TextStyle>,
    style_idx: HashMap<String, StyleId>,
    pub dim_styles: Vec<DimStyle>,
    pub mleader_styles: Vec<MLeaderStyle>,
    blocks: Vec<Block>,
    block_idx: HashMap<String, BlockId>,
    model: BlockId,
    paper: BlockId,
    layouts: Vec<Layout>,
    sort_tables: Vec<(BlockId, Vec<(Handle, Handle)>)>,
    pub fonts: Fonts,
    pub stats: SceneStats,
}

fn key(name: &str) -> String { name.trim().to_uppercase() }

impl SceneBuilder {
    pub fn new(fonts: Fonts) -> SceneBuilder {
        let mut b = SceneBuilder {
            header: Header { ltscale: 1.0, psltscale: true, ..Header::default() },
            layers: vec![], layer_idx: HashMap::new(),
            linetypes: vec![], ltype_idx: HashMap::new(), ltype_by_handle: HashMap::new(),
            text_styles: vec![], style_idx: HashMap::new(),
            dim_styles: vec![], mleader_styles: vec![],
            blocks: vec![], block_idx: HashMap::new(), model: 0, paper: 0,
            layouts: vec![], sort_tables: vec![], fonts, stats: SceneStats::default(),
        };
        b.layer("0");
        b.model = b.block("*Model_Space");
        b.paper = b.block("*Paper_Space");
        b
    }

    // ---- layers -----------------------------------------------------------
    /// Id of a layer, created with default properties if new.
    pub fn layer(&mut self, name: &str) -> LayerId {
        let k = key(name);
        if let Some(&id) = self.layer_idx.get(&k) { return id; }
        let id = self.layers.len() as LayerId;
        self.layers.push(Layer::new(name.trim()));
        self.layer_idx.insert(k, id);
        id
    }
    pub fn layer_mut(&mut self, id: LayerId) -> &mut Layer { &mut self.layers[id as usize] }
    pub fn layers(&self) -> &[Layer] { &self.layers }

    // ---- linetypes --------------------------------------------------------
    pub fn add_linetype(&mut self, lt: Linetype) -> LinetypeId {
        let k = key(&lt.name);
        if let Some(&id) = self.ltype_idx.get(&k) { return id; }
        let id = self.linetypes.len() as LinetypeId;
        if lt.handle != 0 { self.ltype_by_handle.insert(lt.handle, id); }
        self.ltype_idx.insert(k, id);
        self.linetypes.push(lt);
        id
    }
    /// Linetype id by name. BYLAYER, BYBLOCK and CONTINUOUS give the special
    /// ids; an unknown name (or one without dashes) is drawn solid.
    pub fn linetype_id(&self, name: &str) -> LinetypeId {
        let k = key(name);
        match k.as_str() {
            "BYLAYER" | "" => LT_BYLAYER,
            "BYBLOCK" => LT_BYBLOCK,
            "CONTINUOUS" => LT_SOLID,
            _ => match self.ltype_idx.get(&k) { Some(&id) if !self.linetypes[id as usize].pattern.is_empty() => id, _ => LT_SOLID },
        }
    }
    pub fn linetype_by_handle(&self, h: Handle) -> Option<LinetypeId> { self.ltype_by_handle.get(&h).copied() }
    /// The linetype has gaps (is not drawn as a solid line).
    pub fn linetype_has_dashes(&self, id: LinetypeId) -> bool { self.linetypes.get(id as usize).is_some_and(|lt| lt.pattern.iter().any(|d| *d < 0.0)) }

    // ---- text styles ------------------------------------------------------
    /// Add a text style (or replace one with the same name). The font face is chosen here.
    pub fn add_text_style(&mut self, mut st: TextStyle) -> StyleId {
        st.face = self.fonts.face_for(&st.font);
        let k = key(&st.name);
        if let Some(&id) = self.style_idx.get(&k) { self.text_styles[id as usize] = st; return id; }
        let id = self.text_styles.len() as StyleId;
        self.style_idx.insert(k, id);
        self.text_styles.push(st);
        id
    }
    pub fn text_style(&self, name: &str) -> Option<&TextStyle> { self.style_idx.get(&key(name)).map(|&i| &self.text_styles[i as usize]) }
    /// Text style by handle (MULTILEADER refers to styles by handle). Few styles: a scan is fine.
    pub fn text_style_by_handle(&self, h: Handle) -> Option<&TextStyle> { if h == 0 { None } else { self.text_styles.iter().find(|s| s.handle == h) } }

    // ---- blocks -----------------------------------------------------------
    /// Id of a block by name, created empty if new (a block can be used before it is defined).
    pub fn block(&mut self, name: &str) -> BlockId {
        let k = key(name);
        if let Some(&id) = self.block_idx.get(&k) { return id; }
        let id = self.blocks.len() as BlockId;
        self.blocks.push(Block { name: name.trim().to_string(), ..Block::default() });
        self.block_idx.insert(k, id);
        id
    }
    pub fn find_block(&self, name: &str) -> Option<BlockId> { self.block_idx.get(&key(name)).copied() }
    pub fn block_mut(&mut self, id: BlockId) -> &mut Block { &mut self.blocks[id as usize] }
    /// All blocks (passes over every block after reading, for example WIPEOUT frames).
    pub fn blocks_mut(&mut self) -> &mut [Block] { &mut self.blocks }
    pub fn block_name(&self, id: BlockId) -> &str { &self.blocks[id as usize].name }
    pub fn model(&self) -> BlockId { self.model }
    /// The active paper-space block (`*Paper_Space`).
    pub fn paper(&self) -> BlockId { self.paper }
    pub fn push(&mut self, block: BlockId, e: Entity) { self.blocks[block as usize].ents.push(e); }

    // ---- layouts and draw order ------------------------------------------
    pub fn add_layout(&mut self, name: &str, order: i32, block: BlockId, handle: Handle, paper: super::PaperSetup) {
        self.layouts.push(Layout { name: name.to_string(), order, block, handle, viewports: vec![], paper });
    }
    /// SORTENTS table of a block: (entity handle, sort handle) pairs.
    pub fn add_sort_table(&mut self, block: BlockId, pairs: Vec<(Handle, Handle)>) {
        if !pairs.is_empty() { self.sort_tables.push((block, pairs)); }
    }

    pub fn count(&mut self, type_name: &str) { *self.stats.counts.entry(type_name.to_string()).or_default() += 1; }
    pub fn skip(&mut self, type_name: &str) { *self.stats.skipped.entry(type_name.to_string()).or_default() += 1; }
    pub fn warn(&mut self, msg: impl Into<String>) {
        let m = msg.into();
        if self.stats.warnings.len() < 100 && !self.stats.warnings.contains(&m) { self.stats.warnings.push(m); }
    }

    /// Finish: resolve handles, compute boxes, draw order, viewports, tab order.
    pub fn build(mut self) -> Scene {
        // layers frozen per viewport: handles -> ids
        let by_handle: HashMap<Handle, LayerId> = self.layers.iter().enumerate().filter(|(_, l)| l.handle != 0).map(|(i, l)| (l.handle, i as LayerId)).collect();
        for b in &mut self.blocks {
            for e in &mut b.ents {
                if let Kind::Viewport(v) = &mut e.kind {
                    v.frozen_layers = v.frozen_handles.iter().filter_map(|h| by_handle.get(h).copied()).collect();
                }
            }
        }
        for b in &mut self.blocks { resolve_viewport_clips(b); }
        let prepare_order = prepare_blocks(&mut self.blocks);
        for (block, pairs) in std::mem::take(&mut self.sort_tables) {
            let Some(b) = self.blocks.get_mut(block as usize) else { continue };
            if let Some(order) = draw_order(&b.ents, &pairs) {
                // store entities in draw order; keep the way back to file order
                let mut slots: Vec<Option<Entity>> = std::mem::take(&mut b.ents).into_iter().map(Some).collect();
                let mut file_order = vec![0u32; order.len()];
                for (pos, &i) in order.iter().enumerate() {
                    file_order[i as usize] = pos as u32;
                    if let Some(e) = slots[i as usize].take() { b.ents.push(e); }
                }
                b.file_order = Some(file_order.into_boxed_slice());
            }
        }
        self.layouts.sort_by_key(|l| l.order);
        for l in &mut self.layouts { l.viewports = select_viewports(&self.blocks[l.block as usize]); }
        Scene {
            header: self.header, layers: self.layers, linetypes: self.linetypes, text_styles: self.text_styles,
            dim_styles: self.dim_styles, mleader_styles: self.mleader_styles, blocks: self.blocks, model: self.model,
            layouts: self.layouts, fonts: self.fonts, prepare_order, stats: self.stats,
        }
    }
}

/// Bounding boxes, expanded counts and depth for every block, bottom-up with
/// an explicit stack (deep or cyclic block trees cannot overflow the stack).
/// Returns the bottom-up order.
fn prepare_blocks(blocks: &mut [Block]) -> Vec<BlockId> {
    let n = blocks.len();
    let mut info: Vec<BlockInfo> = blocks.iter().map(|b| BlockInfo { bbox: BBox::EMPTY, base: b.base, expanded: 0, depth: 0 }).collect();
    let mut state = vec![0u8; n]; // 0 new, 1 busy, 2 done
    let mut order = Vec::with_capacity(n);
    let mut stack: Vec<(usize, Vec<BlockId>, usize)> = vec![];
    for root in 0..n {
        if state[root] != 0 { continue; }
        state[root] = 1;
        stack.push((root, refs(&blocks[root]), 0));
        while let Some(top) = stack.last_mut() {
            if top.2 < top.1.len() {
                let child = top.1[top.2] as usize;
                top.2 += 1;
                if child < n && state[child] == 0 {
                    state[child] = 1;
                    let r = refs(&blocks[child]);
                    stack.push((child, r, 0));
                }
                continue;
            }
            let b = top.0;
            stack.pop();
            let cx = PrepareCx { blocks: &info };
            let (mut bb, mut expanded, mut depth) = (BBox::EMPTY, 0u64, 0u32);
            for e in blocks[b].ents.iter_mut() {
                e.bbox = e.kind.prepare(&cx);
                expanded += e.kind.expanded(&cx);
                depth = depth.max(e.kind.depth(&cx));
                if !matches!(e.kind, Kind::Viewport(_)) { bb.union(&e.bbox); }
            }
            info[b] = BlockInfo { bbox: bb, base: blocks[b].base, expanded, depth };
            let bl = &mut blocks[b];
            bl.bbox = bb; bl.expanded = expanded; bl.depth = depth;
            state[b] = 2;
            order.push(b as BlockId);
        }
    }
    order
}

/// Non-rectangular viewports: turn the clip boundary entity (a closed
/// polyline, circle, ellipse, spline ... in the same paper-space block) into a
/// polygon in paper coordinates.
fn resolve_viewport_clips(b: &mut Block) {
    let wanted: Vec<Handle> = b.ents.iter().filter_map(|e| match &e.kind {
        // flag 0x10000 = non-rectangular clipping on; LibreDWG writes 0 for all flags
        Kind::Viewport(v) if v.clip_handle != 0 && (v.flags & 0x10000 != 0 || v.flags == 0) => Some(v.clip_handle),
        _ => None,
    }).collect();
    if wanted.is_empty() { return; }
    let mut polys: HashMap<Handle, Vec<[f64; 2]>> = HashMap::new();
    for e in &b.ents {
        if !wanted.contains(&e.props.handle) { continue; }
        let path = match &e.kind { Kind::Stroke(s) => &s.path, Kind::Fill(f) => &f.path, _ => continue };
        let bb = path.bbox();
        let mut out = vec![];
        crate::geom::flatten(path, ((bb.w() + bb.h()) * 1e-4).max(1e-9), &mut out);
        // the biggest ring is the boundary
        if let Some(p) = out.into_iter().filter(|p| p.len() >= 3).max_by_key(|p| p.len()) { polys.insert(e.props.handle, p); }
    }
    for e in &mut b.ents {
        if let Kind::Viewport(v) = &mut e.kind {
            if let Some(p) = polys.get(&v.clip_handle) { if wanted.contains(&v.clip_handle) { v.clip = Some(p.clone()); } }
        }
    }
}

fn refs(b: &Block) -> Vec<BlockId> {
    let mut v = vec![];
    for e in &b.ents { e.kind.block_refs(&mut v); }
    v.sort_unstable();
    v.dedup();
    v
}

/// Draw order from SORTENTS: entities sort by their sort handle, entities not
/// in the table by their own handle (AutoCAD's rule). `None` if it is the
/// file order anyway, or if handles are missing.
fn draw_order(ents: &[Entity], pairs: &[(Handle, Handle)]) -> Option<Box<[u32]>> {
    if ents.iter().any(|e| e.props.handle == 0) { return None; }
    let map: HashMap<Handle, Handle> = pairs.iter().copied().collect();
    let mut keyed: Vec<(Handle, u32)> = ents.iter().enumerate().map(|(i, e)| (map.get(&e.props.handle).copied().unwrap_or(e.props.handle), i as u32)).collect();
    keyed.sort_by_key(|k| k.0); // stable: equal keys keep file order
    if keyed.iter().enumerate().all(|(i, k)| k.1 as usize == i) { return None; }
    Some(keyed.into_iter().map(|k| k.1).collect())
}

/// Which viewports of a sheet are drawn. Normal files: status > 0, sorted by
/// status, without status 1 (the paper-space viewport). LibreDWG writes status
/// 0 for every viewport: then all are treated as on, except the first one
/// (the paper-space viewport). Viewports that cannot be drawn are left out.
fn select_viewports(block: &Block) -> Vec<u32> {
    // in file order: "the first viewport" means the first one in the file
    let n = block.ents.len() as u32;
    let idx: Vec<u32> = match &block.file_order { Some(o) => o.to_vec(), None => (0..n).collect() };
    let mut all: Vec<(u32, i32)> = idx.into_iter().filter_map(|i| match &block.ents.get(i as usize)?.kind { Kind::Viewport(v) => Some((i, v.status)), _ => None }).collect();
    if all.iter().any(|v| v.1 > 0) {
        all.sort_by_key(|v| v.1);
        all.retain(|v| v.1 > 0);
        if all.first().is_some_and(|v| v.1 == 1) { all.remove(0); }
    } else if !all.is_empty() {
        all.remove(0);
    }
    all.into_iter().map(|v| v.0).filter(|&i| matches!(&block.ents[i as usize].kind, Kind::Viewport(v) if v.drawable())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::insert::InsertEnt;
    use crate::entity::stroke::StrokeEnt;
    use crate::entity::viewport::ViewportEnt;
    use crate::entity::Props;
    use crate::geom::{Aff, PathData};

    fn line(x0: f64, y0: f64, x1: f64, y1: f64, handle: Handle) -> Entity {
        let mut p = PathData::default();
        p.move_to(x0, y0); p.line_to(x1, y1);
        Entity::new(Props { handle, ..Props::default() }, StrokeEnt { path: p, width: 0.0 })
    }

    #[test]
    fn nested_blocks_get_boxes_and_counts() {
        let mut b = SceneBuilder::new(Fonts::new());
        let inner = b.block("INNER");
        b.block_mut(inner).base = [1.0, 1.0];
        b.push(inner, line(1.0, 1.0, 2.0, 2.0, 0));
        let outer = b.block("OUTER");
        b.push(outer, Entity::new(Props::default(), InsertEnt::new(inner, Aff::translate(10.0, 0.0))));
        b.push(outer, Entity::new(Props::default(), InsertEnt::new(inner, Aff::translate(20.0, 0.0))));
        let m = b.model();
        b.push(m, Entity::new(Props::default(), InsertEnt::new(outer, Aff::scale(2.0, 2.0))));
        let s = b.build();
        let model = s.model_block();
        assert_eq!(model.expanded, 2);
        assert_eq!(model.depth, 2);
        let bb = model.bbox;
        assert_eq!((bb.x0, bb.y0, bb.x1, bb.y1), (20.0, 0.0, 42.0, 2.0));
        // bottom-up order: INNER before OUTER before model
        let pos = |id: BlockId| s.prepare_order.iter().position(|&x| x == id).unwrap();
        assert!(pos(inner) < pos(outer) && pos(outer) < pos(s.model));
    }

    #[test]
    fn cyclic_blocks_do_not_hang() {
        let mut b = SceneBuilder::new(Fonts::new());
        let a = b.block("A");
        let c = b.block("C");
        b.push(a, Entity::new(Props::default(), InsertEnt::new(c, Aff::ID)));
        b.push(c, Entity::new(Props::default(), InsertEnt::new(a, Aff::ID)));
        b.push(c, line(0.0, 0.0, 1.0, 1.0, 0));
        let s = b.build();
        assert_eq!(s.prepare_order.len(), s.blocks.len());
    }

    #[test]
    fn sortents_order() {
        let ents = vec![line(0.0, 0.0, 1.0, 1.0, 0x10), line(0.0, 0.0, 1.0, 1.0, 0x11), line(0.0, 0.0, 1.0, 1.0, 0x12)];
        // put the first entity on top (largest sort handle)
        let o = draw_order(&ents, &[(0x10, 0x99)]).unwrap();
        assert_eq!(&o[..], &[1, 2, 0]);
        assert!(draw_order(&ents, &[(0x11, 0x11)]).is_none());
    }

    #[test]
    fn sortents_reorders_storage() {
        let mut b = SceneBuilder::new(Fonts::new());
        let m = b.model();
        for h in [0x10, 0x11, 0x12] { b.push(m, line(0.0, 0.0, h as f64, 1.0, h)); }
        b.add_sort_table(m, vec![(0x10, 0x99)]);
        let s = b.build();
        let blk = s.model_block();
        let handles: Vec<u64> = blk.ents.iter().map(|e| e.props.handle).collect();
        assert_eq!(handles, vec![0x11, 0x12, 0x10]);
        // file order: entity 0 of the file (0x10) is now at position 2
        assert_eq!(blk.file_order.as_deref(), Some(&[2u32, 0, 1][..]));
    }

    fn vp(status: i32) -> Entity {
        Entity::new(Props::default(), ViewportEnt { status, w: 10.0, h: 10.0, view_h: 10.0, top_view: true, ..ViewportEnt::default() })
    }

    #[test]
    fn viewport_selection() {
        let blk = |v: Vec<Entity>| Block { ents: v, ..Block::default() };
        // normal: status 1 is paper space, the rest by status
        assert_eq!(select_viewports(&blk(vec![vp(3), vp(1), vp(2), vp(0)])), vec![2, 0]);
        // LibreDWG: all 0 -> all on except the first
        assert_eq!(select_viewports(&blk(vec![vp(0), vp(0), vp(0)])), vec![1, 2]);
    }
}
