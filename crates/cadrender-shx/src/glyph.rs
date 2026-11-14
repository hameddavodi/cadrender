//! One parsed glyph: polylines in font units plus the pen end point.

/// A parsed SHX glyph (one shape).
///
/// All values are in font units. y goes up, the glyph origin is (0, 0) on the
/// baseline. The points of all polylines are stored in one flat array, so a
/// glyph is two allocations, made once when the glyph is first used.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Glyph {
    points: Box<[[f32; 2]]>,
    /// End index (exclusive) of each polyline in `points`.
    ends: Box<[u32]>,
    /// Pen position when the shape ended. `end[0]` is the advance width.
    end: [f32; 2],
    /// Ink box `[min_x, min_y, max_x, max_y]`; all zero for an empty glyph.
    bbox: [f32; 4],
}

impl Glyph {
    /// Horizontal advance: the x of the pen when the shape ended.
    #[inline]
    pub fn advance(&self) -> f32 {
        self.end[0]
    }

    /// Pen position when the shape ended (x, y).
    #[inline]
    pub fn end_point(&self) -> [f32; 2] {
        self.end
    }

    /// Ink box `[min_x, min_y, max_x, max_y]` (all zero when there is no ink).
    #[inline]
    pub fn bbox(&self) -> [f32; 4] {
        self.bbox
    }

    /// True when the glyph draws nothing (for example a space).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }

    /// Number of polylines.
    #[inline]
    pub fn polyline_count(&self) -> usize {
        self.ends.len()
    }

    /// All points of all polylines, one after the other.
    #[inline]
    pub fn points(&self) -> &[[f32; 2]] {
        &self.points
    }

    /// End index (exclusive) of each polyline in [`Glyph::points`].
    #[inline]
    pub fn ends(&self) -> &[u32] {
        &self.ends
    }

    /// Iterate over the polylines. Every polyline has at least 2 points.
    pub fn polylines(&self) -> impl Iterator<Item = &[[f32; 2]]> + '_ {
        let mut start = 0usize;
        self.ends.iter().map(move |&e| {
            let s = start;
            start = e as usize;
            &self.points[s..e as usize]
        })
    }

    pub(crate) fn with_advance(mut self, x: f32) -> Glyph {
        self.end = [x, 0.0];
        self
    }
}

/// Collects pen moves into polylines while a shape is interpreted.
#[derive(Debug, Default)]
pub(crate) struct GlyphBuilder {
    points: Vec<[f32; 2]>,
    ends: Vec<u32>,
    /// Start index of the open polyline, if one is open.
    open: Option<usize>,
}

impl GlyphBuilder {
    /// Draw a line from `a` to `b` (pen down).
    pub(crate) fn line(&mut self, a: [f64; 2], b: [f64; 2]) {
        let a32 = [a[0] as f32, a[1] as f32];
        let b32 = [b[0] as f32, b[1] as f32];
        match self.open {
            Some(_) if self.points.last() == Some(&a32) => {}
            _ => {
                self.close();
                self.open = Some(self.points.len());
                self.points.push(a32);
            }
        }
        if self.points.last() != Some(&b32) {
            self.points.push(b32);
        }
    }

    /// Lift the pen: finish the open polyline.
    pub(crate) fn close(&mut self) {
        if let Some(start) = self.open.take() {
            if self.points.len() - start >= 2 {
                self.ends.push(self.points.len() as u32);
            } else {
                self.points.truncate(start);
            }
        }
    }

    pub(crate) fn finish(mut self, end: [f64; 2]) -> Glyph {
        self.close();
        let mut bbox = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];
        for p in &self.points {
            bbox[0] = bbox[0].min(p[0]);
            bbox[1] = bbox[1].min(p[1]);
            bbox[2] = bbox[2].max(p[0]);
            bbox[3] = bbox[3].max(p[1]);
        }
        if self.points.is_empty() {
            bbox = [0.0; 4];
        }
        Glyph {
            points: self.points.into_boxed_slice(),
            ends: self.ends.into_boxed_slice(),
            end: [end[0] as f32, end[1] as f32],
            bbox,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_joins_connected_lines_and_splits_others() {
        let mut b = GlyphBuilder::default();
        b.line([0.0, 0.0], [1.0, 0.0]);
        b.line([1.0, 0.0], [1.0, 1.0]);
        b.close();
        b.line([5.0, 5.0], [6.0, 5.0]);
        let g = b.finish([7.0, 0.0]);
        let lines: Vec<_> = g.polylines().collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], &[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]);
        assert_eq!(lines[1], &[[5.0, 5.0], [6.0, 5.0]]);
        assert_eq!(g.advance(), 7.0);
        assert_eq!(g.bbox(), [0.0, 0.0, 6.0, 5.0]);
    }

    #[test]
    fn zero_length_line_makes_no_polyline() {
        let mut b = GlyphBuilder::default();
        b.line([1.0, 1.0], [1.0, 1.0]);
        let g = b.finish([0.0, 0.0]);
        assert!(g.is_empty());
        assert_eq!(g.bbox(), [0.0; 4]);
    }
}
