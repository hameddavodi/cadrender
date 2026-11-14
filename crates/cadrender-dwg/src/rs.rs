//! Reed-Solomon codes of R2007 files (ODA spec 5.13).
//!
//! R2007 protects its pages with Reed-Solomon (RS) codes over GF(256).
//! A *code word* is 255 bytes: `k` data bytes, then `255 - k` parity bytes.
//!
//! * System pages (file header, page map, section map): RS(255, 239),
//!   primitive polynomial x^8 + x^6 + x^5 + x^3 + 1 (0x169). Corrects up to
//!   8 bad bytes per code word.
//! * Data pages: RS(255, 251), primitive polynomial x^8 + x^4 + x^3 + x^2 + 1
//!   (0x11D). Corrects up to 2 bad bytes per code word.
//!
//! The generator polynomial has the roots alpha^1 .. alpha^(255-k). Inside
//! one code word, data byte `i` is the coefficient of x^(i + 255 - k) and
//! parity byte `j` the coefficient of x^j (checked on real files: all
//! syndromes are zero with this layout).
//!
//! With *interleaving* (system pages and "encoding 4" data pages) the bytes
//! of `blocks` code words are mixed: byte `j` of code word `b` is stored at
//! `j * blocks + b`. Without interleaving the data blocks come first.
//!
//! Reading a healthy page needs no math at all: the code is systematic, so
//! [`deinterleave`] just picks the data bytes. [`RsCode::correct`] is the
//! slow path, used only when a page checksum says the data is damaged.

use crate::error::{DwgError, Result};

/// Bytes in one RS code word.
pub const N: usize = 255;

/// Copy the data bytes of `blocks` interleaved code words (each with `k`
/// data bytes) from `encoded` into `out`. `out.len()` may be smaller than
/// `blocks * k` (the end of the last block is padding).
pub fn deinterleave(encoded: &[u8], out: &mut [u8], blocks: usize, k: usize) -> Result<()> {
    if blocks == 0 {
        return if out.is_empty() { Ok(()) } else { Err(DwgError::corrupt("RS: no code words")) };
    }
    if out.len() > blocks * k {
        return Err(DwgError::corrupt("RS: output larger than the code words hold"));
    }
    let need = blocks * N;
    if encoded.len() < need {
        return Err(DwgError::corrupt(format!(
            "RS: {} code words need {need} bytes, page has {}",
            blocks,
            encoded.len()
        )));
    }
    for (b, chunk) in out.chunks_mut(k).enumerate() {
        for (j, o) in chunk.iter_mut().enumerate() {
            *o = encoded[j * blocks + b];
        }
    }
    Ok(())
}

/// One RS(255, k) code with its Galois field tables.
pub struct RsCode {
    /// Data bytes per code word.
    pub k: usize,
    exp: [u8; 512],
    log: [u16; 256],
}

impl RsCode {
    /// Build a code from its primitive polynomial (with the x^8 bit, for
    /// example 0x169) and its number of data bytes.
    pub fn new(poly: u16, k: usize) -> RsCode {
        let mut exp = [0u8; 512];
        let mut log = [0u16; 256];
        let mut x: u16 = 1;
        for (i, e) in exp.iter_mut().enumerate().take(N) {
            *e = x as u8;
            log[x as usize] = i as u16;
            x <<= 1;
            if x & 0x100 != 0 {
                x ^= poly;
            }
        }
        for i in N..512 {
            exp[i] = exp[i - N];
        }
        RsCode { k, exp, log }
    }

    /// The code of system pages: RS(255, 239), polynomial 0x169.
    pub fn system() -> RsCode {
        RsCode::new(0x169, 239)
    }

    /// The code of data pages: RS(255, 251), polynomial 0x11D.
    pub fn data() -> RsCode {
        RsCode::new(0x11D, 251)
    }

    fn parity_len(&self) -> usize {
        N - self.k
    }

