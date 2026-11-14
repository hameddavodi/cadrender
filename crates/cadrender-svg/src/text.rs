//! Real `<text>` elements: one per line of a laid-out text, so the text
//! stays selectable and searchable in a browser or PDF viewer.
//!
//! The walk already placed every glyph (alignment, wrapping, line spacing).
//! We write each line at the position of its first glyph, with a matrix for
//! rotation, width factor, oblique angle and mirroring, and with
//! `textLength`, so the line has the same width as in the PNG even if the
//! viewer uses a slightly different font.
//!
//! Formatted MTEXT: runs that share one size, width factor and oblique
//! share the `<text>` of a line; a colour or bold change is a `<tspan>`.
//! A run with another transform gets its own `<text>` with its own matrix.
//! Stacked fractions are plain glyphs at their places (smaller texts above
//! and below); underline, overline, strike-through and fraction bars are
//! filled boxes, exactly where the PNG draws them. Bold is `font-weight`
//! (the PNG strokes the outline instead), italic is the oblique matrix.

use crate::num::write_num;
use cadrender_core::geom::Aff;
use cadrender_core::sink::{RichText, TextRun};
use cadrender_core::text::layout::Glyph;
use cadrender_core::text::Face;

/// Cap height of Arial as a part of its em (1467 / 2048). The SVG asks for
/// Arial first, so the font size is chosen to give Arial the right cap height.
pub const ARIAL_CAP: f64 = 1467.0 / 2048.0;

/// The CSS rule for all texts (font policy: one sans font).
pub const TEXT_RULE: &[u8] = b"text{font-family:Arial,\"Liberation Sans\",sans-serif;white-space:pre}\n";

fn escape(out: &mut Vec<u8>, ch: char) {
    match ch {
        '&' => out.extend_from_slice(b"&amp;"),
        '<' => out.extend_from_slice(b"&lt;"),
        '>' => out.extend_from_slice(b"&gt;"),
        // not allowed in XML 1.0
        c if (c as u32) < 0x20 || c == '\u{fffe}' || c == '\u{ffff}' => out.push(b' '),
        c => { let mut b = [0u8; 4]; out.extend_from_slice(c.encode_utf8(&mut b).as_bytes()); }
    }
}

/// How a piece of text looks: its CSS class (colour) and weight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Look<'a> { pub class: &'a [u8], pub bold: bool }

/// Characters written as one unit (a `<text>` or a positioned `<tspan>`).
struct Piece {
    look: u16,
    /// Start in font units: the first glyph, or before it when the piece
    /// starts with `lead` spaces (a new look right after a space).
    x: f64,
    lead: usize,
    /// (x, advance, character) in font units.
    chars: Vec<(f64, f64, char)>,
}

impl Piece {
    /// `textLength` of a piece of 2 or more characters (keeps the PNG width
    /// even if the viewer's font is a little different).
    fn text_length(&self, out: &mut Vec<u8>, k: f64, dec: u8) {
        let Some(last) = self.chars.last() else { return };
        let w = last.0 + last.1 - self.x;
        if self.lead + self.chars.len() > 1 && w > 0.0 {
            out.extend_from_slice(b" textLength=\"");
            write_num(out, w / k, dec);
            out.extend_from_slice(b"\" lengthAdjust=\"spacingAndGlyphs\"");
        }
    }
}

/// Cut one line into pieces. A tab, or a gap that is not a whole number of
/// spaces, starts a new piece; so does a new look (the spaces before it go
/// into the new piece, so copied text keeps them).
fn pieces(face: &Face, line: &[Glyph], looks: &[u16], space: f64) -> Vec<Piece> {
    let mut out: Vec<Piece> = vec![];
    let mut prev_end: Option<f64> = None;
    for (n, g) in line.iter().enumerate() {
        let ch = face.chars[g.id as usize];
        let (gx, ga) = (g.x as f64, face.glyphs[g.id as usize].advance);
        let look = looks.get(n).copied().unwrap_or(0);
        if (ch as u32) < 0x20 { prev_end = None; continue; }
        // number of spaces in the gap since the last glyph, if it is a whole number
        let whole = prev_end.and_then(|e| { let s = ((gx - e) / space).round(); (s >= 0.0 && (gx - e - s * space).abs() <= 0.25 * space).then_some(s) });
        match (whole, out.last_mut()) {
            (Some(_), Some(p)) if p.look == look => p.chars.push((gx, ga, ch)),
            (Some(s), Some(_)) => out.push(Piece { look, x: gx - s * space, lead: s.min(1000.0) as usize, chars: vec![(gx, ga, ch)] }),
            _ => out.push(Piece { look, x: gx, lead: 0, chars: vec![(gx, ga, ch)] }),
        }
        prev_end = Some(gx + ga);
    }
    out
}

/// Look attributes of a `<tspan>` that differ from its `<text>`.
fn look_attrs(out: &mut Vec<u8>, l: Look<'_>, outer: Look<'_>) {
    if l.class != outer.class { out.extend_from_slice(b" class=\""); out.extend_from_slice(l.class); out.push(b'"'); }
    if l.bold != outer.bold { out.extend_from_slice(if l.bold { b" font-weight=\"bold\"" } else { b" font-weight=\"normal\"" }); }
}

/// `true` if every glyph of `glyphs` has a known character in `face`.
pub fn known(face: &Face, glyphs: &[Glyph]) -> bool {
    glyphs.iter().all(|g| (g.id as usize) < face.chars.len() && (g.id as usize) < face.glyphs.len())
}

