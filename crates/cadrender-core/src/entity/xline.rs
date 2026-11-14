//! RAY and XLINE: lines without end (RAY: one end). Cut to the visible
//! rectangle when drawn. They do not count for the sheet frame (like
//! AutoCAD's zoom extents), when clean rules are on.

use super::{Entity, EntityKind, PrepareCx};
use crate::geom::{Aff, BBox, PathData};
use crate::render::walk::{Visit, Walker};
use crate::scene::tables::{BlockId, LayerId};
use crate::sink::Sink;

/// Stands for "infinite" in bounding boxes (drawing units).
const FAR: f64 = 1e10;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct XLineEnt {
    pub base: [f64; 2],
    /// Direction (not zero).
    pub dir: [f64; 2],
    /// RAY: starts at `base`. XLINE: goes both ways.
    pub ray: bool,
}

/// Parameter range `[t0, t1]` of `p + t d` inside box `c` (Liang-Barsky).
pub fn clip_line(p: [f64; 2], d: [f64; 2], mut t0: f64, mut t1: f64, c: &BBox) -> Option<(f64, f64)> {
    for (pi, di, lo, hi) in [(p[0], d[0], c.x0, c.x1), (p[1], d[1], c.y0, c.y1)] {
        if di.abs() < 1e-300 {
            if pi < lo || pi > hi { return None; }
        } else {
            let (a, b) = ((lo - pi) / di, (hi - pi) / di);
            let (a, b) = if a < b { (a, b) } else { (b, a) };
            t0 = t0.max(a);
            t1 = t1.min(b);
        }
    }
    if t0 < t1 { Some((t0, t1)) } else { None }
}

impl EntityKind for XLineEnt {
    fn prepare(&mut self, _: &PrepareCx) -> BBox {
        let l = self.dir[0].hypot(self.dir[1]);
        if !(l > 0.0 && l.is_finite()) { return BBox::EMPTY; }
        let d = [self.dir[0] / l, self.dir[1] / l];
        let mut b = BBox::EMPTY;
        b.add(self.base[0] + d[0] * FAR, self.base[1] + d[1] * FAR);
        if self.ray { b.add(self.base[0], self.base[1]); } else { b.add(self.base[0] - d[0] * FAR, self.base[1] - d[1] * FAR); }
        b
    }

    fn draw<S: Sink>(&self, e: &Entity, v: &Visit, w: &mut Walker<'_, S>) {
        let (st, _) = w.style(e, v.layer, &v.inh, 0.0);
        let (px, py) = v.m.apply(self.base[0], self.base[1]);
        let [a, b, c, d, _, _] = v.m.0;
        let dd = [a * self.dir[0] + c * self.dir[1], b * self.dir[0] + d * self.dir[1]];
        let t0 = if self.ray { 0.0 } else { f64::NEG_INFINITY };
        let Some((t0, t1)) = clip_line([px, py], dd, t0, f64::INFINITY, &w.clip.grow(2.0)) else { return };
        let mut p = PathData::default();
        p.move_to(px + dd[0] * t0, py + dd[1] * t0);
        p.line_to(px + dd[0] * t1, py + dd[1] * t1);
        w.stroke(&p, &Aff::ID, st);
    }

    fn clean_bbox(&self, _: &Entity, _: &dyn Fn(LayerId) -> bool, _: &dyn Fn(BlockId) -> Option<BBox>) -> Option<BBox> { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clip_line_cases() {
        let c = BBox::new(0.0, 0.0, 10.0, 10.0);
        assert_eq!(clip_line([5.0, 5.0], [1.0, 0.0], f64::NEG_INFINITY, f64::INFINITY, &c), Some((-5.0, 5.0)));
        assert_eq!(clip_line([5.0, 5.0], [1.0, 0.0], 0.0, f64::INFINITY, &c), Some((0.0, 5.0)));
        assert_eq!(clip_line([5.0, 20.0], [1.0, 0.0], f64::NEG_INFINITY, f64::INFINITY, &c), None);
        assert_eq!(clip_line([20.0, 5.0], [1.0, 0.0], 0.0, f64::INFINITY, &c), None);
    }
}
