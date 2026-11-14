//! Simple text layout: lines, word wrap, glyph positions in font units.

use super::Face;

/// One placed glyph: id in its face, position in font units
/// (first baseline at y = 0, next lines below).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glyph { pub id: u32, pub x: f32, pub y: f32 }

/// Result of laying out a text.
#[derive(Clone, Debug, Default)]
pub struct Laid { pub glyphs: Vec<Glyph>, pub width: f64, pub lines: usize }

/// Lay out lines. `wrap` is the column width in font units (0 = no wrap).
/// `line_factor` is the line spacing as a multiple of the cap height.
pub fn layout(face: &mut Face, lines: &[String], wrap: f64, line_factor: f64) -> Laid {
    let mut out = vec![];
    let mut maxw = 0f64;
    let mut row = 0usize;
    let lh = face.cap_height * line_factor;
    let space = face.space;
    for line in lines {
        let mut x = 0f64;
        for (wi, word) in line.split(' ').enumerate() {
            let mut wg: Vec<(u32, f64)> = vec![];
            let mut ww = 0f64;
            for ch in word.chars() { if let Some((g, a)) = face.glyph(ch) { wg.push((g, ww)); ww += a; } }
            if wi > 0 { x += space; }
            if wrap > 0.0 && x > 0.0 && x + ww > wrap { maxw = maxw.max(x - space); row += 1; x = 0.0; }
            for (g, gx) in wg { out.push(Glyph { id: g, x: (x + gx) as f32, y: -((row as f64) * lh) as f32 }); }
            x += ww;
        }
        maxw = maxw.max(x);
        row += 1;
    }
    Laid { glyphs: out, width: maxw, lines: row }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::PathData;
    use crate::text::GlyphSource;

    /// Every glyph is a 600 x 700 box, space 300.
    struct Boxes;
    impl GlyphSource for Boxes {
        fn cap_height(&self) -> f64 { 700.0 }
        fn space_advance(&self) -> f64 { 300.0 }
        fn glyph(&self, ch: char) -> Option<(PathData, f64)> {
            if ch == '~' { return None; }
            let mut p = PathData::default();
            p.move_to(0.0, 0.0); p.line_to(600.0, 0.0); p.line_to(600.0, 700.0); p.close();
            Some((p, 600.0))
        }
    }

    #[test]
    fn wraps_words() {
        let mut f = Face::new("boxes", Box::new(Boxes));
        let l = layout(&mut f, &["ab cd".to_string()], 0.0, 5.0 / 3.0);
        assert_eq!((l.glyphs.len(), l.lines), (4, 1));
        assert_eq!(l.width, 2700.0);
        let l = layout(&mut f, &["ab cd".to_string()], 1500.0, 5.0 / 3.0);
        assert_eq!(l.lines, 2);
        assert!(l.glyphs[2].y < 0.0 && l.glyphs[2].x == 0.0);
        // glyphs are stored once per character
        assert_eq!(f.glyphs.len(), 4);
        layout(&mut f, &["dcba".to_string()], 0.0, 1.0);
        assert_eq!(f.glyphs.len(), 4);
    }

    #[test]
    fn missing_char_uses_question_mark() {
        let mut f = Face::new("boxes", Box::new(Boxes));
        assert!(f.glyph('~').is_some());
    }
}
