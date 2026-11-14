//! The three streams of an R2007+ object (or header): data, strings and
//! handles (ODA spec 20.1).
//!
//! From R2007 on, an object's bits are split in three parts:
//! * the **data stream**: numbers, points, flags;
//! * the **string stream**: all texts (TV), at the end of the data part;
//! * the **handle stream**: all handle references, after the data part.
//!
//! [`ObjReader`] holds one [`BitReader`] per stream and sends each read to
//! the right one, so object decoders can follow the spec field list in order.

use crate::bits::{BitReader, HandleRef};
use crate::container::Version;
use crate::error::{DwgError, Result};

/// A colour as stored in DWG.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Color {
    #[default]
    ByLayer,
    ByBlock,
    /// AutoCAD colour index 1..=255.
    Index(u8),
    /// True colour 0xRRGGBB.
    Rgb(u32),
    /// "No colour" (seen in some styles).
    None,
}

impl Color {
    /// Decode the 32-bit "RGB value" of a CMC colour (top byte is a method).
    pub fn from_cmc_rgb(v: u32) -> Color {
        match v >> 24 {
            0xC0 => Color::ByLayer,
            0xC1 => Color::ByBlock,
            0xC2 => Color::Rgb(v & 0x00ff_ffff),
            0xC3 => match (v & 0xff) as u8 {
                0 => Color::ByBlock,
                i => Color::Index(i),
            },
            0xC8 => Color::None,
            _ => Color::Index((v & 0xff) as u8),
        }
    }

    /// Decode an AutoCAD colour index (0 = by block, 256 = by layer).
    pub fn from_index(i: i32) -> Color {
        match i {
            0 => Color::ByBlock,
            256 => Color::ByLayer,
            1..=255 => Color::Index(i as u8),
            _ => Color::ByLayer,
        }
    }
}

/// The result of an entity colour (ENC) read.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EntityColor {
    pub color: Color,
    /// Raw transparency value when stored (type in top byte, alpha in low).
    pub transparency: Option<u32>,
    /// True when a DBCOLOR (colour book) handle follows in the handle stream.
    pub has_book_handle: bool,
}

/// Reader over the data, string and handle streams of one object.
#[derive(Clone, Copy, Debug)]
pub struct ObjReader<'a> {
    pub d: BitReader<'a>,
    /// String stream (R2007+). `None` when the object has no strings or the
    /// file is older (then strings are in the data stream).
    pub s: Option<BitReader<'a>>,
    pub h: BitReader<'a>,
    pub ver: Version,
    /// Handle of the current object (base for relative handle refs).
    pub handle: u64,
}

/// Find the string stream that ends at bit `flag_pos` (the "string stream
/// present" bit). Returns `None` when the bit is 0.
pub fn string_stream<'a>(buf: &'a [u8], start_limit: usize, flag_pos: usize) -> Result<Option<BitReader<'a>>> {
    let mut r = BitReader::with_range(buf, flag_pos, flag_pos + 1);
    if !r.b()? {
        return Ok(None);
    }
    if flag_pos < start_limit + 16 {
        return Err(DwgError::corrupt("string stream size field before object start"));
    }
    let mut end = flag_pos - 16;
    let mut sz = BitReader::with_range(buf, end, end + 16);
    let mut size = sz.rs()? as usize;
    if size & 0x8000 != 0 {
        if end < start_limit + 16 {
            return Err(DwgError::corrupt("string stream hi size before object start"));
        }
        end -= 16;
        let mut hr = BitReader::with_range(buf, end, end + 16);
        let hi = hr.rs()? as usize;
        size = (size & 0x7fff) | (hi << 15);
    }
    if size > end || end - size < start_limit {
        return Err(DwgError::corrupt(format!("string stream size {size} larger than object")));
    }
    Ok(Some(BitReader::with_range(buf, end - size, end)))
}

