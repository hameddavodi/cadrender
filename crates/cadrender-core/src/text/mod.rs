//! Fonts and text layout.
//!
//! Text is laid out once, when the scene is built ("parse once, render
//! many"): a text entity stores glyph ids and positions. Glyph outlines are
//! stored once per font face and shared by every text that uses them.
//!
//! A font face gets its glyphs from a [`GlyphSource`]. Today there is one:
//! TrueType/OpenType through `ttf-parser` ([`TtfSource`]). An SHX reader plugs
//! in as another `GlyphSource` with `filled() == false` (SHX letters are lines).

pub mod build;
pub mod layout;
pub mod mtext;

use crate::geom::PathData;
use std::collections::HashMap;

/// Index into `Fonts::faces`.
pub type FaceId = u16;

/// Where glyph outlines come from. Units are font units; y up; the baseline is y = 0.
pub trait GlyphSource: Send + Sync {
    /// Height of a capital letter in font units. Text height in a drawing is the cap height.
    fn cap_height(&self) -> f64;
    /// Advance of a space in font units.
    fn space_advance(&self) -> f64;
    /// Outline and advance of one character. `None` if the font has no glyph for it.
    fn glyph(&self, ch: char) -> Option<(PathData, f64)>;
    /// `true` for outline fonts (glyphs are filled), `false` for line fonts like SHX (stroked).
    fn filled(&self) -> bool { true }
}

/// One stored glyph.
#[derive(Clone, Debug, Default)]
pub struct GlyphData { pub path: PathData, pub advance: f64 }

/// A font face with its glyph cache.
pub struct Face {
    /// Name used to find the face (file name, lower case).
    pub name: String,
    pub cap_height: f64,
    pub space: f64,
    pub filled: bool,
    /// Glyphs loaded so far, indexed by glyph id (`Glyph::id`).
    pub glyphs: Vec<GlyphData>,
    /// The character of each glyph id (for sinks that write real text, like SVG).
    pub chars: Vec<char>,
    cache: HashMap<char, Option<u32>>,
    source: Box<dyn GlyphSource>,
}

impl std::fmt::Debug for Face {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Face").field("name", &self.name).field("glyphs", &self.glyphs.len()).finish()
    }
}

impl Face {
    pub fn new(name: &str, source: Box<dyn GlyphSource>) -> Face {
        Face { name: name.to_ascii_lowercase(), cap_height: source.cap_height(), space: source.space_advance(), filled: source.filled(), glyphs: vec![], chars: vec![], cache: HashMap::new(), source }
    }
    /// Glyph id and advance for a character, loading it on first use.
    /// Missing characters fall back to '?'.
    pub fn glyph(&mut self, ch: char) -> Option<(u32, f64)> {
        let id = match self.cache.get(&ch) {
            Some(id) => *id,
            None => {
                let id = self.source.glyph(ch).map(|(path, advance)| { self.glyphs.push(GlyphData { path, advance }); self.chars.push(ch); (self.glyphs.len() - 1) as u32 });
                self.cache.insert(ch, id);
                id
            }
        };
        match id {
            Some(id) => Some((id, self.glyphs[id as usize].advance)),
            None if ch != '?' => self.glyph('?'),
            None => None,
        }
    }
}

/// All font faces of one scene. Face 0 is the fallback.
#[derive(Debug, Default)]
pub struct Fonts {
    pub faces: Vec<Face>,
    by_name: HashMap<String, FaceId>,
}

impl Fonts {
    pub fn new() -> Fonts { Fonts::default() }
    /// Fonts with one fallback face.
    pub fn with_fallback(face: Face) -> Fonts { let mut f = Fonts::new(); f.add(face); f }
    pub fn add(&mut self, face: Face) -> FaceId {
        let id = self.faces.len() as FaceId;
        self.by_name.insert(face.name.clone(), id);
        self.faces.push(face);
        id
    }
    pub fn is_empty(&self) -> bool { self.faces.is_empty() }
    /// Face for a text style's font file. Today: a face added under that file
    /// name (case-insensitive, without folder), else the fallback face 0.
    /// `None` if there is no font at all.
    pub fn face_for(&self, font_file: &str) -> Option<FaceId> {
        if self.faces.is_empty() { return None; }
        let base = font_file.rsplit(['/', '\\']).next().unwrap_or("").to_ascii_lowercase();
        Some(self.by_name.get(&base).copied().unwrap_or(0))
    }
}

/// TrueType / OpenType glyph source (`ttf-parser`).
pub struct TtfSource { data: Vec<u8>, index: u32, cap: f64, space: f64 }

impl TtfSource {
    pub fn from_bytes(data: Vec<u8>, index: u32) -> Result<TtfSource, String> {
        let face = ttf_parser::Face::parse(&data, index).map_err(|e| e.to_string())?;
        let cap = face.capital_height().map(|c| c as f64).filter(|c| *c > 0.0).unwrap_or(face.units_per_em() as f64 * 0.7);
        let space = face.glyph_index(' ').and_then(|g| face.glyph_hor_advance(g)).unwrap_or(face.units_per_em() / 4) as f64;
        Ok(TtfSource { data, index, cap, space })
    }
    pub fn from_file(path: &std::path::Path) -> Result<TtfSource, String> {
        let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::from_bytes(data, 0).map_err(|e| format!("{}: {e}", path.display()))
    }
}

