//! The R2007 (AC1021) file structure (ODA spec chapter 5).
//!
//! Layout of the file:
//! 1. A plain header at 0x00..0x80 (same fields as R2004).
//! 2. The *file header* at 0x80: 0x400 bytes, Reed-Solomon coded
//!    (3 interleaved RS(255,239) code words), then LZ77-R21 compressed.
//!    It holds the place, size and CRCs of the page map and section map.
//! 3. Pages from 0x480 on. All page addresses are relative to 0x480.
//!    * The *page map* (system page): (size, id) pairs; page addresses
//!      follow from adding up the sizes.
//!    * The *section map* (system page): per section its name, sizes,
//!      encoding and list of pages.
//!    * Data pages: RS(255,251) coded (interleaved for "encoding 4"), then
//!      optionally LZ77-R21 compressed.
//!
//! System pages store their data several times (a "correction factor");
//! we read the first copy. Healthy pages need no Reed-Solomon math (the
//! code is systematic). When a CRC says a page is damaged we run the RS
//! decoder ([`crate::rs`]) on it and try again.
//!
//! Pages are independent, so [`R21Container::section`] decodes them in
//! parallel, like the R2004 reader.

use std::collections::HashMap;

use rayon::prelude::*;

use crate::container::{PageRef, SectionDesc};
use crate::crc64::{crc64_mirrored, crc64_normal, decode_seed, page_checksum_r21, update_seed1, update_seed2};
use crate::error::{DwgError, Result};
use crate::lz77_r21;
use crate::rs::{self, RsCode};

/// Start of the page area; all page addresses are relative to it.
const PAGE_BASE: u64 = 0x480;

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn le64(b: &[u8], at: usize) -> u64 {
    le32(b, at) as u64 | ((le32(b, at + 4) as u64) << 32)
}

/// The decoded R2007 file header (spec 5.2, the 0x110 bytes). Only the
/// fields the reader uses are kept.
#[derive(Clone, Debug, Default)]
pub struct R21Header {
    pub file_size: u64,
    pub pages_map_crc_compressed: u64,
    pub pages_map_correction: u64,
    pub pages_map_crc_seed: u64,
    pub pages_map_offset: u64,
    pub pages_map_id: u64,
    pub pages_map_size_compressed: u64,
    pub pages_map_size_uncompressed: u64,
    pub pages_amount: u64,
    pub pages_max_id: u64,
    pub pages_map_crc_uncompressed: u64,
    pub sections_amount: u64,
    pub sections_map_crc_uncompressed: u64,
    pub sections_map_size_compressed: u64,
    pub sections_map_id: u64,
    pub sections_map_size_uncompressed: u64,
    pub sections_map_crc_compressed: u64,
    pub sections_map_correction: u64,
    pub sections_map_crc_seed: u64,
    pub stream_version: u64,
    pub crc_seed: u64,
}

/// One page of an R2007 section.
#[derive(Clone, Copy, Debug)]
pub struct R21Page {
    /// Where the page's data goes inside the section.
    pub offset: u64,
    /// Bytes of the page in the file (after RS coding and padding).
    pub size: u64,
    pub id: i64,
    pub uncompressed: u64,
    pub compressed: u64,
    pub checksum: u64,
    pub crc: u64,
}

/// One entry of the R2007 section map.
#[derive(Clone, Debug)]
pub struct R21Section {
    pub name: String,
    pub data_size: u64,
    pub max_size: u64,
    pub encrypted: u64,
    pub hash: u64,
    /// 1 = RS coded, not interleaved; 4 = RS coded and interleaved.
    pub encoding: u64,
    pub pages: Vec<R21Page>,
}

/// The parsed table of contents of an R2007 file.
pub struct R21Container {
    pub header: R21Header,
    /// page id -> (file address, size in the file)
    pages: HashMap<i64, (u64, u64)>,
    pub sections: Vec<R21Section>,
}