impl<'a> ObjReader<'a> {
    /// TV: variable text. R2007+: unicode from the string stream.
    pub fn tv(&mut self) -> Result<String> {
        if self.ver.r2007_plus() {
            match self.s.as_mut() {
                Some(s) => s.tu(),
                None => Ok(String::new()),
            }
        } else {
            self.d.t8()
        }
    }

    /// H: a handle reference from the handle stream.
    #[inline]
    pub fn href(&mut self) -> Result<HandleRef> {
        self.h.h()
    }

    /// H resolved to an absolute handle (relative codes use the object's handle).
    #[inline]
    pub fn handle_ref(&mut self) -> Result<u64> {
        Ok(self.h.h()?.absolute(self.handle))
    }

    /// CMC: R2004+ colour (BS index, BL rgb, RC flags, optional names).
    pub fn cmc(&mut self) -> Result<Color> {
        let idx = self.d.bs()?;
        let rgb = self.d.bl()? as u32;
        let flags = self.d.rc()?;
        if flags & 1 != 0 {
            self.tv()?;
        }
        if flags & 2 != 0 {
            self.tv()?;
        }
        if rgb == 0 && idx != 0 {
            return Ok(Color::from_index(idx as i32));
        }
        Ok(Color::from_cmc_rgb(rgb))
    }

    /// ENC: entity colour with flags (R2004+, spec 2.11).
    pub fn enc(&mut self) -> Result<EntityColor> {
        let v = self.d.bs()? as u16;
        let flags = v & 0xff00;
        let mut out = EntityColor::default();
        if v == 0 {
            out.color = Color::ByBlock;
            return Ok(out);
        }
        if flags & 0x4000 != 0 {
            // Colour book colour: no RGB value here, only a DBCOLOR handle in
            // the handle stream. The spec suggests an RGB value follows; real
            // files (and ODA's output) show it does not. LibreDWG 0.14 reads
            // one and loses the rest of these objects.
            out.has_book_handle = true;
            out.color = Color::ByLayer;
        } else if flags & 0x8000 != 0 {
            let rgb = self.d.bl()? as u32;
            out.color = Color::from_cmc_rgb(rgb);
            if rgb >> 24 == 0 {
                out.color = Color::Rgb(rgb & 0x00ff_ffff);
            }
        } else {
            out.color = Color::from_index((v & 0x0fff) as i32);
        }
        if flags & 0x2000 != 0 {
            out.transparency = Some(self.d.bl()? as u32);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cmc_rgb_methods() {
        assert_eq!(Color::from_cmc_rgb(0xC000_0000), Color::ByLayer);
        assert_eq!(Color::from_cmc_rgb(0xC100_0000), Color::ByBlock);
        assert_eq!(Color::from_cmc_rgb(0xC2FF_8000), Color::Rgb(0xFF8000));
        assert_eq!(Color::from_cmc_rgb(0xC300_0007), Color::Index(7));
        assert_eq!(Color::from_index(256), Color::ByLayer);
        assert_eq!(Color::from_index(0), Color::ByBlock);
    }

    #[test]
    fn string_stream_absent_and_present() {
        // Layout (bits): [16 bits of text area][16-bit size][flag]
        // Text area holds one byte 0xAB padded; size = 8 bits.
        let mut bytes = vec![0xAB, 0x00];
        bytes.extend_from_slice(&8u16.to_le_bytes());
        bytes.push(0x80); // flag bit = 1 at bit 32
        let s = string_stream(&bytes, 0, 32).unwrap().unwrap();
        assert_eq!(s.pos(), 8);
        assert_eq!(s.end(), 16);
        let mut s = s;
        assert_eq!(s.rc().unwrap(), 0x00);

        bytes[4] = 0x00; // flag 0
        assert!(string_stream(&bytes, 0, 32).unwrap().is_none());
    }

    #[test]
    fn string_stream_size_too_large_is_error() {
        let mut bytes = vec![0u8; 2];
        bytes.extend_from_slice(&0x7000u16.to_le_bytes());
        bytes.push(0x80);
        assert!(string_stream(&bytes, 0, 32).is_err());
    }
}
