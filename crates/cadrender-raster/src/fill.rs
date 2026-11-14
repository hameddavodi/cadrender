//! Filling polygons with exact area coverage (the "cell" method of FreeType
//! and AGG, in floating point).
//!
//! Every edge adds, in each pixel it crosses, its height (`cover`) and the
//! part of the pixel to its right (`area`). Then each row is swept from left
//! to right: the running sum of `cover` is the winding number, and the fill
//! rule turns it into coverage. Pixels between two touched cells form one span
//! with the same coverage, so big fills cost little more than their outline.

use crate::canvas::{Blit, Px};
use crate::path::Poly;
use cadrender_core::sink::FillRule;

#[derive(Clone, Copy)]
struct Cell {
    /// `y << 32 | x` (both made non-negative), the sort key.
    key: u64,
    cover: f32,
    area: f32,
}

/// Reusable buffers for filling.
#[derive(Default)]
pub struct Filler {
    cells: Vec<Cell>,
    w: i32,
    h: i32,
}

impl Filler {
    /// Start a new fill on a `w` x `h` canvas.
    pub fn begin(&mut self, w: i32, h: i32) { self.cells.clear(); self.w = w; self.h = h; }

    /// Add the edges of a polyline (closed back to its first point).
    pub fn add_poly(&mut self, p: &Poly) {
        let n = p.pts.len();
        if n < 2 { return; }
        for i in 0..n {
            let (a, b) = (p.pts[i], p.pts[(i + 1) % n]);
            self.edge(a[0], a[1], b[0], b[1]);
        }
    }

    #[inline]
    fn push(&mut self, x: i32, y: i32, cover: f32, area: f32) {
        // x is clamped to [-1, w]: cells left of the canvas keep their winding
        let x = x.clamp(-1, self.w) + 1;
        self.cells.push(Cell { key: ((y as u64) << 32) | x as u64, cover, area });
    }

    /// One edge. Parts above and below the canvas are dropped; parts left of
    /// it are moved onto its left border (they still count for the winding).
    pub fn edge(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) || y0 == y1 { return; }
        let hf = self.h as f32;
        let (ya, yb) = (y0.min(y1), y0.max(y1));
        if yb <= 0.0 || ya >= hf { return; }
        let dxdy = (x1 - x0) / (y1 - y0);
        // only the canvas rows; x follows the edge from its real start
        let (sx, sy) = (x0, y0);
        let ey = y1.clamp(0.0, hf);
        let wf = self.w as f32;
        let dir = if y1 > y0 { 1.0 } else { -1.0 };
        let (top, bot) = (sy.clamp(0.0, hf).min(ey), sy.clamp(0.0, hf).max(ey));
        let mut y = top;
        let mut row = top.floor() as i32;
        while y < bot {
            let yn = ((row + 1) as f32).min(bot);
            let xa = sx + (y - sy) * dxdy;
            let xb = sx + (yn - sy) * dxdy;
            self.row_piece(row, xa.clamp(-1.0, wf + 1.0), xb.clamp(-1.0, wf + 1.0), (yn - y) * dir);
            y = yn;
            row += 1;
        }
    }

    /// The part of an edge inside pixel row `row`, from `xa` to `xb`, with
    /// height `dy` (signed).
    #[inline]
    fn row_piece(&mut self, row: i32, xa: f32, xb: f32, dy: f32) {
        let (cl, cr) = (xa.min(xb).floor() as i32, xa.max(xb).floor() as i32);
        if cl == cr {
            let xm = 0.5 * (xa + xb) - cl as f32;
            self.push(cl, row, dy, dy * (1.0 - xm));
            return;
        }
        // split at the pixel borders; dy is shared by the length in x
        let (l, r) = (xa.min(xb), xa.max(xb));
        let per = dy / (r - l);
        let mut x = l;
        for c in cl..=cr {
            let xn = ((c + 1) as f32).min(r);
            let d = (xn - x) * per;
            if d != 0.0 {
                let xm = 0.5 * (x + xn) - c as f32;
                self.push(c, row, d, d * (1.0 - xm));
            }
            x = xn;
        }
    }

    /// Sweep the cells and blend the covered pixels. `f(row, x0, x1, cov)`
    /// gets spans (x1 exclusive), cov 0..=255.
    pub fn sweep(&mut self, rule: FillRule, mut span: impl FnMut(i32, i32, i32, u32)) {
        if self.cells.is_empty() { return; }
        self.cells.sort_unstable_by_key(|c| c.key);
        let cov = |v: f32| -> u32 {
            let a = match rule {
                FillRule::NonZero => v.abs().min(1.0),
                FillRule::EvenOdd => { let m = v.abs() % 2.0; if m > 1.0 { 2.0 - m } else { m } }
            };
            (a * 255.0 + 0.5) as u32
        };
        let cells = &self.cells;
        let mut i = 0;
        while i < cells.len() {
            let row = (cells[i].key >> 32) as i32;
            let mut acc = 0.0f32;
            while i < cells.len() && (cells[i].key >> 32) as i32 == row {
                let x = (cells[i].key & 0xffff_ffff) as i32 - 1;
                let (mut cover, mut area) = (0.0f32, 0.0f32);
                while i < cells.len() && (cells[i].key >> 32) as i32 == row && ((cells[i].key & 0xffff_ffff) as i32 - 1) == x {
                    cover += cells[i].cover;
                    area += cells[i].area;
                    i += 1;
                }
                let c = cov(acc + area);
                if c > 0 { span(row, x, x + 1, c); }
                acc += cover;
                // pixels up to the next cell in this row share one coverage
                let next = if i < cells.len() && (cells[i].key >> 32) as i32 == row { (cells[i].key & 0xffff_ffff) as i32 - 1 } else { self.w };
                if next > x + 1 {
                    let c = cov(acc);
                    if c > 0 { span(row, x + 1, next, c); }
                }
            }
        }
    }

    /// Sweep straight into a canvas.
    pub fn fill<P: Px>(&mut self, b: &mut Blit<P>, rule: FillRule) {
        self.sweep(rule, |y, x0, x1, c| if x1 == x0 + 1 { b.px(x0, y, c) } else { b.span(x0, x1, y, c) });
    }
}
