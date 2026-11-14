//! A dashed polyline without "linetype generation" (polyline flag 128 off,
//! the AutoCAD default): the dash pattern starts again at every vertex, and a
//! segment shorter than one pattern is drawn solid. (That is why a dashed
//! spline-like polyline with many short segments looks solid in AutoCAD.)
//!
//! The path is stored with one sub-path per segment. AutoCAD starts and ends
//! every segment with a dash; this is done here (the dashes are made in device
//! pixels, the sink only gets solid strokes).

use super::{Entity, EntityKind, PrepareCx};
use crate::geom::{self, Aff, BBox, PathData};
use crate::render::batch::NO_DASH;
use crate::render::walk::{Visit, Walker};
use crate::sink::Sink;

#[derive(Clone, Debug, Default)]
pub struct PlineEnt {
    /// One sub-path per polyline segment, block coordinates.
    pub segs: PathData,
    /// Constant polyline width in drawing units (0 = use the lineweight).
    pub width: f64,
}

/// Length of each sub-path in device pixels (control polygon for curves) and
/// its verb / point ranges.
fn split(p: &PathData, m: &Aff, mut f: impl FnMut(f64, std::ops::Range<usize>, std::ops::Range<usize>)) {
    let (mut v0, mut p0, mut len) = (0usize, 0usize, 0.0f64);
    let mut last = (0.0, 0.0);
    let mut pi = 0;
    for (vi, &v) in p.verbs.iter().enumerate() {
        let n = match v { geom::M | geom::L => 1, geom::Q => 2, geom::C => 3, _ => 0 };
        if v == geom::M && vi > v0 { f(len, v0..vi, p0..pi); v0 = vi; p0 = pi; len = 0.0; }
        for k in 0..n {
            let (x, y) = m.apply(p.pts[pi + k][0], p.pts[pi + k][1]);
            if !(v == geom::M && k == 0) { len += (x - last.0).hypot(y - last.1); }
            last = (x, y);
        }
        pi += n;
    }
    if p.verbs.len() > v0 { f(len, v0..p.verbs.len(), p0..pi); }
}

fn copy(dst: &mut PathData, src: &PathData, v: std::ops::Range<usize>, p: std::ops::Range<usize>) {
    dst.verbs.extend_from_slice(&src.verbs[v]);
    dst.pts.extend_from_slice(&src.pts[p]);
}

impl EntityKind for PlineEnt {
    fn prepare(&mut self, _: &PrepareCx) -> BBox { self.segs.bbox().grow(self.width) }

    fn draw<S: Sink>(&self, e: &Entity, v: &Visit, w: &mut Walker<'_, S>) {
        let (st, ih) = w.style(e, v.layer, &v.inh, self.width * v.scale);
        if v.tiny {
            w.out.set_stroke(st);
            w.dot(&v.db);
            return;
        }
        if let Some(lt) = crate::linetype::complex(&w.scene.linetypes, ih.ltype) {
            let sc = w.lt_px(e, &v.inh);
            crate::linetype::draw(w, &self.segs, &v.m, st, lt, sc);
            return;
        }
        let pattern: Vec<f64> = match w.scene.linetypes.get(ih.ltype as usize) {
            Some(lt) if st.dash != NO_DASH => { let sc = w.lt_px(e, &v.inh); lt.pattern.iter().map(|d| d * sc).collect() }
            _ => vec![],
        };
        let period: f64 = pattern.iter().map(|d| d.abs()).sum();
        if period.is_nan() || period <= 0.0 { w.stroke(&self.segs, &v.m, st); return; }
        // AutoCAD: each segment starts and ends with a dash. The pattern is
        // stretched a little so that n patterns plus the first dash fill it.
        let first = pattern[0].max(0.0);
        let (mut short, mut dashes, mut tmp, mut polys) = (PathData::default(), PathData::default(), PathData::default(), vec![]);
        split(&self.segs, &v.m, |l, vr, pr| {
            if l < period { copy(&mut short, &self.segs, vr, pr); return; }
            tmp.verbs.clear(); tmp.pts.clear();
            copy(&mut tmp, &self.segs, vr, pr);
            tmp.transform(&v.m);
            polys.clear();
            geom::flatten(&tmp, 0.3, &mut polys);
            let len: f64 = polys.iter().map(|p| crate::linetype::len(p)).sum();
            let n = ((len - first) / period).round().max(1.0);
            let k = if first > 0.0 { len / (n * period + first) } else { 1.0 };
            let els: Vec<f64> = pattern.iter().map(|d| d * k).collect();
            for p in &polys { crate::linetype::walk_pattern(p, &els, &mut dashes, &mut |_, _, _, _| {}); }
        });
        if !dashes.verbs.is_empty() { w.out.set_stroke(st.solid()); w.out.push(&dashes, &Aff::ID); w.stats.drawn += 1; }
        if !short.verbs.is_empty() { w.stroke(&short, &v.m, st.solid()); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn split_measures_sub_paths() {
        let mut p = PathData::default();
        p.move_to(0.0, 0.0); p.line_to(10.0, 0.0);
        p.move_to(10.0, 0.0); p.line_to(10.0, 2.0);
        let mut got = vec![];
        split(&p, &Aff::scale(2.0, 2.0), |l, v, q| got.push((l, v, q)));
        assert_eq!(got, vec![(20.0, 0..2, 0..2), (4.0, 2..4, 2..4)]);
    }
}
