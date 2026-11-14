//! Tag reader: (group code, value) pairs straight from the mapped file.
//! Values stay byte slices of the file until they are needed (zero copy).

pub type Tag<'a> = (i32, &'a [u8]);

pub struct Tags<'a> { buf: &'a [u8], pos: usize, peeked: Option<Tag<'a>> }

impl<'a> Tags<'a> {
    pub fn new(buf: &'a [u8]) -> Tags<'a> {
        // skip a UTF-8 byte order mark
        let pos = if buf.starts_with(&[0xEF, 0xBB, 0xBF]) { 3 } else { 0 };
        Tags { buf, pos, peeked: None }
    }
    /// Byte offset of the reader (for error messages).
    pub fn pos(&self) -> usize { self.pos }

    #[inline]
    fn line(&mut self) -> Option<&'a [u8]> {
        if self.pos >= self.buf.len() { return None; }
        let rest = &self.buf[self.pos..];
        let (mut l, adv) = match memchr::memchr(b'\n', rest) { Some(i) => (&rest[..i], i + 1), None => (rest, rest.len()) };
        self.pos += adv;
        if let [h @ .., b'\r'] = l { l = h; }
        Some(l)
    }

    #[inline]
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> Option<Tag<'a>> {
        if let Some(t) = self.peeked.take() { return Some(t); }
        let c = self.line()?;
        let v = self.line()?;
        let mut code = 0i32;
        let mut neg = false;
        for &b in c { match b { b'0'..=b'9' => code = code.wrapping_mul(10).wrapping_add((b - b'0') as i32), b'-' => neg = true, _ => {} } }
        Some((if neg { -code } else { code }, v))
    }

    pub fn peek(&mut self) -> Option<Tag<'a>> {
        if self.peeked.is_none() { self.peeked = self.next(); }
        self.peeked
    }

    /// Tags of one object: everything up to the next code 0.
    pub fn body(&mut self, out: &mut Vec<Tag<'a>>) {
        out.clear();
        while let Some((c, v)) = self.peek() {
            if c == 0 { break; }
            out.push((c, v));
            self.peeked = None;
        }
    }
}

#[inline]
pub fn trim(v: &[u8]) -> &[u8] {
    let mut s = v;
    while let [b' ' | b'\t', r @ ..] = s { s = r; }
    while let [r @ .., b' ' | b'\t'] = s { s = r; }
    s
}
/// Float value; 0.0 if it does not parse.
#[inline]
pub fn f(v: &[u8]) -> f64 {
    let x: f64 = std::str::from_utf8(trim(v)).ok().and_then(|s| s.parse().ok()).unwrap_or(0.0);
    if x.is_finite() { x } else { 0.0 }
}
/// Integer value; 0 if it does not parse.
#[inline]
pub fn i(v: &[u8]) -> i32 {
    let s = trim(v);
    std::str::from_utf8(s).ok().and_then(|s| s.parse::<i64>().ok().or_else(|| s.parse::<f64>().ok().map(|x| x as i64))).map(|x| x.clamp(i32::MIN as i64, i32::MAX as i64) as i32).unwrap_or(0)
}
/// 64-bit integer, no clamping (raw colours may be written signed or unsigned).
pub fn long(v: &[u8]) -> i64 { std::str::from_utf8(trim(v)).ok().and_then(|s| s.parse().ok()).unwrap_or(0) }
/// Hex handle; 0 if it does not parse.
pub fn hex(v: &[u8]) -> u64 { std::str::from_utf8(trim(v)).ok().and_then(|s| u64::from_str_radix(s, 16).ok()).unwrap_or(0) }
/// Text value. UTF-8 if valid, else read as Latin-1 (close to the old ANSI code pages).
pub fn s(v: &[u8]) -> String {
    let t = trim(v);
    match std::str::from_utf8(t) { Ok(x) => x.to_string(), Err(_) => t.iter().map(|&b| b as char).collect() }
}
/// Text value without trimming (text content keeps its spaces).
pub fn raw(v: &[u8]) -> String {
    match std::str::from_utf8(v) { Ok(x) => x.to_string(), Err(_) => v.iter().map(|&b| b as char).collect() }
}

/// First value of a group code in a body.
pub fn find<'a>(body: &[Tag<'a>], code: i32) -> Option<&'a [u8]> { body.iter().find(|t| t.0 == code).map(|t| t.1) }
pub fn gf(body: &[Tag], code: i32) -> Option<f64> { find(body, code).map(f) }
pub fn gi(body: &[Tag], code: i32) -> Option<i32> { find(body, code).map(i) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reads_crlf_and_spaces() {
        let mut t = Tags::new(b"  0\r\nSECTION\r\n  2\r\nHEADER\r\n 40\r\n 1.5 \r\n");
        assert_eq!(t.next(), Some((0, &b"SECTION"[..])));
        assert_eq!(t.peek(), Some((2, &b"HEADER"[..])));
        let mut body = vec![];
        t.body(&mut body);
        assert_eq!(body.len(), 2);
        assert_eq!(f(body[1].1), 1.5);
        assert_eq!(t.next(), None);
    }
    #[test]
    fn tolerant_values() {
        assert_eq!(i(b" 12 "), 12);
        assert_eq!(i(b"7.0"), 7);
        assert_eq!(i(b"junk"), 0);
        assert_eq!(f(b"nan"), 0.0);
        assert_eq!(hex(b"1F"), 31);
        assert_eq!(s(b"caf\xe9"), "café");
    }
}
