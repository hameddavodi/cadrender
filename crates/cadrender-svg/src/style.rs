//! Shared styles: every `<path>` gets a short class, and the `<style>` block
//! at the top says once what each class means. Class names come from the
//! style itself (colour, width), so regions drawn on other threads use the
//! same names without talking to each other.

use crate::num::write_num;
use cadrender_core::color::Rgba;

/// One paint style. Width in 1/16 pixel (as the walk gives it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Style {
    Stroke { rgba: u32, w16: u32 },
    Fill { rgba: u32, even_odd: bool },
}

pub fn pack(c: Rgba) -> u32 { (c.r as u32) << 24 | (c.g as u32) << 16 | (c.b as u32) << 8 | c.a as u32 }

fn hex2(out: &mut Vec<u8>, b: u8) {
    const H: &[u8; 16] = b"0123456789abcdef";
    out.push(H[(b >> 4) as usize]);
    out.push(H[(b & 15) as usize]);
}

/// `#rgb` when possible, else `#rrggbb`.
pub fn write_color(out: &mut Vec<u8>, rgba: u32) {
    let [r, g, b] = [(rgba >> 24) as u8, (rgba >> 16) as u8, (rgba >> 8) as u8];
    out.push(b'#');
    if [r, g, b].iter().all(|x| x >> 4 == x & 15) {
        const H: &[u8; 16] = b"0123456789abcdef";
        for x in [r, g, b] { out.push(H[(x & 15) as usize]); }
    } else {
        for x in [r, g, b] { hex2(out, x); }
    }
}

impl Style {
    /// Class name: `s000000-16` (stroke, colour, width), `f808080` (fill),
    /// `e808080` (even-odd fill). Two more hex digits when not opaque.
    pub fn write_class(&self, out: &mut Vec<u8>) {
        let (c, rgba) = match *self { Style::Stroke { rgba, .. } => (b's', rgba), Style::Fill { rgba, even_odd } => (if even_odd { b'e' } else { b'f' }, rgba) };
        out.push(c);
        for sh in [24, 16, 8] { hex2(out, (rgba >> sh) as u8); }
        if rgba & 0xff != 0xff { hex2(out, rgba as u8); }
        if let Style::Stroke { w16, .. } = self {
            out.push(b'-');
            out.extend_from_slice(w16.to_string().as_bytes());
        }
    }

    /// The CSS rule for this class. Strokes at or below `hairline_px` keep
    /// their screen width when the viewer zooms.
    pub fn write_rule(&self, out: &mut Vec<u8>, hairline_px: f32) {
        out.push(b'.');
        self.write_class(out);
        let alpha = |rgba: u32| (rgba & 0xff) as f64 / 255.0;
        match *self {
            Style::Stroke { rgba, w16 } => {
                out.extend_from_slice(b"{fill:none;stroke:");
                write_color(out, rgba);
                out.extend_from_slice(b";stroke-width:");
                write_num(out, w16 as f64 / 16.0, 4);
                if rgba & 0xff != 0xff { out.extend_from_slice(b";stroke-opacity:"); write_num(out, alpha(rgba), 3); }
                if w16 as f32 / 16.0 <= hairline_px { out.extend_from_slice(b";vector-effect:non-scaling-stroke"); }
            }
            Style::Fill { rgba, even_odd } => {
                out.extend_from_slice(b"{fill:");
                write_color(out, rgba);
                if rgba & 0xff != 0xff { out.extend_from_slice(b";fill-opacity:"); write_num(out, alpha(rgba), 3); }
                if even_odd { out.extend_from_slice(b";fill-rule:evenodd"); }
            }
        }
        out.extend_from_slice(b"}\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(f: impl Fn(&mut Vec<u8>)) -> String { let mut v = vec![]; f(&mut v); String::from_utf8(v).unwrap() }

    #[test]
    fn classes_and_rules() {
        let st = Style::Stroke { rgba: 0x000000ff, w16: 24 };
        assert_eq!(s(|o| st.write_class(o)), "s000000-24");
        assert_eq!(s(|o| st.write_rule(o, 1.0)), ".s000000-24{fill:none;stroke:#000;stroke-width:1.5}\n");
        let thin = Style::Stroke { rgba: 0x12345680, w16: 16 };
        assert_eq!(s(|o| thin.write_rule(o, 1.0)), ".s12345680-16{fill:none;stroke:#123456;stroke-width:1;stroke-opacity:.502;vector-effect:non-scaling-stroke}\n");
        let f = Style::Fill { rgba: 0xffffffff, even_odd: true };
        assert_eq!(s(|o| f.write_rule(o, 1.0)), ".effffff{fill:#fff;fill-rule:evenodd}\n");
    }
}
