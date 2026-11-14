//! Built-in stroke fonts, always available, no files needed.
//!
//! They are made from the public Hershey font data by
//! `tools/hershey_to_shx.py` (see `fonts/HERSHEY-NOTICE.txt`). AutoCAD's own
//! romans.shx / simplex.shx are Hershey Roman Simplex too, so the shapes and
//! widths are very close to the real fonts.

use std::sync::OnceLock;

use crate::shx::ShxFont;

static SIMPLEX_BYTES: &[u8] = include_bytes!("../fonts/hershey_simplex.shx");
static DUPLEX_BYTES: &[u8] = include_bytes!("../fonts/hershey_duplex.shx");

/// A built-in fallback font.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuiltinFont {
    /// Hershey Roman Simplex: single stroke. Fallback for romans, simplex,
    /// txt, isocp, monotxt and most other SHX fonts.
    Simplex,
    /// Hershey Roman Duplex: double stroke. Fallback for romand, romanc,
    /// romant, complex, italicc.
    Duplex,
}

impl BuiltinFont {
    /// The parsed font (parsed once per process).
    pub fn font(self) -> &'static ShxFont {
        static SIMPLEX: OnceLock<ShxFont> = OnceLock::new();
        static DUPLEX: OnceLock<ShxFont> = OnceLock::new();
        let (cell, bytes) = match self {
            BuiltinFont::Simplex => (&SIMPLEX, SIMPLEX_BYTES),
            BuiltinFont::Duplex => (&DUPLEX, DUPLEX_BYTES),
        };
        cell.get_or_init(|| ShxFont::from_bytes(bytes.to_vec()).expect("built-in font is valid"))
    }

    /// The compiled SHX bytes (for example to write the font to a file).
    pub fn bytes(self) -> &'static [u8] {
        match self {
            BuiltinFont::Simplex => SIMPLEX_BYTES,
            BuiltinFont::Duplex => DUPLEX_BYTES,
        }
    }

    /// Short name used in logs and reports.
    pub fn name(self) -> &'static str {
        match self {
            BuiltinFont::Simplex => "builtin:hershey-simplex",
            BuiltinFont::Duplex => "builtin:hershey-duplex",
        }
    }
}
