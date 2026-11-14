//! The instancing walk: go down the block tree with one matrix, cull by
//! bounding box at every level, resolve the style, and hand shapes to the
//! batcher. Blocks are never copied.

use super::batch::{Batcher, StrokeKey, NO_DASH};
use super::prep::{Prepared, ViewStyle};
use super::Stats;
use crate::color::{EntColor, Rgba};
use crate::entity::Entity;
use crate::geom::{Aff, BBox, PathData};
use crate::options::{LineweightPolicy, RenderOptions};
use crate::scene::tables::{LayerId, LinetypeId, LT_BYBLOCK, LT_BYLAYER, LT_SOLID, LW_BYBLOCK, LW_BYLAYER};
use crate::scene::Scene;
use crate::sink::Sink;

/// Style handed down from an INSERT to the entities of its block
/// (for layer "0" and BYBLOCK properties).
#[derive(Clone, Copy, Debug)]
pub struct Inherit {
    pub layer: LayerId,
    pub color: Rgba,
    pub lw: i16,
    pub ltype: LinetypeId,
}

impl Inherit {
    pub fn top() -> Inherit { Inherit { layer: 0, color: Rgba::BLACK, lw: 25, ltype: LT_SOLID } }
}

/// One entity visit: everything a kind needs to draw itself.
pub struct Visit {
    /// Entity (block) coordinates to device pixels.
    pub m: Aff,
    /// Mean scale of `m` (pixels per drawing unit).
    pub scale: f64,
    /// Entity bounding box in device pixels.
    pub db: BBox,
    /// Smaller than the level-of-detail dot size.
    pub tiny: bool,
    /// Layer after layer-"0" inheritance.
    pub layer: LayerId,
    pub inh: Inherit,
    pub depth: u32,
}

/// Things that went wrong in a sheet but did not stop it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Warnings {
    /// INSERT nesting deeper than `Limits::max_depth` was cut.
    pub depth_limit: bool,
    /// The sheet stopped at `Limits::max_visits`.
    pub visit_limit: bool,
}

impl Warnings {
    pub fn merge(&mut self, o: Warnings) { self.depth_limit |= o.depth_limit; self.visit_limit |= o.visit_limit; }
    pub fn messages(&self) -> Vec<String> {
        let mut v = vec![];
        if self.depth_limit { v.push("block nesting deeper than the limit was cut".to_string()); }
        if self.visit_limit { v.push("entity visit limit reached, sheet is incomplete".to_string()); }
        v
    }
}

pub struct Walker<'a, S: Sink> {
    pub scene: &'a Scene,
    pub opts: &'a RenderOptions,
    pub prep: &'a Prepared,
    pub view: &'a ViewStyle,
    pub out: Batcher<S>,
    /// Visible device rectangle.
    pub clip: BBox,
    /// Pixels per paper unit.
    pub paper_scale: f64,
    /// Pixels per linetype unit (depends on PSLTSCALE inside viewports).
    pub dash_scale: f64,
    /// Paper units per mm (for paper lineweights).
    pub paper_units_per_mm: f64,
    pub stats: Stats,
    pub warnings: Warnings,
}

impl<'a, S: Sink> Walker<'a, S> {
    /// Walk a list of entities (stored in draw order) with matrix `m`.
    /// `file_order` is the block's way back to file order, used only when the
    /// options turn draw order off.
    pub fn walk(&mut self, ents: &[Entity], file_order: Option<&[u32]>, m: &Aff, inh: &Inherit, depth: u32) {
        if self.warnings.visit_limit { return; }
        let scale = m.scale_factor();
        // top level of model space: ask the grid which entities are near the window
        if depth == 0 && (file_order.is_none() || self.opts.draw_order) {
            if let Some(ix) = &self.prep.model_index {
                if std::ptr::eq(ents.as_ptr(), self.scene.model_block().ents.as_ptr()) {
                    let r = self.clip.grow(3.0).transformed(&m.inverse());
                    if r.x0.is_finite() && r.y0.is_finite() && r.x1.is_finite() && r.y1.is_finite() {
                        let mut near = Vec::new();
                        ix.query(&r, &mut near);
                        for &i in &near { self.visit(&ents[i as usize], m, scale, inh, depth); }
                        return;
                    }
                }
            }
        }
        match file_order {
            Some(o) if !self.opts.draw_order => { for &i in o { if let Some(e) = ents.get(i as usize) { self.visit(e, m, scale, inh, depth); } } }
            _ => { for e in ents { self.visit(e, m, scale, inh, depth); } }
        }
    }

