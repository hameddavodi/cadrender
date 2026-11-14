//! REGION, 3DSOLID, BODY: their outline from the ACIS data (cadrender-acis).
//!
//! DXF R2000-R2010 keeps the SAT text in the entity (groups 1 and 3, with
//! AutoCAD's character cipher). DXF R2013+ keeps SAB binary in the ACDSDATA
//! section at the end of the file, keyed by the entity handle (group 320).
//! For those the entity is pushed as an empty stroke first; the outline is
//! filled in by [`fill_pending`] once the whole file is read.

use super::Common;
use crate::reader::Reader;
use crate::tags::{hex, trim, Tag};
use cadrender_core::entity::stroke::StrokeEnt;
use cadrender_core::entity::Kind;
use cadrender_core::geom::PathData;

pub fn body(r: &mut Reader, body: &[Tag], com: &Common) -> Option<Kind> {
    // SAT lines: group 1 starts a line, group 3 continues it
    let mut sat: Vec<u8> = vec![];
    for &(c, v) in body {
        match c {
            1 => { if !sat.is_empty() { sat.push(b'\n'); } sat.extend_from_slice(trim_eol(v)); }
            3 => sat.extend_from_slice(trim_eol(v)),
            _ => {}
        }
    }
    if sat.is_empty() {
        r.objs.acis_pending.insert(com.props.handle);
        return Some(StrokeEnt { path: PathData::default(), width: 0.0 }.into());
    }
    let text = cadrender_acis::decipher(&sat, true);
    stroke(r, text.as_bytes())
}

fn trim_eol(v: &[u8]) -> &[u8] { let mut v = v; while let [rest @ .., b'\r' | b'\n'] = v { v = rest; } v }

/// Outline of SAT/SAB bytes as a stroke; counts what cannot be drawn.
fn stroke(r: &mut Reader, data: &[u8]) -> Option<Kind> {
    let Some(o) = cadrender_acis::outline(data) else { r.b.skip("ACIS(unreadable)"); return None };
    note_skipped(r, o.skipped);
    if o.path.is_empty() { return None; }
    Some(StrokeEnt { path: o.path, width: 0.0 }.into())
}

fn note_skipped(r: &mut Reader, n: u32) {
    if n == 0 { return; }
    for _ in 0..n { r.b.skip("ACIS_SPLINE_EDGE"); }
    r.b.warn("REGION/3DSOLID/BODY: spline edges are not drawn");
}

impl Reader {
    /// One ACDSRECORD of the ACDSDATA section: keep the SAB bytes of ACIS records.
    pub(crate) fn acds_record(&mut self, body: &[Tag]) {
        let Some(h) = body.iter().find(|t| t.0 == 320).map(|t| hex(t.1)) else { return };
        if !self.objs.acis_pending.contains(&h) { return; }
        let mut bytes = Vec::new();
        for &(c, v) in body {
            if c != 310 { continue; }
            let v = trim(v);
            bytes.extend(v.chunks_exact(2).filter_map(|p| std::str::from_utf8(p).ok().and_then(|s| u8::from_str_radix(s, 16).ok())));
        }
        if !bytes.is_empty() { self.objs.acis_blobs.insert(h, bytes); }
    }
}

/// Put the SAB outlines into the empty strokes of R2013+ entities. Entities
/// without data are removed.
pub fn fill_pending(r: &mut Reader) {
    let pending = std::mem::take(&mut r.objs.acis_pending);
    if pending.is_empty() { return; }
    let mut blobs = std::mem::take(&mut r.objs.acis_blobs);
    let mut outlines = std::collections::HashMap::new();
    for h in &pending {
        let path = blobs.remove(h).and_then(|b| cadrender_acis::outline(&b));
        match path {
            Some(o) => { note_skipped(r, o.skipped); outlines.insert(*h, o.path); }
            None => r.b.skip("ACIS(no data)"),
        }
    }
    for b in r.b.blocks_mut() {
        let mut changed = false;
        for e in b.ents.iter_mut() {
            let Kind::Stroke(s) = &mut e.kind else { continue };
            if !s.path.is_empty() || !pending.contains(&e.props.handle) { continue; }
            if let Some(p) = outlines.remove(&e.props.handle) { s.path = p; }
            changed = true;
        }
        if changed { b.ents.retain(|e| !matches!(&e.kind, Kind::Stroke(s) if s.path.is_empty() && pending.contains(&e.props.handle))); }
    }
}
