//! Single-line text layout (TEXT, ATTRIB, ATTDEF, and one MTEXT run).
//!
//! The layout does not copy glyph geometry. It returns glyph ids with a pen
//! x position plus one transform from font units to *text space*. Text space
//! is the text's own frame: the alignment point is (0, 0), the baseline runs
//! along +x, the text height is already applied. The caller adds rotation and
//! the insertion point (and the OCS / block matrix).
//!
//! Special codes handled by [`tokens`]: `%%c` (diameter), `%%d` (degree),
//! `%%p` (plus-minus), `%%%` (percent), `%%nnn` (character number nnn),
//! `%%u` / `%%o` / `%%k` (underline / overline / strike-through toggles),
//! `\U+XXXX` (Unicode) and `\M+nXXXX` (raw double-byte code for big fonts).

use crate::face::{FontFace, StrokeGlyphs};

/// One item of a text string after the special codes are decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextToken {
    /// A normal character.
    Char(char),
    /// A raw character code (`%%nnn`, `\M+nXXXX`).
    Code(u32),
    /// `%%u`: underline on/off.
    Underline,
    /// `%%o`: overline on/off.
    Overline,
    /// `%%k`: strike-through on/off.
    Strike,
}

/// Iterator over the [`TextToken`]s of a string. Does not allocate.
#[derive(Debug, Clone)]
pub struct Tokens<'a> {
    s: &'a str,
    i: usize,
}

/// Decode the special codes of a TEXT string.
pub fn tokens(s: &str) -> Tokens<'_> {
    Tokens { s, i: 0 }
}

fn hex(b: &[u8]) -> Option<u32> {
    let mut v = 0u32;
    for &c in b {
        v = v * 16 + (c as char).to_digit(16)?;
    }
    Some(v)
}

impl Iterator for Tokens<'_> {
    type Item = TextToken;

    fn next(&mut self) -> Option<TextToken> {
        let b = self.s.as_bytes();
        let i = self.i;
        if i >= b.len() {
            return None;
        }
        if b[i] == b'%' && b.get(i + 1) == Some(&b'%') {
            if let Some(&c) = b.get(i + 2) {
                let (tok, used) = match c.to_ascii_lowercase() {
                    b'c' => (Some(TextToken::Char('\u{2205}')), 3),
                    b'd' => (Some(TextToken::Char('\u{B0}')), 3),
                    b'p' => (Some(TextToken::Char('\u{B1}')), 3),
                    b'u' => (Some(TextToken::Underline), 3),
                    b'o' => (Some(TextToken::Overline), 3),
                    b'k' => (Some(TextToken::Strike), 3),
                    b'%' => (Some(TextToken::Char('%')), 3),
                    b'0'..=b'9' => match b.get(i + 2..i + 5) {
                        Some(d) if d.iter().all(u8::is_ascii_digit) => {
                            let n = (d[0] - b'0') as u32 * 100
                                + (d[1] - b'0') as u32 * 10
                                + (d[2] - b'0') as u32;
                            (Some(TextToken::Code(n)), 5)
                        }
                        _ => (None, 0),
                    },
                    _ => (None, 0),
                };
                if let Some(t) = tok {
                    self.i += used;
                    return Some(t);
                }
                // Unknown "%%x": drop the "%%", keep the character.
                self.i += 2;
                return self.next();
            }
        }
        if b[i] == b'\\' && b.get(i + 2) == Some(&b'+') {
            match b.get(i + 1) {
                Some(b'U' | b'u') => {
                    if let Some(c) = b.get(i + 3..i + 7).and_then(hex).and_then(char::from_u32) {
                        self.i += 7;
                        return Some(TextToken::Char(c));
                    }
                }
                Some(b'M' | b'm') => {
                    // \M+nXXXX: n = code page number (1..5), XXXX = double-byte code.
                    if let Some(v) = b.get(i + 4..i + 8).and_then(hex) {
                        if b.get(i + 3).is_some_and(u8::is_ascii_digit) {
                            self.i += 8;
                            return Some(TextToken::Code(v));
                        }
                    }
                }
                _ => {}
            }
        }
        let c = self.s[i..].chars().next()?;
        self.i += c.len_utf8();
        Some(TextToken::Char(c))
    }
}

/// Horizontal alignment (TEXT group code 72).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HAlign {
    #[default]
    Left,
    Center,
    Right,
    /// Fit between two points, height changes with the width.
    Aligned,
    /// Centered horizontally and vertically.
    Middle,
    /// Fit between two points, height stays, width factor changes.
    Fit,
}

