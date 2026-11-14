//! The R2004+ file structure (ODA spec chapter 4). R2010, R2013 and R2018
//! use the same structure (spec chapters 6, 7 and 8).
//!
//! Layout of the file:
//! 1. A plain header at 0x00..0x80 (version, code page, security flags).
//! 2. 0x6C encrypted bytes at 0x80: the "R2004 file header data". They are
//!    XOR'ed with a fixed random sequence and protected by a CRC-32.
//! 3. Pages from 0x100 on. Two system pages are the table of contents:
//!    the *section page map* (page number -> file address) and the
//!    *section map* (section name -> list of pages).
//! 4. Data pages: a 32-byte masked header, then LZ77-compressed data.
//!
//! [`Container::section`] puts the pages of one logical section back
//! together. Pages are independent, so it decompresses them in parallel.

use std::collections::HashMap;

use rayon::prelude::*;

use crate::crc::{crc32, page_checksum};
use crate::error::{DwgError, Result};
use crate::lz77;

/// The DWG versions this crate knows about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Version {
    /// AC1018, AutoCAD 2004-2006
    R2004,
    /// AC1021, AutoCAD 2007-2009
    R2007,
    /// AC1024, AutoCAD 2010-2012
    R2010,
    /// AC1027, AutoCAD 2013-2017
    R2013,
    /// AC1032, AutoCAD 2018+
    R2018,
}

impl Version {
    /// Parse the 6-byte version string at the start of the file.
    pub fn from_magic(magic: &[u8]) -> Result<Version> {
        match magic {
            b"AC1018" => Ok(Version::R2004),
            b"AC1021" => Ok(Version::R2007),
            b"AC1024" => Ok(Version::R2010),
            b"AC1027" => Ok(Version::R2013),
            b"AC1032" => Ok(Version::R2018),
            m if m.len() == 6 && m.starts_with(b"AC") && m[2..].iter().all(u8::is_ascii_digit) => {
                Err(DwgError::UnsupportedVersion(String::from_utf8_lossy(m).into_owned()))
            }
            _ => Err(DwgError::NotDwg),
        }
    }

    /// The version string, for example "AC1032".
    pub fn magic(self) -> &'static str {
        match self {
            Version::R2004 => "AC1018",
            Version::R2007 => "AC1021",
            Version::R2010 => "AC1024",
            Version::R2013 => "AC1027",
            Version::R2018 => "AC1032",
        }
    }

    pub fn r2007_plus(self) -> bool {
        self >= Version::R2007
    }
    pub fn r2010_plus(self) -> bool {
        self >= Version::R2010
    }
    pub fn r2013_plus(self) -> bool {
        self >= Version::R2013
    }
    pub fn r2018_plus(self) -> bool {
        self >= Version::R2018
    }
}

/// Fields of the plain file header (0x00..0x80) and of the decrypted
/// R2004 header data.
#[derive(Clone, Debug)]
pub struct FileHeader {
    pub version: Version,
    /// Maintenance release version (byte 0x0B).
    pub maint_release: u8,
    pub preview_address: u32,
    pub app_version: u8,
    pub app_maint_version: u8,
    pub codepage: u16,
    pub security_flags: u32,
    pub summary_info_address: u32,
    pub vba_address: u32,
    pub last_section_page_id: u32,
    pub last_section_page_end: u64,
    pub second_header_address: u64,
    pub gap_amount: u32,
    pub section_page_amount: u32,
    pub page_map_id: u32,
    pub page_map_address: u64,
    pub section_map_id: u32,
    pub section_page_array_size: u32,
    pub gap_array_size: u32,
}

/// One page of a logical section.
#[derive(Clone, Copy, Debug)]
pub struct PageRef {
    /// Index into the section page map.
    pub page_number: i32,
    /// Bytes stored in the file after the page header.
    pub data_size: u32,
    /// Where the page's data goes inside the logical section.
    pub start_offset: u64,
}

