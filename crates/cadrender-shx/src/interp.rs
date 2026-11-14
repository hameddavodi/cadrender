//! The shape byte-code interpreter.
//!
//! Written from the public AutoCAD shape description ("About Shape Descriptions",
//! "Special Codes Reference" in the AutoCAD Customization Guide). Every special
//! code 0..14 and the length/direction vector bytes are handled.
//!
//! Interpretation choices where the public text is not exact:
//! - The octant byte of codes 10 and 11 is sign + magnitude: bit 7 set means
//!   clockwise, bits 4..6 are the start octant, bits 0..2 the octant count
//!   (0 = 8 octants). This is what the compiler writes for `-0SC`.
//! - Code 11 with end offset 0 ends on the boundary after the last octant.
//! - Inside the byte code, 2-byte shape numbers (unifont and big font code 7)
//!   are high byte first. If that shape does not exist but the byte-swapped
//!   number does, the swapped number is used (tolerance).
//! - Pop (code 6) moves the pen without drawing.
//! - The position stack accepts 8 entries (the spec says 4); more pushes are
//!   ignored, a pop on an empty stack does nothing.
//! - Bad or truncated byte code never panics: the glyph simply ends there.

use crate::glyph::{Glyph, GlyphBuilder};
use crate::shx::{ShxFont, ShxKind};

const STACK_MAX: usize = 8;
const MAX_DEPTH: u32 = 10;
/// Most byte-code steps per glyph. Subshapes can call subshapes, so a broken
/// font could otherwise make the work grow very large.
const BUDGET: u32 = 200_000;
/// Arc step: 11.25 degrees (32 segments for a full circle).
const ARC_STEP: f64 = std::f64::consts::PI / 16.0;
const OCTANT: f64 = std::f64::consts::FRAC_PI_4;

/// Direction vectors of the 16 directions of a length/direction byte.
const DIRS: [[f64; 2]; 16] = [
    [1.0, 0.0],
    [1.0, 0.5],
    [1.0, 1.0],
    [0.5, 1.0],
    [0.0, 1.0],
    [-0.5, 1.0],
    [-1.0, 1.0],
    [-1.0, 0.5],
    [-1.0, 0.0],
    [-1.0, -0.5],
    [-1.0, -1.0],
    [-0.5, -1.0],
    [0.0, -1.0],
    [0.5, -1.0],
    [1.0, -1.0],
    [1.0, -0.5],
];

/// Pen state of one shape level.
struct State {
    pos: [f64; 2],
    pen: bool,
    scale: f64,
    stack: [[f64; 2]; STACK_MAX],
    sp: usize,
}

impl State {
    fn new() -> State {
        State {
            pos: [0.0, 0.0],
            pen: true, // "Draw is activated at the start of each shape."
            scale: 1.0,
            stack: [[0.0; 2]; STACK_MAX],
            sp: 0,
        }
    }
}

/// Maps local shape coordinates to glyph coordinates: `o + (x*kx, y*ky)`.
#[derive(Clone, Copy)]
struct Xf {
    o: [f64; 2],
    kx: f64,
    ky: f64,
}

impl Xf {
    const ID: Xf = Xf {
        o: [0.0, 0.0],
        kx: 1.0,
        ky: 1.0,
    };
    #[inline]
    fn apply(&self, p: [f64; 2]) -> [f64; 2] {
        [self.o[0] + p[0] * self.kx, self.o[1] + p[1] * self.ky]
    }
}

pub(crate) struct Interp<'a> {
    font: &'a ShxFont,
    vertical: bool,
    out: GlyphBuilder,
    budget: u32,
}

#[inline]
fn i8v(b: u8) -> f64 {
    b as i8 as f64
}

impl<'a> Interp<'a> {
    /// Interpret the byte code of one shape into a glyph.
    pub(crate) fn run_shape(font: &'a ShxFont, code: &[u8], vertical: bool) -> Glyph {
        let mut it = Interp {
            font,
            vertical,
            out: GlyphBuilder::default(),
            budget: BUDGET,
        };
        let mut st = State::new();
        it.run(code, &mut st, Xf::ID, 0);
        let end = st.pos;
        it.out.finish(end)
    }

    fn go(&mut self, st: &mut State, xf: Xf, to: [f64; 2]) {
        if st.pen {
            self.out.line(xf.apply(st.pos), xf.apply(to));
        } else {
            self.out.close();
        }
        st.pos = to;
    }

    /// Arc around `c` with radius `r` from angle `a0` over `sweep` (radians,
    /// positive = counter-clockwise). The pen ends exactly on the arc end.
    fn arc(&mut self, st: &mut State, xf: Xf, c: [f64; 2], r: f64, a0: f64, sweep: f64) {
        let n = ((sweep.abs() / ARC_STEP).ceil() as usize).clamp(1, 64);
        self.budget = self.budget.saturating_sub(n as u32);
        for i in 1..=n {
            let a = a0 + sweep * (i as f64) / (n as f64);
            let p = [c[0] + r * a.cos(), c[1] + r * a.sin()];
            self.go(st, xf, p);
        }
    }