/// Decode the 0x400-byte file header at 0x80 into its 0x110 data bytes.
fn read_file_header(data: &[u8], warnings: &mut Vec<String>) -> Result<[u8; 0x110]> {
    let mut raw = data
        .get(0x80..0x480)
        .ok_or_else(|| DwgError::corrupt("file shorter than the R2007 file header"))?
        .to_vec();
    let code = RsCode::system();
    for attempt in 0..2 {
        let mut dec = [0u8; 3 * 239];
        rs::deinterleave(&raw, &mut dec, 3, 239)?;
        match decode_file_header(&dec) {
            Ok(h) => return Ok(h),
            Err(e) if attempt == 0 => {
                // Try to repair the RS code words, then decode again.
                let n = code.correct_interleaved(&mut raw, 3).map_err(|_| e)?;
                warnings.push(format!("R2007 file header: {n} bytes repaired by Reed-Solomon"));
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!("loop returns on its second pass")
}

/// The RS-decoded file header block: CRCs, size, then (compressed) data.
fn decode_file_header(dec: &[u8]) -> Result<[u8; 0x110]> {
    let comp_len = le32(dec, 0x18) as i32;
    let mut out = [0u8; 0x110];
    let len = comp_len.unsigned_abs() as usize;
    let cdata = dec
        .get(0x20..0x20 + len)
        .ok_or_else(|| DwgError::corrupt(format!("R2007 file header: bad data length {comp_len}")))?;
    if comp_len < 0 {
        let n = len.min(out.len());
        out[..n].copy_from_slice(&cdata[..n]);
    } else {
        let n = lz77_r21::decompress(cdata, &mut out)?;
        if n != out.len() {
            return Err(DwgError::corrupt(format!("R2007 file header: decompressed {n} bytes, expected 0x110")));
        }
    }
    // Header CRC64 (normal CRC, seed from UpdateSeed2) over the data with
    // the CRC field set to zero.
    let stored = le64(&out, 0x108);
    let mut tmp = out;
    tmp[0x108..].fill(0);
    let crc = crc64_normal(update_seed2(0, tmp.len() as u64), &tmp);
    if crc != stored {
        return Err(DwgError::corrupt(format!(
            "R2007 file header CRC mismatch: stored {stored:#x}, computed {crc:#x}"
        )));
    }
    Ok(out)
}

fn parse_header(h: &[u8; 0x110]) -> R21Header {
    let f = |at: usize| le64(h, at);
    R21Header {
        file_size: f(0x08),
        pages_map_crc_compressed: f(0x10),
        pages_map_correction: f(0x18),
        pages_map_crc_seed: f(0x20),
        pages_map_offset: f(0x38),
        pages_map_id: f(0x40),
        pages_map_size_compressed: f(0x50),
        pages_map_size_uncompressed: f(0x58),
        pages_amount: f(0x60),
        pages_max_id: f(0x68),
        pages_map_crc_uncompressed: f(0x80),
        sections_amount: f(0xA0),
        sections_map_crc_uncompressed: f(0xA8),
        sections_map_size_compressed: f(0xB0),
        sections_map_id: f(0xC0),
        sections_map_size_uncompressed: f(0xC8),
        sections_map_crc_compressed: f(0xD0),
        sections_map_correction: f(0xD8),
        sections_map_crc_seed: f(0xE0),
        stream_version: f(0xE8),
        crc_seed: f(0xF0),
    }
}

/// Where a system page is and how to check it (all from the file header).
struct SysPage {
    name: &'static str,
    /// File address of the page.
    addr: u64,
    compressed: u64,
    uncompressed: u64,
    /// How many copies of the data the page holds.
    correction: u64,
    crc_compressed: u64,
    crc_uncompressed: u64,
    seed: u64,
}

/// Bytes after padding to the 8-byte CRC block size.
fn aligned8(n: u64) -> u64 {
    (n + 7) & !7
}

/// Read, check and decompress a system page (spec 5.3).
fn read_system_page(data: &[u8], sp: &SysPage, warnings: &mut Vec<String>) -> Result<Vec<u8>> {
    let name = sp.name;
    if sp.compressed == 0 || sp.compressed > 1 << 26 || sp.uncompressed > 1 << 26 {
        return Err(DwgError::corrupt(format!("{name}: implausible size {}", sp.compressed)));
    }
    if sp.correction == 0 || sp.correction > 1 << 16 {
        return Err(DwgError::corrupt(format!("{name}: implausible correction factor {}", sp.correction)));
    }
    let blocks = (aligned8(sp.compressed) * sp.correction).div_ceil(239) as usize;
    let start = usize::try_from(sp.addr).map_err(|_| DwgError::corrupt("address too large"))?;
    let raw = data
        .get(start..start + blocks * rs::N)
        .ok_or_else(|| DwgError::corrupt(format!("{name}: page at {start:#x} is past the end of the file")))?;
    let comp = sp.compressed as usize;
    let mut cdata = vec![0u8; comp];
    rs::deinterleave(raw, &mut cdata, blocks, 239)?;
    let seed = update_seed1(sp.seed, sp.compressed);
    if crc64_mirrored(seed, &cdata) != sp.crc_compressed {
        // Damaged: repair the RS code words and read again.
        let mut fixed = raw.to_vec();
        match RsCode::system().correct_interleaved(&mut fixed, blocks) {
            Ok(n) => {
                rs::deinterleave(&fixed, &mut cdata, blocks, 239)?;
                warnings.push(format!("{name}: CRC mismatch, {n} bytes repaired by Reed-Solomon"));
            }
            Err(_) => warnings.push(format!("{name}: CRC mismatch and Reed-Solomon could not repair it")),
        }
    }
    let mut out = vec![0u8; sp.uncompressed as usize];
    if sp.compressed < sp.uncompressed {
        let n = lz77_r21::decompress(&cdata, &mut out)?;
        if n != out.len() {
            return Err(DwgError::corrupt(format!("{name}: decompressed {n} bytes, expected {}", out.len())));
        }
    } else {
        let n = out.len();
        out.copy_from_slice(&cdata[..n]);
    }
    let crc = crc64_mirrored(update_seed1(sp.seed, sp.uncompressed), &out);
    if crc != sp.crc_uncompressed {
        warnings.push(format!(
            "{name}: data CRC stored {:#x}, computed {crc:#x}",
            sp.crc_uncompressed
        ));
    }
    Ok(out)
}

/// Page map: (size, id) pairs. Page addresses follow from the sizes.
fn parse_page_map(pm: &[u8]) -> HashMap<i64, (u64, u64)> {
    let mut pages = HashMap::with_capacity(pm.len() / 16);
    let mut addr = PAGE_BASE;
    for pair in pm.chunks_exact(16) {
        let size = le64(pair, 0);
        let id = le64(pair, 8) as i64;
        // A negative id marks a free gap; it still takes room in the file.
        if id > 0 {
            pages.insert(id, (addr, size));
        }
        addr = addr.saturating_add(size);
    }
    pages
}

fn parse_section_map(sm: &[u8]) -> Result<Vec<R21Section>> {
    let short = || DwgError::corrupt("R2007 section map ended inside an entry");
    let mut out = Vec::new();
    let mut i = 0;
    while i + 0x40 <= sm.len() {
        let f = |k: usize| le64(sm, i + k * 8);
        let (data_size, max_size, encrypted, hash) = (f(0), f(1), f(2), f(3));
        let name_len = f(4) as usize;
        let encoding = f(6);
        let page_count = f(7) as usize;
        i += 0x40;
        // The name is UTF-16LE; its length is in bytes and includes the
        // two closing zero bytes.
        let raw = sm.get(i..i.checked_add(name_len).ok_or_else(short)?).ok_or_else(short)?;
        let units: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        let name = String::from_utf16_lossy(&units).trim_end_matches('\0').to_string();
        i += name_len;
        if page_count > (sm.len() - i) / 56 {
            return Err(short());
        }
        let mut pages = Vec::with_capacity(page_count);
        for _ in 0..page_count {
            let g = |k: usize| le64(sm, i + k * 8);
            pages.push(R21Page {
                offset: g(0),
                size: g(1),
                id: g(2) as i64,
                uncompressed: g(3),
                compressed: g(4),
                checksum: g(5),
                crc: g(6),
            });
            i += 56;
        }
        out.push(R21Section { name, data_size, max_size, encrypted, hash, encoding, pages });
    }
    Ok(out)
}

impl R21Container {
    /// Parse the file header, the page map and the section map.
    pub fn parse(data: &[u8], warnings: &mut Vec<String>) -> Result<R21Container> {
        let raw_header = read_file_header(data, warnings)?;
        let header = parse_header(&raw_header);
        let pm = read_system_page(
            data,
            &SysPage {
                name: "page map",
                addr: PAGE_BASE.saturating_add(header.pages_map_offset),
                compressed: header.pages_map_size_compressed,
                uncompressed: header.pages_map_size_uncompressed,
                correction: header.pages_map_correction,
                crc_compressed: header.pages_map_crc_compressed,
                crc_uncompressed: header.pages_map_crc_uncompressed,
                seed: decode_seed(header.pages_map_crc_seed),
            },
            warnings,
        )?;
        let pages = parse_page_map(&pm);
        let &(sm_addr, _) = pages
            .get(&(header.sections_map_id as i64))
            .ok_or_else(|| DwgError::corrupt("R2007 section map page not in the page map"))?;
        let sm = read_system_page(
            data,
            &SysPage {
                name: "section map",
                addr: sm_addr,
                compressed: header.sections_map_size_compressed,
                uncompressed: header.sections_map_size_uncompressed,
                correction: header.sections_map_correction,
                crc_compressed: header.sections_map_crc_compressed,
                crc_uncompressed: header.sections_map_crc_uncompressed,
                seed: decode_seed(header.sections_map_crc_seed),
            },
            warnings,
        )?;
        let sections = parse_section_map(&sm)?;
        Ok(R21Container { header, pages, sections })
    }

    pub fn find(&self, name: &str) -> Option<&R21Section> {
        self.sections.iter().find(|s| s.name == name)
    }

    /// The sections in the R2004-style form that [`crate::container::Container`]
    /// shows to callers (for listings; reading goes through [`Self::section`]).
    pub fn section_descs(&self) -> Vec<SectionDesc> {
        self.sections
            .iter()
            .filter(|s| !s.name.is_empty())
            .map(|s| SectionDesc {
                name: s.name.clone(),
                size: s.data_size,
                page_count: s.pages.len() as u32,
                max_page_size: s.max_size as u32,
                compressed: if s.pages.iter().any(|p| p.compressed < p.uncompressed) { 2 } else { 1 },
                id: s.hash as u32,
                encrypted: s.encrypted as u32,
                pages: s
                    .pages
                    .iter()
                    .map(|p| PageRef { page_number: p.id as i32, data_size: p.compressed as u32, start_offset: p.offset })
                    .collect(),
            })
            .collect()
    }

    /// Read and decompress one logical section, pages in parallel. CRC and
    /// checksum problems go to `warnings`. `None` if there is no such section.
    pub fn section(&self, data: &[u8], name: &str, warnings: &mut Vec<String>) -> Result<Option<Vec<u8>>> {
        let Some(s) = self.find(name) else {
            return Ok(None);
        };
        if s.encrypted == 1 {
            return Err(DwgError::Encrypted);
        }
        let mut pages = s.pages.clone();
        pages.sort_by_key(|p| p.offset);
        let end = pages.iter().map(|p| p.offset.saturating_add(p.uncompressed)).max().unwrap_or(0);
        let total = end.max(s.data_size);
        // Protect against absurd sizes from a damaged section map.
        if total > (data.len() as u64).saturating_mul(64).max(1 << 24) {
            return Err(DwgError::corrupt(format!("section {name}: size {total} is not plausible")));
        }
        let mut buf = vec![0u8; total as usize];
        // One output slice per page. Gaps between pages stay zero ("zero
        // pages" are not written to the file, spec 5.2).
        let mut work: Vec<(&mut [u8], R21Page, u64, u64)> = Vec::with_capacity(pages.len());
        let mut rest: &mut [u8] = &mut buf;
        let mut at = 0u64;
        for p in pages {
            if p.offset < at {
                return Err(DwgError::corrupt(format!("section {name}: pages overlap at {:#x}", p.offset)));
            }
            let &(addr, size) = self
                .pages
                .get(&p.id)
                .ok_or_else(|| DwgError::corrupt(format!("section {name}: page {} not in the page map", p.id)))?;
            let tail = std::mem::take(&mut rest).split_at_mut((p.offset - at) as usize).1;
            let (mine, tail) = tail.split_at_mut(p.uncompressed as usize);
            rest = tail;
            at = p.offset + p.uncompressed;
            work.push((mine, p, addr, size));
        }
        let (encoding, seed) = (s.encoding, self.header.crc_seed);
        let results: Vec<Result<Option<String>>> = work
            .into_par_iter()
            .map(|(out, p, addr, size)| read_data_page(data, addr, size, &p, encoding, seed, out))
            .collect();
        for r in results {
            if let Some(w) = r? {
                warnings.push(format!("{name}: {w}"));
            }
        }
        buf.truncate(s.data_size as usize);
        Ok(Some(buf))
    }
}

/// Read one data page into `out` (spec 5.4). Returns a warning text when a
/// check fails.
fn read_data_page(
    data: &[u8],
    addr: u64,
    size: u64,
    p: &R21Page,
    encoding: u64,
    crc_seed: u64,
    out: &mut [u8],
) -> Result<Option<String>> {
    let start = usize::try_from(addr).map_err(|_| DwgError::corrupt("address too large"))?;
    let raw = data
        .get(start..start.saturating_add(size as usize))
        .ok_or_else(|| DwgError::corrupt(format!("page {} at {start:#x} is past the end of the file", p.id)))?;
    let comp = p.compressed as usize;
    let mut warning = None;
    let mut buf;
    let cdata: &[u8] = if encoding == 4 {
        let blocks = aligned8(p.compressed).div_ceil(251) as usize;
        buf = vec![0u8; comp];
        rs::deinterleave(raw, &mut buf, blocks, 251)?;
        if crc64_mirrored(update_seed1(crc_seed, p.compressed), &buf) != p.crc {
            let mut fixed = raw.to_vec();
            warning = Some(match RsCode::data().correct_interleaved(&mut fixed, blocks) {
                Ok(n) => {
                    rs::deinterleave(&fixed, &mut buf, blocks, 251)?;
                    format!("page {}: CRC mismatch, {n} bytes repaired by Reed-Solomon", p.id)
                }
                Err(_) => format!("page {}: CRC mismatch, Reed-Solomon could not repair it", p.id),
            });
        }
        &buf
    } else {
        // Encoding 1: stored as is (no RS parity in the file).
        let d = raw
            .get(..comp)
            .ok_or_else(|| DwgError::corrupt(format!("page {}: data larger than the page", p.id)))?;
        if crc64_mirrored(update_seed1(crc_seed, p.compressed), d) != p.crc {
            warning = Some(format!("page {}: CRC mismatch", p.id));
        }
        d
    };
    if p.compressed < p.uncompressed {
        let n = lz77_r21::decompress(cdata, out)?;
        if n != out.len() {
            return Err(DwgError::corrupt(format!("page {}: decompressed {n} bytes, expected {}", p.id, out.len())));
        }
    } else {
        let n = out.len();
        out.copy_from_slice(cdata.get(..n).ok_or_else(|| DwgError::corrupt("page data too short"))?);
    }
    let sum = page_checksum_r21(0, out) as u64;
    if sum != p.checksum && warning.is_none() {
        warning = Some(format!("page {}: checksum stored {:#x}, computed {sum:#x}", p.id, p.checksum));
    }
    Ok(warning)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// AC1021 files in $CADRENDER_DWG_SAMPLES (your own files, not in git).
    fn r2007_samples() -> Vec<Vec<u8>> {
        let Ok(dir) = std::env::var("CADRENDER_DWG_SAMPLES") else {
            return Vec::new();
        };
        let mut paths: Vec<_> = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok().map(|e| e.path())).collect();
        paths.sort();
        paths
            .into_iter()
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dwg")))
            .map(|p| std::fs::read(p).unwrap())
            .filter(|d| d.starts_with(b"AC1021"))
            .collect()
    }

    fn syndromes_zero(code: &RsCode, enc: &[u8], blocks: usize) -> bool {
        (0..blocks).all(|b| {
            let mut w = [0u8; rs::N];
            for (j, v) in w.iter_mut().enumerate() {
                *v = enc[j * blocks + b];
            }
            code.syndromes(&w).iter().all(|&s| s == 0)
        })
    }

    /// Proves the RS code word layout on real files: every code word of the
    /// file header and of every interleaved data page is a valid code word.
    #[test]
    #[ignore]
    fn sample_rs_code_words_are_valid() {
        for data in r2007_samples() {
            assert!(syndromes_zero(&RsCode::system(), &data[0x80..0x480], 3));
            let c = R21Container::parse(&data, &mut Vec::new()).unwrap();
            let mut checked = 0;
            for s in c.sections.iter().filter(|s| s.encoding == 4) {
                for p in &s.pages {
                    let (addr, size) = c.pages[&p.id];
                    let blocks = aligned8(p.compressed).div_ceil(251) as usize;
                    let raw = &data[addr as usize..(addr + size) as usize];
                    assert!(syndromes_zero(&RsCode::data(), raw, blocks), "{} page {}", s.name, p.id);
                    checked += 1;
                }
            }
            assert!(checked > 10);
        }
    }

    /// Damage the file header and two data pages (within what RS can fix):
    /// the sections must come back unchanged, with "repaired" warnings.
    #[test]
    #[ignore]
    fn sample_damaged_pages_are_repaired() {
        for data in r2007_samples() {
            let mut w = Vec::new();
            let c = R21Container::parse(&data, &mut w).unwrap();
            assert!(w.is_empty(), "healthy file: {w:?}");
            let good = c.section(&data, "AcDb:Handles", &mut w).unwrap().unwrap();
            assert!(w.is_empty(), "healthy file: {w:?}");
            let mut bad = data.clone();
            // Bytes 40 and 41 of the first header code word (interleaved by 3):
            // inside the compressed header data.
            bad[0x80 + 40 * 3] ^= 0xFF;
            bad[0x80 + 41 * 3] ^= 0x0F;
            for p in &c.find("AcDb:Handles").unwrap().pages[..2] {
                let (addr, _) = c.pages[&p.id];
                bad[addr as usize + 100] ^= 0x55;
                bad[addr as usize + 5000] ^= 0x01;
            }
            let mut w = Vec::new();
            let c2 = R21Container::parse(&bad, &mut w).unwrap();
            assert!(w.iter().any(|m| m.contains("file header")), "{w:?}");
            let mut w = Vec::new();
            let again = c2.section(&bad, "AcDb:Handles", &mut w).unwrap().unwrap();
            assert_eq!(again, good);
            assert_eq!(w.iter().filter(|m| m.contains("repaired")).count(), 2, "{w:?}");
        }
    }


}
