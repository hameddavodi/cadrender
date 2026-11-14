//! TEXT special codes (%%c, \U+XXXX) and MTEXT inline codes -> plain lines.
//! [`parse`] keeps the formatting (height, width, oblique, colour, lines, stacks);
//! [`mtext_lines`] is the plain-text fallback.

use super::layout::Glyph;
use super::Face;
use crate::color::EntColor;

fn specials(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    let b: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < b.len() {
        if b[i] == '%' && i + 2 < b.len() && b[i + 1] == '%' {
            match b[i + 2].to_ascii_lowercase() { 'c' => o.push('\u{2300}'), 'd' => o.push('°'), 'p' => o.push('±'), 'u' | 'o' | 'k' => {}, c => o.push(c) }
            i += 3; continue;
        }
        if b[i] == '\\' && i + 6 < b.len() && b[i + 1] == 'U' && b[i + 2] == '+' {
            let h: String = b[i + 3..i + 7].iter().collect();
            if let Some(c) = u32::from_str_radix(&h, 16).ok().and_then(char::from_u32) { o.push(c); i += 7; continue; }
        }
        o.push(b[i]); i += 1;
    }
    o
}
/// Decode DXF caret notation: `^I` tab, `^J` line feed, `^M` carriage return,
/// `^ ` a literal caret, and in general `^X` (X in `@`..`_`) the control character X-64.
/// Any other `^X` stays as it is. DXF writes control characters this way; DWG
/// stores them raw, so only the DXF reader calls this. The layout code below
/// then handles the raw characters for both readers.
pub fn caret_decode(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains('^') { return std::borrow::Cow::Borrowed(s); }
    let mut o = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    // Inside an MTEXT stack (`\S...;`) a caret is the stack divider: `\SA^B;`.
    let mut in_stack = false;
    while let Some(c) = it.next() {
        if c == '\\' {
            o.push(c);
            if let Some(n) = it.next() { in_stack |= n == 'S'; o.push(n); }
            continue;
        }
        if c == ';' { in_stack = false; }
        if c == '^' {
            match it.peek().copied() {
                Some(' ') => { o.push('^'); it.next(); continue; }
                Some(_) if in_stack => {}
                Some(n @ '@'..='_') => { o.push(char::from(n as u8 - 64)); it.next(); continue; }
                _ => {}
            }
        }
        o.push(c);
    }
    std::borrow::Cow::Owned(o)
}

/// TEXT value -> one plain line. A tab becomes a space; other control characters are dropped.
pub fn plain(s: &str) -> String {
    let t = specials(s.trim_end_matches(['\r', '\n']));
    if !t.chars().any(|c| c.is_control()) { return t; }
    t.chars().filter_map(|c| if c == '\t' { Some(' ') } else if c.is_control() { None } else { Some(c) }).collect()
}

/// MTEXT inline codes -> plain lines. Font, height and colour changes are dropped.
pub fn mtext_lines(raw: &str) -> Vec<String> {
    let s = specials(raw);
    let b: Vec<char> = s.chars().collect();
    let mut lines = vec![String::new()];
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == '\\' && i + 1 < b.len() {
            let n = b[i + 1];
            match n {
                'P' | 'n' | 'N' | 'X' => { lines.push(String::new()); i += 2; }
                '~' => { lines.last_mut().unwrap().push(' '); i += 2; }
                '\\' | '{' | '}' => { lines.last_mut().unwrap().push(n); i += 2; }
                'L' | 'l' | 'O' | 'o' | 'K' | 'k' => i += 2,
                'S' => {
                    i += 2;
                    while i < b.len() && b[i] != ';' { lines.last_mut().unwrap().push(if b[i] == '^' || b[i] == '#' { '/' } else { b[i] }); i += 1; }
                    i += 1;
                }
                'f' | 'F' | 'H' | 'W' | 'Q' | 'T' | 'A' | 'C' | 'c' | 'p' => { while i < b.len() && b[i] != ';' { i += 1; } i += 1; }
                _ => { i += 1; }
            }
        } else if c == '{' || c == '}' { i += 1; }
        else if c == '\n' { lines.push(String::new()); i += 1; }
        else if c == '\t' { lines.last_mut().unwrap().push(' '); i += 1; }
        else if c.is_control() { i += 1; }
        else { lines.last_mut().unwrap().push(c); i += 1; }
    }
    lines
}