    /// Arc segment from the pen by displacement (dx, dy) with bulge byte.
    fn bulge(&mut self, st: &mut State, xf: Xf, dx: u8, dy: u8, bulge: u8) {
        let d = [i8v(dx) * st.scale, i8v(dy) * st.scale];
        let p0 = st.pos;
        let p1 = [p0[0] + d[0], p0[1] + d[1]];
        let b = i8v(bulge) / 127.0;
        if b == 0.0 || (d[0] == 0.0 && d[1] == 0.0) {
            self.go(st, xf, p1);
            return;
        }
        // Same bulge as DXF polylines: b = tan(sweep / 4) = 2 * sagitta / chord.
        let sweep = 4.0 * b.atan();
        let k = (1.0 - b * b) / (4.0 * b);
        let c = [p0[0] + d[0] / 2.0 - d[1] * k, p0[1] + d[1] / 2.0 + d[0] * k];
        let r = ((p0[0] - c[0]).powi(2) + (p0[1] - c[1]).powi(2)).sqrt();
        let a0 = (p0[1] - c[1]).atan2(p0[0] - c[0]);
        self.arc(st, xf, c, r, a0, sweep);
        st.pos = p1; // exact end, no rounding drift
    }

    fn subshape_bytes(&self, n: u16, alt: u16) -> Option<&'a [u8]> {
        let f = self.font;
        f.shape_code(n).or_else(|| f.shape_code(alt))
    }

    fn run(&mut self, code: &[u8], st: &mut State, xf: Xf, depth: u32) {
        let font = self.font;
        let mut i = 0usize;
        let at = |k: usize| code.get(k).copied();
        while let Some(op) = at(i) {
            if self.budget == 0 {
                return;
            }
            self.budget -= 1;
            i += 1;
            match op {
                0 => return,
                1 => st.pen = true,
                2 => {
                    st.pen = false;
                    self.out.close();
                }
                3 | 4 => {
                    let Some(n) = at(i) else { return };
                    i += 1;
                    if n != 0 {
                        if op == 3 {
                            st.scale /= n as f64;
                        } else {
                            st.scale *= n as f64;
                        }
                    }
                }
                5 => {
                    if st.sp < STACK_MAX {
                        st.stack[st.sp] = st.pos;
                        st.sp += 1;
                    }
                }
                6 => {
                    if st.sp > 0 {
                        st.sp -= 1;
                        st.pos = st.stack[st.sp];
                        self.out.close();
                    }
                }
                7 => {
                    let Some(b1) = at(i) else { return };
                    match font.kind() {
                        ShxKind::Shapes => {
                            i += 1;
                            if depth < MAX_DEPTH {
                                if let Some(sub) = font.shape_code(b1 as u16) {
                                    self.run(sub, st, xf, depth + 1);
                                }
                            }
                        }
                        ShxKind::Unifont => {
                            let Some(b2) = at(i + 1) else { return };
                            i += 2;
                            let n = u16::from_be_bytes([b1, b2]);
                            if depth < MAX_DEPTH {
                                if let Some(sub) = self.subshape_bytes(n, n.swap_bytes()) {
                                    self.run(sub, st, xf, depth + 1);
                                }
                            }
                        }
                        ShxKind::BigFont => {
                            if b1 == 0 {
                                // 7,0,hi,lo,x,y,[w],h : primitive placed in a box
                                let ext = font.info().is_some_and(|f| f.extended_big);
                                let need = if ext { 7 } else { 6 };
                                if i + need > code.len() {
                                    return;
                                }
                                let n = u16::from_be_bytes([code[i + 1], code[i + 2]]);
                                let bx = i8v(code[i + 3]);
                                let by = i8v(code[i + 4]);
                                let (w, h) = if ext {
                                    (code[i + 5] as f64, code[i + 6] as f64)
                                } else {
                                    (code[i + 5] as f64, code[i + 5] as f64)
                                };
                                i += need;
                                if depth < MAX_DEPTH {
                                    if let Some(sub) = self.subshape_bytes(n, n.swap_bytes()) {
                                        let (ch, cw) = font.big_box();
                                        let o = xf.apply([bx * st.scale, by * st.scale]);
                                        let inner = Xf {
                                            o,
                                            kx: xf.kx * st.scale * w / cw,
                                            ky: xf.ky * st.scale * h / ch,
                                        };
                                        let mut sub_st = State::new();
                                        sub_st.pen = st.pen;
                                        self.run(sub, &mut sub_st, inner, depth + 1);
                                        self.out.close();
                                    }
                                }
                            } else {
                                let Some(b2) = at(i + 1) else { return };
                                i += 2;
                                let n = u16::from_be_bytes([b1, b2]);
                                if depth < MAX_DEPTH {
                                    if let Some(sub) = self.subshape_bytes(n, n.swap_bytes()) {
                                        self.run(sub, st, xf, depth + 1);
                                    }
                                }
                            }
                        }
                    }
                }
                8 => {
                    let (Some(x), Some(y)) = (at(i), at(i + 1)) else {
                        return;
                    };
                    i += 2;
                    let to = [st.pos[0] + i8v(x) * st.scale, st.pos[1] + i8v(y) * st.scale];
                    self.go(st, xf, to);
                }
                9 => loop {
                    let (Some(x), Some(y)) = (at(i), at(i + 1)) else {
                        return;
                    };
                    i += 2;
                    if x == 0 && y == 0 {
                        break;
                    }
                    let to = [st.pos[0] + i8v(x) * st.scale, st.pos[1] + i8v(y) * st.scale];
                    self.go(st, xf, to);
                },
                10 => {
                    let (Some(r), Some(spec)) = (at(i), at(i + 1)) else {
                        return;
                    };
                    i += 2;
                    let r = r as f64 * st.scale;
                    let (s, n, cw) = octant_spec(spec);
                    let a0 = s as f64 * OCTANT;
                    let sweep = n as f64 * OCTANT * if cw { -1.0 } else { 1.0 };
                    let c = [st.pos[0] - r * a0.cos(), st.pos[1] - r * a0.sin()];
                    self.arc(st, xf, c, r, a0, sweep);
                }
                11 => {
                    if i + 5 > code.len() {
                        return;
                    }
                    let so = code[i] as f64;
                    let eo = code[i + 1] as f64;
                    let r = (code[i + 2] as f64 * 256.0 + code[i + 3] as f64) * st.scale;
                    let (s, n, cw) = octant_spec(code[i + 4]);
                    i += 5;
                    let (s, n) = (s as f64, n as f64);
                    let unit = OCTANT / 256.0;
                    let (a0, a1) = if !cw {
                        let a0 = s * OCTANT + so * unit;
                        let a1 = if eo == 0.0 {
                            (s + n) * OCTANT
                        } else {
                            (s + n - 1.0) * OCTANT + eo * unit
                        };
                        (a0, a1)
                    } else {
                        let a0 = s * OCTANT - so * unit;
                        let a1 = if eo == 0.0 {
                            (s - n) * OCTANT
                        } else {
                            (s - n + 1.0) * OCTANT - eo * unit
                        };
                        (a0, a1)
                    };
                    let tau = std::f64::consts::TAU;
                    let mut sweep = a1 - a0;
                    if !cw && sweep <= 0.0 {
                        sweep += tau;
                    }
                    if cw && sweep >= 0.0 {
                        sweep -= tau;
                    }
                    let c = [st.pos[0] - r * a0.cos(), st.pos[1] - r * a0.sin()];
                    self.arc(st, xf, c, r, a0, sweep);
                }
                12 => {
                    if i + 3 > code.len() {
                        return;
                    }
                    let (x, y, b) = (code[i], code[i + 1], code[i + 2]);
                    i += 3;
                    self.bulge(st, xf, x, y, b);
                }
                13 => loop {
                    let (Some(x), Some(y)) = (at(i), at(i + 1)) else {
                        return;
                    };
                    if x == 0 && y == 0 {
                        i += 2;
                        break;
                    }
                    let Some(b) = at(i + 2) else { return };
                    i += 3;
                    self.bulge(st, xf, x, y, b);
                },
                14 => {
                    if !self.vertical {
                        i = skip_command(code, i, font);
                    }
                }
                15 => {} // not defined: ignore
                v => {
                    let len = (v >> 4) as f64 * st.scale;
                    let d = DIRS[(v & 0x0F) as usize];
                    let to = [st.pos[0] + d[0] * len, st.pos[1] + d[1] * len];
                    self.go(st, xf, to);
                }
            }
        }
    }
}

