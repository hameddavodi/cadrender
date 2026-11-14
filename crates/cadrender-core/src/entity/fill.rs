//! Filled areas: SOLID, TRACE, and masks (WIPEOUT) in the page colour.

use super::{Entity, EntityKind, PrepareCx};
use crate::geom::{BBox, PathData};
use crate::render::walk::{Visit, Walker};
use crate::sink::Sink;

#[derive(Clone, Debug, Default)]
pub struct FillEnt {
    pub path: PathData,
    /// A mask: filled with the page background colour, not the entity colour.
    pub background: bool,
}

impl EntityKind for FillEnt {
    fn prepare(&mut self, _: &PrepareCx) -> BBox { self.path.bbox() }

    fn draw<S: Sink>(&self, e: &Entity, v: &Visit, w: &mut Walker<'_, S>) {
        if v.tiny { return; }
        let color = if self.background { w.opts.background } else {
            let (st, _) = w.style(e, v.layer, &v.inh, 0.0);
            st.color
        };
        w.out.fill_now(&self.path, &v.m, color);
        w.stats.drawn += 1;
    }
}