/// Character format inside an MTEXT. Sizes are relative to the entity's text
/// height and width factor (1 = as the entity); the oblique is absolute (degrees).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fmt {
    pub h: f64,
    pub wf: f64,
    pub oblique: f64,
    /// Tracking: 1 = normal character spacing.
    pub track: f64,
    pub color: Option<EntColor>,
    pub under: bool,
    pub over: bool,
    pub strike: bool,
    pub bold: bool,
    pub italic: bool,
    /// `\fSymbol`: Latin letters stand for Greek ones.
    pub symbol: bool,
    /// `\A`: 0 bottom, 1 middle, 2 top (for mixed heights on one line).
    pub valign: u8,
}

impl Fmt {
    pub fn new(oblique: f64) -> Fmt {
        Fmt { h: 1.0, wf: 1.0, oblique, track: 1.0, color: None, under: false, over: false, strike: false, bold: false, italic: false, symbol: false, valign: 0 }
    }
}

/// One item of a parsed MTEXT.
#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    Ch(char, Fmt),
    /// A breakable space.
    Space(Fmt),
    Tab(Fmt),
    /// `\S`: stacked text; kind '/' (bar), '#' (slash), '^' (tolerance, no bar).
    Stack { top: String, bot: String, kind: char, fmt: Fmt },
    /// End of paragraph (`\P`, column break `\N`).
    Para,
    /// `\pxq?`: alignment of the current paragraph: 0 left, 1 centre, 2 right, 3 justified.
    PAlign(u8),
}

/// `\fSymbol` font: Latin letters -> Greek.
fn greek(c: char) -> char {
    const LO: &str = "αβχδεφγηιϕκλμνοπθρστυϖωξψζ";
    const UP: &str = "ΑΒΧΔΕΦΓΗΙϑΚΛΜΝΟΠΘΡΣΤΥςΩΞΨΖ";
    match c {
        'a'..='z' => LO.chars().nth(c as usize - 'a' as usize).unwrap_or(c),
        'A'..='Z' => UP.chars().nth(c as usize - 'A' as usize).unwrap_or(c),
        _ => c,
    }
}

/// Number at the start of `s` (ignores a trailing 'x').
fn num(s: &str) -> Option<f64> { s.trim().trim_end_matches(['x', 'X']).parse().ok().filter(|v: &f64| v.is_finite()) }

