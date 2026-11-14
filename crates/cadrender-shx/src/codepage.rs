//! Windows-1252 mapping for normal (non-Unicode) shape fonts.
//!
//! A normal shape font numbers its shapes by the byte of the drawing code
//! page. All sample drawings use ANSI_1252. Bytes 0xA0..=0xFF are the same as
//! Unicode; only 0x80..=0x9F differ, listed here.

const CP1252_80: [u16; 32] = [
    0x20AC, 0, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
    0x0152, 0, 0x017D, 0, 0, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC,
    0x2122, 0x0161, 0x203A, 0x0153, 0, 0x017E, 0x0178,
];

/// The Windows-1252 byte for a character, if it has one.
pub fn cp1252_byte(ch: char) -> Option<u8> {
    let c = ch as u32;
    if c < 0x80 || (0xA0..=0xFF).contains(&c) {
        return Some(c as u8);
    }
    CP1252_80
        .iter()
        .position(|&u| u != 0 && u as u32 == c)
        .map(|i| 0x80 + i as u8)
}

/// The character for a Windows-1252 byte.
pub fn cp1252_char(b: u8) -> char {
    if (0x80..0xA0).contains(&b) {
        let u = CP1252_80[(b - 0x80) as usize];
        if u != 0 {
            return char::from_u32(u as u32).unwrap_or('?');
        }
    }
    b as char
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        assert_eq!(cp1252_byte('€'), Some(0x80));
        assert_eq!(cp1252_byte('é'), Some(0xE9));
        assert_eq!(cp1252_byte('A'), Some(b'A'));
        assert_eq!(cp1252_byte('Ω'), None);
        for b in 0..=255u8 {
            let c = cp1252_char(b);
            if let Some(back) = cp1252_byte(c) {
                assert_eq!(back, b);
            }
        }
    }
}
