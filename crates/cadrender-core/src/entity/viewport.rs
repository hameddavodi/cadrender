//! VIEWPORT: a window on a paper-space sheet that shows part of model space.
//! It draws nothing itself; the sheet renderer draws model space through it.

use super::{Entity, EntityKind, PrepareCx};
use crate::geom::{Aff, BBox};
use crate::render::walk::{Visit, Walker};
use crate::scene::tables::{Handle, LayerId};
use crate::sink::Sink;

#[derive(Clone, Debug, Default)]
pub struct ViewportEnt {
    pub handle: Handle,
    /// Centre on paper and full size (paper units).
    pub center: [f64; 2],
    pub w: f64,
    pub h: f64,
    /// View centre in display coordinates (DCS).
    pub view_center: [f64; 2],
    /// View target point (model).
    pub target: [f64; 2],
    /// Height of the view in model units.
    pub view_h: f64,
    /// Twist angle in degrees.
    pub twist: f64,
    /// Group 68: 0 = off, > 0 = on (1 = the paper-space viewport). LibreDWG writes 0 for all.
    pub status: i32,
    /// Group 69.
    pub id: i32,
    /// Group 90 status flags.
    pub flags: i32,
    /// Handles of layers frozen in this viewport (as read).
    pub frozen_handles: Vec<Handle>,
    /// The same, resolved to layer ids when the scene is built.
    pub frozen_layers: Vec<LayerId>,
    /// View direction is +Z (plan view).
    pub top_view: bool,
    /// View direction (from the target towards the viewer). (0, 0, 0) = plan view.
    pub view_dir: [f64; 3],
    /// Z of the view target point.
    pub target_z: f64,
    /// Handle of the clip boundary entity (non-rectangular viewport), 0 = none.
    pub clip_handle: Handle,
    /// Clip polygon in paper coordinates, resolved from `clip_handle` when the scene is built.
    pub clip: Option<Vec<[f64; 2]>>,
}

impl ViewportEnt {
    /// The two axes of the display coordinate system (DCS) in world space: the
    /// arbitrary axis rule of AutoCAD applied to the view direction.
    pub fn dcs_axes(&self) -> ([f64; 3], [f64; 3]) {
        let d = self.view_dir;
        let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let n = if l > 1e-300 && l.is_finite() { [d[0] / l, d[1] / l, d[2] / l] } else { [0.0, 0.0, 1.0] };
        let cross = |a: [f64; 3], b: [f64; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
        let ax = if n[0].abs() < 1.0 / 64.0 && n[1].abs() < 1.0 / 64.0 { cross([0.0, 1.0, 0.0], n) } else { cross([0.0, 0.0, 1.0], n) };
        let al = (ax[0] * ax[0] + ax[1] * ax[1] + ax[2] * ax[2]).sqrt().max(1e-300);
        let ax = [ax[0] / al, ax[1] / al, ax[2] / al];
        (ax, cross(n, ax))
    }

    /// Plan view (looking down +Z, X to the right): model and paper axes are parallel up to twist.
    pub fn is_plan(&self) -> bool {
        let (ax, ay) = self.dcs_axes();
        (ax[0] - 1.0).abs() < 1e-12 && (ay[1] - 1.0).abs() < 1e-12
    }

    /// Model coordinates -> paper coordinates. Non-plan views are a parallel
    /// projection of the XY plane (the scene is 2D: z is not kept).
    pub fn matrix(&self) -> Aff {
        let s = self.h / self.view_h;
        let off = [self.center[0] - self.view_center[0] * s, self.center[1] - self.view_center[1] * s];
        let (ax, ay) = self.dcs_axes();
        let t = [self.target[0], self.target[1], self.target_z];
        let dot = |a: [f64; 3]| a[0] * t[0] + a[1] * t[1] + a[2] * t[2];
        let proj = Aff([ax[0], ay[0], ax[1], ay[1], -dot(ax), -dot(ay)]);
        Aff::translate(off[0], off[1]).mul(&Aff::rotate(self.twist.to_radians())).mul(&Aff::scale(s, s)).mul(&proj)
    }
    /// The viewport rectangle on paper.
    pub fn rect(&self) -> BBox { BBox::centered(self.center, self.w, self.h) }
    /// The part of the paper the viewport can show: the rectangle, cut to the
    /// clip polygon's box if there is one.
    pub fn visible_rect(&self) -> BBox {
        let r = self.rect();
        match &self.clip {
            Some(c) => { let mut b = BBox::EMPTY; for p in c { b.add(p[0], p[1]); } r.intersect(&b) }
            None => r,
        }
    }
    /// Paper units per model unit.
    pub fn scale(&self) -> f64 { if self.view_h > 0.0 { self.h / self.view_h } else { 0.0 } }
    /// Can it be drawn? Views looking along the XY plane (edge-on) show
    /// nothing of a 2D drawing and are not drawn.
    pub fn drawable(&self) -> bool {
        let ok = self.w > 0.0 && self.h > 0.0 && self.view_h > 0.0 && self.view_h.is_finite();
        ok && { let m = self.matrix(); let d = m.det(); d.is_finite() && d.abs() > 1e-9 * self.scale() * self.scale() && m.0.iter().all(|x| x.is_finite()) }
    }
}

impl EntityKind for ViewportEnt {
    fn prepare(&mut self, _: &PrepareCx) -> BBox { self.rect() }
    fn draw<S: Sink>(&self, _: &Entity, _: &Visit, _: &mut Walker<'_, S>) {}
    fn clean_bbox(&self, _: &Entity, _: &dyn Fn(LayerId) -> bool, _: &dyn Fn(u32) -> Option<BBox>) -> Option<BBox> { None }
}