/// Parse MTEXT inline codes. `base_h` is the entity text height (for absolute `\H`),
/// `base_wf` its width factor (for absolute `\W`), `oblique` its style oblique angle.
pub fn parse(raw: &str, base_h: f64, base_wf: f64, oblique: f64) -> Vec<Tok> {
    let s = specials(raw);
    let b: Vec<char> = s.chars().collect();
    let mut out = Vec::with_capacity(b.len());
    let mut stack: Vec<Fmt> = vec![];
    let mut f = Fmt::new(oblique);
    let mut i = 0;
    // the value of a code: up to ';' (or the end)
    let arg = |i: &mut usize| -> String {
        let st = *i;
        while *i < b.len() && b[*i] != ';' { *i += 1; }
        let v: String = b[st..*i].iter().collect();
        *i += 1;
        v
    };
    while i < b.len() {
        let c = b[i];
        if c == '\\' && i + 1 < b.len() {
            let n = b[i + 1];
            i += 2;
            match n {
                'P' | 'n' | 'N' | 'X' => out.push(Tok::Para),
                '~' => out.push(Tok::Ch('\u{a0}', f)),
                '\\' | '{' | '}' => out.push(Tok::Ch(n, f)),
                'L' => f.under = true, 'l' => f.under = false,
                'O' => f.over = true, 'o' => f.over = false,
                'K' => f.strike = true, 'k' => f.strike = false,
                'H' => { let v = arg(&mut i); if let Some(x) = num(&v).filter(|x| *x > 0.0) { f.h = if v.trim().ends_with(['x', 'X']) { f.h * x } else { x / base_h.max(1e-12) }; } }
                'W' => { let v = arg(&mut i); if let Some(x) = num(&v).filter(|x| *x > 0.0) { f.wf = if v.trim().ends_with(['x', 'X']) { f.wf * x } else { x / base_wf.max(1e-12) }; } }
                'Q' => { if let Some(x) = num(&arg(&mut i)) { f.oblique = x.clamp(-85.0, 85.0); } }
                'T' => { if let Some(x) = num(&arg(&mut i)) { f.track = x.clamp(0.75, 4.0); } }
                'A' => { if let Some(x) = num(&arg(&mut i)) { f.valign = (x as i32).clamp(0, 2) as u8; } }
                'C' => { if let Some(x) = num(&arg(&mut i)) { f.color = Some(EntColor::aci(x as i32)); } }
                'c' => { if let Some(x) = num(&arg(&mut i)) { let v = x as u32; f.color = Some(EntColor::true_color(((v & 0xff) << 16) | (v & 0xff00) | ((v >> 16) & 0xff))); } }
                'f' | 'F' => {
                    let v = arg(&mut i);
                    let mut parts = v.split('|');
                    let name = parts.next().unwrap_or("").to_ascii_lowercase();
                    f.symbol = name.trim_end_matches(".ttf") == "symbol";
                    for p in parts { match p.as_bytes() { [b'b', d, ..] => f.bold = *d == b'1', [b'i', d, ..] => f.italic = *d == b'1', _ => {} } }
                }
                'p' => { let v = arg(&mut i); for p in v.split(',') { if let Some(q) = p.split_once('q').map(|x| x.1) { out.push(Tok::PAlign(match q.chars().next() { Some('c') => 1, Some('r') => 2, Some('j') => 3, _ => 0 })); } } }
                'S' => {
                    let v = arg(&mut i);
                    let (k, pos) = match v.char_indices().find(|(j, ch)| matches!(ch, '/' | '#' | '^') && (*j == 0 || v.as_bytes()[j - 1] != b'\\')) { Some((j, ch)) => (ch, j), None => ('/', v.len()) };
                    let clean = |t: &str| t.replace("\\/", "/").replace("\\#", "#").replace("\\^", "^");
                    let (top, bot) = (clean(&v[..pos]), clean(v.get(pos + 1..).unwrap_or("")));
                    out.push(Tok::Stack { top, bot, kind: k, fmt: f });
                }
                _ => {}
            }
        } else if c == '{' { stack.push(f); i += 1; }
        else if c == '}' { if let Some(p) = stack.pop() { f = p; } i += 1; }
        else if c == '\n' { out.push(Tok::Para); i += 1; }
        else if c == ' ' { out.push(Tok::Space(f)); i += 1; }
        else if c == '\t' { out.push(Tok::Tab(f)); i += 1; }
        else if c.is_control() { i += 1; }
        else { out.push(Tok::Ch(if f.symbol { greek(c) } else { c }, f)); i += 1; }
    }
    out
}

// ---- formatted layout ----------------------------------------------------

/// Glyphs `[previous end, end)` drawn with this transform (font units, applied
/// before the entity matrix) and colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GRun { pub end: u32, pub sx: f32, pub sy: f32, pub k: f32, pub color: Option<EntColor>, pub bold: bool }

/// A filled rectangle in font units: underline, overline, strike-through, stack bar.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Deco { pub x0: f32, pub y0: f32, pub x1: f32, pub y1: f32, pub color: Option<EntColor> }

/// Formatting that plain glyph positions cannot carry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rich { pub runs: Vec<GRun>, pub deco: Vec<Deco> }

/// Result of [`layout_rich`]. `top` = cap height of the first line above the
/// first baseline (y = 0); `block_h` = from that top to the last baseline.
#[derive(Clone, Debug, Default)]
pub struct RichLaid { pub glyphs: Vec<Glyph>, pub rich: Option<Box<Rich>>, pub width: f64, pub col: f64, /** x range of the lines inside the column */ pub ink_x: (f64, f64), pub top: f64, pub block_h: f64, pub lines: usize }

/// Part of a line before its baseline is known.
struct Pending { id: u32, x: f64, dy: f64, h: f64, valign: u8, run: GRun }
struct PendDeco { x0: f64, x1: f64, dy0: f64, dy1: f64, h: f64, valign: u8, color: Option<EntColor> }

struct Lines<'a> {
    face: &'a mut Face,
    cap: f64,
    base_wf: f64,
    base_obl: f64,
    line_factor: f64,
    out: RichLaid,
    runs: Vec<GRun>,
    deco: Vec<Deco>,
    cur: Vec<Pending>,
    cur_deco: Vec<PendDeco>,
    /// Line heights (factor) and widths, and where each line's glyphs/decos start.
    line_h: f64,
    x: f64,
    baseline: f64,
    prev_h: f64,
    /// (first glyph, first deco, width, paragraph alignment) per finished line.
    done: Vec<(usize, usize, f64, u8)>,
    align: u8,
}