struct Outline(PathData);
impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) { self.0.move_to(x as f64, y as f64) }
    fn line_to(&mut self, x: f32, y: f32) { self.0.line_to(x as f64, y as f64) }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) { self.0.quad_to(x1 as f64, y1 as f64, x as f64, y as f64) }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) { self.0.cubic_to(x1 as f64, y1 as f64, x2 as f64, y2 as f64, x as f64, y as f64) }
    fn close(&mut self) { self.0.close() }
}

impl GlyphSource for TtfSource {
    fn cap_height(&self) -> f64 { self.cap }
    fn space_advance(&self) -> f64 { self.space }
    fn glyph(&self, ch: char) -> Option<(PathData, f64)> {
        // parsing the table directory is cheap, and this runs once per character
        let face = ttf_parser::Face::parse(&self.data, self.index).ok()?;
        let g = face.glyph_index(ch)?;
        let adv = face.glyph_hor_advance(g).unwrap_or(0) as f64;
        let mut o = Outline(PathData::default());
        face.outline_glyph(g, &mut o);
        Some((o.0, adv))
    }
}

// ---- the font policy ------------------------------------------------------
//
// No AutoCAD or other non-free fonts are shipped. Every text style (SHX and
// TrueType alike) is drawn with one readable sans font: Arial if it is
// installed, else Liberation Sans (OFL, same metrics as Arial), which is
// built into the library. Height, width factor, oblique angle and alignment
// of each style are kept.

/// Liberation Sans Regular 2.1.5 (SIL Open Font License 1.1, see `fonts/`).
pub static LIBERATION_SANS: &[u8] = include_bytes!("../../fonts/LiberationSans-Regular.ttf");

/// Where Arial usually is (macOS, Linux msttcorefonts, Windows).
pub const ARIAL_PATHS: [&str; 6] = [
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/Library/Fonts/Arial.ttf",
    "/usr/share/fonts/truetype/msttcorefonts/Arial.ttf",
    "/usr/share/fonts/truetype/msttcorefonts/arial.ttf",
    "/usr/share/fonts/TTF/arial.ttf",
    "C:\\Windows\\Fonts\\arial.ttf",
];

/// Which font draws the text.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum FontChoice {
    /// Arial if installed, else the built-in Liberation Sans (the default).
    #[default]
    Auto,
    /// Always the built-in Liberation Sans (same picture on every machine).
    Liberation,
    /// A TrueType/OpenType file.
    File(std::path::PathBuf),
}

impl FontChoice {
    /// `"auto"`, `"arial"` (same as auto), `"liberation"`, or a file path.
    pub fn parse(s: &str) -> FontChoice {
        match s.to_ascii_lowercase().as_str() {
            "auto" | "arial" | "" => FontChoice::Auto,
            "liberation" | "liberation-sans" | "builtin" => FontChoice::Liberation,
            _ => FontChoice::File(s.into()),
        }
    }
}

/// The installed Arial, if any.
pub fn find_arial() -> Option<std::path::PathBuf> {
    ARIAL_PATHS.iter().map(std::path::PathBuf::from).find(|p| p.exists())
}

/// The face for a font choice. `Err` only if a given file cannot be read.
pub fn load_face(choice: &FontChoice) -> Result<Face, String> {
    let liberation = || TtfSource::from_bytes(LIBERATION_SANS.to_vec(), 0).map(|s| Face::new("LiberationSans-Regular.ttf", Box::new(s)));
    match choice {
        FontChoice::Liberation => liberation(),
        FontChoice::File(p) => {
            let src = TtfSource::from_file(p)?;
            Ok(Face::new(&p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(), Box::new(src)))
        }
        FontChoice::Auto => match find_arial().and_then(|p| TtfSource::from_file(&p).ok()) {
            Some(src) => Ok(Face::new("Arial.ttf", Box::new(src))),
            None => liberation(),
        },
    }
}

/// Fonts for a scene: the chosen face as face 0, used by every text style.
pub fn load_fonts(choice: &FontChoice) -> Result<Fonts, String> { Ok(Fonts::with_fallback(load_face(choice)?)) }

#[cfg(test)]
mod policy_tests {
    use super::*;

    #[test]
    fn builtin_liberation_loads_and_has_glyphs() {
        let mut f = load_face(&FontChoice::Liberation).unwrap();
        assert!(f.cap_height > 0.0);
        let (_, adv) = f.glyph('A').unwrap();
        assert!(adv > 0.0);
        assert!(!f.glyphs[0].path.is_empty());
    }

    #[test]
    fn auto_always_gives_a_font() {
        let fonts = load_fonts(&FontChoice::Auto).unwrap();
        assert_eq!(fonts.faces.len(), 1);
        // every style maps to the one face
        assert_eq!(fonts.face_for("romans.shx"), Some(0));
        assert_eq!(fonts.face_for("swiss.ttf"), Some(0));
    }

    #[test]
    fn choice_parsing() {
        assert_eq!(FontChoice::parse("Liberation"), FontChoice::Liberation);
        assert_eq!(FontChoice::parse("arial"), FontChoice::Auto);
        assert_eq!(FontChoice::parse("/x/y.ttf"), FontChoice::File("/x/y.ttf".into()));
        assert!(load_face(&FontChoice::File("/no/such/font.ttf".into())).is_err());
    }
}