impl HAlign {
    /// From DXF group code 72.
    pub fn from_dxf(v: i32) -> HAlign {
        match v {
            1 => HAlign::Center,
            2 => HAlign::Right,
            3 => HAlign::Aligned,
            4 => HAlign::Middle,
            5 => HAlign::Fit,
            _ => HAlign::Left,
        }
    }
}

/// Vertical alignment (TEXT group code 73; ATTRIB/ATTDEF code 74).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VAlign {
    #[default]
    Baseline,
    /// Bottom of descenders.
    Bottom,
    /// Half the text height.
    Middle,
    /// Text height (top of capitals).
    Top,
}

impl VAlign {
    /// From DXF group code 73 (TEXT) or 74 (ATTRIB, ATTDEF).
    pub fn from_dxf(v: i32) -> VAlign {
        match v {
            1 => VAlign::Bottom,
            2 => VAlign::Middle,
            3 => VAlign::Top,
            _ => VAlign::Baseline,
        }
    }
}

/// Text settings, mostly straight from the entity and its style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextParams {
    /// Text height in drawing units (height of capitals).
    pub height: f64,
    /// Width factor (1.0 = normal). 0 or less is read as 1.
    pub width_factor: f64,
    /// Oblique angle in degrees (positive slants to the right).
    pub oblique_deg: f64,
    pub halign: HAlign,
    pub valign: VAlign,
    /// Distance between the two alignment points, for `Aligned` and `Fit`.
    /// 0 means "not known": the text keeps its natural width.
    pub fit_width: f64,
    /// TEXT generation flag 2: mirrored in X.
    pub backward: bool,
    /// TEXT generation flag 4: mirrored in Y.
    pub upside_down: bool,
}

impl Default for TextParams {
    fn default() -> Self {
        TextParams {
            height: 1.0,
            width_factor: 1.0,
            oblique_deg: 0.0,
            halign: HAlign::Left,
            valign: VAlign::Baseline,
            fit_width: 0.0,
            backward: false,
            upside_down: false,
        }
    }
}

/// 2D affine transform: `x' = a*x + c*y + e`, `y' = b*x + d*y + f`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl Affine {
    pub const IDENTITY: Affine = Affine {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    #[inline]
    pub fn apply(&self, p: [f64; 2]) -> [f64; 2] {
        [
            self.a * p[0] + self.c * p[1] + self.e,
            self.b * p[0] + self.d * p[1] + self.f,
        ]
    }

    /// `self` after `inner`: first `inner`, then `self`.
    pub fn then_after(&self, inner: &Affine) -> Affine {
        Affine {
            a: self.a * inner.a + self.c * inner.b,
            b: self.b * inner.a + self.d * inner.b,
            c: self.a * inner.c + self.c * inner.d,
            d: self.b * inner.c + self.d * inner.d,
            e: self.a * inner.e + self.c * inner.f + self.e,
            f: self.b * inner.e + self.d * inner.f + self.f,
        }
    }
}

/// One glyph of a layout: id, pen x (font units, baseline y = 0) and the
/// glyph's own scale (1.0 except big font glyphs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedGlyph {
    pub id: u32,
    pub x: f32,
    pub scale: f32,
}

/// Which line a decoration is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecorationKind {
    Underline,
    Overline,
    Strike,
}

/// An underline / overline / strike segment, in font units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decoration {
    pub kind: DecorationKind,
    pub x0: f32,
    pub x1: f32,
    pub y: f32,
}

/// Result of [`layout_text`]. Reuse one value for many texts: `layout_text`
/// clears it, and the vectors keep their memory.
#[derive(Debug, Clone, Default)]
pub struct TextLayout {
    pub glyphs: Vec<PlacedGlyph>,
    pub decorations: Vec<Decoration>,
    /// Font units -> text space.
    pub transform: Affine,
    /// Total advance in font units.
    pub advance: f32,
    /// Cap height and descent of the face, font units.
    pub cap_height: f32,
    pub descent: f32,
}

impl Default for Affine {
    fn default() -> Self {
        Affine::IDENTITY
    }
}

/// Polylines in text space, flat like [`crate::Glyph`]. Reuse one value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Polylines {
    pub points: Vec<[f64; 2]>,
    /// End index (exclusive) of each polyline in `points`.
    pub ends: Vec<u32>,
}

impl Polylines {
    pub fn clear(&mut self) {
        self.points.clear();
        self.ends.clear();
    }
    pub fn len(&self) -> usize {
        self.ends.len()
    }
    pub fn is_empty(&self) -> bool {
        self.ends.is_empty()
    }
    pub fn iter(&self) -> impl Iterator<Item = &[[f64; 2]]> + '_ {
        let mut start = 0usize;
        self.ends.iter().map(move |&e| {
            let s = start;
            start = e as usize;
            &self.points[s..e as usize]
        })
    }
}