/// One entry of the section map.
#[derive(Clone, Debug)]
pub struct SectionDesc {
    pub name: String,
    pub size: u64,
    pub page_count: u32,
    pub max_page_size: u32,
    /// 1 = not compressed, 2 = compressed
    pub compressed: u32,
    pub id: u32,
    /// 0 = no, 1 = yes, 2 = unknown
    pub encrypted: u32,
    pub pages: Vec<PageRef>,
}

/// The parsed table of contents of an R2004+ file.
pub struct Container<'a> {
    data: &'a [u8],
    pub header: FileHeader,
    /// page number -> (file address, page size)
    pages: HashMap<i32, (u64, u64)>,
    pub sections: Vec<SectionDesc>,
    /// Checksum problems and other things that did not stop reading.
    pub warnings: Vec<String>,
    /// R2007 (AC1021) files have their own maps, see [`crate::container_r21`].
    r21: Option<crate::container_r21::R21Container>,
}

const SYS_PAGE_MAP: u32 = 0x4163_0e3b;
const SYS_SECTION_MAP: u32 = 0x4163_003b;
const DATA_PAGE: u32 = 0x4163_043b;

fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]])
}

fn le64(b: &[u8], at: usize) -> u64 {
    le32(b, at) as u64 | ((le32(b, at + 4) as u64) << 32)
}

fn le16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

/// The fixed sequence that masks the R2004 header data (spec 4.1).
pub fn magic_sequence() -> [u8; 0x100] {
    let mut out = [0u8; 0x100];
    let mut seed: u32 = 1;
    for b in out.iter_mut() {
        seed = seed.wrapping_mul(0x343fd).wrapping_add(0x269ec3);
        *b = (seed >> 16) as u8;
    }
    out
}

fn slice(data: &[u8], start: u64, len: u64) -> Result<&[u8]> {
    let s = usize::try_from(start).map_err(|_| DwgError::corrupt("address too large"))?;
    let l = usize::try_from(len).map_err(|_| DwgError::corrupt("length too large"))?;
    let e = s.checked_add(l).ok_or_else(|| DwgError::corrupt("address overflow"))?;
    data.get(s..e).ok_or_else(|| {
        DwgError::corrupt(format!("range {s:#x}..{e:#x} is past the end of the file ({:#x})", data.len()))
    })
}

