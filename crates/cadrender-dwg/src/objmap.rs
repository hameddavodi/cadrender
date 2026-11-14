//! AcDb:Handles, the object map (ODA spec chapter 23).
//!
//! A list of (handle, offset) pairs: where each object starts inside the
//! AcDb:AcDbObjects section. It is stored in blocks of about 2032 bytes.
//! Each block starts with a big-endian size and ends with a big-endian CRC.
//! Inside a block, handles and offsets are deltas (modular chars) from the
//! previous pair; both start from 0 again in every block.

use crate::bits::BitReader;
use crate::crc::crc16;
use crate::error::{DwgError, Result};

/// One object map entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapEntry {
    pub handle: u64,
    /// Byte offset inside AcDb:AcDbObjects.
    pub offset: u64,
}

/// Parse the object map. The result is sorted by handle, without duplicates
/// (a later entry for the same handle wins, as in AutoCAD).
/// `bad_crc` counts blocks whose CRC did not match.
pub fn parse(buf: &[u8]) -> Result<(Vec<MapEntry>, usize)> {
    let mut out = Vec::with_capacity(buf.len() / 3);
    let mut bad_crc = 0;
    let mut pos = 0usize;
    loop {
        if pos + 2 > buf.len() {
            return Err(DwgError::corrupt("object map ended without the final empty block"));
        }
        let size = u16::from_be_bytes([buf[pos], buf[pos + 1]]) as usize;
        if size == 2 {
            break;
        }
        if size < 2 || pos + size + 2 > buf.len() {
            return Err(DwgError::corrupt(format!("object map block size {size} at {pos} out of range")));
        }
        // The spec says blocks hold at most 2032 bytes, but real files have
        // blocks one byte longer (a pair may cross the limit), so the size
        // field alone decides.
        let body_end = pos + size;
        let mut r = BitReader::with_range(buf, (pos + 2) * 8, body_end * 8);
        let mut handle = 0u64;
        let mut offset = 0i64;
        while r.remaining() > 0 {
            let dh = r.mc_unsigned()?;
            let doff = r.mc_signed()?;
            handle = handle.wrapping_add(dh);
            offset = offset.wrapping_add(doff);
            if dh > 0 && offset >= 0 {
                out.push(MapEntry { handle, offset: offset as u64 });
            }
        }
        let stored = u16::from_be_bytes([buf[pos + size], buf[pos + size + 1]]);
        let crc = crc16(0xC0C1, &buf[pos..pos + size]);
        if crc != stored {
            bad_crc += 1;
        }
        pos += size + 2;
    }
    // Usually sorted already; make sure, and keep the last duplicate.
    if !out.windows(2).all(|w| w[0].handle < w[1].handle) {
        out.reverse();
        out.sort_by_key(|e| e.handle);
        out.dedup_by_key(|e| e.handle);
    }
    Ok((out, bad_crc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(pairs: &[(u8, u8)]) -> Vec<u8> {
        let mut body = Vec::new();
        for &(h, o) in pairs {
            body.push(h);
            body.push(o);
        }
        let size = (body.len() + 2) as u16;
        let mut b = size.to_be_bytes().to_vec();
        b.extend_from_slice(&body);
        let crc = crc16(0xC0C1, &b);
        b.extend_from_slice(&crc.to_be_bytes());
        b
    }

    #[test]
    fn two_blocks_reset_deltas() {
        let mut buf = block(&[(1, 10), (2, 5)]); // handles 1, 3 at 10, 15
        buf.extend(block(&[(7, 0x20), (1, 0x41)])); // handles 7, 8 at 32, 31
        buf.extend_from_slice(&[0, 2, 0, 0]);
        let (m, bad) = parse(&buf).unwrap();
        assert_eq!(bad, 0);
        assert_eq!(
            m,
            vec![
                MapEntry { handle: 1, offset: 10 },
                MapEntry { handle: 3, offset: 15 },
                MapEntry { handle: 7, offset: 32 },
                MapEntry { handle: 8, offset: 31 },
            ]
        );
    }

    #[test]
    fn bad_crc_is_counted_not_fatal() {
        let mut buf = block(&[(1, 10)]);
        let n = buf.len();
        buf[n - 1] ^= 0xff;
        buf.extend_from_slice(&[0, 2, 0, 0]);
        let (m, bad) = parse(&buf).unwrap();
        assert_eq!(bad, 1);
        assert_eq!(m.len(), 1);
    }

    #[test]
    fn truncated_map_is_error() {
        let buf = block(&[(1, 10)]);
        assert!(parse(&buf).is_err());
        assert!(parse(&[0x10, 0x00, 1]).is_err());
    }
}
