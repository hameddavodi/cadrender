//! Stroked curves: LINE, ARC, CIRCLE, ELLIPSE, (LW)POLYLINE, SPLINE, LEADER.
//! All are one path, stored once in block coordinates (curves as Béziers).

use super::{Entity, EntityKind, PrepareCx};
use crate::geom::{BBox, PathData};
use crate::render::walk::{Visit, Walker};
use crate::sink::Sink;

#[derive(Clone, Debug, Default)]
pub struct StrokeEnt {
    pub path: PathData,
    /// Constant polyline width in drawing units (0 = use the lineweight).
    pub width: f64,
}

impl EntityKind for StrokeEnt {
    fn prepare(&mut self, _: &PrepareCx) -> BBox { self.path.bbox().grow(self.width) }

    fn draw<S: Sink>(&self, e: &Entity, v: &Visit, w: &mut Walker<'_, S>) {
        let (st, ih) = w.style(e, v.layer, &v.inh, self.width * v.scale);
        if v.tiny {
            w.out.set_stroke(st);
            w.dot(&v.db);
        } else if let Some(lt) = crate::linetype::complex(&w.scene.linetypes, ih.ltype) {
            let sc = w.lt_px(e, &v.inh);
            crate::linetype::draw(w, &self.path, &v.m, st, lt, sc);
        } else {
            w.stroke(&self.path, &v.m, st);
        }
    }
}