/// Write `run` as `<text>` elements, one per line. Glyph `i` looks like
/// `looks[look_of[i]]` (an empty `look_of` = all `looks[0]`); a change of
/// look inside a line becomes a `<tspan>`. Returns `false` (and writes
/// nothing) when a glyph has no known character.
pub fn write_run(out: &mut Vec<u8>, run: &TextRun<'_>, dec: u8, looks: &[Look<'_>], look_of: &[u16]) -> bool {
    let face = run.face;
    if !known(face, run.glyphs) { return false; }
    if run.glyphs.is_empty() || face.cap_height <= 0.0 { return true; }
    let em_fu = face.cap_height / ARIAL_CAP;
    let [a, b, c, d, _, _] = run.m.0;
    // the y axis sets the font size; the matrix keeps width factor and oblique
    let sc = c.hypot(d);
    if !(sc > 0.0 && sc.is_finite()) { return true; }
    // font size in pixels, rounded; the matrix takes the rest
    let fs = ((em_fu * sc * 100.0).round() / 100.0).max(0.01);
    let k = em_fu / fs;
    // font units are y up, SVG text is y down: flip y
    let l = [a * k, b * k, -c * k, -d * k];
    let plain = (l[0] - 1.0).abs() < 1e-3 && l[1].abs() < 1e-3 && l[2].abs() < 1e-3 && (l[3] - 1.0).abs() < 1e-3;
    let space = face.space.max(1e-9);

    let mut i = 0;
    while i < run.glyphs.len() {
        let y = run.glyphs[i].y;
        let mut j = i;
        while j < run.glyphs.len() && run.glyphs[j].y == y { j += 1; }
        let pieces = pieces(face, &run.glyphs[i..j], look_of.get(i..j).unwrap_or(&[]), space);
        i = j;
        let Some(first) = pieces.first() else { continue };
        let look = |p: &Piece| looks.get(p.look as usize).or(looks.first()).copied().unwrap_or(Look { class: b"", bold: false });
        let outer = look(first);
        let x0 = first.x;
        let (ex, ey) = run.m.apply(x0, y as f64);
        out.extend_from_slice(b"<text class=\"");
        out.extend_from_slice(outer.class);
        if plain {
            out.extend_from_slice(b"\" x=\"");
            write_num(out, ex, dec);
            out.extend_from_slice(b"\" y=\"");
            write_num(out, ey, dec);
        } else {
            out.extend_from_slice(b"\" transform=\"matrix(");
            for (n, v) in l.iter().enumerate() { if n > 0 { out.push(b' '); } write_num(out, *v, 4); }
            out.push(b' ');
            write_num(out, ex, dec);
            out.push(b' ');
            write_num(out, ey, dec);
            out.push(b')');
        }
        out.extend_from_slice(b"\" font-size=\"");
        write_num(out, fs, 2);
        out.push(b'"');
        if outer.bold { out.extend_from_slice(b" font-weight=\"bold\""); }
        let one = pieces.len() == 1;
        if one { first.text_length(out, k, dec); }
        out.push(b'>');
        for piece in &pieces {
            if !one {
                out.extend_from_slice(b"<tspan x=\"");
                write_num(out, if plain { ex } else { 0.0 } + (piece.x - x0) / k, dec);
                out.push(b'"');
                look_attrs(out, look(piece), outer);
                piece.text_length(out, k, dec);
                out.push(b'>');
            }
            for _ in 0..piece.lead { out.push(b' '); }
            let mut end: Option<f64> = None;
            for &(gx, ga, ch) in &piece.chars {
                if let Some(e) = end { for _ in 0..(((gx - e) / space).round().clamp(0.0, 1000.0) as usize) { out.push(b' '); } }
                escape(out, ch);
                end = Some(gx + ga);
            }
            if !one { out.extend_from_slice(b"</tspan>"); }
        }
        out.extend_from_slice(b"</text>\n");
    }
    true
}

/// The end of the group of runs starting at run `i` that share one
/// transform (size, width factor, oblique): they differ only in colour or
/// weight, so they can share `<text>` elements.
pub fn group_end(rich: &RichText<'_>, i: usize) -> usize {
    let runs = &rich.rich.runs;
    let same = |a: usize| (runs[a].sx, runs[a].sy, runs[a].k) == (runs[i].sx, runs[i].sy, runs[i].k);
    (i + 1..runs.len()).find(|&a| !same(a)).unwrap_or(runs.len())
}

/// Glyphs of runs that share run `i`'s transform, as a plain run. A glyph
/// sits at `m * translate(g.x, g.y) * [sx 0 k sy]`; we move the run
/// transform into the matrix and give back the glyph positions in the
/// run's own font units, so [`write_run`] can write the run with an exact
/// size, width factor and oblique angle.
pub fn rich_part(run: &TextRun<'_>, rich: &RichText<'_>, i: usize, glyphs: &[Glyph], out: &mut Vec<Glyph>) -> Aff {
    let r = rich.rich.runs[i];
    let (sx, sy, k) = (r.sx as f64, r.sy as f64, r.k as f64);
    out.clear();
    if sx.abs() < 1e-12 || sy.abs() < 1e-12 { return run.m; }
    for g in glyphs {
        let y = g.y as f64 / sy;
        out.push(Glyph { id: g.id, x: ((g.x as f64 - k * y) / sx) as f32, y: y as f32 });
    }
    run.m.mul(&Aff([sx, 0.0, k, sy, 0.0, 0.0]))
}