    #[inline]
    fn visit(&mut self, e: &Entity, m: &Aff, scale: f64, inh: &Inherit, depth: u32) {
        self.stats.visited += 1;
        if self.stats.visited > self.opts.limits.max_visits { self.warnings.visit_limit = true; return; }
        let layer = if e.props.layer == 0 && depth > 0 { inh.layer } else { e.props.layer };
        if self.view.hidden.get(layer as usize).copied().unwrap_or(false) { return; }
        let db = e.bbox.transformed(m);
        let c = &self.clip;
        if db.is_empty() || db.x1 < c.x0 - 2.0 || db.x0 > c.x1 + 2.0 || db.y1 < c.y0 - 2.0 || db.y0 > c.y1 + 2.0 {
            self.stats.culled += 1;
            return;
        }
        let lod = &self.opts.lod;
        let tiny = lod.enabled && db.w().max(db.h()) < lod.dot_px;
        let v = Visit { m: *m, scale, db, tiny, layer, inh: *inh, depth };
        e.kind.draw(e, &v, self);
    }

    /// Colour of an entity colour value, with BYLAYER/BYBLOCK resolved.
    #[inline]
    pub fn color(&self, c: EntColor, layer: LayerId, inh: &Inherit) -> Rgba {
        match c {
            EntColor::BYLAYER => self.view.layers[layer as usize].color,
            EntColor::BYBLOCK => inh.color,
            c if c.is_true_color() => self.opts.color.map_rgb(c.0 as u32 & 0xff_ffff, self.opts.background),
            EntColor(i) => self.prep.aci[i.clamp(0, 255) as usize],
        }
    }

    /// Lineweight (1/100 mm) to pen width in pixels.
    #[inline]
    pub fn lw_px(&self, lw: i16) -> f64 {
        let o = &self.opts.lineweight;
        let mm = (lw as f64 / 100.0).max(0.01);
        let px = match o.policy {
            LineweightPolicy::Preview => mm * self.opts.dpi / 72.0,
            LineweightPolicy::Paper => mm * self.paper_units_per_mm * self.paper_scale,
            LineweightPolicy::Hairline => o.min_px,
        };
        px.max(o.min_px)
    }

    /// Resolve the stroke style of an entity. `extra_px` is a minimum width
    /// (polyline width). Also returns the style its children inherit.
    pub fn style(&mut self, e: &Entity, layer: LayerId, inh: &Inherit, extra_px: f64) -> (StrokeKey, Inherit) {
        let p = &e.props;
        let look = &self.view.layers[layer as usize];
        let default_lw = self.opts.lineweight.default_lw;
        let color = self.color(p.color, layer, inh);
        let lw = match p.lw { LW_BYLAYER => if look.lw < 0 { default_lw } else { look.lw }, LW_BYBLOCK => inh.lw, x if x < 0 => default_lw, x => x };
        let ltype = match p.ltype { LT_BYLAYER => look.ltype, LT_BYBLOCK => inh.ltype, x => x };
        let alpha = if p.alpha < 255 { p.alpha } else { look.alpha };
        let px = self.lw_px(lw).max(extra_px).min(4000.0);
        let mut key = StrokeKey { color: color.with_alpha(alpha), width_q: (px * 16.0).round() as u16, dash: NO_DASH };
        if ltype < LT_SOLID {
            if let Some(lt) = self.scene.linetypes.get(ltype as usize) {
                let sc = self.lt_px(e, inh) as f32;
                let period: f64 = lt.pattern.iter().map(|d| d.abs()).sum::<f64>() * sc as f64;
                if period >= self.opts.lod.dash_min_period_px && period < 1e5 {
                    key.dash = self.out.dash_id(ltype, sc, &lt.pattern);
                }
            }
        }
        (key, Inherit { layer, color, lw, ltype })
    }

    /// Pixels per linetype pattern unit for entity `e`: entity scale x
    /// `$LTSCALE` x (paper or viewport scale, PSLTSCALE). Like AutoCAD, the
    /// scale of the INSERTs above does not change the pattern size, and
    /// `$CELTSCALE` is not used: it only sets group 48 of new entities.
    #[inline]
    pub fn lt_px(&self, e: &Entity, _inh: &Inherit) -> f64 {
        e.props.ltscale as f64 * self.scene.header.ltscale * self.dash_scale
    }

    /// Stroke a stored path.
    #[inline]
    pub fn stroke(&mut self, path: &PathData, m: &Aff, key: StrokeKey) {
        self.out.set_stroke(key);
        self.out.push(path, m);
        self.stats.drawn += 1;
    }

    /// One dot (for things smaller than a pixel), in the current stroke style.
    pub fn dot(&mut self, db: &BBox) {
        let (cx, cy) = (((db.x0 + db.x1) * 0.5) as f32, ((db.y0 + db.y1) * 0.5) as f32);
        self.out.seg(cx - 0.5, cy, cx + 0.5, cy);
        self.stats.tiny += 1;
    }
}
