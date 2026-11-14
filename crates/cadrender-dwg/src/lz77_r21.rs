//! The R2007 ("R21") LZ77 variant (ODA spec 5.10). It is different from
//! the R2004 variant in [`crate::lz77`].
//!
//! The stream is a run of literal bytes, then a chain of back copies
//! ("copy `length` bytes from `offset` bytes back in the output"), then
//! literals again, and so on. Two odd details:
//! * literal bytes are stored in a shuffled order: blocks of 32 bytes have
//!   their four 8-byte groups reversed, and the last 1..31 bytes follow a
//!   fixed pattern per length (spec table in 5.10.1, [`LITERAL_SEGMENTS`]);
//! * the low 3 bits of the last opcode byte of a back copy give the length
//!   of the next literal run (0 = read a new length).
//!
//! The spec's pseudo code mixes up two index names; ACadSharp (MIT) was
//! used as a second reference for the exact loop. The code here is our own.
//! Reading past the input or writing past the fixed-size output is an
//! error, never a panic.

use crate::error::{DwgError, Result};

/// How the last `n` (1..=32) literal bytes are stored: a list of
/// (block size, source offset) pairs, written to the output in order.
/// Blocks of 2 and 3 bytes are reversed, blocks of 16 bytes are two 8-byte
/// halves swapped, blocks of 1, 4 and 8 bytes keep their order.
const LITERAL_SEGMENTS: [&[(u8, u8)]; 33] = [
    &[],
    &[(1, 0)],
    &[(2, 0)],
    &[(3, 0)],
    &[(4, 0)],
    &[(1, 4), (4, 0)],
    &[(1, 5), (4, 1), (1, 0)],
    &[(2, 5), (4, 1), (1, 0)],
    &[(8, 0)],
    &[(1, 8), (8, 0)],
    &[(1, 9), (8, 1), (1, 0)],
    &[(2, 9), (8, 1), (1, 0)],
    &[(4, 8), (8, 0)],
    &[(1, 12), (4, 8), (8, 0)],
    &[(1, 13), (4, 9), (8, 1), (1, 0)],
    &[(2, 13), (4, 9), (8, 1), (1, 0)],
    &[(16, 0)],
    &[(8, 9), (1, 8), (8, 0)],
    &[(1, 17), (16, 1), (1, 0)],
    &[(3, 16), (16, 0)],
    &[(4, 16), (16, 0)],
    &[(1, 20), (4, 16), (16, 0)],
    &[(2, 20), (4, 16), (16, 0)],
    &[(3, 20), (4, 16), (16, 0)],
    &[(8, 16), (16, 0)],
    &[(8, 17), (1, 16), (16, 0)],
    &[(1, 25), (8, 17), (1, 16), (16, 0)],
    &[(2, 25), (8, 17), (1, 16), (16, 0)],
    &[(4, 24), (8, 16), (16, 0)],
    &[(1, 28), (4, 24), (8, 16), (16, 0)],
    &[(2, 28), (4, 24), (8, 16), (16, 0)],
    &[(1, 30), (4, 26), (8, 18), (16, 2), (2, 0)],
    &[(16, 16), (16, 0)],
];

/// `PERM[n][i]`: source index of output byte `i` for a literal tail of
/// `n` bytes (built from [`LITERAL_SEGMENTS`] at compile time).
const PERM: [[u8; 32]; 33] = build_perm();

const fn build_perm() -> [[u8; 32]; 33] {
    let mut out = [[0u8; 32]; 33];
    let mut n = 1;
    while n <= 32 {
        let segs = LITERAL_SEGMENTS[n];
        let mut dst = 0usize;
        let mut s = 0;
        while s < segs.len() {
            let (size, off) = segs[s];
            let mut i = 0u8;
            while i < size {
                let inner = match size {
                    2 | 3 => size - 1 - i,
                    16 => (i + 8) % 16,
                    _ => i,
                };
                out[n][dst] = off + inner;
                dst += 1;
                i += 1;
            }
            s += 1;
        }
        assert!(dst == n, "literal segment table does not add up");
        n += 1;
    }
    out
}

struct State<'a> {
    src: &'a [u8],
    pos: usize,
    out: &'a mut [u8],
    dst: usize,
}

