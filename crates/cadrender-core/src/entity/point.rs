//! POINT: a dot or a small symbol, as set by the header variables `$PDMODE`
//! and `$PDSIZE`. Points on the "Defpoints" layer are left out by the reader
//! (AutoCAD never plots that layer).

use super::{Entity, EntityKind, PrepareCx};
use crate::geom::{BBox, PathData};
use crate::render::walk::{Visit, Walker};
use crate::sink::Sink;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PointEnt {
    pub pos: [f64; 2],
}

/// The symbol of `$PDMODE` around (0, 0) with full size 1, in a y-up frame.
/// Returns `None` for "nothing" (mode 1 without circle or square) and for the
/// plain dot (mode 0), which is drawn as a one-pixel dot.
pub fn symbol(pdmode: i32) -> Option<PathData> {
    let mut p = PathData::default();
    match pdmode & 7 {
        2 => { p.move_to(-0.5, 0.0); p.line_to(0.5, 0.0); p.move_to(0.0, -0.5); p.line_to(0.0, 0.5); }
        3 => { p.move_to(-0.5, -0.5); p.line_to(0.5, 0.5); p.move_to(-0.5, 0.5); p.line_to(0.5, -0.5); }
        4 => { p.move_to(0.0, 0.0); p.line_to(0.0, 0.5); }
        _ => {}
    }
    if pdmode & 32 != 0 { p.arc(0.0, 0.0, [0.5, 0.0], [0.0, 0.5], 0.0, std::f64::consts::TAU, false); p.close(); }
    if pdmode & 64 != 0 { p.move_to(-0.5, -0.5); p.line_to(0.5, -0.5); p.line_to(0.5, 0.5); p.line_to(-0.5, 0.5); p.close(); }
    if p.is_empty() { None } else { Some(p) }
}

impl EntityKind for PointEnt {
    fn prepare(&mut self, _: &PrepareCx) -> BBox { BBox::new(self.pos[0], self.pos[1], self.pos[0], self.pos[1]) }

    fn draw<S: Sink>(&self, e: &Entity, v: &Visit, w: &mut Walker<'_, S>) {
        let (mode, size) = (w.scene.header.pdmode, w.scene.header.pdsize);
        if mode & 7 == 1 && mode & 96 == 0 { return; }
        let (st, _) = w.style(e, v.layer, &v.inh, 0.0);
        let st = st.solid();
        let (x, y) = v.m.apply(self.pos[0], self.pos[1]);
        // > 0: drawing units; 0: 5 % of the view height; < 0: percent of the view height
        let px = if size > 0.0 { size * v.scale } else { w.clip.h() * if size == 0.0 { 0.05 } else { (-size / 100.0).min(1.0) } };
        match symbol(mode) {
            Some(sym) if px >= 2.0 => {
                // y-down device frame: flip the symbol's y
                let m = crate::geom::Aff([px, 0.0, 0.0, -px, x, y]);
                w.stroke(&sym, &m, st);
            }
            _ => {
                w.out.set_stroke(st);
                w.out.seg(x as f32 - 0.5, y as f32, x as f32 + 0.5, y as f32);
                w.stats.drawn += 1;
            }
        }
    }
}