    #[inline]
    fn mul(&self, a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            0
        } else {
            self.exp[self.log[a as usize] as usize + self.log[b as usize] as usize]
        }
    }

    #[inline]
    fn div(&self, a: u8, b: u8) -> u8 {
        debug_assert!(b != 0);
        if a == 0 {
            0
        } else {
            self.exp[self.log[a as usize] as usize + N - self.log[b as usize] as usize]
        }
    }

    /// alpha^e for any e >= 0.
    #[inline]
    fn pow(&self, e: usize) -> u8 {
        self.exp[e % N]
    }

    /// Byte position of polynomial degree `d`: data byte `i` has degree
    /// `i + 255 - k`, parity byte `j` degree `j`.
    #[inline]
    fn position(&self, d: usize) -> usize {
        if d >= self.parity_len() {
            d - self.parity_len()
        } else {
            d + self.k
        }
    }

    /// The 255-k syndromes S_1..S_(255-k) of one code word (all zero for a
    /// healthy code word).
    pub fn syndromes(&self, word: &[u8; N]) -> Vec<u8> {
        (1..=self.parity_len())
            .map(|j| {
                let aj = self.pow(j);
                let mut s = 0u8;
                for d in (0..N).rev() {
                    s = self.mul(s, aj) ^ word[self.position(d)];
                }
                s
            })
            .collect()
    }

    /// Compute the parity bytes for `data` (k bytes) and return the full
    /// code word. Our reader never writes files; this is for tests and for
    /// building synthetic pages.
    pub fn encode(&self, data: &[u8]) -> [u8; N] {
        assert_eq!(data.len(), self.k);
        let p = self.parity_len();
        // Generator g(x) = (x + a^1)(x + a^2)...(x + a^p), coefficients low first.
        let mut g = vec![0u8; p + 1];
        g[0] = 1;
        for i in 1..=p {
            let root = self.pow(i);
            for j in (1..=i).rev() {
                g[j] = g[j - 1] ^ self.mul(g[j], root);
            }
            g[0] = self.mul(g[0], root);
        }
        let mut par = vec![0u8; p];
        for i in (0..self.k).rev() {
            let fb = data[i] ^ par[p - 1];
            for j in (1..p).rev() {
                par[j] = par[j - 1] ^ self.mul(g[j], fb);
            }
            par[0] = self.mul(g[0], fb);
        }
        let mut word = [0u8; N];
        word[..self.k].copy_from_slice(data);
        word[self.k..].copy_from_slice(&par);
        word
    }
}

impl RsCode {
    /// Correct a damaged code word in place (Berlekamp-Massey, Chien search,
    /// Forney). Returns the number of corrected bytes, or an error when the
    /// damage is too large to correct.
    pub fn correct(&self, word: &mut [u8; N]) -> Result<usize> {
        let s = self.syndromes(word);
        if s.iter().all(|&v| v == 0) {
            return Ok(0);
        }
        let two_t = s.len();
        // Berlekamp-Massey: error locator polynomial c (low coefficient first).
        let mut c = vec![0u8; two_t + 1];
        let mut b = vec![0u8; two_t + 1];
        c[0] = 1;
        b[0] = 1;
        let (mut l, mut m, mut bb) = (0usize, 1usize, 1u8);
        for n in 0..two_t {
            let mut d = s[n];
            for i in 1..=l {
                d ^= self.mul(c[i], s[n - i]);
            }
            if d == 0 {
                m += 1;
                continue;
            }
            let coef = self.div(d, bb);
            let prev = c.clone();
            for i in 0..=two_t - m {
                c[i + m] ^= self.mul(coef, b[i]);
            }
            if 2 * l <= n {
                l = n + 1 - l;
                b = prev;
                bb = d;
                m = 1;
            } else {
                m += 1;
            }
        }
        if l > two_t / 2 {
            return Err(DwgError::corrupt("RS: too many errors to correct"));
        }
        let eval = |p: &[u8], x: u8| p.iter().rev().fold(0u8, |acc, &co| self.mul(acc, x) ^ co);
        // Omega = S(x) * Lambda(x) mod x^(2t).
        let mut omega = vec![0u8; two_t];
        for (i, o) in omega.iter_mut().enumerate() {
            for j in 0..=i.min(l) {
                *o ^= self.mul(c[j], s[i - j]);
            }
        }
        // Formal derivative of Lambda: odd terms only (characteristic 2).
        let deriv: Vec<u8> = (1..=l).map(|i| if i % 2 == 1 { c[i] } else { 0 }).collect();
        let mut fixed = 0;
        for d in 0..N {
            let x_inv = self.pow(N - d);
            if eval(&c[..=l], x_inv) != 0 {
                continue;
            }
            let den = eval(&deriv, x_inv);
            if den == 0 {
                return Err(DwgError::corrupt("RS: bad error locator"));
            }
            word[self.position(d)] ^= self.div(eval(&omega, x_inv), den);
            fixed += 1;
        }
        if fixed != l || self.syndromes(word).iter().any(|&v| v != 0) {
            return Err(DwgError::corrupt("RS: too many errors to correct"));
        }
        Ok(fixed)
    }

