//! Checksums used by DWG files (ODA spec 2.14 and 4.2).
//!
//! * [`crc16`]: the "8-bit CRC" of the spec. It is the common CRC-16/ARC
//!   (polynomial 0xA001, reflected), with a start value chosen by the caller.
//!   Objects use start value 0xC0C1.
//! * [`crc32`]: the R2004+ 32-bit CRC (normal CRC-32, polynomial 0xEDB88320,
//!   with a seed).
//! * [`page_checksum`]: the Adler-like checksum of R2004+ section pages.

const fn make_crc16_table() -> [u16; 256] {
    let mut t = [0u16; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u16;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xA001 } else { c >> 1 };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

const fn make_crc32_table() -> [u32; 256] {
    let mut t = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
            k += 1;
        }
        t[i] = c;
        i += 1;
    }
    t
}

static CRC16_TABLE: [u16; 256] = make_crc16_table();
static CRC32_TABLE: [u32; 256] = make_crc32_table();

/// The spec's 16-bit CRC over `data`, starting from `seed`.
pub fn crc16(seed: u16, data: &[u8]) -> u16 {
    let mut dx = seed;
    for &b in data {
        let al = b ^ (dx as u8);
        dx = (dx >> 8) ^ CRC16_TABLE[al as usize];
    }
    dx
}

/// The spec's 32-bit CRC over `data`, with `seed` (0 for the file header).
pub fn crc32(seed: u32, data: &[u8]) -> u32 {
    let mut c = !seed;
    for &b in data {
        c = (c >> 8) ^ CRC32_TABLE[((c ^ b as u32) & 0xff) as usize];
    }
    !c
}

/// Checksum of R2004+ section pages (spec 4.2).
pub fn page_checksum(seed: u32, data: &[u8]) -> u32 {
    let mut sum1 = seed & 0xffff;
    let mut sum2 = seed >> 16;
    for chunk in data.chunks(0x15b0) {
        for &b in chunk {
            sum1 += b as u32;
            sum2 += sum1;
        }
        sum1 %= 0xFFF1;
        sum2 %= 0xFFF1;
    }
    (sum2 << 16) | (sum1 & 0xffff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc16_table_matches_spec() {
        // First and last rows of the table printed in the ODA spec.
        assert_eq!(&CRC16_TABLE[..8], &[0x0000, 0xC0C1, 0xC181, 0x0140, 0xC301, 0x03C0, 0x0280, 0xC241]);
        assert_eq!(CRC16_TABLE[255], 0x4040);
    }

    #[test]
    fn crc32_table_matches_spec() {
        assert_eq!(&CRC32_TABLE[..4], &[0x00000000, 0x77073096, 0xee0e612c, 0x990951ba]);
        assert_eq!(CRC32_TABLE[255], 0x2d02ef8d);
    }

    #[test]
    fn crc16_known_value() {
        // CRC-16/ARC check value for "123456789" with start 0 is 0xBB3D.
        assert_eq!(crc16(0, b"123456789"), 0xBB3D);
        // Accumulating in two calls gives the same result.
        let a = crc16(0xC0C1, b"1234");
        assert_eq!(crc16(a, b"56789"), crc16(0xC0C1, b"123456789"));
    }

    #[test]
    fn crc32_known_value() {
        // Standard CRC-32 check value.
        assert_eq!(crc32(0, b"123456789"), 0xCBF43926);
        assert_eq!(crc32(crc32(0, b"1234"), b"56789"), 0xCBF43926);
    }

    #[test]
    fn page_checksum_small() {
        // sum1 = 1+2+3 = 6, sum2 = 1+3+6 = 10
        assert_eq!(page_checksum(0, &[1, 2, 3]), (10 << 16) | 6);
        // seed splits into the two halves
        assert_eq!(page_checksum((5 << 16) | 4, &[]), (5 << 16) | 4);
    }

    #[test]
    fn page_checksum_long_input_wraps() {
        let data = vec![0xffu8; 0x15b0 * 3 + 17];
        let c = page_checksum(0, &data);
        assert!(c & 0xffff < 0xFFF1 && c >> 16 < 0xFFF1);
    }
}