impl Lines<'_> {
    fn run_of(&self, f: &Fmt, scale: f64) -> GRun {
        let (sx, sy) = (f.h * f.wf * scale, f.h * scale);
        let q = f.oblique + if f.italic && f.oblique.abs() < 1e-9 { 12.0 } else { 0.0 };
        let k = (q.to_radians().tan() - self.base_obl.to_radians().tan()) * sy / self.base_wf.max(1e-6);
        GRun { end: 0, sx: sx as f32, sy: sy as f32, k: k as f32, color: f.color, bold: f.bold }
    }
    /// Width of a character in font units with this format.
    fn adv(&mut self, c: char, f: &Fmt, scale: f64) -> f64 {
        if c == '\u{a0}' { return self.face.space * f.h * f.wf * scale; }
        self.face.glyph(c).map_or(0.0, |(_, a)| a) * f.h * f.wf * scale * f.track
    }
    /// Place a string at (x, dy) relative to the baseline; returns its width.
    fn put(&mut self, s: &str, f: &Fmt, scale: f64, x: f64, dy: f64) -> f64 {
        let run = self.run_of(f, scale);
        let mut w = 0.0;
        for c in s.chars() {
            let a = self.adv(c, f, scale);
            if c != '\u{a0}' { if let Some((id, _)) = self.face.glyph(c) { self.cur.push(Pending { id, x: x + w, dy, h: f.h, valign: f.valign, run }); } }
            w += a;
        }
        self.line_h = self.line_h.max(f.h);
        w
    }
    /// Underline / overline / strike for [x0, x1] in format f.
    fn lines_for(&mut self, f: &Fmt, x0: f64, x1: f64) {
        let th = 0.06 * self.cap * f.h;
        for (on, y) in [(f.under, -0.2), (f.over, 1.25), (f.strike, 0.45)] {
            if on { self.cur_deco.push(PendDeco { x0, x1, dy0: y * self.cap * f.h - th / 2.0, dy1: y * self.cap * f.h + th / 2.0, h: f.h, valign: f.valign, color: f.color }); }
        }
    }
    /// Width of one token (spaces and tabs at the current x).
    fn measure(&mut self, t: &Tok) -> f64 {
        match t {
            Tok::Ch(c, f) => self.adv(*c, f, 1.0),
            Tok::Space(f) => self.face.space * f.h * f.wf * f.track,
            Tok::Tab(f) => { let step = 4.0 * self.cap * f.h; ((self.x / step).floor() + 1.0) * step - self.x }
            Tok::Stack { top, bot, kind, fmt } => {
                let (wt, wb): (f64, f64) = (top.chars().map(|c| self.adv(c, fmt, 0.7)).sum(), bot.chars().map(|c| self.adv(c, fmt, 0.7)).sum());
                if *kind == '#' { wt + wb + self.adv('/', fmt, 1.0) } else { wt.max(wb) + 0.1 * self.cap * fmt.h }
            }
            _ => 0.0,
        }
    }
    /// Place one token at the current x.
    fn place(&mut self, t: &Tok) {
        let x0 = self.x;
        let w = self.measure(t);
        let cap = self.cap;
        match t {
            Tok::Ch(c, f) => { let mut b = [0u8; 4]; self.put(c.encode_utf8(&mut b), f, 1.0, x0, 0.0); self.lines_for(f, x0, x0 + w); }
            Tok::Space(f) | Tok::Tab(f) => { self.line_h = self.line_h.max(f.h); self.lines_for(f, x0, x0 + w); }
            Tok::Stack { top, bot, kind, fmt: f } => {
                let h = cap * f.h;
                if *kind == '#' {
                    let wt = self.put(top, f, 0.7, x0, 0.45 * h);
                    let ws = self.put("/", f, 1.0, x0 + wt, 0.0);
                    self.put(bot, f, 0.7, x0 + wt + ws, 0.0);
                } else {
                    let wt: f64 = top.chars().map(|c| self.adv(c, f, 0.7)).sum();
                    let wb: f64 = bot.chars().map(|c| self.adv(c, f, 0.7)).sum();
                    self.put(top, f, 0.7, x0 + (w - wt) / 2.0, 0.55 * h);
                    self.put(bot, f, 0.7, x0 + (w - wb) / 2.0, -0.45 * h);
                    if *kind == '/' { self.cur_deco.push(PendDeco { x0, x1: x0 + w, dy0: 0.4 * h - 0.03 * h, dy1: 0.4 * h + 0.03 * h, h: f.h, valign: f.valign, color: f.color }); }
                }
                self.lines_for(f, x0, x0 + w);
            }
            _ => {}
        }
        self.x += w;
    }
    /// Finish the current line: fix its baseline, move glyphs and decorations out.
    fn end_line(&mut self, width: f64, cur_h: f64) {
        let hh = if self.line_h > 0.0 { self.line_h } else { cur_h };
        if self.done.is_empty() { self.out.top = self.cap * hh; } else { self.baseline -= self.line_factor * self.cap * (hh + self.prev_h) / 2.0; }
        let cap = self.cap;
        let shift = |h: f64, va: u8| (hh - h) * cap * match va { 1 => 0.5, 2 => 1.0, _ => 0.0 };
        self.done.push((self.out.glyphs.len(), self.deco.len(), width, self.align));
        for p in std::mem::take(&mut self.cur) {
            self.out.glyphs.push(Glyph { id: p.id, x: p.x as f32, y: (self.baseline + p.dy + shift(p.h, p.valign)) as f32 });
            let end = self.out.glyphs.len() as u32;
            match self.runs.last_mut() {
                Some(r) if GRun { end: r.end, ..p.run } == *r => r.end = end,
                _ => self.runs.push(GRun { end, ..p.run }),
            }
        }
        for d in std::mem::take(&mut self.cur_deco) {
            let s = self.baseline + shift(d.h, d.valign);
            self.deco.push(Deco { x0: d.x0 as f32, x1: d.x1 as f32, y0: (s + d.dy0) as f32, y1: (s + d.dy1) as f32, color: d.color });
        }
        self.prev_h = hh;
        self.line_h = 0.0;
        self.x = 0.0;
    }
}

