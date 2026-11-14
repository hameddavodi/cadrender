//! The R2004+ LZ77 variant (ODA spec 4.7), used by R2004, R2010, R2013 and
//! R2018 files for system pages and compressed data pages.
//!
//! The stream is a list of "copy from earlier output" (offset, length) pairs,
//! each followed by a run of literal bytes. Copies may overlap the bytes they
//! produce (length larger than offset), so overlapping copies go byte by byte.
//!
//! Notes beyond the spec text (checked against ACadSharp, MIT, and against
//! our real files):
//! * a first byte above 0x11 is a literal run of `byte - 17` bytes (an
//!   opcode cannot come first: there is nothing to copy from yet);
//! * for opcodes 0x12..=0x1F, bit 3 adds 0x4000 to the offset and only the
//!   low 3 bits give the length (the spec says the low 4 bits);
//! * the real back offset is the coded offset plus one.
//!
//! The output buffer has a fixed size. Writing past it, or reading past the
//! input, is an error, never a panic.

use crate::error::{DwgError, Result};

struct Src<'a> {
    data: &'a [u8],
    pos: usize,
}

impl Src<'_> {
    #[inline(always)]
    fn next(&mut self) -> Result<u8> {
        match self.data.get(self.pos) {
            Some(&b) => {
                self.pos += 1;
                Ok(b)
            }
            None => Err(DwgError::corrupt("LZ77 input ended early")),
        }
    }

    /// Literal length (spec "Literal Length"), without the final +3.
    fn literal_count(&mut self, op: u8) -> Result<usize> {
        let mut n = (op & 0x0f) as usize;
        if n == 0 {
            loop {
                let b = self.next()?;
                if b == 0 {
                    n += 0xff;
                } else {
                    n += 0x0f + b as usize;
                    break;
                }
            }
        }
        Ok(n)
    }

    /// Length of a back copy for the 0x10..0x3F opcodes.
    fn compressed_bytes(&mut self, op: u8, valid_bits: u8) -> Result<usize> {
        let mut n = (op & valid_bits) as usize;
        if n == 0 {
            loop {
                let b = self.next()?;
                if b == 0 {
                    n += 0xff;
                } else {
                    n += b as usize + valid_bits as usize;
                    break;
                }
            }
        }
        Ok(n + 2)
    }

    /// Two byte offset; returns (offset part, first byte).
    #[inline]
    fn two_byte_offset(&mut self) -> Result<(usize, u8)> {
        let first = self.next()?;
        let second = self.next()?;
        Ok((((first >> 2) as usize) | ((second as usize) << 6), first))
    }
}

struct Dst<'a> {
    data: &'a mut [u8],
    pos: usize,
}

impl Dst<'_> {
    #[inline]
    fn literal(&mut self, src: &mut Src<'_>, n: usize) -> Result<()> {
        let s = src
            .data
            .get(src.pos..src.pos + n)
            .ok_or_else(|| DwgError::corrupt("LZ77 literal run past input end"))?;
        let d = self
            .data
            .get_mut(self.pos..self.pos + n)
            .ok_or_else(|| DwgError::corrupt("LZ77 output larger than page"))?;
        d.copy_from_slice(s);
        src.pos += n;
        self.pos += n;
        Ok(())
    }

    #[inline]
    fn back_copy(&mut self, offset: usize, n: usize) -> Result<()> {
        if offset == 0 || offset > self.pos {
            return Err(DwgError::corrupt(format!(
                "LZ77 back offset {offset} before start (at {})",
                self.pos
            )));
        }
        if self.pos + n > self.data.len() {
            return Err(DwgError::corrupt("LZ77 output larger than page"));
        }
        let from = self.pos - offset;
        if offset >= n {
            self.data.copy_within(from..from + n, self.pos);
        } else {
            for i in 0..n {
                self.data[self.pos + i] = self.data[from + i];
            }
        }
        self.pos += n;
        Ok(())
    }
}