impl TextLayout {
    pub fn new() -> TextLayout {
        TextLayout::default()
    }

    pub fn clear(&mut self) {
        self.glyphs.clear();
        self.decorations.clear();
        self.transform = Affine::IDENTITY;
        self.advance = 0.0;
    }

    /// Transform of one glyph's outline (font units) into text space.
    #[inline]
    pub fn glyph_transform(&self, g: &PlacedGlyph) -> Affine {
        let t = &self.transform;
        let s = g.scale as f64;
        Affine {
            a: t.a * s,
            b: t.b * s,
            c: t.c * s,
            d: t.d * s,
            e: t.a * g.x as f64 + t.e,
            f: t.b * g.x as f64 + t.f,
        }
    }

    /// Width of the text in text space (advance, after scaling).
    pub fn width(&self) -> f64 {
        (self.transform.a * self.advance as f64).abs()
    }

    /// Text box in text space `[min_x, min_y, max_x, max_y]`: advance width
    /// times descent..cap height, after oblique and mirroring.
    pub fn bounds(&self) -> [f64; 4] {
        let (x1, y0, y1) = (
            self.advance as f64,
            -self.descent as f64,
            self.cap_height as f64,
        );
        let mut b = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for p in [[0.0, y0], [x1, y0], [x1, y1], [0.0, y1]] {
            let q = self.transform.apply(p);
            b[0] = b[0].min(q[0]);
            b[1] = b[1].min(q[1]);
            b[2] = b[2].max(q[0]);
            b[3] = b[3].max(q[1]);
        }
        b
    }

    /// Append all glyph polylines and decoration lines in text space to `out`
    /// (it is not cleared). For SHX fonts.
    pub fn stroke_polylines<F: StrokeGlyphs + ?Sized>(&self, font: &F, out: &mut Polylines) {
        for g in &self.glyphs {
            let glyph = font.stroke_glyph(g.id);
            if glyph.is_empty() {
                continue;
            }
            let t = self.glyph_transform(g);
            out.points.reserve(glyph.points().len());
            for line in glyph.polylines() {
                for p in line {
                    out.points.push(t.apply([p[0] as f64, p[1] as f64]));
                }
                out.ends.push(out.points.len() as u32);
            }
        }
        for d in &self.decorations {
            out.points
                .push(self.transform.apply([d.x0 as f64, d.y as f64]));
            out.points
                .push(self.transform.apply([d.x1 as f64, d.y as f64]));
            out.ends.push(out.points.len() as u32);
        }
    }
}

/// Characters to try when a font lacks one.
fn alternatives(c: char) -> &'static [char] {
    match c {
        '\u{2205}' => &['\u{2300}', '\u{D8}'],
        '\u{2300}' => &['\u{2205}', '\u{D8}'],
        '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{B4}' | '\u{2032}' => &['\''],
        '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{2033}' => &['"'],
        '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2212}' => &['-'],
        '\u{A0}' | '\u{2007}' | '\u{2009}' | '\u{202F}' | '\t' => &[' '],
        '\u{D7}' => &['x'],
        '\u{B2}' => &['2'],
        '\u{B3}' => &['3'],
        _ => &[],
    }
}

#[inline]
fn find_glyph<F: FontFace + ?Sized>(face: &F, c: char) -> Option<u32> {
    if let Some(id) = face.glyph_id(c) {
        return Some(id);
    }
    alternatives(c).iter().find_map(|&a| face.glyph_id(a))
}