/// Decode an octant byte: (start octant, octant count 1..=8, clockwise).
#[inline]
fn octant_spec(b: u8) -> (u8, u8, bool) {
    let cw = b & 0x80 != 0;
    let s = (b >> 4) & 0x07;
    let n = match b & 0x07 {
        0 => 8,
        n => n,
    };
    (s, n, cw)
}

/// Index after the command that starts at `i` (used by code 14).
/// An end marker (0) is never skipped.
pub(crate) fn skip_command(code: &[u8], i: usize, font: &ShxFont) -> usize {
    let len = code.len();
    let Some(&op) = code.get(i) else { return len };
    let next = match op {
        0 => i, // keep the end code
        1 | 2 | 5 | 6 | 14 | 15 => i + 1,
        3 | 4 => i + 2,
        7 => match font.kind() {
            ShxKind::Shapes => i + 2,
            ShxKind::Unifont => i + 3,
            ShxKind::BigFont => {
                if code.get(i + 1) == Some(&0) {
                    let ext = font.info().is_some_and(|f| f.extended_big);
                    i + if ext { 8 } else { 7 }
                } else {
                    i + 3
                }
            }
        },
        8 => i + 3,
        9 => {
            let mut k = i + 1;
            while k + 1 < len && !(code[k] == 0 && code[k + 1] == 0) {
                k += 2;
            }
            k + 2
        }
        10 => i + 3,
        11 => i + 6,
        12 => i + 4,
        13 => {
            let mut k = i + 1;
            while k + 1 < len && !(code[k] == 0 && code[k + 1] == 0) {
                k += 3;
            }
            k + 2
        }
        _ => i + 1,
    };
    next.min(len)
}
