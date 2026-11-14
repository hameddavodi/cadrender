//! Short numbers and short path data.
//!
//! Points are rounded to a grid (`10^-dec` pixels) once, as integers. Path
//! commands are relative (the step from the last point), so the numbers stay
//! small, and because the steps are integer differences there is no rounding
//! drift along a long path.

/// Write an unsigned integer.
fn write_u64(out: &mut Vec<u8>, mut v: u64) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 { break; }
    }
    out.extend_from_slice(&buf[i..]);
}

/// Write `q / 10^dec` with as few characters as possible:
/// `1.5`, `.05`, `-.5`, `12`, `0`.
pub fn write_fixed(out: &mut Vec<u8>, q: i64, dec: u8) {
    if q < 0 { out.push(b'-'); }
    let a = q.unsigned_abs();
    let p = 10u64.pow(dec as u32);
    let (int, mut frac) = (a / p, a % p);
    if int != 0 || frac == 0 { write_u64(out, int); }
    if frac != 0 {
        let mut digits = dec as usize;
        while frac % 10 == 0 { frac /= 10; digits -= 1; }
        out.push(b'.');
        let start = out.len();
        write_u64(out, frac);
        let n = out.len() - start;
        // left-pad with zeros: .05
        for _ in n..digits { out.insert(start, b'0'); }
    }
}

/// Write `v` rounded to `dec` decimals.
pub fn write_num(out: &mut Vec<u8>, v: f64, dec: u8) {
    write_fixed(out, (v * 10f64.powi(dec as i32)).round() as i64, dec);
}

/// A curve whose control points are at most one grid step from the straight
/// line `s`-`e` looks like that line at this precision: write it as a line.
fn flat(s: (i64, i64), e: (i64, i64), ctrl: &[(i64, i64)]) -> bool {
    let (dx, dy) = ((e.0 - s.0) as f64, (e.1 - s.1) as f64);
    let len2 = dx * dx + dy * dy;
    ctrl.iter().all(|c| {
        let (px, py) = ((c.0 - s.0) as f64, (c.1 - s.1) as f64);
        // distance to the segment (not the endless line), so loops stay curves
        let t = if len2 > 0.0 { ((px * dx + py * dy) / len2).clamp(0.0, 1.0) } else { 0.0 };
        let (qx, qy) = (px - t * dx, py - t * dy);
        qx * qx + qy * qy <= 1.0
    })
}

/// Builds the `d` attribute of one `<path>`.
#[derive(Clone, Debug)]
pub struct PathWriter {
    pub d: Vec<u8>,
    scale: f64,
    dec: u8,
    cur: (i64, i64),
    start: (i64, i64),
    /// Last command letter (0 = none). After `m` it is `l` (the implicit one).
    last: u8,
    /// The last byte is part of a number, and whether that number has a dot.
    prev_num: bool,
    prev_dot: bool,
    /// Something visible was added (not only moves).
    pub drawn: bool,
}

impl PathWriter {
    pub fn new(dec: u8) -> PathWriter {
        PathWriter { d: Vec::new(), scale: 10f64.powi(dec as i32), dec, cur: (0, 0), start: (0, 0), last: 0, prev_num: false, prev_dot: false, drawn: false }
    }

    /// Start a new, empty path.
    pub fn reset(&mut self) {
        self.d.clear();
        self.last = 0;
        self.prev_num = false;
        self.drawn = false;
    }

    #[inline]
    fn q(&self, v: f32) -> i64 { (v as f64 * self.scale).round() as i64 }

    #[inline]
    fn cmd(&mut self, c: u8) {
        if self.last != c { self.d.push(c); self.last = c; self.prev_num = false; }
    }

    fn num(&mut self, q: i64) {
        let mut tmp = Vec::with_capacity(24);
        write_fixed(&mut tmp, q, self.dec);
        let dot = tmp.contains(&b'.');
        if self.prev_num {
            let joinable = tmp[0] == b'-' || (tmp[0] == b'.' && self.prev_dot);
            if !joinable { self.d.push(b' '); }
        }
        self.d.extend_from_slice(&tmp);
        self.prev_num = true;
        self.prev_dot = dot;
    }

    pub fn move_to(&mut self, x: f32, y: f32) {
        let p = (self.q(x), self.q(y));
        if self.d.is_empty() {
            self.cmd(b'M');
            self.num(p.0); self.num(p.1);
            self.last = b'L';
        } else {
            self.last = 0;
            self.cmd(b'm');
            self.num(p.0 - self.cur.0); self.num(p.1 - self.cur.1);
            self.last = b'l';
        }
        self.cur = p;
        self.start = p;
    }

    pub fn line_to(&mut self, x: f32, y: f32) {
        let p = (self.q(x), self.q(y));
        self.line_q(p);
    }