/// Lay out a parsed MTEXT in font units: word wrap at `wrap` (0 = none),
/// mixed heights, stacks, paragraph alignment. `base_wf`/`base_obl` are the
/// width factor and oblique already in the entity matrix.
pub fn layout_rich(face: &mut Face, toks: &[Tok], wrap: f64, line_factor: f64, base_wf: f64, base_obl: f64, default_align: u8) -> RichLaid {
    let cap = face.cap_height;
    let mut l = Lines { face, cap, base_wf, base_obl, line_factor, out: RichLaid::default(), runs: vec![], deco: vec![], cur: vec![], cur_deco: vec![],
        line_h: 0.0, x: 0.0, baseline: 0.0, prev_h: 1.0, done: vec![], align: default_align };
    let mut cur_h = 1.0;
    let mut content_x = 0.0; // x after the last non-space item of the line
    for para in toks.split(|t| *t == Tok::Para) {
        // paragraph alignment carries over to the next paragraphs
        let mut k = 0;
        while k < para.len() {
            // spaces, then a word
            let s0 = k;
            while k < para.len() && matches!(para[k], Tok::Space(_) | Tok::Tab(_) | Tok::PAlign(_)) { if let Tok::PAlign(a) = para[k] { l.align = a; } k += 1; }
            let w0 = k;
            while k < para.len() && !matches!(para[k], Tok::Space(_) | Tok::Tab(_) | Tok::PAlign(_)) { k += 1; }
            let x_save = l.x;
            let sw: f64 = para[s0..w0].iter().map(|t| { let w = l.measure(t); l.x += w; w }).sum();
            let ww: f64 = para[w0..k].iter().map(|t| l.measure(t)).sum();
            l.x = x_save;
            if wrap > 0.0 && content_x > 0.0 && l.x + sw + ww > wrap * 1.0001 && w0 < k {
                l.end_line(content_x, cur_h);
                content_x = 0.0;
            } else {
                for t in &para[s0..w0] { l.place(t); }
            }
            for t in &para[w0..k] { if let Tok::Ch(_, f) | Tok::Stack { fmt: f, .. } = t { cur_h = f.h; } l.place(t); }
            if w0 < k { content_x = l.x; }
        }
        l.end_line(content_x, cur_h);
        content_x = 0.0;
    }
    finish(l, wrap)
}

