//! A small trait so SHX and TrueType fonts look the same to the layout code.

use crate::glyph::Glyph;
use crate::shx::{ShxFont, ShxKind};

/// What the layout needs to know about a font. Implemented by [`ShxFont`]
/// and [`ShxStyleFont`] here; the core implements it for TrueType faces
/// (with `ttf-parser`), so one layout function serves both.
///
/// All values are in font units. Glyph ids are chosen by the font; the
/// layout only passes them back.
pub trait FontFace {
    /// Font units that equal the text height (cap height). SHX: "above".
    /// TrueType: the cap height (AutoCAD scales TrueType text so capitals
    /// are the text height).
    fn cap_height(&self) -> f32;

    /// Descender depth below the baseline, as a positive number.
    fn descent(&self) -> f32;

    /// Glyph id of a character, or `None` if the font does not have it.
    fn glyph_id(&self, ch: char) -> Option<u32>;

    /// Glyph id of a raw character code (`%%nnn` or `\M+` in the text).
    /// The default treats the code as a Unicode code point.
    fn glyph_id_for_code(&self, code: u32) -> Option<u32> {
        char::from_u32(code).and_then(|c| self.glyph_id(c))
    }

    /// Horizontal advance of a glyph, in this face's units (after
    /// [`FontFace::glyph_scale`]).
    fn advance(&self, id: u32) -> f32;

    /// Scale to apply to the glyph's own outline so it matches this face's
    /// units. 1.0 except for big font glyphs mixed into a normal font.
    fn glyph_scale(&self, _id: u32) -> f32 {
        1.0
    }
}

/// Fonts whose glyphs are polylines (SHX). Used by
/// [`crate::TextLayout::stroke_polylines`].
pub trait StrokeGlyphs: FontFace {
    /// The cached polyline glyph of an id from [`FontFace::glyph_id`].
    fn stroke_glyph(&self, id: u32) -> &Glyph;
}

impl FontFace for ShxFont {
    #[inline]
    fn cap_height(&self) -> f32 {
        self.above()
    }
    #[inline]
    fn descent(&self) -> f32 {
        self.below()
    }
    #[inline]
    fn glyph_id(&self, ch: char) -> Option<u32> {
        if self.kind() == ShxKind::BigFont && (ch as u32) >= 0x80 {
            // Big fonts are numbered in a double-byte code page, not Unicode.
            return None;
        }
        self.slot_for_char(ch)
    }
    #[inline]
    fn glyph_id_for_code(&self, code: u32) -> Option<u32> {
        self.slot_for_code(code)
    }
    #[inline]
    fn advance(&self, id: u32) -> f32 {
        self.glyph(id).advance()
    }
}

impl StrokeGlyphs for ShxFont {
    #[inline]
    fn stroke_glyph(&self, id: u32) -> &Glyph {
        self.glyph(id)
    }
}

/// Bit that marks a big font glyph id in [`ShxStyleFont`].
const BIG: u32 = 1 << 31;

/// A text style with a primary SHX font and an optional big font
/// (STYLE group codes 3 and 4). Cheap to build; create one per style.
///
/// Characters come from the primary font. Raw codes (`\M+nXXXX`, or any
/// code whose high byte is an escape byte of the big font) come from the
/// big font, scaled to the primary font's cap height.
#[derive(Clone, Copy, Debug)]
pub struct ShxStyleFont<'a> {
    pub primary: &'a ShxFont,
    pub big: Option<&'a ShxFont>,
}

impl<'a> ShxStyleFont<'a> {
    pub fn new(primary: &'a ShxFont, big: Option<&'a ShxFont>) -> Self {
        ShxStyleFont { primary, big }
    }

    fn big_scale(&self) -> f32 {
        match self.big {
            Some(b) => self.primary.above() / b.above(),
            None => 1.0,
        }
    }

    fn is_escape(big: &ShxFont, code: u32) -> bool {
        let hi = (code >> 8) as u16;
        code > 0xFF && big.escape_ranges().iter().any(|&(a, b)| hi >= a && hi <= b)
    }
}

impl FontFace for ShxStyleFont<'_> {
    fn cap_height(&self) -> f32 {
        self.primary.above()
    }
    fn descent(&self) -> f32 {
        self.primary.below()
    }
    fn glyph_id(&self, ch: char) -> Option<u32> {
        self.primary.glyph_id(ch)
    }
    fn glyph_id_for_code(&self, code: u32) -> Option<u32> {
        if let Some(big) = self.big {
            if Self::is_escape(big, code) || self.primary.slot_for_code(code).is_none() {
                if let Some(s) = big.slot_for_code(code) {
                    return Some(s | BIG);
                }
            }
        }
        self.primary.slot_for_code(code)
    }
    fn advance(&self, id: u32) -> f32 {
        if id & BIG != 0 {
            if let Some(big) = self.big {
                return big.glyph(id & !BIG).advance() * self.big_scale();
            }
        }
        self.primary.glyph(id & !BIG).advance()
    }
    fn glyph_scale(&self, id: u32) -> f32 {
        if id & BIG != 0 {
            self.big_scale()
        } else {
            1.0
        }
    }
}

impl StrokeGlyphs for ShxStyleFont<'_> {
    fn stroke_glyph(&self, id: u32) -> &Glyph {
        if id & BIG != 0 {
            if let Some(big) = self.big {
                return big.glyph(id & !BIG);
            }
        }
        self.primary.glyph(id & !BIG)
    }
}
