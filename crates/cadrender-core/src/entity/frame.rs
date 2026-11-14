//! WIPEOUT frame: the outline of a wipeout, in the entity's colour and
//! lineweight. The readers add one right after each wipeout when the file
//! says frames are plotted (WIPEOUTFRAME = 1). `RenderOptions::wipeout_frames
//! = false` hides them all at render time, without reading the file again.

use super::{Entity, EntityKind, PrepareCx};
use crate::geom::{BBox, PathData};
use crate::render::walk::{Visit, Walker};
use crate::sink::Sink;

#[derive(Clone, Debug, Default)]
pub struct FrameEnt {
    pub path: PathData,
}

impl EntityKind for FrameEnt {
    fn prepare(&mut self, _: &PrepareCx) -> BBox { self.path.bbox() }

    fn draw<S: Sink>(&self, e: &Entity, v: &Visit, w: &mut Walker<'_, S>) {
        if !w.opts.wipeout_frames { return; }
        let (st, _) = w.style(e, v.layer, &v.inh, 0.0);
        if v.tiny {
            w.out.set_stroke(st);
            w.dot(&v.db);
        } else {
            w.stroke(&self.path, &v.m, st);
        }
    }
}
