//! A grid of boxes over the top level of model space, so that a viewport
//! only looks at the entities near its window instead of all of them.
//!
//! Big model spaces have 100 000+ top-level entities and a viewport often
//! shows a few thousand. Without the grid the walk tests every box, for every
//! viewport of every sheet.

use crate::geom::BBox;

/// Below this many entities the plain loop is fast enough.
pub const MIN_ENTITIES: usize = 2000;
/// An entity spanning more cells than this goes in the `big` list instead.
const MAX_CELLS: usize = 64;

pub struct GridIndex {
    b: BBox,
    nx: usize,
    ny: usize,
    /// Cells per unit in x and y.
    sx: f64,
    sy: f64,
    /// Cell `c` holds `items[starts[c]..starts[c + 1]]` (entity indices, ascending).
    starts: Vec<u32>,
    items: Vec<u32>,
    /// Entities with a very big or not finite box: always returned.
    big: Vec<u32>,
}

impl GridIndex {
    /// Index of the boxes, or `None` if there are too few to need one.
    /// Empty boxes are left out: the walk never draws them.
    pub fn build(boxes: &[BBox]) -> Option<GridIndex> {
        if boxes.len() < MIN_ENTITIES { return None; }
        let finite = |b: &BBox| !b.is_empty() && b.x0.is_finite() && b.y0.is_finite() && b.x1.is_finite() && b.y1.is_finite();
        let mut all = BBox::EMPTY;
        for b in boxes.iter().filter(|b| finite(b)) { all.union(b); }
        if all.is_empty() { return None; }
        // about 4 entities per cell, cells roughly square
        let cells = (boxes.len() / 4).clamp(1, 1 << 20) as f64;
        let (w, h) = (all.w().max(1e-9), all.h().max(1e-9));
        let nx = ((cells * w / h).sqrt().round() as usize).clamp(1, 4096);
        let ny = ((cells / nx as f64).round() as usize).clamp(1, 4096);
        let mut g = GridIndex { b: all, nx, ny, sx: nx as f64 / w, sy: ny as f64 / h, starts: vec![0; nx * ny + 1], items: vec![], big: vec![] };
        // two passes: count, then fill (counting sort keeps indices ascending)
        let mut spans = Vec::with_capacity(boxes.len());
        for (i, b) in boxes.iter().enumerate() {
            if b.is_empty() { spans.push(None); continue; }
            if !finite(b) { g.big.push(i as u32); spans.push(None); continue; }
            let r = g.cells(b);
            if (r.2 - r.0 + 1) * (r.3 - r.1 + 1) > MAX_CELLS { g.big.push(i as u32); spans.push(None); continue; }
            for y in r.1..=r.3 { for x in r.0..=r.2 { g.starts[y * nx + x + 1] += 1; } }
            spans.push(Some(r));
        }
        for c in 0..nx * ny { g.starts[c + 1] += g.starts[c]; }
        let mut fill = g.starts.clone();
        g.items = vec![0; g.starts[nx * ny] as usize];
        for (i, r) in spans.iter().enumerate() {
            let Some(r) = r else { continue };
            for y in r.1..=r.3 { for x in r.0..=r.2 { let c = y * nx + x; g.items[fill[c] as usize] = i as u32; fill[c] += 1; } }
        }
        Some(g)
    }

    /// Cell range `(x0, y0, x1, y1)` (inclusive) of a box, clamped to the grid.
    fn cells(&self, b: &BBox) -> (usize, usize, usize, usize) {
        let cx = |x: f64| (((x - self.b.x0) * self.sx).floor().max(0.0) as usize).min(self.nx - 1);
        let cy = |y: f64| (((y - self.b.y0) * self.sy).floor().max(0.0) as usize).min(self.ny - 1);
        (cx(b.x0), cy(b.y0), cx(b.x1), cy(b.y1))
    }

    /// Indices of the entities whose box may touch `r`, ascending (so in
    /// draw order), each once.
    pub fn query(&self, r: &BBox, out: &mut Vec<u32>) {
        out.clear();
        out.extend_from_slice(&self.big);
        let hit = !(r.is_empty() || r.x1 < self.b.x0 || r.x0 > self.b.x1 || r.y1 < self.b.y0 || r.y0 > self.b.y1);
        if hit {
            let c = self.cells(r);
            for y in c.1..=c.3 {
                let row = y * self.nx;
                out.extend_from_slice(&self.items[self.starts[row + c.0] as usize..self.starts[row + c.2 + 1] as usize]);
            }
        }
        out.sort_unstable();
        out.dedup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_matches_brute_force() {
        let mut boxes = vec![];
        let mut s = 12345u64;
        let mut rnd = || { s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); (s >> 33) as f64 / (1u64 << 31) as f64 };
        for _ in 0..5000 {
            let (x, y, w, h) = (rnd() * 1000.0, rnd() * 500.0, rnd() * rnd() * 300.0, rnd() * 20.0);
            boxes.push(BBox::new(x, y, x + w, y + h));
        }
        boxes.push(BBox::EMPTY);
        boxes.push(BBox::new(f64::NEG_INFINITY, 0.0, 1.0, 1.0));
        let g = GridIndex::build(&boxes).unwrap();
        let mut out = vec![];
        for q in [BBox::new(100.0, 100.0, 140.0, 130.0), BBox::new(-50.0, -50.0, 2000.0, 900.0), BBox::new(990.0, 0.0, 995.0, 1.0), BBox::new(5000.0, 0.0, 6000.0, 1.0)] {
            g.query(&q, &mut out);
            let want: Vec<u32> = boxes.iter().enumerate().filter(|(_, b)| !b.intersect(&q).is_empty()).map(|(i, _)| i as u32).collect();
            for w in &want { assert!(out.binary_search(w).is_ok(), "missing {w}"); }
            assert!(out.windows(2).all(|p| p[0] < p[1]), "ascending, no repeats");
            assert!(!out.contains(&5000), "empty box left out");
        }
        assert!(GridIndex::build(&boxes[..10]).is_none());
    }
}
