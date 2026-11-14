//! Tiny JSON writer for the summary (no dependency needed).

use std::collections::BTreeMap;
use std::fmt::Write;

pub struct Json { s: String, first: bool }

pub fn escape(v: &str, out: &mut String) {
    out.push('"');
    for c in v.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => { let _ = write!(out, "\\u{:04x}", c as u32); }
            c => out.push(c),
        }
    }
    out.push('"');
}

impl Json {
    pub fn obj() -> Json { Json { s: "{".into(), first: true } }
    fn key(&mut self, k: &str) {
        if !self.first { self.s.push_str(", "); }
        self.first = false;
        escape(k, &mut self.s);
        self.s.push_str(": ");
    }
    pub fn str(&mut self, k: &str, v: &str) { self.key(k); escape(v, &mut self.s); }
    pub fn bool(&mut self, k: &str, v: bool) { self.key(k); self.s.push_str(if v { "true" } else { "false" }); }
    pub fn num(&mut self, k: &str, v: f64) {
        self.key(k);
        if !v.is_finite() { self.s.push_str("null"); } else if v.fract() == 0.0 && v.abs() < 1e15 { let _ = write!(self.s, "{}", v as i64); } else { let _ = write!(self.s, "{v}"); }
    }
    /// Milliseconds, one decimal.
    pub fn ms(&mut self, k: &str, v: f64) { self.key(k); let _ = write!(self.s, "{:.1}", v); }
    pub fn strs(&mut self, k: &str, v: &[String]) {
        self.key(k);
        self.s.push('[');
        for (i, x) in v.iter().enumerate() { if i > 0 { self.s.push_str(", "); } escape(x, &mut self.s); }
        self.s.push(']');
    }
    pub fn map(&mut self, k: &str, m: &BTreeMap<String, u64>) {
        let mut o = Json::obj();
        for (kk, v) in m { o.num(kk, *v as f64); }
        self.raw(k, o);
    }
    pub fn raw(&mut self, k: &str, v: Json) { self.key(k); self.s.push_str(&v.finish()); }
    pub fn arr(&mut self, k: &str, v: Vec<Json>) {
        self.key(k);
        self.s.push_str("[\n");
        let n = v.len();
        for (i, x) in v.into_iter().enumerate() { self.s.push_str("  "); self.s.push_str(&x.finish()); if i + 1 < n { self.s.push(','); } self.s.push('\n'); }
        self.s.push(']');
    }
    pub fn finish(mut self) -> String { self.s.push('}'); self.s }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn escapes() {
        let mut j = Json::obj();
        j.str("a\"b", "x\ny\u{1}");
        j.num("n", 3.0);
        j.ms("t", 1.234);
        assert_eq!(j.finish(), r#"{"a\"b": "x\ny\u0001", "n": 3, "t": 1.2}"#);
    }
}