    fn line_q(&mut self, p: (i64, i64)) {
        let (dx, dy) = (p.0 - self.cur.0, p.1 - self.cur.1);
        if dx == 0 && dy == 0 { return; }
        if dy == 0 { self.cmd(b'h'); self.num(dx); }
        else if dx == 0 { self.cmd(b'v'); self.num(dy); }
        else { self.cmd(b'l'); self.num(dx); self.num(dy); }
        self.cur = p;
        self.drawn = true;
    }

    pub fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (a, p) = ((self.q(x1), self.q(y1)), (self.q(x), self.q(y)));
        if flat(self.cur, p, &[a]) { return self.line_q(p); }
        self.cmd(b'q');
        let c = self.cur;
        for v in [a.0 - c.0, a.1 - c.1, p.0 - c.0, p.1 - c.1] { self.num(v); }
        self.cur = p;
        self.drawn = true;
    }

    pub fn cubic_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (a, b, p) = ((self.q(x1), self.q(y1)), (self.q(x2), self.q(y2)), (self.q(x), self.q(y)));
        if flat(self.cur, p, &[a, b]) { return self.line_q(p); }
        self.cmd(b'c');
        let c = self.cur;
        for v in [a.0 - c.0, a.1 - c.1, b.0 - c.0, b.1 - c.1, p.0 - c.0, p.1 - c.1] { self.num(v); }
        self.cur = p;
        self.drawn = true;
    }

    pub fn close(&mut self) {
        if self.d.is_empty() { return; }
        self.cmd(b'z');
        self.cur = self.start;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(q: i64, dec: u8) -> String { let mut v = vec![]; write_fixed(&mut v, q, dec); String::from_utf8(v).unwrap() }

    #[test]
    fn fixed_numbers_are_short() {
        assert_eq!(fx(1234, 2), "12.34");
        assert_eq!(fx(1230, 2), "12.3");
        assert_eq!(fx(1200, 2), "12");
        assert_eq!(fx(5, 2), ".05");
        assert_eq!(fx(-5, 2), "-.05");
        assert_eq!(fx(-150, 2), "-1.5");
        assert_eq!(fx(0, 2), "0");
        assert_eq!(fx(7, 0), "7");
        assert_eq!(fx(101, 3), ".101");
    }

    #[test]
    fn path_is_relative_and_compact() {
        let mut p = PathWriter::new(1);
        p.move_to(10.0, 10.0);
        p.line_to(20.0, 10.0); // h
        p.line_to(20.0, 5.0); // v
        p.line_to(25.5, 0.5); // l
        p.line_to(26.0, 1.0); // implicit l
        p.close();
        p.move_to(12.0, 12.0); // relative to the subpath start after z
        p.line_to(12.5, 11.5);
        assert_eq!(String::from_utf8(p.d.clone()).unwrap(), "M10 10h10v-5l5.5-4.5.5.5zm2 2 .5-.5");
        assert!(p.drawn);
    }

    #[test]
    fn zero_steps_are_dropped_and_moves_alone_are_not_drawn() {
        let mut p = PathWriter::new(2);
        p.move_to(1.0, 1.0);
        p.line_to(1.001, 1.0);
        assert!(!p.drawn);
        p.reset();
        assert!(p.d.is_empty());
    }

    #[test]
    fn tiny_curves_become_lines() {
        let mut p = PathWriter::new(2);
        p.move_to(0.0, 0.0);
        p.cubic_to(0.004, 0.0, 0.01, 0.003, 0.02, 0.0); // flat at 0.01 px
        p.quad_to(0.5, 1.0, 1.02, 0.0); // a real bump
        assert_eq!(String::from_utf8(p.d).unwrap(), "M0 0h.02q.48 1 1 0");
    }

    #[test]
    fn no_drift_on_long_paths() {
        let mut p = PathWriter::new(1);
        p.move_to(0.0, 0.0);
        for i in 1..=1000 { p.line_to(i as f32 * 0.33, 0.0); }
        // the end point is exactly the rounded absolute value
        let s = String::from_utf8(p.d).unwrap();
        let sum: i64 = numbers(s.trim_start_matches("M0 0h")).iter().map(|t| (t * 10.0).round() as i64).sum();
        assert_eq!(sum, 3300);
    }

    /// Split SVG path numbers: `1.5-2.5.5` -> 1.5, -2.5, 0.5.
    pub fn numbers(s: &str) -> Vec<f64> {
        let mut out = vec![];
        let mut cur = String::new();
        let push = |cur: &mut String, out: &mut Vec<f64>| { if !cur.is_empty() { out.push(cur.parse().unwrap()); cur.clear(); } };
        for ch in s.chars() {
            match ch {
                '-' => { push(&mut cur, &mut out); cur.push('-'); }
                '.' if cur.contains('.') => { push(&mut cur, &mut out); cur.push('.'); }
                '0'..='9' | '.' => cur.push(ch),
                _ => push(&mut cur, &mut out),
            }
        }
        push(&mut cur, &mut out);
        out
    }

    #[test]
    fn joined_numbers_parse_back() {
        assert_eq!(numbers("1.5-2.5.5 3"), vec![1.5, -2.5, 0.5, 3.0]);
    }
}