impl State<'_> {
    #[inline(always)]
    fn byte(&mut self) -> Result<u8> {
        let b = *self.src.get(self.pos).ok_or_else(|| DwgError::corrupt("R21 LZ77: input ended early"))?;
        self.pos += 1;
        Ok(b)
    }

    /// Length of a literal run that is not given by an opcode.
    fn literal_length(&mut self, op: u8) -> Result<usize> {
        let mut len = op as usize + 8;
        if len == 0x17 {
            let n = self.byte()? as usize;
            len += n;
            if n == 0xff {
                loop {
                    let n = self.byte()? as usize | (self.byte()? as usize) << 8;
                    len += n;
                    if n != 0xffff {
                        break;
                    }
                }
            }
        }
        Ok(len)
    }

    #[inline(always)]
    fn copy_literals(&mut self, len: usize) -> Result<()> {
        let src = self
            .src
            .get(self.pos..self.pos + len)
            .ok_or_else(|| DwgError::corrupt("R21 LZ77: literal run past the input"))?;
        let dst = self
            .out
            .get_mut(self.dst..self.dst + len)
            .ok_or_else(|| DwgError::corrupt("R21 LZ77: output larger than expected"))?;
        let full = len / 32 * 32;
        // A 32-byte block is its four 8-byte groups in reverse order.
        for (d, s) in dst[..full].chunks_exact_mut(32).zip(src[..full].chunks_exact(32)) {
            d[..8].copy_from_slice(&s[24..]);
            d[8..16].copy_from_slice(&s[16..24]);
            d[16..24].copy_from_slice(&s[8..16]);
            d[24..].copy_from_slice(&s[..8]);
        }
        let rest = len - full;
        let perm = &PERM[rest];
        for (i, o) in dst[full..].iter_mut().enumerate() {
            *o = src[full + perm[i] as usize];
        }
        self.pos += len;
        self.dst += len;
        Ok(())
    }

    #[inline(always)]
    fn copy_back(&mut self, offset: usize, len: usize) -> Result<()> {
        if offset == 0 || offset > self.dst {
            return Err(DwgError::corrupt(format!("R21 LZ77: back offset {offset} before the output start")));
        }
        let end = self.dst + len;
        if end > self.out.len() {
            return Err(DwgError::corrupt("R21 LZ77: output larger than expected"));
        }
        let from = self.dst - offset;
        if offset >= len {
            self.out.copy_within(from..from + len, self.dst);
        } else {
            // Overlapping copy: the output repeats with period `offset`.
            // Copy whole periods, doubling the chunk each time.
            let mut done = 0;
            while done < len {
                let n = (offset + done).min(len - done);
                self.out.copy_within(from..from + n, self.dst + done);
                done += n;
            }
        }
        self.dst = end;
        Ok(())
    }

    /// Decode one back-copy instruction. Returns (offset, length, next opcode).
    #[inline(always)]
    fn instruction(&mut self, op: u8) -> Result<(usize, usize, u8)> {
        Ok(match op >> 4 {
            0 => {
                let lo = self.byte()? as usize;
                let next = self.byte()?;
                let len = (op & 0x0f) as usize + 0x13 + ((next as usize >> 3) & 0x10);
                (((next as usize & 0x78) << 5) + 1 + lo, len, next)
            }
            1 => {
                let lo = self.byte()? as usize;
                let next = self.byte()?;
                (((next as usize & 0xf8) << 5) + 1 + lo, (op & 0x0f) as usize + 3, next)
            }
            2 => {
                let mut off = self.byte()? as usize | (self.byte()? as usize) << 8;
                let mut len = (op & 7) as usize;
                let next;
                if op & 8 == 0 {
                    next = self.byte()?;
                    len += (next & 0xf8) as usize;
                } else {
                    off += 1;
                    len += (self.byte()? as usize) << 3;
                    next = self.byte()?;
                    len += ((next as usize & 0xf8) << 8) + 0x100;
                }
                (off, len, next)
            }
            _ => {
                let next = self.byte()?;
                (((next as usize & 0xf8) << 1) + (op & 0x0f) as usize + 1, (op >> 4) as usize, next)
            }
        })
    }
}

