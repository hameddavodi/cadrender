//! The 64-bit CRCs of R2007 files (ODA spec 5.12).
//!
//! Two kinds:
//! * **normal**: ECMA-182 polynomial 0x42F0E1EBA9EA3693, most significant
//!   bit first, result inverted at the end. Used for the file header.
//! * **mirrored**: shifts the other way, with the (reflected) polynomial
//!   0x95AC9329AC4BC9B5, no final inversion. Used for system pages and
//!   data pages.
//!
//! Both tables are built at compile time; the tests compare them with rows
//! of the tables printed in the spec. The seed is first mixed with the data
//! length by [`update_seed1`] or [`update_seed2`], depending on the use.

const NORMAL_POLY: u64 = 0x42F0_E1EB_A9EA_3693;
const MIRRORED_POLY: u64 = 0x95AC_9329_AC4B_C9B5;

const fn normal_table() -> [u64; 256] {
    let mut t = [0u64; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = (i as u64) << 56;
        let mut k = 0;
        while k < 8 {
            c = if c & (1 << 63) != 0 { (c << 1) ^ NORMAL_POLY } else { c << 1 };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

const fn mirrored_table() -> [u64; 256] {
    let mut t = [0u64; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u64;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { (c >> 1) ^ MIRRORED_POLY } else { c >> 1 };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

static NORMAL: [u64; 256] = normal_table();
static MIRRORED: [u64; 256] = mirrored_table();

/// Tables for "slicing by 8": `SLICE[k][i]` is the CRC of byte `i`
/// followed by `k` zero bytes. One step then handles 8 bytes with 8
/// independent lookups instead of 8 dependent ones (about 4x faster).
static MIRRORED_SLICE: [[u64; 256]; 8] = mirrored_slice_tables();

const fn mirrored_slice_tables() -> [[u64; 256]; 8] {
    let mut t = [[0u64; 256]; 8];
    t[0] = mirrored_table();
    let mut k = 1;
    while k < 8 {
        let mut i = 0;
        while i < 256 {
            let prev = t[k - 1][i];
            t[k][i] = (prev >> 8) ^ t[0][(prev & 0xff) as usize];
            i += 1;
        }
        k += 1;
    }
    t
}

/// Decode a value stored with the "CRC random encoding" (spec 5.11): 10
/// data bits spread over 64 bits, the other bits are random. The page map
/// and section map CRC seeds in the file header are stored like this.
pub fn decode_seed(v: u64) -> u64 {
    const BITS: [(u32, u64); 10] = [
        // (bit in the stored value, data bit)
        (32 + 27, 0x01),
        (32 + 21, 0x02),
        (32 + 15, 0x04),
        (32 + 9, 0x08),
        (32 + 3, 0x10),
        (29, 0x20),
        (23, 0x40),
        (17, 0x80),
        (11, 0x100),
        (5, 0x200),
    ];
    BITS.iter().filter(|(bit, _)| v >> bit & 1 != 0).fold(0, |acc, (_, val)| acc | val)
}

/// Spec 5.12 "UpdateSeed1".
pub fn update_seed1(seed: u64, len: u64) -> u64 {
    let mut s = seed.wrapping_add(len).wrapping_mul(0x343fd).wrapping_add(0x269ec3);
    s |= s.wrapping_mul(0x343fd << 32).wrapping_add(0x269ec3 << 32);
    !s
}

/// Spec 5.12 "UpdateSeed2".
pub fn update_seed2(seed: u64, len: u64) -> u64 {
    let s = seed.wrapping_add(len).wrapping_mul(0x343fd).wrapping_add(0x269ec3);
    let s = s.wrapping_mul((1 << 32) + 0x343fd).wrapping_add(len.wrapping_add(0x269ec3));
    !s
}

#[inline(always)]
fn normal_byte(crc: u64, b: u8) -> u64 {
    NORMAL[((crc >> 56) as u8 ^ b) as usize] ^ (crc << 8)
}

#[inline(always)]
fn mirrored_byte(crc: u64, b: u8) -> u64 {
    MIRRORED[(crc as u8 ^ b) as usize] ^ (crc >> 8)
}

/// Byte order inside each full 8-byte block (spec 5.12: "4 [4], 4 [0]"
/// with 4 = "2 [2], 2 [0]").
const ORDER8: [usize; 8] = [6, 7, 4, 5, 2, 3, 0, 1];

/// Byte order of the last 0..7 bytes (spec 5.12 table).
const ORDER_REST: [&[usize]; 8] =
    [&[], &[0], &[0, 1], &[0, 1, 2], &[2, 3, 0, 1], &[2, 3, 0, 1, 4], &[2, 3, 0, 1, 4, 5], &[2, 3, 0, 1, 4, 5, 6]];

/// Run `f` over the bytes of `data` in the order the CRC uses. The order
/// is not the plain byte order; checked on real files (the file header
/// CRCs only match with the spec's order).
#[inline(always)]
fn fold(data: &[u8], mut crc: u64, f: impl Fn(u64, u8) -> u64) -> u64 {
    let mut blocks = data.chunks_exact(8);
    for b in &mut blocks {
        for &i in &ORDER8 {
            crc = f(crc, b[i]);
        }
    }
    let rest = blocks.remainder();
    for &i in ORDER_REST[rest.len()] {
        crc = f(crc, rest[i]);
    }
    crc
}

/// The 32-bit checksum of R2007 data pages (spec 5.4.1). Adler-like, like
/// the R2004 page checksum, but the seed is mixed with the length first and
/// the bytes go in the same shuffled order as the CRC.
pub fn page_checksum_r21(seed: u64, data: &[u8]) -> u32 {
    let s = seed.wrapping_add(data.len() as u64).wrapping_mul(0x343fd).wrapping_add(0x269ec3);
    let mut sums = (s & 0xffff, (s >> 16) & 0xffff);
    // 0x15b0 is a multiple of 8, so only the last chunk has a short tail.
    for chunk in data.chunks(0x15b0) {
        let (a, b) = fold2(chunk, sums);
        sums = (a % 0xfff1, b % 0xfff1);
    }
    ((sums.1 << 16) | (sums.0 & 0xffff)) as u32
}

fn fold2(chunk: &[u8], (mut a, mut b): (u64, u64)) -> (u64, u64) {
    let mut blocks = chunk.chunks_exact(8);
    for blk in &mut blocks {
        for &i in &ORDER8 {
            a += blk[i] as u64;
            b += a;
        }
    }
    let rest = blocks.remainder();
    for &i in ORDER_REST[rest.len()] {
        a += rest[i] as u64;
        b += a;
    }
    (a, b)
}

/// Normal 64-bit CRC (inverted at the end).
pub fn crc64_normal(seed: u64, data: &[u8]) -> u64 {
    !fold(data, seed, normal_byte)
}

/// Mirrored 64-bit CRC. Full 8-byte blocks use slicing by 8: the order
/// 6,7,4,5,2,3,0,1 is the block's four 16-bit lanes in reverse, so the
/// block becomes one little-endian word after a lane swap.
pub fn crc64_mirrored(seed: u64, data: &[u8]) -> u64 {
    let t = &MIRRORED_SLICE;
    let mut crc = seed;
    let mut blocks = data.chunks_exact(8);
    for b in &mut blocks {
        let v = u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]).rotate_left(32);
        let v = ((v & 0x0000_FFFF_0000_FFFF) << 16) | ((v >> 16) & 0x0000_FFFF_0000_FFFF);
        let x = crc ^ v;
        crc = t[7][(x & 0xff) as usize]
            ^ t[6][(x >> 8 & 0xff) as usize]
            ^ t[5][(x >> 16 & 0xff) as usize]
            ^ t[4][(x >> 24 & 0xff) as usize]
            ^ t[3][(x >> 32 & 0xff) as usize]
            ^ t[2][(x >> 40 & 0xff) as usize]
            ^ t[1][(x >> 48 & 0xff) as usize]
            ^ t[0][(x >> 56) as usize];
    }
    fold(blocks.remainder(), crc, mirrored_byte)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_match_spec_rows() {
        // First rows of the two tables printed in spec 5.12.1 and 5.12.2.
        assert_eq!(&NORMAL[..4], &[0, 0x42f0e1eba9ea3693, 0x85e1c3d753d46d26, 0xc711223cfa3e5bb5]);
        assert_eq!(NORMAL[255], 0x9afce626ce85b507);
        assert_eq!(&MIRRORED[..4], &[0, 0x7ad870c830358979, 0xf5b0e190606b12f2, 0x8f689158505e9b8b]);
        assert_eq!(MIRRORED[4], 0xc038e5739841b68f);
    }

    /// Known vectors from an independent bitwise (table-free) script. That
    /// script gives the published CRC-64/WE check value 0x62EC59E3F1A4F00A
    /// for "123456789".
    #[test]
    fn known_vectors() {
        // 1..3 bytes are processed in plain order.
        assert_eq!(crc64_normal(0, b"123"), 0x48d022341be955df);
        assert_eq!(crc64_normal(!0, b"123"), 0xd5f446c587ec3577);
        assert_eq!(crc64_mirrored(0, b"123"), 0xd5bcfb814479b1ac);
        assert_eq!(crc64_mirrored(0x1234, b"123"), 0xa48b1f01c01e2ade);
        // A full 8-byte block goes in the order 6,7,4,5,2,3,0,1.
        assert_eq!(crc64_mirrored(0, &[1, 2, 3, 4, 5, 6, 7, 8]), 0x016b2100efca1eb3);
        assert_eq!(page_checksum_r21(0, b"abc"), 0x42b76be0);
        assert_eq!(page_checksum_r21(0, &[1, 2, 3, 4, 5, 6, 7, 8]), 0xf6abbecf);
    }

    #[test]
    fn slicing_by_8_equals_byte_steps() {
        let data: Vec<u8> = (0..1000u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect();
        for len in [0, 1, 7, 8, 9, 15, 16, 63, 64, 65, 999, 1000] {
            let d = &data[..len];
            assert_eq!(crc64_mirrored(0x1234_5678, d), fold(d, 0x1234_5678, mirrored_byte), "len {len}");
        }
    }

    #[test]
    fn plain_crc64_we_check_value() {
        // The table step alone, in plain byte order, is CRC-64/WE.
        let crc = b"123456789".iter().fold(!0u64, |c, &b| normal_byte(c, b));
        assert_eq!(!crc, 0x62EC59E3F1A4F00A);
    }

    #[test]
    fn seed_decoding() {
        // Build encoded values the way spec 5.11 "Encode" does, with random
        // filler bits, and decode them again.
        const SPOTS: [(u64, u64); 10] = [
            (0x200, 1 << 5),
            (0x100, 1 << 11),
            (0x80, 1 << 17),
            (0x40, 1 << 23),
            (0x20, 1 << 29),
            (0x10, 1 << 35),
            (0x8, 1 << 41),
            (0x4, 1 << 47),
            (0x2, 1 << 53),
            (0x1, 1 << 59),
        ];
        let mut rnd = 0x9E37_79B9_7F4A_7C15u64;
        for value in [0u64, 1, 0x155, 0x2AA, 0x3FF, 0x123] {
            rnd = rnd.rotate_left(13).wrapping_mul(0x2545_F491_4F6C_DD1D);
            let mut enc = rnd & 0xf7df7df7_df7df7df;
            for (bit, spot) in SPOTS {
                if value & bit != 0 {
                    enc |= spot;
                }
            }
            assert_eq!(decode_seed(enc), value);
        }
        // The page map seed of a real ODA-written R2007 file decodes to 0.
        assert_eq!(decode_seed(0xb71e70c0d91511ce), 0);
    }
}
