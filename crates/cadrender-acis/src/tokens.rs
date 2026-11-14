//! Records of a SAT (text) or SAB (binary) file as one token list each.
//!
//! Both formats hold the same records: a type name ("edge",
//! "ellipse-curve"), then values. SAT writes them as words ending with `#`;
//! SAB as tagged binary values. Here both become [`Tok`]s, so the edge code
//! does not care which format it reads. A SAB vector (3 doubles) becomes 3
//! `Num`s, like the 3 numbers SAT writes.

/// One value of a record.
#[derive(Clone, Debug, PartialEq)]
pub enum Tok {
    /// Index of another record (`$12` in SAT); -1 = none.
    Ptr(i64),
    Num(f64),
    /// SAB true/false. In SAT the same values are words ("reversed", "F").
    Bool(bool),
    /// A word or string.
    Str(String),
}

/// One record: type name and values (the first value is the attribute pointer).
#[derive(Clone, Debug, Default)]
pub struct Record {
    pub name: String,
    pub toks: Vec<Tok>,
}

/// A whole file: ACIS version number and records (index = pointer value).
#[derive(Clone, Debug, Default)]
pub struct File {
    pub version: i64,
    pub records: Vec<Record>,
}

const END_MARKERS: [&str; 3] = ["End-of-ACIS-data", "End-of-ASM-data", "Begin-of-ACIS-History-data"];

/// SAB: "ACIS BinaryFile" or "ASM BinaryFile4", 4 ints, 3 strings, 3 doubles, records.
pub fn parse_sab(b: &[u8]) -> Option<File> {
    let sig = [&b"ACIS BinaryFile"[..], &b"ASM BinaryFile4"[..]].into_iter().find(|s| b.starts_with(s))?;
    let mut r = Bin { b, p: sig.len() };
    let version = r.i32()? as i64;
    for _ in 0..3 { r.i32()?; }
    // product, ACIS version, date; units and two tolerances
    for _ in 0..3 { if r.u8()? != 7 { return None; } let n = r.u8()? as usize; r.take(n)?; }
    for _ in 0..3 { if r.u8()? != 6 { return None; } r.take(8)?; }
    let mut f = File { version, records: vec![] };
    let mut rec = Record::default();
    let mut name_parts: Vec<String> = vec![];
    while r.p < b.len() {
        let tag = r.u8()?;
        match tag {
            4 | 0x15 => rec.toks.push(Tok::Num(r.i32()? as f64)),
            6 | 0x17 => rec.toks.push(Tok::Num(r.f64()?)),
            7 => { let n = r.u8()? as usize; rec.toks.push(Tok::Str(r.str(n)?)); }
            0x12 => { let n = r.i32()?.max(0) as usize; rec.toks.push(Tok::Str(r.str(n)?)); }
            0x0C => rec.toks.push(Tok::Ptr(r.i32()? as i64)),
            0x0A => rec.toks.push(Tok::Bool(true)),
            0x0B => rec.toks.push(Tok::Bool(false)),
            0x0E => { let n = r.u8()? as usize; name_parts.push(r.str(n)?); }
            0x0D => {
                let n = r.u8()? as usize;
                name_parts.push(r.str(n)?);
                let name = name_parts.join("-");
                name_parts.clear();
                if rec.name.is_empty() {
                    if END_MARKERS.contains(&name.as_str()) { break; }
                    rec.name = name;
                } else {
                    // a type name inside the values (sub-types): keep as a word
                    rec.toks.push(Tok::Str(name));
                }
            }
            0x13 | 0x14 => for _ in 0..3 { rec.toks.push(Tok::Num(r.f64()?)); },
            0x0F | 0x10 => {}
            0x11 => f.records.push(std::mem::take(&mut rec)),
            _ => return if f.records.is_empty() { None } else { Some(f) },
        }
    }
    Some(f)
}

struct Bin<'a> { b: &'a [u8], p: usize }
impl Bin<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> { let s = self.b.get(self.p..self.p.checked_add(n)?)?; self.p += n; Some(s) }
    fn u8(&mut self) -> Option<u8> { Some(self.take(1)?[0]) }
    fn i32(&mut self) -> Option<i32> { Some(i32::from_le_bytes(self.take(4)?.try_into().ok()?)) }
    fn f64(&mut self) -> Option<f64> { Some(f64::from_le_bytes(self.take(8)?.try_into().ok()?)) }
    fn str(&mut self, n: usize) -> Option<String> { Some(String::from_utf8_lossy(self.take(n)?).into_owned()) }
}

/// SAT: 3 header lines, then records of words, each ending with `#`.
pub fn parse_sat(text: &str) -> Option<File> {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let version: i64 = lines.next()?.split_whitespace().next()?.parse().ok()?;
    lines.next()?; // product, ACIS version, date
    lines.next()?; // units, tolerances
    let rest: String = lines.collect::<Vec<_>>().join(" ");
    let mut f = File { version, records: vec![] };
    let mut rec = Record::default();
    let mut w = Words { s: rest.as_bytes(), p: 0 };
    while let Some(word) = w.next() {
        if word == "#" {
            if !rec.name.is_empty() { f.records.push(std::mem::take(&mut rec)); }
            continue;
        }
        if rec.name.is_empty() {
            // optional "-12" sequence number before the type name
            if word.starts_with('-') && word[1..].bytes().all(|c| c.is_ascii_digit()) && word.len() > 1 { continue; }
            if END_MARKERS.iter().any(|m| word.starts_with(m)) { break; }
            rec.name = word;
            continue;
        }
        rec.toks.push(sat_tok(word));
    }
    Some(f)
}

fn sat_tok(w: String) -> Tok {
    if let Some(n) = w.strip_prefix('$') { if let Ok(i) = n.parse() { return Tok::Ptr(i); } }
    match w.parse::<f64>() {
        Ok(x) => Tok::Num(x),
        Err(_) => Tok::Str(w),
    }
}

/// Words separated by spaces. `#` is its own word; `@7 abcdefg` is one string of 7 bytes.
struct Words<'a> { s: &'a [u8], p: usize }
impl Words<'_> {
    fn next(&mut self) -> Option<String> {
        let s = self.s;
        while self.p < s.len() && s[self.p].is_ascii_whitespace() { self.p += 1; }
        if self.p >= s.len() { return None; }
        if s[self.p] == b'#' { self.p += 1; return Some("#".into()); }
        let start = self.p;
        while self.p < s.len() && !s[self.p].is_ascii_whitespace() && s[self.p] != b'#' { self.p += 1; }
        let word = &s[start..self.p];
        if word.len() > 1 && word[0] == b'@' {
            if let Some(n) = std::str::from_utf8(&word[1..]).ok().and_then(|n| n.parse::<usize>().ok()) {
                let a = (self.p + 1).min(s.len());
                let b = (a + n).min(s.len());
                self.p = b;
                return Some(String::from_utf8_lossy(&s[a..b]).into_owned());
            }
        }
        Some(String::from_utf8_lossy(word).into_owned())
    }
}

/// Undo AutoCAD's SAT "cipher": every character `c` except space is stored
/// as `159 - c`. In DXF a `^` is written as `^ ` (caret escape); `dxf`
/// removes that space first.
pub fn decipher(raw: &[u8], dxf: bool) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let c = raw[i];
        i += 1;
        if dxf && c == b'^' && raw.get(i) == Some(&b' ') { i += 1; }
        out.push(match c {
            b' ' | b'\n' | b'\r' => c as char,
            c if !(32..=159).contains(&c) => c as char,
            c => (159 - c) as char,
        });
    }
    out
}