/// Decompress `src` into `out` (the expected size). Returns the number of
/// bytes written.
pub fn decompress(src: &[u8], out: &mut [u8]) -> Result<usize> {
    let end = src.len();
    if end == 0 {
        return Ok(0);
    }
    let mut st = State { src, pos: 1, out, dst: 0 };
    let mut op = src[0];
    let mut lit = 0usize;
    if op & 0xf0 == 0x20 {
        st.pos = 4;
        lit = (*src.get(3).ok_or_else(|| DwgError::corrupt("R21 LZ77: input ended early"))? & 7) as usize;
    }
    while st.pos < end {
        if lit == 0 {
            lit = st.literal_length(op)?;
        }
        st.copy_literals(lit)?;
        if st.pos >= end {
            break;
        }
        op = st.byte()?;
        let (mut off, mut len, mut next) = st.instruction(op)?;
        loop {
            st.copy_back(off, len)?;
            lit = (next & 7) as usize;
            if lit != 0 || st.pos >= end {
                break;
            }
            op = st.byte()?;
            if op >> 4 == 0 {
                break;
            }
            if op >> 4 == 15 {
                op &= 0x0f;
            }
            (off, len, next) = st.instruction(op)?;
        }
    }
    Ok(st.dst)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Store `plain` the way the format stores a literal run.
    fn shuffle(plain: &[u8]) -> Vec<u8> {
        let mut out = vec![0u8; plain.len()];
        let full = plain.len() / 32 * 32;
        for b in (0..full).step_by(32) {
            for i in 0..32 {
                out[b + PERM[32][i] as usize] = plain[b + i];
            }
        }
        let rest = plain.len() - full;
        for i in 0..rest {
            out[full + PERM[rest][i] as usize] = plain[full + i];
        }
        out
    }

    #[test]
    fn literal_order_matches_spec_table() {
        // Rows of the spec table in 5.10.1, read by hand.
        assert_eq!(&PERM[2][..2], &[1, 0]);
        assert_eq!(&PERM[5][..5], &[4, 0, 1, 2, 3]);
        assert_eq!(&PERM[7][..7], &[6, 5, 1, 2, 3, 4, 0]);
        assert_eq!(&PERM[16][..16], &[8, 9, 10, 11, 12, 13, 14, 15, 0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(&PERM[19][..4], &[18, 17, 16, 8]);
        assert_eq!(&PERM[31][..7], &[30, 26, 27, 28, 29, 18, 19]);
        assert_eq!(&PERM[31][29..31], &[1, 0]);
        let p32: Vec<u8> = (24..32).chain(16..24).chain(8..16).chain(0..8).collect();
        assert_eq!(&PERM[32][..], &p32[..]);
        // Every row is a permutation.
        for n in 1..=32 {
            let mut seen = [false; 32];
            for &i in &PERM[n][..n] {
                assert!(!seen[i as usize]);
                seen[i as usize] = true;
            }
        }
    }

    #[test]
    fn literal_run_then_back_copy() {
        // op 0x02 -> literal run of 2 + 8 = 10 bytes.
        let mut s = vec![0x02];
        s.extend(shuffle(b"0123456789"));
        // op 0x39: length 3, offset (0 << 1) + 9 + 1 = 10; next 0x00: no literals.
        s.extend([0x39, 0x00]);
        let mut out = [0u8; 13];
        assert_eq!(decompress(&s, &mut out).unwrap(), 13);
        assert_eq!(&out, b"0123456789012");
    }

    #[test]
    fn long_literal_run_and_overlapping_copy() {
        // op 0x0F -> 0x17 + next byte: 0x17 + 0x30 = 71 bytes of literals.
        let plain: Vec<u8> = (0..71u8).map(|i| i.wrapping_mul(3)).collect();
        let mut s = vec![0x0F, 0x30];
        s.extend(shuffle(&plain));
        // op 0x12: length 2 + 3 = 5, offset ((0x00 & 0xf8) << 5) + 1 + 0 = 1
        // (repeat the last byte 5 times); next byte 0x02 -> 2 literals.
        s.extend([0x12, 0x00, 0x02]);
        s.extend(shuffle(b"xy"));
        let mut out = vec![0u8; 78];
        assert_eq!(decompress(&s, &mut out).unwrap(), 78);
        assert_eq!(&out[..71], &plain[..]);
        assert_eq!(&out[71..76], &[plain[70]; 5]);
        assert_eq!(&out[76..], b"xy");
    }

    #[test]
    fn opcode_2x_start_and_long_copy() {
        // First byte 0x2_: skip 3 bytes, literal count = 4th byte & 7.
        let mut s = vec![0x20, 0, 0, 0x04];
        s.extend(shuffle(b"abcd"));
        // op 0x20: offset = 0x0004 (2 bytes), len = op & 7 = 0, next 0x08:
        // len += 8 -> copy 8 bytes from 4 back (overlapping).
        s.extend([0x20, 0x04, 0x00, 0x08]);
        let mut out = [0u8; 12];
        assert_eq!(decompress(&s, &mut out).unwrap(), 12);
        assert_eq!(&out, b"abcdabcdabcd");
    }

    #[test]
    fn damaged_input_is_an_error() {
        let mut s = vec![0x02];
        s.extend(shuffle(b"0123456789"));
        s.extend([0x39, 0x00]);
        // output too small
        assert!(decompress(&s, &mut [0u8; 11]).is_err());
        // literal run cut short
        assert!(decompress(&s[..5], &mut [0u8; 20]).is_err());
        // back offset before the output start
        let bad = [0x02, b'0', b'1', b'2', b'3', b'4', b'5', b'6', b'7', b'8', b'9', 0x3F, 0x10];
        assert!(decompress(&bad, &mut [0u8; 40]).is_err());
        assert_eq!(decompress(&[], &mut []).unwrap(), 0);
    }
}
