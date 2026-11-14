//! SHX file reader: normal shape fonts / shape files, unifonts and big fonts.
//!
//! Binary layouts (all numbers in the header and index are little endian):
//!
//! ```text
//! all:      "AutoCAD-86 <kind> 1.0\r\n" 0x1A
//! shapes:   u16 first, u16 last, u16 count,
//!           count * (u16 number, u16 length), then the records one after another,
//!           then "EOF"
//! unifont:  u32 count (records + 1), u16 info length, info record,
//!           (count - 1) * (u16 number, u16 length, record)
//! bigfont:  u16 header size, u16 count, u16 range count,
//!           ranges * (u16 first, u16 last)   (escape byte ranges),
//!           count * (u16 number, u16 length, u32 file offset)
//! record:   name bytes, 0, shape byte code (ends with code 0)
//! info:     shape 0 (or the unifont info record):
//!           name, 0, above, below, modes, [encoding, embedding], 0
//!           extended big font: name, 0, height, 0, modes, width, 0
//! ```
//!
//! The reader is tolerant: a truncated file keeps the shapes that are complete,
//! duplicate numbers keep the first one, a missing `EOF` is ignored.

use std::path::Path;
use std::sync::OnceLock;

use crate::glyph::Glyph;
use crate::interp::Interp;

/// The three SHX file kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShxKind {
    /// "AutoCAD-86 shapes 1.0/1.1": a text font (has shape 0) or a shape file
    /// (no shape 0, used by complex linetypes).
    Shapes,
    /// "AutoCAD-86 unifont 1.0": shape numbers are Unicode code points.
    Unifont,
    /// "AutoCAD-86 bigfont 1.0": double-byte (Asian) code page fonts.
    BigFont,
}

/// Font information from shape 0 (or the unifont info record).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FontInfo {
    /// Free text name stored in the font.
    pub name: String,
    /// Height of capital letters above the baseline, in font units.
    /// A text of height H scales the font by `H / above`.
    pub above: u8,
    /// Depth of descenders below the baseline, in font units.
    pub below: u8,
    /// 0 = horizontal only, 2 = horizontal and vertical (code 14 is used).
    pub modes: u8,
    /// Unifont encoding byte (0 = Unicode, 1 = packed multibyte, 2 = shape file).
    pub encoding: u8,
    /// Unifont embedding byte (0 = may embed, 1 = may not, 2 = read-only).
    pub embedding: u8,
    /// Extended big font: character width of the primitive box.
    pub char_width: u8,
    /// True for an extended big font (subshapes placed with `7,0,...`).
    pub extended_big: bool,
}

/// Why a file could not be read at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShxError {
    /// The file does not start with an "AutoCAD-86" SHX signature.
    NotShx,
    /// The signature is fine but not one shape could be read.
    NoShapes,
    /// The file could not be read from disk.
    Io(String),
}

impl std::fmt::Display for ShxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShxError::NotShx => write!(f, "not an SHX file (no AutoCAD-86 signature)"),
            ShxError::NoShapes => write!(f, "SHX file has no readable shapes"),
            ShxError::Io(e) => write!(f, "cannot read SHX file: {e}"),
        }
    }
}

impl std::error::Error for ShxError {}

const NONE: u32 = u32::MAX;

/// A loaded SHX file. Glyphs are parsed on first use and cached; the cache
/// is lock-free after the first use, so one font can be shared by many
/// threads (`ShxFont` is `Send + Sync`).
pub struct ShxFont {
    data: Box<[u8]>,
    kind: ShxKind,
    info: Option<FontInfo>,
    /// Sorted shape numbers (shape 0 is not included).
    codes: Box<[u16]>,
    /// Byte-code span `[start, end)` per slot.
    spans: Box<[(u32, u32)]>,
    /// Name span `[start, end)` per slot.
    names: Box<[(u32, u32)]>,
    /// Slot of shape numbers 0..=255 (NONE = missing).
    fast: Box<[u32; 256]>,
    /// Big font escape byte ranges.
    escapes: Box<[(u16, u16)]>,
    cache: Box<[OnceLock<Glyph>]>,
    vcache: OnceLock<Box<[OnceLock<Glyph>]>>,
    issues: Vec<String>,
}

impl std::fmt::Debug for ShxFont {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShxFont")
            .field("kind", &self.kind)
            .field("info", &self.info)
            .field("shapes", &self.codes.len())
            .field("issues", &self.issues)
            .finish()
    }
}

struct Reader<'a> {
    d: &'a [u8],
    p: usize,
}

