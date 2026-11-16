//! Metadata: block attributes and text content, for callers that want the
//! drawing's data, not its picture (title blocks, sheet lists, labels).
//!
//! Kept apart from the entities: the walk never reads it, and `Entity` stays
//! small. Readers add records with `SceneBuilder::record_insert` and
//! `record_text`; values are already plain text (decoded, formatting removed).

use super::tables::{BlockId, Handle, LayerId};
use crate::text::mtext;

/// An INSERT that has attributes.
#[derive(Clone, Debug, PartialEq)]
pub struct InsertRec {
    /// The block the INSERT is in: model space, a sheet's block or another block.
    pub owner: BlockId,
    /// The inserted block.
    pub block: BlockId,
    pub layer: LayerId,
    pub handle: Handle,
    /// Insertion point in the owner's coordinates.
    pub pos: [f64; 2],
    /// (tag, value) in file order. Hidden attributes are included.
    pub attribs: Vec<(String, String)>,
}

/// Type of a text record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextKind { Text, MText, MLeader }

impl TextKind {
    /// DXF type name.
    pub fn name(self) -> &'static str {
        match self { TextKind::Text => "TEXT", TextKind::MText => "MTEXT", TextKind::MLeader => "MULTILEADER" }
    }
}

/// A TEXT, MTEXT or MULTILEADER text, as plain text (MTEXT lines joined with `\n`).
#[derive(Clone, Debug, PartialEq)]
pub struct TextRec {
    pub owner: BlockId,
    pub kind: TextKind,
    pub layer: LayerId,
    pub handle: Handle,
    /// Insertion point in the owner's coordinates.
    pub pos: [f64; 2],
    pub text: String,
}

/// All metadata records of a drawing. Order: model space, then the sheets in
/// tab order, then other blocks by name; file order inside each.
#[derive(Clone, Debug, Default)]
pub struct Meta {
    pub inserts: Vec<InsertRec>,
    pub texts: Vec<TextRec>,
}

/// A TEXT or ATTRIB value as plain text: `%%d`, `\U+XXXX` decoded, trimmed.
pub fn plain_text(raw: &str) -> String { mtext::plain(&cp1252(raw)).trim().to_string() }

/// An MTEXT value as plain text: formatting removed, lines joined with `\n`, trimmed.
pub fn plain_mtext(raw: &str) -> String {
    let lines = mtext::mtext_lines(&cp1252(raw));
    lines.iter().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n").trim().to_string()
}

/// Old (pre-2007) files store text in the ANSI code page. The readers turn
/// such bytes into chars one to one (Latin-1), which is right except for
/// 0x80..0x9F, where Windows-1252 has letters and Latin-1 has control
/// characters. Map those back (`’` `€` `–` ...).
fn cp1252(s: &str) -> std::borrow::Cow<'_, str> {
    const HI: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}',
        '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    if !s.chars().any(|c| ('\u{80}'..='\u{9f}').contains(&c)) { return std::borrow::Cow::Borrowed(s); }
    s.chars().map(|c| if ('\u{80}'..='\u{9f}').contains(&c) { HI[c as usize - 0x80] } else { c }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_values() {
        assert_eq!(plain_text("  Citt\\U+00E0 di prova "), "Città di prova");
        assert_eq!(plain_text("45%%d"), "45°");
        assert_eq!(plain_text("l\u{92}area \u{80} 5"), "l’area € 5");
        assert_eq!(plain_text(""), "");
    }

    #[test]
    fn plain_mtext_values() {
        assert_eq!(plain_mtext("{\\fArial|b1;First} line\\PSecond\\U+00E8 "), "First line\nSecondè");
        assert_eq!(plain_mtext("\\A1;one"), "one");
    }
}