/// Lay out one line of text. `out` is cleared first.
///
/// Missing characters become '?' (like AutoCAD); if the font has no '?'
/// either, they take half the cap height of space. Control characters
/// (line breaks) are skipped: TEXT is one line.
pub fn layout_text<F: FontFace + ?Sized>(
    face: &F,
    text: &str,
    p: &TextParams,
    out: &mut TextLayout,
) {
    out.clear();
    let cap = face.cap_height();
    let cap = if cap > 0.0 { cap } else { 1.0 };
    out.cap_height = cap;
    out.descent = face.descent().max(0.0);
    let question = face.glyph_id('?');
    let mut x = 0f32;
    let mut open: [Option<f32>; 3] = [None; 3];
    let deco_y = [-0.2 * cap, 1.2 * cap, 0.5 * cap];
    let kinds = [
        DecorationKind::Underline,
        DecorationKind::Overline,
        DecorationKind::Strike,
    ];
    for tok in tokens(text) {
        let id = match tok {
            TextToken::Char(c) if (c as u32) < 0x20 && c != '\t' => continue,
            TextToken::Char(c) => find_glyph(face, c).or(question),
            TextToken::Code(n) => face.glyph_id_for_code(n).or(question),
            TextToken::Underline | TextToken::Overline | TextToken::Strike => {
                let k = match tok {
                    TextToken::Underline => 0,
                    TextToken::Overline => 1,
                    _ => 2,
                };
                match open[k].take() {
                    Some(x0) => {
                        if x > x0 {
                            out.decorations.push(Decoration {
                                kind: kinds[k],
                                x0,
                                x1: x,
                                y: deco_y[k],
                            })
                        }
                    }
                    None => open[k] = Some(x),
                }
                continue;
            }
        };
        match id {
            Some(id) => {
                out.glyphs.push(PlacedGlyph {
                    id,
                    x,
                    scale: face.glyph_scale(id),
                });
                x += face.advance(id);
            }
            None => x += 0.5 * cap,
        }
    }
    for k in 0..3 {
        if let Some(x0) = open[k] {
            if x > x0 {
                out.decorations.push(Decoration {
                    kind: kinds[k],
                    x0,
                    x1: x,
                    y: deco_y[k],
                });
            }
        }
    }
    out.advance = x;

    let wf = if p.width_factor > 0.0 {
        p.width_factor
    } else {
        1.0
    };
    let mut sy = p.height / cap as f64;
    let mut sx = sy * wf;
    let adv = x as f64;
    if p.fit_width > 0.0 && adv > 0.0 {
        match p.halign {
            HAlign::Fit => sx = p.fit_width / adv,
            HAlign::Aligned => {
                let k = p.fit_width / (adv * sx);
                sx *= k;
                sy *= k;
            }
            _ => {}
        }
    }
    let shear = p.oblique_deg.to_radians().tan();
    let w = adv * sx;
    let (capf, desc) = (cap as f64, out.descent as f64);
    let x_off = match p.halign {
        HAlign::Left | HAlign::Aligned | HAlign::Fit => 0.0,
        HAlign::Center | HAlign::Middle => -w / 2.0,
        HAlign::Right => -w,
    };
    let y_off = match p.halign {
        HAlign::Aligned | HAlign::Fit => 0.0,
        HAlign::Middle => -(capf - desc) / 2.0 * sy,
        _ => match p.valign {
            VAlign::Baseline => 0.0,
            VAlign::Bottom => desc * sy,
            VAlign::Middle => -capf / 2.0 * sy,
            VAlign::Top => -capf * sy,
        },
    };
    let mx = if p.backward { -1.0 } else { 1.0 };
    let my = if p.upside_down { -1.0 } else { 1.0 };
    out.transform = Affine {
        a: mx * sx,
        b: 0.0,
        c: mx * shear * sy,
        d: my * sy,
        e: mx * x_off,
        f: my * y_off,
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Vec<TextToken> {
        tokens(s).collect()
    }

    #[test]
    fn special_codes() {
        use TextToken::*;
        assert_eq!(toks("%%c50"), vec![Char('\u{2205}'), Char('5'), Char('0')]);
        assert_eq!(toks("90%%D"), vec![Char('9'), Char('0'), Char('\u{B0}')]);
        assert_eq!(toks("%%p1"), vec![Char('\u{B1}'), Char('1')]);
        assert_eq!(toks("%%uA%%u"), vec![Underline, Char('A'), Underline]);
        assert_eq!(toks("%%oB%%k"), vec![Overline, Char('B'), Strike]);
        assert_eq!(
            toks("100%%%"),
            vec![Char('1'), Char('0'), Char('0'), Char('%')]
        );
        assert_eq!(toks("%%065"), vec![Code(65)]);
        assert_eq!(toks("%%x"), vec![Char('x')]);
        assert_eq!(toks("50%"), vec![Char('5'), Char('0'), Char('%')]);
        assert_eq!(toks("%%"), vec![Char('%'), Char('%')]);
    }

    #[test]
    fn unicode_escapes() {
        use TextToken::*;
        assert_eq!(toks("\\U+00B0C"), vec![Char('\u{B0}'), Char('C')]);
        assert_eq!(toks("\\u+2205"), vec![Char('\u{2205}')]);
        assert_eq!(toks("\\M+18140"), vec![Code(0x8140)]);
        // broken escapes stay as text
        assert_eq!(
            toks("\\U+12"),
            vec![Char('\\'), Char('U'), Char('+'), Char('1'), Char('2')]
        );
        assert_eq!(toks("é€"), vec![Char('é'), Char('€')]);
    }

    #[test]
    fn affine_compose() {
        let t = Affine {
            a: 2.0,
            b: 0.0,
            c: 0.0,
            d: 3.0,
            e: 1.0,
            f: 1.0,
        };
        let u = Affine {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 5.0,
            f: 0.0,
        };
        let p = t.then_after(&u).apply([1.0, 1.0]);
        assert_eq!(p, t.apply(u.apply([1.0, 1.0])));
    }
}
