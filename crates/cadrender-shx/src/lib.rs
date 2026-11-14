//! # cadrender-shx
//!
//! Fonts for CAD text, in pure Rust with no dependencies:
//!
//! - [`ShxFont`]: reader for compiled AutoCAD shape files (`.shx`): normal
//!   shape fonts, shape files for linetypes, Unicode fonts (unifont) and
//!   Asian big fonts. Every shape code is supported. Glyphs become
//!   polylines in font units, parsed once on first use and cached
//!   (thread-safe, lock-free after the first use).
//! - [`FontResolver`]: maps a text style's font name (`romans.shx`,
//!   `arial.ttf`, "Swis721 Lt BT", ...) to a file on disk, a similar free font,
//!   or a built-in stroke font ([`BuiltinFont`]).
//! - [`layout_text`]: single-line text layout with height, width factor,
//!   oblique angle, all TEXT alignments, `%%c %%d %%p %%u %%o %%k %%nnn`
//!   and `\U+XXXX`. Works for any font that implements [`FontFace`], so the
//!   core can use it for TrueType too.
//!
//! ```
//! use cadrender_shx::{layout_text, BuiltinFont, Polylines, TextLayout, TextParams};
//!
//! let font = BuiltinFont::Simplex.font();
//! let mut layout = TextLayout::new();
//! layout_text(font, "%%c 50", &TextParams { height: 2.5, ..Default::default() }, &mut layout);
//! let mut lines = Polylines::default();
//! layout.stroke_polylines(font, &mut lines);
//! assert!(lines.len() > 3);
//! // text space: the cap height is 2.5; these 4 characters are about 9 wide
//! assert!(layout.width() > 5.0 && layout.width() < 12.0);
//! ```

mod builtin;
pub mod codepage;
mod face;
mod glyph;
mod interp;
mod layout;
mod resolve;
mod shx;

pub use builtin::BuiltinFont;
pub use face::{FontFace, ShxStyleFont, StrokeGlyphs};
pub use glyph::Glyph;
pub use layout::{
    layout_text, tokens, Affine, Decoration, DecorationKind, HAlign, PlacedGlyph, Polylines,
    TextLayout, TextParams, TextToken, Tokens, VAlign,
};
pub use resolve::{
    default_font_dirs, normalize_file, FallbackRow, FontResolver, FontSource, MatchQuality,
    Resolved, DEFAULT_TTF, FALLBACKS,
};
pub use shx::{FontInfo, ShxError, ShxFont, ShxKind};
