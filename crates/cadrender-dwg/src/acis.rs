//! REGION, 3DSOLID, BODY: the raw ACIS model data (ODA spec, "ACIS data").
//!
//! Up to R2010 the data is in the object: version 1 is SAT text in blocks
//! (each character `c` stored as `159 - c`), version 2 is SAB binary up to
//! its end marker. From R2013 the data lives in the AcDs data section,
//! not in the object; then [`Acis::data`] is empty.
//! Turning the data into lines is done by `cadrender-acis`.

use crate::error::Result;
use crate::streams::ObjReader;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Acis {
    /// SAT text (already deciphered) or SAB bytes. Empty if none here.
    pub data: Vec<u8>,
}

const SAB_ENDS: [&[u8]; 2] = [b"\x0e\x03End\x0e\x02of\x0e\x04ACIS\x0d\x04data", b"\x0e\x03End\x0e\x02of\x0e\x03ASM\x0d\x04data"];

pub(crate) fn acis(r: &mut ObjReader<'_>) -> Result<Acis> {
    let mut a = Acis::default();
    if r.ver.r2013_plus() || r.d.b()? {
        return Ok(a); // in the AcDs section (R2013+), or no data
    }
    r.d.b()?; // unknown
    match r.d.bs()? {
        1 => loop {
            let n = r.d.bl()?;
            if n <= 0 || n as usize > r.d.remaining() / 8 { break; }
            let block = r.d.bytes(n as usize)?;
            a.data.extend(block.iter().map(|&c| if c <= 32 || c > 159 { c } else { 159 - c }));
        },
        2 => {
            // SAB: raw bytes up to and including the end marker
            while r.d.remaining() >= 8 {
                a.data.push(r.d.rc()?);
                if SAB_ENDS.iter().any(|e| a.data.ends_with(e)) { break; }
            }
        }
        _ => {}
    }
    // the wireframe and silhouette data that follow are not needed
    Ok(a)
}

/// R2013+: put the SAB data of the "AcDb:AcDsPrototype_1b" section into the
/// ACIS entities that have none. Only reads the section when such an entity exists.
pub(crate) fn fill_from_acds(c: &crate::container::Container, objects: &mut [crate::model::Object], warnings: &mut Vec<String>) {
    use crate::model::{EntityKind, ObjectKind};
    let empty = |o: &crate::model::Object| matches!(&o.kind, ObjectKind::Entity(e) if matches!(&e.kind, EntityKind::Acis(a) if a.data.is_empty()));
    if !objects.iter().any(empty) { return; }
    let Ok(Some(sec)) = c.section("AcDb:AcDsPrototype_1b", warnings) else { return };
    let mut blobs = acds_blobs(&sec);
    for o in objects.iter_mut() {
        if let ObjectKind::Entity(e) = &mut o.kind {
            if let EntityKind::Acis(a) = &mut e.kind {
                if a.data.is_empty() { if let Some(b) = blobs.remove(&o.handle) { a.data = b; } }
            }
        }
    }
}

/// SAB blobs of an AcDs data section by entity handle.
///
/// Layout (seen in AutoCAD 2018 files): segments start with `AC D5` and a
/// 6-byte name. A `_data_` segment has a 48-byte header, then entries of
/// 20 bytes (`u32 20`, `u32 1`, `u64 handle`, `u32 offset`), then padding to
/// 16 bytes; each entry's data is a `u32` size and the bytes, at `offset`
/// from there. Only blobs that start with a SAB signature are kept.
pub fn acds_blobs(b: &[u8]) -> std::collections::HashMap<u64, Vec<u8>> {
    let u32_at = |p: usize| b.get(p..p + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]) as usize);
    let mut out = std::collections::HashMap::new();
    let mut s = 0;
    while let Some(off) = b.get(s..).and_then(|r| r.windows(8).position(|w| w == b"\xac\xd5_data_")) {
        let seg = s + off;
        s = seg + 8;
        let mut p = seg + 0x30;
        let mut entries = vec![];
        while u32_at(p) == Some(20) && u32_at(p + 4) == Some(1) {
            let Some(h) = b.get(p + 8..p + 16).map(|x| u64::from_le_bytes(x.try_into().unwrap_or([0; 8]))) else { break };
            let Some(o) = u32_at(p + 16) else { break };
            entries.push((h, o));
            p += 20;
        }
        let base = (p + 15) & !15;
        for (h, o) in entries {
            let at = base + o;
            let Some(n) = u32_at(at) else { continue };
            let Some(data) = b.get(at + 4..at + 4 + n) else { continue };
            if data.starts_with(b"ACIS BinaryFile") || data.starts_with(b"ASM BinaryFile") { out.insert(h, data.to_vec()); }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn acds_blobs_by_handle() {
        let mut b = vec![0u8; 0x80];
        b.extend_from_slice(b"\xac\xd5_data_");
        b.resize(0x80 + 0x30, 0x55);
        let sab = b"ASM BinaryFile4 rest".to_vec();
        for (h, off) in [(0x2Bu64, 0u32), (0x2C, 8)] {
            b.extend_from_slice(&20u32.to_le_bytes());
            b.extend_from_slice(&1u32.to_le_bytes());
            b.extend_from_slice(&h.to_le_bytes());
            b.extend_from_slice(&off.to_le_bytes());
        }
        while !b.len().is_multiple_of(16) { b.push(b'b'); }
        // entry 0x2B: a PNG thumbnail (dropped); entry 0x2C: SAB
        b.extend_from_slice(&4u32.to_le_bytes());
        b.extend_from_slice(b"\x89PNG");
        b.extend_from_slice(&(sab.len() as u32).to_le_bytes());
        b.extend_from_slice(&sab);
        let m = super::acds_blobs(&b);
        assert_eq!(m.len(), 1);
        assert_eq!(m.get(&0x2C), Some(&sab));
        assert!(super::acds_blobs(&b[..0x90]).is_empty());
    }
}