/// Paragraph alignment, width, and the fast path when nothing is formatted.
fn finish(mut l: Lines, wrap: f64) -> RichLaid {
    let width = l.done.iter().map(|d| d.2).fold(0.0, f64::max);
    let col = if wrap > 0.0 { wrap.max(width) } else { width };
    let n = l.done.len();
    let (mut x0, mut x1) = (f64::MAX, 0.0f64);
    for i in 0..n {
        let (g0, d0, w, a) = l.done[i];
        let dx = match a { 1 => (col - w) / 2.0, 2 => col - w, _ => 0.0 };
        x0 = x0.min(dx);
        x1 = x1.max(dx + w);
        let dx = dx as f32;
        if dx == 0.0 { continue; }
        let (g1, d1) = if i + 1 < n { (l.done[i + 1].0, l.done[i + 1].1) } else { (l.out.glyphs.len(), l.deco.len()) };
        for g in &mut l.out.glyphs[g0..g1] { g.x += dx; }
        for d in &mut l.deco[d0..d1] { d.x0 += dx; d.x1 += dx; }
    }
    l.out.width = width;
    l.out.col = col;
    l.out.ink_x = if n > 0 { (x0.min(x1), x1) } else { (0.0, 0.0) };
    l.out.lines = n;
    l.out.block_h = l.out.top - l.baseline;
    let plain = GRun { end: l.out.glyphs.len() as u32, sx: 1.0, sy: 1.0, k: 0.0, color: None, bold: false };
    let simple = l.deco.is_empty() && (l.runs.is_empty() || (l.runs.len() == 1 && l.runs[0] == plain));
    if !simple { l.out.rich = Some(Box::new(Rich { runs: l.runs, deco: l.deco })); }
    l.out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn specials_and_unicode() {
        assert_eq!(plain("%%c50 %%d %%p1\r\n"), "\u{2300}50 ° ±1");
        assert_eq!(plain("A\\U+00E8B"), "AèB");
    }
    #[test]
    fn caret_escapes() {
        assert_eq!(caret_decode("A^IB^JC^MD"), "A\tB\nC\rD");
        assert_eq!(caret_decode("x^ y"), "x^y");
        assert_eq!(caret_decode("^@^_"), "\u{0}\u{1f}");
        // Not caret notation: stays as it is.
        assert_eq!(caret_decode("2^3 ^a ^"), "2^3 ^a ^");
        assert!(matches!(caret_decode("no caret"), std::borrow::Cow::Borrowed(_)));
        // MTEXT stack divider is kept, escapes after the stack are decoded.
        assert_eq!(caret_decode("\\SA^B;^J\\\\^I"), "\\SA^B;\n\\\\\t");
        // Decoded control characters in layout.
        assert_eq!(plain("A\tB\u{7}C\r\n"), "A BC");
        assert_eq!(mtext_lines("A\tB\nC\rD"), vec!["A B", "CD"]);
        let toks = parse("A\tB\nC\rD", 1.0, 1.0, 0.0);
        assert!(matches!(toks[1], Tok::Tab(_)) && matches!(toks[3], Tok::Para));
        assert_eq!(toks.len(), 6);
    }
    #[test]
    fn mtext_codes() {
        assert_eq!(mtext_lines("{\\fArial|b1;Hello}\\PWorld"), vec!["Hello", "World"]);
        assert_eq!(mtext_lines("\\A1;1\\S1/2;"), vec!["11/2"]);
        assert_eq!(mtext_lines("a\\~b\\\\c"), vec!["a b\\c"]);
    }

    fn chars(t: &[Tok]) -> String { t.iter().filter_map(|t| if let Tok::Ch(c, _) = t { Some(*c) } else { None }).collect() }
    fn fmt_of(t: &[Tok], ch: char) -> Fmt { t.iter().find_map(|t| match t { Tok::Ch(c, f) if *c == ch => Some(*f), _ => None }).unwrap() }

    #[test]
    fn parse_formats() {
        let t = parse("a{\\H5;b}\\H2x;c\\W0.5;d\\Q15;e\\C1;f\\c255;g", 2.5, 1.0, 0.0);
        assert_eq!(chars(&t), "abcdefg");
        assert_eq!(fmt_of(&t, 'a'), Fmt::new(0.0));
        assert_eq!(fmt_of(&t, 'b').h, 2.0); // absolute 5 / base 2.5
        assert_eq!(fmt_of(&t, 'c').h, 2.0); // braces restored 1.0, then 2x
        assert_eq!(fmt_of(&t, 'd').wf, 0.5);
        assert_eq!(fmt_of(&t, 'e').oblique, 15.0);
        assert_eq!(fmt_of(&t, 'f').color, Some(EntColor::aci(1)));
        // \c is 0xBBGGRR: 255 = pure red
        assert_eq!(fmt_of(&t, 'g').color, Some(EntColor::true_color(0xff0000)));
    }

    #[test]
    fn parse_lines_stacks_symbol_paragraphs() {
        let t = parse("\\Lu\\lx\\Oo\\Kk\\pxqc;\\S1/2;\\S+0.1^-0.2;{\\fSymbol;abp}\\Pnext", 1.0, 1.0, 0.0);
        assert!(fmt_of(&t, 'u').under && !fmt_of(&t, 'x').under);
        assert!(fmt_of(&t, 'o').over && fmt_of(&t, 'k').strike);
        assert!(t.contains(&Tok::PAlign(1)));
        assert!(t.iter().any(|x| matches!(x, Tok::Stack { top, bot, kind: '/', .. } if top == "1" && bot == "2")));
        assert!(t.iter().any(|x| matches!(x, Tok::Stack { top, bot, kind: '^', .. } if top == "+0.1" && bot == "-0.2")));
        assert!(chars(&t).contains("αβπ"));
        assert_eq!(t.iter().filter(|x| **x == Tok::Para).count(), 1);
        // bold / italic flags from \f
        let t = parse("{\\fArial|b1|i1|c0;B}", 1.0, 1.0, 0.0);
        assert!(fmt_of(&t, 'B').bold && fmt_of(&t, 'B').italic);
    }

    /// Every glyph is a 600 x 700 box, space 300.
    struct Boxes;
    impl crate::text::GlyphSource for Boxes {
        fn cap_height(&self) -> f64 { 700.0 }
        fn space_advance(&self) -> f64 { 300.0 }
        fn glyph(&self, _: char) -> Option<(crate::geom::PathData, f64)> {
            let mut p = crate::geom::PathData::default();
            p.move_to(0.0, 0.0); p.line_to(600.0, 0.0); p.line_to(600.0, 700.0); p.close();
            Some((p, 600.0))
        }
    }

    fn lay(s: &str, wrap: f64, align: u8) -> RichLaid {
        let mut f = Face::new("boxes", Box::new(Boxes));
        layout_rich(&mut f, &parse(s, 1.0, 1.0, 0.0), wrap, 5.0 / 3.0, 1.0, 0.0, align)
    }

    #[test]
    fn plain_text_takes_the_fast_path() {
        let l = lay("ab cd\\Pe", 0.0, 0);
        assert!(l.rich.is_none());
        assert_eq!((l.glyphs.len(), l.lines, l.width), (5, 2, 2700.0));
        assert_eq!(l.top, 700.0);
        assert!((l.block_h - (700.0 + 700.0 * 5.0 / 3.0)).abs() < 1e-9);
    }

    #[test]
    fn wrap_heights_and_alignment() {
        // wrap at 1500: "ab" / "cd"
        let l = lay("ab cd", 1500.0, 0);
        assert_eq!(l.lines, 2);
        assert_eq!((l.glyphs[2].x, l.col), (0.0, 1500.0));
        // centred: the short second line moves to the middle of the widest
        let l = lay("abc\\Pa", 0.0, 1);
        assert_eq!(l.glyphs[3].x, 600.0);
        // a double-height word makes its line taller and gets its own run
        let l = lay("a\\H2x;b", 0.0, 0);
        assert_eq!(l.top, 1400.0);
        let r = l.rich.unwrap();
        assert_eq!(r.runs.len(), 2);
        assert_eq!((r.runs[1].sx, r.runs[1].sy), (2.0, 2.0));
        assert_eq!(l.width, 600.0 + 1200.0);
    }

    #[test]
    fn stack_and_underline_make_decorations() {
        let l = lay("\\S1/2;", 0.0, 0);
        let r = l.rich.unwrap();
        assert_eq!(r.deco.len(), 1); // the fraction bar
        assert_eq!(l.glyphs.len(), 2);
        assert!(l.glyphs[0].y > 0.0 && l.glyphs[1].y < 0.0, "{:?}", l.glyphs);
        let l = lay("\\Lab", 0.0, 0);
        let d = l.rich.unwrap().deco;
        assert_eq!(d.len(), 2);
        assert!(d[0].y1 < 0.0);
    }
}