    /// Correct every code word of an interleaved run of `blocks` code words
    /// in place. Returns the number of corrected bytes.
    pub fn correct_interleaved(&self, encoded: &mut [u8], blocks: usize) -> Result<usize> {
        if encoded.len() < blocks * N {
            return Err(DwgError::corrupt("RS: page shorter than its code words"));
        }
        let mut total = 0;
        let mut word = [0u8; N];
        for b in 0..blocks {
            for (j, w) in word.iter_mut().enumerate() {
                *w = encoded[j * blocks + b];
            }
            let n = self.correct(&mut word)?;
            if n > 0 {
                for (j, w) in word.iter().enumerate() {
                    encoded[j * blocks + b] = *w;
                }
                total += n;
            }
        }
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_data(k: usize) -> Vec<u8> {
        (0..k).map(|i| (i * 7 + 3) as u8).collect()
    }

    /// Parity bytes made by an independent long-division script (not by
    /// the code under test).
    #[test]
    fn encode_known_vectors() {
        let sys = RsCode::system();
        let w = sys.encode(&sample_data(239));
        assert_eq!(
            &w[239..],
            &[0x69, 0x9F, 0xF9, 0x01, 0xFF, 0x7B, 0x41, 0x89, 0xB4, 0x78, 0x3C, 0x56, 0x76, 0xFE, 0x44, 0x9F]
        );
        let dat = RsCode::data();
        let w = dat.encode(&sample_data(251));
        assert_eq!(&w[251..], &[0x39, 0xC6, 0x19, 0x0E]);
    }

    #[test]
    fn field_tables() {
        // 0x11D is the common "QR code" field: alpha^8 = 0x1D.
        assert_eq!(RsCode::data().pow(8), 0x1D);
        assert_eq!(RsCode::system().pow(8), 0x69);
        let c = RsCode::system();
        for a in 1..=255u8 {
            assert_eq!(c.mul(c.div(1, a), a), 1);
        }
    }

    #[test]
    fn healthy_word_has_zero_syndromes() {
        let c = RsCode::system();
        let w = c.encode(&sample_data(239));
        assert!(c.syndromes(&w).iter().all(|&s| s == 0));
    }

    #[test]
    fn corrects_up_to_t_errors() {
        for (c, t) in [(RsCode::system(), 8), (RsCode::data(), 2)] {
            let good = c.encode(&sample_data(c.k));
            for n in 1..=t {
                let mut w = good;
                for e in 0..n {
                    w[(e * 37 + 5) % N] ^= (e as u8).wrapping_mul(29) | 1;
                }
                assert_eq!(c.correct(&mut w).unwrap(), n);
                assert_eq!(w, good);
            }
            // One error too many: must not claim success with wrong data.
            let mut w = good;
            for e in 0..=t {
                w[(e * 37 + 5) % N] ^= 0x5A;
            }
            if c.correct(&mut w).is_ok() {
                assert_ne!(w, good, "a miscorrection is allowed, a silent pass is not");
            }
        }
    }

    #[test]
    fn deinterleave_and_correct_interleaved() {
        let c = RsCode::system();
        let blocks = 3;
        let data: Vec<u8> = (0..blocks * 239).map(|i| (i * 13 + 1) as u8).collect();
        let words: Vec<[u8; N]> = data.chunks(239).map(|d| c.encode(d)).collect();
        let mut enc = vec![0u8; blocks * N];
        for (b, w) in words.iter().enumerate() {
            for (j, &v) in w.iter().enumerate() {
                enc[j * blocks + b] = v;
            }
        }
        let mut out = vec![0u8; 700];
        deinterleave(&enc, &mut out, blocks, 239).unwrap();
        assert_eq!(out, data[..700]);

        enc[10] ^= 0xFF;
        enc[400] ^= 0x01;
        assert_eq!(c.correct_interleaved(&mut enc, blocks).unwrap(), 2);
        deinterleave(&enc, &mut out, blocks, 239).unwrap();
        assert_eq!(out, data[..700]);

        assert!(deinterleave(&enc[..100], &mut out, blocks, 239).is_err());
        assert!(deinterleave(&enc, &mut vec![0u8; 800], blocks, 239).is_err());
    }
}