impl<'a> Container<'a> {
    /// Parse the file header, the page map and the section map.
    pub fn parse(data: &'a [u8]) -> Result<Container<'a>> {
        if data.len() < 0x100 {
            return Err(if data.len() >= 6 && data.starts_with(b"AC") {
                DwgError::corrupt("file shorter than the R2004 header")
            } else {
                DwgError::NotDwg
            });
        }
        let version = Version::from_magic(&data[..6])?;
        let mut warnings = Vec::new();
        if version == Version::R2007 {
            let r21 = crate::container_r21::R21Container::parse(data, &mut warnings)?;
            let header = FileHeader {
                version,
                maint_release: data[0x0b],
                preview_address: le32(data, 0x0d),
                app_version: data[0x11],
                app_maint_version: data[0x12],
                codepage: le16(data, 0x13),
                security_flags: le32(data, 0x18),
                summary_info_address: le32(data, 0x20),
                vba_address: le32(data, 0x24),
                last_section_page_id: 0,
                last_section_page_end: 0,
                second_header_address: 0,
                gap_amount: 0,
                section_page_amount: r21.header.pages_amount as u32,
                page_map_id: r21.header.pages_map_id as u32,
                page_map_address: r21.header.pages_map_offset,
                section_map_id: r21.header.sections_map_id as u32,
                section_page_array_size: 0,
                gap_array_size: 0,
            };
            let sections = r21.section_descs();
            return Ok(Container { data, header, pages: HashMap::new(), sections, warnings, r21: Some(r21) });
        }

        // Decrypt the R2004 header data.
        let magic = magic_sequence();
        let mut hd = [0u8; 0x6c];
        for (i, b) in hd.iter_mut().enumerate() {
            *b = data[0x80 + i] ^ magic[i];
        }
        if &hd[..11] != b"AcFssFcAJMB" {
            return Err(DwgError::corrupt("R2004 header data id string not found"));
        }
        let stored_crc = le32(&hd, 0x68);
        let mut tmp = hd;
        tmp[0x68..0x6c].fill(0);
        let crc = crc32(0, &tmp);
        if crc != stored_crc {
            warnings.push(format!("file header CRC32 mismatch: stored {stored_crc:#x}, computed {crc:#x}"));
        }

        let header = FileHeader {
            version,
            maint_release: data[0x0b],
            preview_address: le32(data, 0x0d),
            app_version: data[0x11],
            app_maint_version: data[0x12],
            codepage: le16(data, 0x13),
            security_flags: le32(data, 0x18),
            summary_info_address: le32(data, 0x20),
            vba_address: le32(data, 0x24),
            last_section_page_id: le32(&hd, 0x28),
            last_section_page_end: le64(&hd, 0x2c),
            second_header_address: le64(&hd, 0x34),
            gap_amount: le32(&hd, 0x3c),
            section_page_amount: le32(&hd, 0x40),
            page_map_id: le32(&hd, 0x50),
            page_map_address: le64(&hd, 0x54),
            section_map_id: le32(&hd, 0x5c),
            section_page_array_size: le32(&hd, 0x60),
            gap_array_size: le32(&hd, 0x64),
        };

        // Section page map (system page).
        let pm_addr = header
            .page_map_address
            .checked_add(0x100)
            .ok_or_else(|| DwgError::corrupt("page map address overflow"))?;
        let pm = read_system_page(data, pm_addr, SYS_PAGE_MAP, &mut warnings)?;
        let mut pages = HashMap::new();
        let mut addr: u64 = 0x100;
        let mut i = 0;
        while i + 8 <= pm.len() {
            let number = le32(&pm, i) as i32;
            let size = le32(&pm, i + 4) as u64;
            i += 8;
            if number < 0 {
                // gap: parent, left, right, 0
                i += 16;
            } else {
                pages.insert(number, (addr, size));
            }
            addr = addr.saturating_add(size);
        }

        // Section map (system page).
        let &(sm_addr, _) = pages
            .get(&(header.section_map_id as i32))
            .ok_or_else(|| DwgError::corrupt("section map page not in page map"))?;
        let sm = read_system_page(data, sm_addr, SYS_SECTION_MAP, &mut warnings)?;
        let sections = parse_section_map(&sm)?;

        Ok(Container { data, header, pages, sections, warnings, r21: None })
    }

    /// Find a section description by name (for example "AcDb:Header").
    pub fn find(&self, name: &str) -> Option<&SectionDesc> {
        self.sections.iter().find(|s| s.name == name)
    }

    /// Read and decompress one logical section. Pages are decompressed in
    /// parallel. Checksum problems are added to `warnings` (they do not stop
    /// reading). Returns `None` if the section is not in the file.
    pub fn section(&self, name: &str, warnings: &mut Vec<String>) -> Result<Option<Vec<u8>>> {
        if let Some(r21) = &self.r21 {
            return r21.section(self.data, name, warnings);
        }
        let Some(desc) = self.find(name) else {
            return Ok(None);
        };
        if desc.encrypted == 1 {
            return Err(DwgError::Encrypted);
        }
        let page_size = desc.max_page_size as usize;
        if page_size == 0 || page_size > 0x100_0000 {
            return Err(DwgError::corrupt(format!("section {name}: bad page size {page_size}")));
        }
        let size = usize::try_from(desc.size).map_err(|_| DwgError::corrupt("section too large"))?;
        let slots = size.div_ceil(page_size).max(desc.pages.len());
        // Protect against absurd sizes from a damaged section map.
        let total = slots
            .checked_mul(page_size)
            .filter(|t| *t <= self.data.len().saturating_mul(64).max(1 << 24))
            .ok_or_else(|| DwgError::corrupt(format!("section {name}: size {size} is not plausible")))?;
        let mut buf = vec![0u8; total];

        // Map each page to its slot. Slots without a page stay zero
        // ("zero pages" are not written to the file, spec 4.5).
        let mut jobs: Vec<(usize, PageRef, u64)> = Vec::with_capacity(desc.pages.len());
        for p in &desc.pages {
            let slot = (p.start_offset / page_size as u64) as usize;
            if p.start_offset % page_size as u64 != 0 || slot >= slots {
                return Err(DwgError::corrupt(format!(
                    "section {name}: page start offset {:#x} does not fit page size {page_size:#x}",
                    p.start_offset
                )));
            }
            let &(addr, _) = self
                .pages
                .get(&p.page_number)
                .ok_or_else(|| DwgError::corrupt(format!("section {name}: page {} not in page map", p.page_number)))?;
            jobs.push((slot, *p, addr));
        }
        jobs.sort_by_key(|j| j.0);
        if jobs.windows(2).any(|w| w[0].0 == w[1].0) {
            return Err(DwgError::corrupt(format!("section {name}: two pages share one offset")));
        }

        // Split the buffer into one mutable chunk per slot.
        let mut chunks: Vec<Option<&mut [u8]>> = buf.chunks_mut(page_size).map(Some).collect();
        let mut work: Vec<(&mut [u8], PageRef, u64)> = Vec::with_capacity(jobs.len());
        for (slot, p, addr) in jobs {
            if let Some(chunk) = chunks[slot].take() {
                work.push((chunk, p, addr));
            }
        }
        let compressed = desc.compressed == 2;
        let data = self.data;
        let results: Vec<Result<Option<String>>> = work
            .into_par_iter()
            .map(|(chunk, p, addr)| read_data_page(data, addr, p, compressed, chunk))
            .collect();
        for r in results {
            if let Some(w) = r? {
                warnings.push(format!("{name}: {w}"));
            }
        }
        buf.truncate(size);
        Ok(Some(buf))
    }
}

/// Read a system page (page map or section map) and decompress it.
fn read_system_page(data: &[u8], addr: u64, want_type: u32, warnings: &mut Vec<String>) -> Result<Vec<u8>> {
    let hdr = slice(data, addr, 0x14)?;
    let ptype = le32(hdr, 0);
    if ptype != want_type {
        return Err(DwgError::corrupt(format!(
            "system page at {addr:#x} has type {ptype:#x}, expected {want_type:#x}"
        )));
    }
    let decomp = le32(hdr, 4) as usize;
    let comp = le32(hdr, 8) as u64;
    let ctype = le32(hdr, 12);
    let stored = le32(hdr, 16);
    if decomp > 0x400_0000 {
        return Err(DwgError::corrupt(format!("system page decompressed size {decomp} too large")));
    }
    let cdata = slice(data, addr + 0x14, comp)?;

    let mut h = [0u8; 0x14];
    h.copy_from_slice(hdr);
    h[16..20].fill(0);
    let c = page_checksum(page_checksum(0, &h), cdata);
    if c != stored {
        warnings.push(format!("system page {want_type:#x} checksum: stored {stored:#x}, computed {c:#x}"));
    }

    let mut out = vec![0u8; decomp];
    if ctype == 2 {
        let n = lz77::decompress(cdata, &mut out)?;
        if n != decomp {
            return Err(DwgError::corrupt(format!("system page: decompressed {n} bytes, expected {decomp}")));
        }
    } else {
        let n = decomp.min(cdata.len());
        out[..n].copy_from_slice(&cdata[..n]);
    }
    Ok(out)
}

fn parse_section_map(sm: &[u8]) -> Result<Vec<SectionDesc>> {
    if sm.len() < 20 {
        return Err(DwgError::corrupt("section map too short"));
    }
    let count = le32(sm, 0) as usize;
    let mut out = Vec::with_capacity(count.min(64));
    let mut i = 20;
    for _ in 0..count {
        if i + 0x60 > sm.len() {
            return Err(DwgError::corrupt("section map ended inside a description"));
        }
        let size = le64(sm, i);
        let page_count = le32(sm, i + 8);
        let max_page_size = le32(sm, i + 12);
        let compressed = le32(sm, i + 20);
        let id = le32(sm, i + 24);
        let encrypted = le32(sm, i + 28);
        let name_bytes = &sm[i + 32..i + 96];
        let nul = name_bytes.iter().position(|&b| b == 0).unwrap_or(64);
        let name = String::from_utf8_lossy(&name_bytes[..nul]).into_owned();
        i += 0x60;
        let mut pages = Vec::with_capacity((page_count as usize).min(1 << 16));
        for _ in 0..page_count {
            if i + 16 > sm.len() {
                return Err(DwgError::corrupt("section map ended inside a page list"));
            }
            pages.push(PageRef {
                page_number: le32(sm, i) as i32,
                data_size: le32(sm, i + 4),
                start_offset: le64(sm, i + 8),
            });
            i += 16;
        }
        out.push(SectionDesc { name, size, page_count, max_page_size, compressed, id, encrypted, pages });
    }
    Ok(out)
}

/// Read one data page into `out` (one page-size chunk of the section).
/// Returns a warning text when a checksum does not match.
fn read_data_page(data: &[u8], addr: u64, p: PageRef, compressed: bool, out: &mut [u8]) -> Result<Option<String>> {
    let raw = slice(data, addr, 32)?;
    let mask = 0x4164_536b ^ (addr as u32);
    let mut h = [0u8; 32];
    for k in 0..8 {
        let v = le32(raw, k * 4) ^ mask;
        h[k * 4..k * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    let ptype = le32(&h, 0);
    if ptype != DATA_PAGE {
        return Err(DwgError::corrupt(format!(
            "data page {} at {addr:#x} has type {ptype:#x}",
            p.page_number
        )));
    }
    let data_size = le32(&h, 8) as u64;
    let start = le64(&h, 16);
    let hdr_sum = le32(&h, 24);
    let data_sum = le32(&h, 28);
    if start != p.start_offset {
        return Err(DwgError::corrupt(format!(
            "data page {}: header start offset {start:#x} differs from section map {:#x}",
            p.page_number, p.start_offset
        )));
    }
    let cdata = slice(data, addr + 32, data_size)?;

    let mut warning = None;
    let ds = page_checksum(0, cdata);
    let mut hz = h;
    hz[24..28].fill(0);
    let hs = page_checksum(ds, &hz);
    if ds != data_sum || hs != hdr_sum {
        warning = Some(format!(
            "page {} checksum: data {data_sum:#x}/{ds:#x}, header {hdr_sum:#x}/{hs:#x}",
            p.page_number
        ));
    }

    if compressed {
        lz77::decompress(cdata, out)?;
    } else {
        let n = out.len().min(cdata.len());
        out[..n].copy_from_slice(&cdata[..n]);
    }
    Ok(warning)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_sequence_matches_spec() {
        let m = magic_sequence();
        assert_eq!(&m[..16], &[0x29, 0x23, 0xBE, 0x84, 0xE1, 0x6C, 0xD6, 0xAE, 0x52, 0x90, 0x49, 0xF1, 0xF1, 0xBB, 0xE9, 0xEB]);
        // (The table printed in the spec has only 92 of the 108 bytes, so we
        // check the first rows; the real-file tests check the rest.)
        assert_eq!(&m[0x20..0x30], &[0xB3, 0x12, 0x4D, 0xC8, 0x43, 0xBB, 0x8B, 0xA6, 0x1F, 0x03, 0x5A, 0x7D, 0x09, 0x38, 0x25, 0x1F]);
    }

    #[test]
    fn version_strings() {
        assert_eq!(Version::from_magic(b"AC1032").unwrap(), Version::R2018);
        assert_eq!(Version::from_magic(b"AC1027").unwrap(), Version::R2013);
        assert!(matches!(Version::from_magic(b"AC1015"), Err(DwgError::UnsupportedVersion(_))));
        assert!(matches!(Version::from_magic(b"PK\x03\x04ab"), Err(DwgError::NotDwg)));
    }

    #[test]
    fn short_or_foreign_files_fail_cleanly() {
        assert!(matches!(Container::parse(b"hello"), Err(DwgError::NotDwg)));
        let mut f = vec![0u8; 0x200];
        f[..6].copy_from_slice(b"AC1032");
        // header data id string missing -> corrupt, not a panic
        assert!(matches!(Container::parse(&f), Err(DwgError::Corrupt(_))));
        // R2007 is read now; a file without its coded header is corrupt.
        f[..6].copy_from_slice(b"AC1021");
        assert!(matches!(Container::parse(&f), Err(DwgError::Corrupt(_))));
        f.resize(0x800, 0);
        assert!(matches!(Container::parse(&f), Err(DwgError::Corrupt(_))));
    }

    /// Build a tiny but complete R2004-style file in memory: header,
    /// one data section with two pages (one compressed literal run each),
    /// a section map and a page map. Checks the whole path end to end.
    #[test]
    fn synthetic_file_round_trip() {
        let file = build_synthetic();
        let c = Container::parse(&file).unwrap();
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        assert_eq!(c.sections.len(), 1);
        let mut w = Vec::new();
        let s = c.section("AcDb:Test", &mut w).unwrap().unwrap();
        assert!(w.is_empty(), "{w:?}");
        assert_eq!(s.len(), 12);
        assert_eq!(&s, b"abcdefghWXYZ");
        assert!(c.section("AcDb:Missing", &mut w).unwrap().is_none());

        // Flip one byte of the second data page: checksum warning, and
        // the decoded text changes, but no panic.
        let mut bad = file.clone();
        let p2 = 0x100 + 0x40 + 32 + 2;
        bad[p2] ^= 0x01;
        let c = Container::parse(&bad).unwrap();
        let mut w = Vec::new();
        let _ = c.section("AcDb:Test", &mut w);
        assert!(!w.is_empty());
    }

    fn sys_page(ptype: u32, payload: &[u8]) -> Vec<u8> {
        // compress as: literal length code + bytes + 0x11
        let comp = literal_stream(payload);
        let mut h = vec![0u8; 0x14];
        h[0..4].copy_from_slice(&ptype.to_le_bytes());
        h[4..8].copy_from_slice(&(payload.len() as u32).to_le_bytes());
        h[8..12].copy_from_slice(&(comp.len() as u32).to_le_bytes());
        h[12..16].copy_from_slice(&2u32.to_le_bytes());
        let sum = page_checksum(page_checksum(0, &h), &comp);
        h[16..20].copy_from_slice(&sum.to_le_bytes());
        h.extend_from_slice(&comp);
        while !h.len().is_multiple_of(0x20) {
            h.push(0);
        }
        h
    }

    fn literal_stream(payload: &[u8]) -> Vec<u8> {
        // literal length L >= 4: code (L-3) if L-3 <= 0x0E (0x01..0x0E),
        // else 0x00 then (L-3-0x0F) (must be non-zero and < 256 here).
        let l = payload.len();
        assert!(l >= 4);
        let mut out = Vec::new();
        if l - 3 <= 0x0e {
            out.push((l - 3) as u8);
        } else {
            let rest = l - 3 - 0x0f;
            assert!(rest > 0 && rest < 256);
            out.push(0);
            out.push(rest as u8);
        }
        out.extend_from_slice(payload);
        out.push(0x11);
        out
    }

    fn data_page(addr: u64, section: u32, start: u64, payload: &[u8]) -> Vec<u8> {
        let comp = literal_stream(payload);
        let mut h = [0u8; 32];
        h[0..4].copy_from_slice(&DATA_PAGE.to_le_bytes());
        h[4..8].copy_from_slice(&section.to_le_bytes());
        h[8..12].copy_from_slice(&(comp.len() as u32).to_le_bytes());
        h[12..16].copy_from_slice(&0x40u32.to_le_bytes());
        h[16..24].copy_from_slice(&start.to_le_bytes());
        let ds = page_checksum(0, &comp);
        h[28..32].copy_from_slice(&ds.to_le_bytes());
        let hs = page_checksum(ds, &h);
        h[24..28].copy_from_slice(&hs.to_le_bytes());
        let mask = 0x4164_536b ^ (addr as u32);
        let mut out = Vec::new();
        for k in 0..8 {
            let v = le32(&h, k * 4) ^ mask;
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&comp);
        out.resize(0x40, 0);
        out
    }

    fn build_synthetic() -> Vec<u8> {
        let page_size = 8u32;
        // pages: 1 = data "abcdefgh", 2 = data "WXYZ", 3 = section map, 4 = page map
        let p1 = data_page(0x100, 1, 0, b"abcdefgh");
        let p2 = data_page(0x140, 1, 8, b"WXYZ");

        let mut sm = Vec::new();
        for v in [1u32, 2, 0x7400, 0, 1] {
            sm.extend_from_slice(&v.to_le_bytes());
        }
        sm.extend_from_slice(&12u64.to_le_bytes());
        for v in [2u32, page_size, 1, 2, 1, 0] {
            sm.extend_from_slice(&v.to_le_bytes());
        }
        let mut name = [0u8; 64];
        name[..9].copy_from_slice(b"AcDb:Test");
        sm.extend_from_slice(&name);
        for (num, start) in [(1u32, 0u64), (2, 8)] {
            sm.extend_from_slice(&num.to_le_bytes());
            sm.extend_from_slice(&0u32.to_le_bytes());
            sm.extend_from_slice(&start.to_le_bytes());
        }
        let p3 = sys_page(SYS_SECTION_MAP, &sm);

        let mut pm = Vec::new();
        for (num, size) in [(1u32, 0x40u32), (2, 0x40), (3, p3.len() as u32)] {
            pm.extend_from_slice(&num.to_le_bytes());
            pm.extend_from_slice(&size.to_le_bytes());
        }
        // page map's own entry (size filled after building it once)
        let pm_len_guess = {
            let mut t = pm.clone();
            t.extend_from_slice(&4u32.to_le_bytes());
            t.extend_from_slice(&0u32.to_le_bytes());
            sys_page(SYS_PAGE_MAP, &t).len() as u32
        };
        pm.extend_from_slice(&4u32.to_le_bytes());
        pm.extend_from_slice(&pm_len_guess.to_le_bytes());
        let p4 = sys_page(SYS_PAGE_MAP, &pm);

        let pm_addr = 0x100 + 0x40 + 0x40 + p3.len() as u64;
        let mut hd = [0u8; 0x6c];
        hd[..12].copy_from_slice(b"AcFssFcAJMB\0");
        hd[0x50..0x54].copy_from_slice(&4u32.to_le_bytes());
        hd[0x54..0x5c].copy_from_slice(&(pm_addr - 0x100).to_le_bytes());
        hd[0x5c..0x60].copy_from_slice(&3u32.to_le_bytes());
        let crc = crc32(0, &hd);
        hd[0x68..0x6c].copy_from_slice(&crc.to_le_bytes());

        let mut f = vec![0u8; 0x100];
        f[..6].copy_from_slice(b"AC1032");
        let magic = magic_sequence();
        for i in 0..0x6c {
            f[0x80 + i] = hd[i] ^ magic[i];
        }
        f.extend_from_slice(&p1);
        f.extend_from_slice(&p2);
        f.extend_from_slice(&p3);
        f.extend_from_slice(&p4);
        f
    }
}
