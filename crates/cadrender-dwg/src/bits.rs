//! Bounds-checked bit reader for the DWG bit codes.
//!
//! DWG objects are "bit packed": values do not start on byte borders.
//! The Open Design Specification (chapter 2) defines short codes such as
//! BS (bit short), BL (bit long), BD (bit double), MC (modular char) and
//! handle references. This module reads all of them.
//!
//! Every read checks the limit first and returns [`DwgError::Eof`] instead of
//! panicking. Bits are read most significant bit first inside each byte;
//! multi-byte raw values are little-endian.

use crate::error::{DwgError, Result};

/// A reader over a byte slice that counts in bits.
///
/// It is `Copy`, so a caller can save a position by copying the reader.
#[derive(Clone, Copy, Debug)]
pub struct BitReader<'a> {
    buf: &'a [u8],
    /// Current position in bits from the start of `buf`.
    pos: usize,
    /// Reads may not go past this bit position.
    end: usize,
}

/// A handle reference as stored in the file: `CODE.COUNTER.BYTES`.
///
/// `code` 2..=5 means the value is absolute. Codes 6, 8, 0xA and 0xC are
/// offsets from a reference handle (see [`HandleRef::absolute`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HandleRef {
    pub code: u8,
    pub value: u64,
}

impl HandleRef {
    /// Turn the reference into an absolute handle. `base` is the handle of
    /// the object that holds this reference (used by the offset codes).
    #[inline]
    pub fn absolute(self, base: u64) -> u64 {
        match self.code {
            0x6 => base.wrapping_add(1),
            0x8 => base.wrapping_sub(1),
            0xA => base.wrapping_add(self.value),
            0xC => base.wrapping_sub(self.value),
            _ => self.value,
        }
    }
}

#[cold]
#[inline(never)]
fn eof(at: usize, limit: usize) -> DwgError {
    DwgError::Eof { at, limit }
}

impl<'a> BitReader<'a> {
    /// A reader over the whole slice, starting at bit 0.
    pub fn new(buf: &'a [u8]) -> Self {
        BitReader { buf, pos: 0, end: buf.len() * 8 }
    }

    /// A reader over `buf` that may only read bits `start..end`.
    /// `end` is clamped to the slice length.
    pub fn with_range(buf: &'a [u8], start: usize, end: usize) -> Self {
        let end = end.min(buf.len() * 8);
        BitReader { buf, pos: start.min(end), end }
    }

    /// Current bit position (from the start of the underlying slice).
    #[inline]
    pub fn pos(&self) -> usize {
        self.pos
    }

    /// The bit limit.
    #[inline]
    pub fn end(&self) -> usize {
        self.end
    }