impl Reader<'_> {
    fn u16(&mut self) -> Option<u16> {
        let b = self.d.get(self.p..self.p + 2)?;
        self.p += 2;
        Some(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self) -> Option<u32> {
        let b = self.d.get(self.p..self.p + 4)?;
        self.p += 4;
        Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
}

/// Raw record found in the file: number and its byte span.
struct Rec {
    number: u16,
    start: usize,
    end: usize,
}

impl ShxFont {
    /// Read an SHX file from disk.
    pub fn from_file(path: impl AsRef<Path>) -> Result<ShxFont, ShxError> {
        let data = std::fs::read(path.as_ref())
            .map_err(|e| ShxError::Io(format!("{}: {e}", path.as_ref().display())))?;
        ShxFont::from_bytes(data)
    }

    /// Parse an SHX file from memory.
    pub fn from_bytes(data: impl Into<Box<[u8]>>) -> Result<ShxFont, ShxError> {
        let data: Box<[u8]> = data.into();
        let head = &data[..data.len().min(48)];
        if !head.starts_with(b"AutoCAD-86 ") {
            return Err(ShxError::NotShx);
        }
        let kind = if head[11..].starts_with(b"unifont") {
            ShxKind::Unifont
        } else if head[11..].starts_with(b"bigfont") {
            ShxKind::BigFont
        } else if head[11..].starts_with(b"shapes") {
            ShxKind::Shapes
        } else {
            return Err(ShxError::NotShx);
        };
        let Some(sig_end) = head.iter().position(|&b| b == 0x1A) else {
            return Err(ShxError::NotShx);
        };
        let mut issues = Vec::new();
        let mut escapes = Vec::new();
        let mut info_span: Option<(usize, usize)> = None;
        let mut recs: Vec<Rec> = Vec::new();
        let mut r = Reader {
            d: &data,
            p: sig_end + 1,
        };
        let len = data.len();
        match kind {
            ShxKind::Shapes => {
                let (_first, _last, count) = match (r.u16(), r.u16(), r.u16()) {
                    (Some(a), Some(b), Some(c)) => (a, b, c),
                    _ => return Err(ShxError::NoShapes),
                };
                let mut index = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    match (r.u16(), r.u16()) {
                        (Some(n), Some(l)) => index.push((n, l as usize)),
                        _ => {
                            issues.push("index table is truncated".into());
                            break;
                        }
                    }
                }
                let mut off = r.p;
                for (n, l) in index {
                    if off + l > len {
                        issues.push(format!("shape {n} is cut off by the end of the file"));
                        break;
                    }
                    recs.push(Rec {
                        number: n,
                        start: off,
                        end: off + l,
                    });
                    off += l;
                }
                if data.get(off..off + 3) != Some(b"EOF".as_slice()) && issues.is_empty() {
                    issues.push("no EOF marker".into());
                }
            }
            ShxKind::Unifont => {
                let (count, info_len) = match (r.u32(), r.u16()) {
                    (Some(c), Some(l)) => (c, l as usize),
                    _ => return Err(ShxError::NoShapes),
                };
                if r.p + info_len <= len {
                    info_span = Some((r.p, r.p + info_len));
                    r.p += info_len;
                } else {
                    issues.push("info record is cut off".into());
                }
                let mut left = count.saturating_sub(1);
                while left > 0 && r.p < len {
                    let (Some(n), Some(l)) = (r.u16(), r.u16()) else {
                        issues.push("record header is cut off".into());
                        break;
                    };
                    let l = l as usize;
                    if r.p + l > len {
                        issues.push(format!("shape {n} is cut off by the end of the file"));
                        break;
                    }
                    recs.push(Rec {
                        number: n,
                        start: r.p,
                        end: r.p + l,
                    });
                    r.p += l;
                    left -= 1;
                }
            }
            ShxKind::BigFont => {
                let (_hdr, count, nranges) = match (r.u16(), r.u16(), r.u16()) {
                    (Some(a), Some(b), Some(c)) => (a, b, c),
                    _ => return Err(ShxError::NoShapes),
                };
                for _ in 0..nranges {
                    match (r.u16(), r.u16()) {
                        (Some(a), Some(b)) => escapes.push((a.min(b), a.max(b))),
                        _ => break,
                    }
                }
                for _ in 0..count {
                    let (Some(n), Some(l), Some(o)) = (r.u16(), r.u16(), r.u32()) else {
                        issues.push("index table is truncated".into());
                        break;
                    };
                    let (l, o) = (l as usize, o as usize);
                    if l == 0 {
                        continue;
                    }
                    if o + l > len {
                        issues.push(format!("shape {n} points outside the file"));
                        continue;
                    }
                    recs.push(Rec {
                        number: n,
                        start: o,
                        end: o + l,
                    });
                }
            }
        }

        // Shape 0 is the font info (shapes and big fonts).
        if let Some(pos) = recs.iter().position(|r| r.number == 0) {
            let r0 = recs.remove(pos);
            if info_span.is_none() {
                info_span = Some((r0.start, r0.end));
            }
            recs.retain(|r| r.number != 0);
        }
        let info = info_span.map(|(s, e)| parse_info(&data[s..e], kind));

        // Sort by number, keep the first of duplicates (stable sort).
        recs.sort_by_key(|r| r.number);
        let before = recs.len();
        recs.dedup_by_key(|r| r.number);
        if recs.len() != before {
            issues.push(format!(
                "{} duplicate shape numbers ignored",
                before - recs.len()
            ));
        }
        if recs.is_empty() {
            return Err(ShxError::NoShapes);
        }

        let mut codes = Vec::with_capacity(recs.len());
        let mut spans = Vec::with_capacity(recs.len());
        let mut names = Vec::with_capacity(recs.len());
        let mut fast = Box::new([NONE; 256]);
        for (slot, r) in recs.iter().enumerate() {
            let rec = &data[r.start..r.end];
            let nul = rec.iter().position(|&b| b == 0).unwrap_or(rec.len());
            let code_start = (r.start + nul + 1).min(r.end);
            codes.push(r.number);
            names.push((r.start as u32, (r.start + nul) as u32));
            spans.push((code_start as u32, r.end as u32));
            if r.number < 256 {
                fast[r.number as usize] = slot as u32;
            }
        }
        let n = codes.len();
        Ok(ShxFont {
            data,
            kind,
            info,
            codes: codes.into_boxed_slice(),
            spans: spans.into_boxed_slice(),
            names: names.into_boxed_slice(),
            fast,
            escapes: escapes.into_boxed_slice(),
            cache: (0..n).map(|_| OnceLock::new()).collect(),
            vcache: OnceLock::new(),
            issues,
        })
    }

    /// File kind.
    pub fn kind(&self) -> ShxKind {
        self.kind
    }

    /// Font info (shape 0). `None` for a pure shape file.
    pub fn info(&self) -> Option<&FontInfo> {
        self.info.as_ref()
    }

    /// True when the file is a text font (not a linetype shape file).
    pub fn is_font(&self) -> bool {
        match self.kind {
            ShxKind::Shapes => self.info.is_some(),
            ShxKind::Unifont => self.info.as_ref().is_none_or(|i| i.encoding != 2),
            ShxKind::BigFont => true,
        }
    }

    /// Cap height in font units ("above"). Falls back to the tallest of
    /// 'A'..'Z' or 10 when the font does not say.
    pub fn above(&self) -> f32 {
        match &self.info {
            Some(i) if i.above > 0 => i.above as f32,
            _ => self
                .slot_for_code('H' as u32)
                .map(|s| self.glyph(s).bbox()[3])
                .filter(|h| *h > 0.0)
                .unwrap_or(10.0),
        }
    }

    /// Descender depth in font units ("below").
    pub fn below(&self) -> f32 {
        self.info.as_ref().map_or(0.0, |i| i.below as f32)
    }

    /// Problems found while reading (the font still works).
    pub fn issues(&self) -> &[String] {
        &self.issues
    }

    /// Number of shapes (shape 0 not counted).
    pub fn shape_count(&self) -> usize {
        self.codes.len()
    }

    /// Sorted shape numbers.
    pub fn codes(&self) -> &[u16] {
        &self.codes
    }

    /// Shape number of a slot.
    pub fn code_of(&self, slot: u32) -> u16 {
        self.codes[slot as usize]
    }

    /// Shape name stored in the file (shape files use names, fonts mostly not).
    pub fn shape_name(&self, slot: u32) -> &str {
        let (s, e) = self.names[slot as usize];
        std::str::from_utf8(&self.data[s as usize..e as usize]).unwrap_or("")
    }

    /// Find a shape by name (case-insensitive), for complex linetypes.
    pub fn slot_by_name(&self, name: &str) -> Option<u32> {
        (0..self.codes.len() as u32).find(|&s| self.shape_name(s).eq_ignore_ascii_case(name))
    }

    /// Big font escape byte ranges.
    pub fn escape_ranges(&self) -> &[(u16, u16)] {
        &self.escapes
    }

    /// Slot of a raw shape number.
    #[inline]
    pub fn slot_for_code(&self, code: u32) -> Option<u32> {
        if code < 256 {
            let s = self.fast[code as usize];
            return (s != NONE).then_some(s);
        }
        if code > 0xFFFF {
            return None;
        }
        self.codes
            .binary_search(&(code as u16))
            .ok()
            .map(|s| s as u32)
    }

    /// Slot for a character. Order: the Unicode code point as shape number,
    /// then the Windows-1252 byte (normal shape fonts), then the old AutoCAD
    /// numbers 127 (degree), 128 (plus-minus), 129 (diameter).
    #[inline]
    pub fn slot_for_char(&self, ch: char) -> Option<u32> {
        let c = ch as u32;
        if let Some(s) = self.slot_for_code(c) {
            return Some(s);
        }
        if c < 0x80 {
            return None;
        }
        if self.kind == ShxKind::Shapes {
            if let Some(b) = crate::codepage::cp1252_byte(ch) {
                if let Some(s) = self.slot_for_code(b as u32) {
                    return Some(s);
                }
            }
        }
        let (alias, legacy) = match ch {
            '\u{B0}' => (None, 127),
            '\u{B1}' => (None, 128),
            '\u{2205}' => (Some(0x2300), 129),
            '\u{2300}' => (Some(0x2205), 129),
            _ => return None,
        };
        alias
            .and_then(|a| self.slot_for_code(a))
            .or_else(|| self.slot_for_code(legacy))
    }

    /// Parsed glyph of a slot (horizontal text). Parsed once, then cached.
    #[inline]
    pub fn glyph(&self, slot: u32) -> &Glyph {
        self.cache[slot as usize].get_or_init(|| self.build(slot, false))
    }

    /// Parsed glyph for vertical text (code 14 commands are executed).
    pub fn glyph_vertical(&self, slot: u32) -> &Glyph {
        let v = self
            .vcache
            .get_or_init(|| (0..self.codes.len()).map(|_| OnceLock::new()).collect());
        v[slot as usize].get_or_init(|| self.build(slot, true))
    }

    /// Glyph for a character, if the font has one.
    pub fn glyph_for_char(&self, ch: char) -> Option<&Glyph> {
        self.slot_for_char(ch).map(|s| self.glyph(s))
    }

    /// Parse every glyph now (for example before sharing the font with
    /// many threads, or to measure parse time).
    pub fn parse_all(&self) {
        for s in 0..self.codes.len() as u32 {
            self.glyph(s);
        }
    }

    /// Byte code of a shape number (used for subshapes).
    pub(crate) fn shape_code(&self, code: u16) -> Option<&[u8]> {
        let s = self.slot_for_code(code as u32)?;
        let (a, b) = self.spans[s as usize];
        Some(&self.data[a as usize..b as usize])
    }

    /// Extended big font primitive box (height, width), never zero.
    pub(crate) fn big_box(&self) -> (f64, f64) {
        let i = self.info.as_ref();
        let h = i.map_or(0, |i| i.above).max(1) as f64;
        let w = i.map_or(0, |i| i.char_width);
        (h, if w == 0 { h } else { w as f64 })
    }

    fn build(&self, slot: u32, vertical: bool) -> Glyph {
        let (a, b) = self.spans[slot as usize];
        let code = &self.data[a as usize..b as usize];
        let g = Interp::run_shape(self, code, vertical);
        if self.kind == ShxKind::BigFont && g.advance() <= 0.0 {
            // Extended big font characters are only primitives: the pen does not
            // move, so the advance is the character width from the font info.
            let (h, w) = self.big_box();
            let w = if self.info.as_ref().is_some_and(|i| i.extended_big) {
                w
            } else {
                h
            };
            return g.with_advance(w as f32);
        }
        g
    }
}

fn parse_info(rec: &[u8], kind: ShxKind) -> FontInfo {
    let nul = rec.iter().position(|&b| b == 0).unwrap_or(rec.len());
    let name = String::from_utf8_lossy(&rec[..nul]).trim().to_string();
    let rest = rec.get(nul + 1..).unwrap_or(&[]);
    let at = |k: usize| rest.get(k).copied().unwrap_or(0);
    let mut info = FontInfo {
        name,
        ..FontInfo::default()
    };
    if kind == ShxKind::BigFont {
        if rest.len() >= 5 && at(1) == 0 {
            info.above = at(0);
            info.modes = at(2);
            info.char_width = at(3);
            info.extended_big = true;
        } else if rest.len() >= 4 {
            info.above = at(0);
            info.below = at(1);
            info.modes = at(2);
        } else {
            info.above = at(0);
            info.modes = at(1);
        }
    } else {
        info.above = at(0);
        info.below = at(1);
        info.modes = at(2);
        if rest.len() >= 6 {
            info.encoding = at(3);
            info.embedding = at(4);
        }
    }
    info
}