/// Decompress `src` into `dst`. Returns the number of bytes written.
///
/// `dst` must be large enough for the whole output; the output may be shorter
/// than `dst` (the caller decides whether that is an error).
pub fn decompress(src: &[u8], dst: &mut [u8]) -> Result<usize> {
    let mut s = Src { data: src, pos: 0 };
    let mut d = Dst { data: dst, pos: 0 };

    let mut op = s.next()?;
    if op > 0x11 {
        d.literal(&mut s, (op - 17) as usize)?;
        op = s.next()?;
    }
    if op & 0xf0 == 0 {
        let n = s.literal_count(op)? + 3;
        d.literal(&mut s, n)?;
        op = s.next()?;
    }

    while op != 0x11 {
        let (comp_bytes, comp_offset, lit_op) = if op >= 0x40 {
            let op2 = s.next()?;
            let len = ((op >> 4) - 1) as usize;
            let off = ((((op >> 2) & 3) as usize) | ((op2 as usize) << 2)) + 1;
            (len, off, op)
        } else if op >= 0x20 {
            let len = s.compressed_bytes(op, 0x1f)?;
            let (off, first) = s.two_byte_offset()?;
            (len, off + 1, first)
        } else if op >= 0x10 {
            let len = s.compressed_bytes(op, 0x07)?;
            let (off, first) = s.two_byte_offset()?;
            (len, off + (((op & 8) as usize) << 11) + 0x4000, first)
        } else {
            return Err(DwgError::corrupt(format!(
                "LZ77 opcode {op:#04x} not allowed here (input byte {})",
                s.pos - 1
            )));
        };

        d.back_copy(comp_offset, comp_bytes)?;

        let mut lit = (lit_op & 3) as usize;
        if lit == 0 {
            op = s.next()?;
            if op & 0xf0 == 0 {
                lit = s.literal_count(op)? + 3;
            }
        }
        if lit > 0 {
            d.literal(&mut s, lit)?;
            op = s.next()?;
        }
    }
    Ok(d.pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_only_stream() {
        // literal length 0x01 -> 4 bytes, then end marker 0x11
        let src = [0x01, b'a', b'b', b'c', b'd', 0x11];
        let mut out = [0u8; 16];
        let n = decompress(&src, &mut out).unwrap();
        assert_eq!(&out[..n], b"abcd");
    }

    #[test]
    fn long_literal_length() {
        // 0x00 0x02 -> 0x0F + 2 + 3 = 20 bytes (spec example)
        let mut src = vec![0x00, 0x02];
        src.extend((0..20u8).map(|i| b'A' + i));
        src.push(0x11);
        let mut out = [0u8; 32];
        let n = decompress(&src, &mut out).unwrap();
        assert_eq!(n, 20);
        assert_eq!(out[19], b'A' + 19);
    }

    #[test]
    fn short_back_copy_with_overlap() {
        // "abcd", then opcode 0x40..: len = (op>>4)-1, offset = ((op>>2)&3 | op2<<2)+1
        // op = 0x51: len = 4, offset bits 0 -> offset 1 -> repeat 'd' 4 times,
        // literal count = op & 3 = 1 -> one literal 'z', then 0x11.
        let src = [0x01, b'a', b'b', b'c', b'd', 0x51, 0x00, b'z', 0x11];
        let mut out = [0u8; 16];
        let n = decompress(&src, &mut out).unwrap();
        assert_eq!(&out[..n], b"abcdddddz");
    }

    #[test]
    fn opcode_0x2x_copy() {
        // "abcd", op 0x22: len = 2+2 = 4; two-byte offset: first = 0x0c
        // (offset bits 3, lit 0), second 0 -> offset 3+1 = 4 -> copies "abcd".
        // lit = 0 -> next byte 0x11 is the end marker (high nibble set).
        let src = [0x01, b'a', b'b', b'c', b'd', 0x22, 0x0c, 0x00, 0x11];
        let mut out = [0u8; 16];
        let n = decompress(&src, &mut out).unwrap();
        assert_eq!(&out[..n], b"abcdabcd");
    }

    #[test]
    fn first_byte_above_0x11_is_literal_run() {
        let src = [0x12 + 2, b'x', b'y', b'z', 0x11];
        let mut out = [0u8; 8];
        let n = decompress(&src, &mut out).unwrap();
        assert_eq!(&out[..n], b"xyz");
    }

    #[test]
    fn bad_offset_is_error() {
        // copy from before the start
        let src = [0x01, b'a', b'b', b'c', b'd', 0x5c, 0xff, 0x11];
        let mut out = [0u8; 16];
        assert!(decompress(&src, &mut out).is_err());
    }

    #[test]
    fn output_overflow_is_error() {
        let src = [0x01, b'a', b'b', b'c', b'd', 0x11];
        let mut out = [0u8; 3];
        assert!(decompress(&src, &mut out).is_err());
    }

    #[test]
    fn truncated_input_is_error() {
        let src = [0x01, b'a', b'b'];
        let mut out = [0u8; 16];
        assert!(decompress(&src, &mut out).is_err());
        assert!(decompress(&[], &mut out).is_err());
    }
}