    /// The underlying byte slice.
    pub fn buf(&self) -> &'a [u8] {
        self.buf
    }

    /// Bits left before the limit.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.end.saturating_sub(self.pos)
    }

    /// Move to an absolute bit position. Fails if it is past the limit.
    pub fn seek(&mut self, bit: usize) -> Result<()> {
        if bit > self.end {
            return Err(eof(bit, self.end));
        }
        self.pos = bit;
        Ok(())
    }

    /// Change the limit (it can only shrink to inside the slice).
    pub fn set_end(&mut self, end: usize) {
        self.end = end.min(self.buf.len() * 8);
    }

    /// Skip `n` bits.
    #[inline]
    pub fn skip(&mut self, n: usize) -> Result<()> {
        let np = self.pos.checked_add(n).ok_or_else(|| eof(usize::MAX, self.end))?;
        if np > self.end {
            return Err(eof(np, self.end));
        }
        self.pos = np;
        Ok(())
    }

    /// Move forward to the next byte border.
    pub fn align(&mut self) {
        self.pos = (self.pos + 7) & !7;
        if self.pos > self.end {
            self.pos = self.end;
        }
    }

    /// Load 8 bytes starting at the byte that holds the current bit,
    /// as a big-endian number. Bytes past the slice read as 0.
    #[inline(always)]
    fn peek64(&self) -> u64 {
        let i = self.pos >> 3;
        if let Some(b) = self.buf.get(i..i + 8) {
            u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]])
        } else {
            let mut tmp = [0u8; 8];
            let tail = self.buf.get(i..).unwrap_or(&[]);
            tmp[..tail.len()].copy_from_slice(tail);
            u64::from_be_bytes(tmp)
        }
    }

    /// Read `n` bits (1..=57) as an unsigned number, first bit is the top bit.
    #[inline(always)]
    pub fn bits(&mut self, n: u32) -> Result<u64> {
        debug_assert!((1..=57).contains(&n));
        let np = self.pos + n as usize;
        if np > self.end {
            return Err(eof(np, self.end));
        }
        let v = (self.peek64() << (self.pos & 7)) >> (64 - n);
        self.pos = np;
        Ok(v)
    }

    /// B: one bit.
    #[inline(always)]
    pub fn b(&mut self) -> Result<bool> {
        Ok(self.bits(1)? != 0)
    }

    /// BB: two bits.
    #[inline(always)]
    pub fn bb(&mut self) -> Result<u8> {
        Ok(self.bits(2)? as u8)
    }

    /// 3B: one to three bits, stop at the first zero bit.
    pub fn b3(&mut self) -> Result<u8> {
        let mut v = 0u8;
        for _ in 0..3 {
            v <<= 1;
            if self.b()? {
                v |= 1;
            } else {
                break;
            }
        }
        Ok(v)
    }

    /// RC: raw char (one byte, maybe not aligned).
    #[inline(always)]
    pub fn rc(&mut self) -> Result<u8> {
        Ok(self.bits(8)? as u8)
    }

    /// RS: raw little-endian short.
    #[inline(always)]
    pub fn rs(&mut self) -> Result<u16> {
        Ok((self.bits(16)? as u16).swap_bytes())
    }

    /// RL: raw little-endian 32-bit long.
    #[inline(always)]
    pub fn rl(&mut self) -> Result<u32> {
        Ok((self.bits(32)? as u32).swap_bytes())
    }

    /// Raw little-endian 64-bit value.
    #[inline(always)]
    pub fn rll(&mut self) -> Result<u64> {
        let lo = self.rl()? as u64;
        let hi = self.rl()? as u64;
        Ok(lo | (hi << 32))
    }

    /// RD: raw IEEE double.
    #[inline(always)]
    pub fn rd(&mut self) -> Result<f64> {
        Ok(f64::from_bits(self.rll()?))
    }

    /// 2RD
    #[inline]
    pub fn rd2(&mut self) -> Result<[f64; 2]> {
        Ok([self.rd()?, self.rd()?])
    }

    /// 3RD
    #[inline]
    pub fn rd3(&mut self) -> Result<[f64; 3]> {
        Ok([self.rd()?, self.rd()?, self.rd()?])
    }

    /// BS: bit short.
    #[inline]
    pub fn bs(&mut self) -> Result<i16> {
        match self.bb()? {
            0 => Ok(self.rs()? as i16),
            1 => Ok(self.rc()? as i16),
            2 => Ok(0),
            _ => Ok(256),
        }
    }

    /// BL: bit long.
    #[inline]
    pub fn bl(&mut self) -> Result<i32> {
        match self.bb()? {
            0 => Ok(self.rl()? as i32),
            1 => Ok(self.rc()? as i32),
            2 => Ok(0),
            _ => Err(DwgError::corrupt(format!("BL code 3 at bit {}", self.pos - 2))),
        }
    }

    /// BLL: bit long long. A fixed 3-bit byte count (0..=7), then that many
    /// bytes, LSB first. (The spec calls the count "3B"; real R2018 files
    /// and ACadSharp use a plain 3-bit field, see README.)
    pub fn bll(&mut self) -> Result<u64> {
        let len = self.bits(3)? as u8;
        let mut v = 0u64;
        for i in 0..len {
            v |= (self.rc()? as u64) << (8 * i);
        }
        Ok(v)
    }

    /// BD: bit double.
    #[inline]
    pub fn bd(&mut self) -> Result<f64> {
        match self.bb()? {
            0 => self.rd(),
            1 => Ok(1.0),
            2 => Ok(0.0),
            _ => Err(DwgError::corrupt(format!("BD code 3 at bit {}", self.pos - 2))),
        }
    }

    /// 2BD
    #[inline]
    pub fn bd2(&mut self) -> Result<[f64; 2]> {
        Ok([self.bd()?, self.bd()?])
    }

    /// 3BD
    #[inline]
    pub fn bd3(&mut self) -> Result<[f64; 3]> {
        Ok([self.bd()?, self.bd()?, self.bd()?])
    }

    /// DD: bit double with a default value.
    pub fn dd(&mut self, default: f64) -> Result<f64> {
        match self.bb()? {
            0 => Ok(default),
            1 => {
                let mut b = default.to_le_bytes();
                for x in b.iter_mut().take(4) {
                    *x = self.rc()?;
                }
                Ok(f64::from_le_bytes(b))
            }
            2 => {
                let mut b = default.to_le_bytes();
                b[4] = self.rc()?;
                b[5] = self.rc()?;
                for x in b.iter_mut().take(4) {
                    *x = self.rc()?;
                }
                Ok(f64::from_le_bytes(b))
            }
            _ => self.rd(),
        }
    }

    /// 2DD with a default point.
    pub fn dd2(&mut self, def: [f64; 2]) -> Result<[f64; 2]> {
        Ok([self.dd(def[0])?, self.dd(def[1])?])
    }

    /// BE: bit extrusion (R2000+). One bit 1 means (0,0,1).
    #[inline]
    pub fn be(&mut self) -> Result<[f64; 3]> {
        if self.b()? {
            Ok([0.0, 0.0, 1.0])
        } else {
            self.bd3()
        }
    }

    /// BT: bit thickness (R2000+). One bit 1 means 0.0.
    #[inline]
    pub fn bt(&mut self) -> Result<f64> {
        if self.b()? {
            Ok(0.0)
        } else {
            self.bd()
        }
    }

    /// MC: unsigned modular char (used in the object map for handles).
    pub fn mc_unsigned(&mut self) -> Result<u64> {
        let mut v = 0u64;
        let mut shift = 0u32;
        loop {
            let byte = self.rc()?;
            if shift < 64 {
                v |= ((byte & 0x7f) as u64) << shift;
            }
            if byte & 0x80 == 0 {
                return Ok(v);
            }
            shift += 7;
            if shift > 63 {
                return Err(DwgError::corrupt("modular char longer than 9 bytes"));
            }
        }
    }

    /// MC: signed modular char. In the last byte, bit 0x40 is the sign.
    pub fn mc_signed(&mut self) -> Result<i64> {
        let mut v = 0i64;
        let mut shift = 0u32;
        loop {
            let byte = self.rc()?;
            if byte & 0x80 == 0 {
                v |= ((byte & 0x3f) as i64) << shift;
                if byte & 0x40 != 0 {
                    v = -v;
                }
                return Ok(v);
            }
            v |= ((byte & 0x7f) as i64) << shift;
            shift += 7;
            if shift > 56 {
                return Err(DwgError::corrupt("signed modular char longer than 9 bytes"));
            }
        }
    }

    /// MS: modular short (object sizes). Each module is a little-endian
    /// short; its top bit says another module follows.
    pub fn ms(&mut self) -> Result<u32> {
        let mut v = 0u32;
        let mut shift = 0u32;
        loop {
            let m = self.rs()? as u32;
            v |= (m & 0x7fff) << shift;
            if m & 0x8000 == 0 {
                return Ok(v);
            }
            shift += 15;
            if shift > 30 {
                return Err(DwgError::corrupt("modular short longer than 3 modules"));
            }
        }
    }

    /// OT: object type, R2010+ form (two bits, then one or two bytes).
    pub fn ot_r2010(&mut self) -> Result<u16> {
        match self.bb()? {
            0 => Ok(self.rc()? as u16),
            1 => Ok(self.rc()? as u16 + 0x1f0),
            _ => self.rs(),
        }
    }

    /// H: handle reference. The handle bytes are stored MSB first.
    #[inline]
    pub fn h(&mut self) -> Result<HandleRef> {
        let form = self.rc()?;
        let code = form >> 4;
        let counter = (form & 0x0f) as u32;
        let value = match counter {
            0 => 0,
            1..=7 => self.bits(8 * counter)?,
            8 => {
                let hi = self.bits(32)?;
                let lo = self.bits(32)?;
                (hi << 32) | lo
            }
            _ => {
                return Err(DwgError::corrupt(format!(
                    "handle with {counter} bytes at bit {}",
                    self.pos - 8
                )))
            }
        };
        Ok(HandleRef { code, value })
    }

    /// Read `n` raw bytes (maybe not aligned) into a new vector.
    pub fn bytes(&mut self, n: usize) -> Result<Vec<u8>> {
        let need = n.checked_mul(8).ok_or_else(|| eof(usize::MAX, self.end))?;
        if self.pos + need > self.end {
            return Err(eof(self.pos + need, self.end));
        }
        if self.pos & 7 == 0 {
            let i = self.pos >> 3;
            self.pos += need;
            return Ok(self.buf[i..i + n].to_vec());
        }
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            out.push(self.rc()?);
        }
        Ok(out)
    }

    /// TU: unicode text (BS length in characters, then UTF-16LE).
    pub fn tu(&mut self) -> Result<String> {
        let len = self.bs()? as u16 as usize;
        if len * 16 > self.remaining() {
            return Err(eof(self.pos + len * 16, self.end));
        }
        let mut units = Vec::with_capacity(len);
        for _ in 0..len {
            units.push(self.rs()?);
        }
        // Strings are often stored with a trailing zero.
        while units.last() == Some(&0) {
            units.pop();
        }
        Ok(String::from_utf16_lossy(&units))
    }

    /// T: 8-bit text (R2004 and older): BS length, then bytes.
    /// Bytes are read as Windows-1252 / Latin-1, which covers the code
    /// pages seen in our drawings.
    pub fn t8(&mut self) -> Result<String> {
        let len = self.bs()? as u16 as usize;
        if len * 8 > self.remaining() {
            return Err(eof(self.pos + len * 8, self.end));
        }
        let mut s = String::with_capacity(len);
        for _ in 0..len {
            let c = self.rc()?;
            if c == 0 {
                continue;
            }
            s.push(c as char);
        }
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pack a string of '0'/'1' characters into bytes (MSB first).
    fn pack(bits: &str) -> Vec<u8> {
        let bits: Vec<u8> = bits.bytes().filter(|c| *c == b'0' || *c == b'1').collect();
        let mut out = vec![0u8; bits.len().div_ceil(8)];
        for (i, c) in bits.iter().enumerate() {
            if *c == b'1' {
                out[i / 8] |= 0x80 >> (i % 8);
            }
        }
        out
    }

    #[test]
    fn bitshort_spec_example() {
        // ODA spec 2.2: five shorts 257, 0, 256, 15, 0.
        let data = pack("0000000001000000011011010000111110");
        let mut r = BitReader::with_range(&data, 0, 34);
        assert_eq!(r.bs().unwrap(), 257);
        assert_eq!(r.bs().unwrap(), 0);
        assert_eq!(r.bs().unwrap(), 256);
        assert_eq!(r.bs().unwrap(), 15);
        assert_eq!(r.bs().unwrap(), 0);
        assert!(r.bs().is_err(), "must stop at the limit");
    }

    #[test]
    fn bitlong_spec_example() {
        // ODA spec 2.3: 257, 0, 15, 0.
        let data = pack("00 00000001 00000001 00000000 00000000 10 01 00001111 10");
        let mut r = BitReader::new(&data);
        assert_eq!(r.bl().unwrap(), 257);
        assert_eq!(r.bl().unwrap(), 0);
        assert_eq!(r.bl().unwrap(), 15);
        assert_eq!(r.bl().unwrap(), 0);
    }

    #[test]
    fn negative_bitshort_uses_full_form() {
        let data = pack("00 11111111 11111111");
        let mut r = BitReader::new(&data);
        assert_eq!(r.bs().unwrap(), -1);
    }

    #[test]
    fn bitdouble_codes() {
        let mut bits = String::from("01 10 00");
        for b in 2.5f64.to_le_bytes() {
            bits.push_str(&format!("{b:08b}"));
        }
        let data = pack(&bits);
        let mut r = BitReader::new(&data);
        assert_eq!(r.bd().unwrap(), 1.0);
        assert_eq!(r.bd().unwrap(), 0.0);
        assert_eq!(r.bd().unwrap(), 2.5);
    }

    #[test]
    fn bitdouble_code3_is_an_error_not_a_panic() {
        let data = pack("11000000");
        let mut r = BitReader::new(&data);
        assert!(matches!(r.bd(), Err(DwgError::Corrupt(_))));
    }

    #[test]
    fn modular_short_spec_example() {
        // ODA spec 2.7: 31 F4 8D 00 -> 4650033
        let data = [0x31, 0xF4, 0x8D, 0x00];
        let mut r = BitReader::new(&data);
        assert_eq!(r.ms().unwrap(), 4_650_033);
    }

    #[test]
    fn modular_chars() {
        let data = [0x82, 0x24];
        assert_eq!(BitReader::new(&data).mc_unsigned().unwrap(), 2 | (0x24 << 7));
        // 0x45: last byte, sign bit 0x40 set, value 5 -> -5
        assert_eq!(BitReader::new(&[0x45]).mc_signed().unwrap(), -5);
        assert_eq!(BitReader::new(&[0x05]).mc_signed().unwrap(), 5);
        // 0x81 0x41 -> 1 | (1 << 7) = 129, negative
        assert_eq!(BitReader::new(&[0x81, 0x41]).mc_signed().unwrap(), -129);
        // runaway modular char must fail cleanly
        assert!(BitReader::new(&[0xff; 12]).mc_unsigned().is_err());
        assert!(BitReader::new(&[0xff; 3]).mc_unsigned().is_err());
    }

    #[test]
    fn handle_reference_spec_example() {
        // ODA spec 2.13: 5.2.05.E7 -> hard pointer to 0x5E7
        let data = [0x52, 0x05, 0xE7];
        let h = BitReader::new(&data).h().unwrap();
        assert_eq!(h, HandleRef { code: 5, value: 0x5E7 });
        assert_eq!(h.absolute(0x10), 0x5E7);
        // offset forms
        assert_eq!(HandleRef { code: 6, value: 0 }.absolute(0x10), 0x11);
        assert_eq!(HandleRef { code: 8, value: 0 }.absolute(0x10), 0xF);
        assert_eq!(HandleRef { code: 0xA, value: 3 }.absolute(0x10), 0x13);
        assert_eq!(HandleRef { code: 0xC, value: 3 }.absolute(0x10), 0xD);
    }

    #[test]
    fn handle_unaligned() {
        // one padding bit, then 5.1.0F
        let data = pack("1 0101 0001 00001111");
        let mut r = BitReader::new(&data);
        assert!(r.b().unwrap());
        assert_eq!(r.h().unwrap(), HandleRef { code: 5, value: 0xF });
    }

    #[test]
    fn handle_with_bad_counter_is_error() {
        let data = [0x5A, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        assert!(BitReader::new(&data).h().is_err());
    }

    #[test]
    fn raw_values_unaligned() {
        // 3 bits offset then RS 0x1234 and RL 0xdeadbeef
        let mut bits = String::from("101");
        for b in [0x34u8, 0x12, 0xef, 0xbe, 0xad, 0xde] {
            bits.push_str(&format!("{b:08b}"));
        }
        let data = pack(&bits);
        let mut r = BitReader::new(&data);
        assert_eq!(r.bits(3).unwrap(), 0b101);
        assert_eq!(r.rs().unwrap(), 0x1234);
        assert_eq!(r.rl().unwrap(), 0xdeadbeef);
    }

    #[test]
    fn default_double() {
        // code 00 -> default
        let data = pack("00");
        assert_eq!(BitReader::new(&data).dd(7.0).unwrap(), 7.0);
        // code 11 -> full RD
        let mut bits = String::from("11");
        for b in 3.25f64.to_le_bytes() {
            bits.push_str(&format!("{b:08b}"));
        }
        let data = pack(&bits);
        assert_eq!(BitReader::new(&data).dd(7.0).unwrap(), 3.25);
        // code 01 -> patch first 4 bytes
        let target = 7.0000001f64;
        let tb = target.to_le_bytes();
        let mut bits = String::from("01");
        for b in &tb[..4] {
            bits.push_str(&format!("{b:08b}"));
        }
        let data = pack(&bits);
        assert_eq!(BitReader::new(&data).dd(7.0).unwrap(), f64::from_le_bytes({
            let mut d = 7.0f64.to_le_bytes();
            d[..4].copy_from_slice(&tb[..4]);
            d
        }));
    }

    #[test]
    fn bitlonglong_and_3b() {
        // 3-bit length 2 ("010"), then two bytes 0x34 0x12
        let data = pack("010 00110100 00010010");
        assert_eq!(BitReader::new(&data).bll().unwrap(), 0x1234);
        let data = pack("111");
        assert_eq!(BitReader::new(&data).b3().unwrap(), 7);
    }

    #[test]
    fn unicode_text() {
        // BS length 3 (01 + 0x03), then "Ab" and a trailing zero
        let mut bits = String::from("01 00000011");
        for b in [b'A', 0, b'b', 0, 0, 0] {
            bits.push_str(&format!("{b:08b}"));
        }
        let data = pack(&bits);
        assert_eq!(BitReader::new(&data).tu().unwrap(), "Ab");
    }

    #[test]
    fn huge_text_length_is_eof_not_alloc() {
        // BS full form 0x7fff chars but nothing behind it
        let data = pack("00 11111111 01111111");
        assert!(matches!(BitReader::new(&data).tu(), Err(DwgError::Eof { .. })));
    }

    #[test]
    fn object_type_r2010() {
        assert_eq!(BitReader::new(&pack("00 00010011")).ot_r2010().unwrap(), 0x13);
        assert_eq!(BitReader::new(&pack("01 00000100")).ot_r2010().unwrap(), 0x1f4);
        assert_eq!(BitReader::new(&pack("10 11110100 00000001")).ot_r2010().unwrap(), 0x1f4);
    }

    #[test]
    fn reads_near_end_of_buffer() {
        // peek64 must pad when fewer than 8 bytes are left
        let data = [0xAB];
        let mut r = BitReader::new(&data);
        assert_eq!(r.rc().unwrap(), 0xAB);
        assert!(r.b().is_err());
    }
}
