//! Work done once per (scene, options) and shared by all sheets: clean rules
//! turned into flags, ACI colours through the colour policy, cleaned block
//! boxes for framing. And the per-viewport layer table (`ViewStyle`).

use crate::color::Rgba;
use crate::entity::viewport::ViewportEnt;
use crate::geom::BBox;
use crate::options::RenderOptions;
use crate::scene::tables::{LayerId, LinetypeId};
use crate::scene::Scene;

pub struct Prepared {
    /// Layer is removed by the clean rules.
    pub dropped_layer: Vec<bool>,
    /// INSERTs of this block are removed by the clean rules.
    pub dropped_block: Vec<bool>,
    /// ACI index -> colour after the colour policy.
    pub aci: Box<[Rgba; 256]>,
    /// Block boxes without the cleaned entities (`None` if no clean rules).
    clean_bbox: Option<Vec<BBox>>,
    /// Framing boxes of the top-level model-space entities (`None` if no clean
    /// rules). Every viewport of every sheet needs them, so they are made once.
    model_frame: Option<Vec<Option<BBox>>>,
    /// Grid over the top-level model-space boxes (`None` for small drawings).
    pub model_index: Option<super::index::GridIndex>,
}

/// Look of one layer in one view (paper space or one viewport), after
/// per-viewport overrides.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerLook {
    pub color: Rgba,
    pub lw: i16,
    pub ltype: LinetypeId,
    pub alpha: u8,
}

/// The layer table for one view.
pub struct ViewStyle {
    /// Layer is not drawn in this view (off, frozen, frozen in the viewport, or cleaned).
    pub hidden: Vec<bool>,
    pub layers: Vec<LayerLook>,
}

impl Prepared {
    pub fn new(scene: &Scene, opts: &RenderOptions) -> Prepared {
        let dropped_layer: Vec<bool> = scene.layers.iter().map(|l| opts.clean.drops_layer(&l.name)).collect();
        let dropped_block: Vec<bool> = scene.blocks.iter().map(|b| opts.clean.drops_block(&b.name)).collect();
        let mut aci = Box::new([Rgba::BLACK; 256]);
        for (i, c) in aci.iter_mut().enumerate() { *c = opts.color.map_aci(i as i32, opts.background); }
        let clean_bbox = if opts.clean.is_empty() { None } else {
            let mut bb = vec![BBox::EMPTY; scene.blocks.len()];
            let mut done = vec![false; scene.blocks.len()];
            for &b in &scene.prepare_order {
                let mut acc = BBox::EMPTY;
                {
                    let dl = |l: LayerId| dropped_layer.get(l as usize).copied().unwrap_or(false);
                    let cb = |id: u32| -> Option<BBox> {
                        let i = id as usize;
                        if dropped_block.get(i).copied().unwrap_or(false) || !done.get(i).copied().unwrap_or(false) { None } else { Some(bb[i]) }
                    };
                    for e in &scene.blocks[b as usize].ents {
                        if matches!(e.kind, crate::entity::Kind::Viewport(_)) || dl(e.props.layer) { continue; }
                        if let Some(x) = e.kind.clean_bbox(e, &dl, &cb) { acc.union(&x); }
                    }
                }
                bb[b as usize] = acc;
                done[b as usize] = true;
            }
            Some(bb)
        };
        let boxes: Vec<BBox> = scene.model_block().ents.iter().map(|e| e.bbox).collect();
        let model_index = super::index::GridIndex::build(&boxes);
        let mut p = Prepared { dropped_layer, dropped_block, aci, clean_bbox, model_frame: None, model_index };
        if p.clean_bbox.is_some() {
            p.model_frame = Some(scene.model_block().ents.iter().map(|e| p.frame_bbox(e)).collect());
        }
        p
    }

    /// Framing box of the i-th top-level model-space entity.
    #[inline]
    pub fn model_frame_bbox(&self, i: usize, e: &crate::entity::Entity) -> Option<BBox> {
        match &self.model_frame { Some(v) => v.get(i).copied().flatten(), None => Some(e.bbox) }
    }

    /// Box of a top-level entity for framing the sheet, after the clean rules.
    /// `None` = the entity is removed.
    pub fn frame_bbox(&self, e: &crate::entity::Entity) -> Option<BBox> {
        let Some(cbb) = &self.clean_bbox else { return Some(e.bbox) };
        if self.dropped_layer.get(e.props.layer as usize).copied().unwrap_or(false) { return None; }
        let dl = |l: LayerId| self.dropped_layer.get(l as usize).copied().unwrap_or(false);
        let cb = |id: u32| -> Option<BBox> { if self.dropped_block.get(id as usize).copied().unwrap_or(false) { None } else { cbb.get(id as usize).copied() } };
        e.kind.clean_bbox(e, &dl, &cb)
    }

    /// The layer table for paper space (`vp = None`) or for one viewport.
    pub fn view(&self, scene: &Scene, opts: &RenderOptions, vp: Option<&ViewportEnt>) -> ViewStyle {
        let n = scene.layers.len();
        let mut hidden: Vec<bool> = scene.layers.iter().enumerate().map(|(i, l)| l.off || l.frozen || self.dropped_layer[i]).collect();
        let mut layers: Vec<LayerLook> = scene.layers.iter().map(|l| {
            let color = if let Some(rgb) = l.true_color { opts.color.map_rgb(rgb, opts.background) } else { self.aci[l.color.clamp(0, 255) as usize] };
            LayerLook { color, lw: l.lw, ltype: l.ltype, alpha: l.alpha }
        }).collect();
        if let Some(v) = vp {
            for &l in &v.frozen_layers { if (l as usize) < n { hidden[l as usize] = true; } }
            if v.handle != 0 {
                for (i, l) in scene.layers.iter().enumerate() {
                    for o in l.vp_overrides.iter().filter(|o| o.viewport == v.handle) {
                        let look = &mut layers[i];
                        if let Some(c) = o.color {
                            look.color = if c.is_true_color() { opts.color.map_rgb(c.0 as u32 & 0xff_ffff, opts.background) } else { self.aci[c.0.clamp(0, 255) as usize] };
                        }
                        if let Some(lt) = o.ltype { look.ltype = lt; }
                        if let Some(lw) = o.lw { look.lw = lw; }
                        if let Some(a) = o.alpha { look.alpha = a; }
                    }
                }
            }
        }
        ViewStyle { hidden, layers }
    }
}
