//! IMAGE: where the picture goes, its clip boundary and the IMAGEDEF file
//! name. The IMAGEDEF is in the OBJECTS section (read later), so the file
//! name is filled in by [`fill_files`] at the end. The picture is loaded
//! after reading (see `cadrender_core::entity::image`).

use super::Common;
use crate::reader::Reader;
use crate::tags::{gf, gi, hex, s, Tag};
use cadrender_core::entity::image::ImageEnt;
use cadrender_core::entity::Kind;
use cadrender_core::geom::PathData;

fn g(body: &[Tag], code: i32) -> f64 { gf(body, code).unwrap_or(0.0) }

pub fn image(r: &mut Reader, body: &[Tag], com: &Common) -> Option<Kind> {
    let (w, h) = (g(body, 13), g(body, 23));
    if !(w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite()) { return None; }
    let m = ImageEnt::matrix([g(body, 10), g(body, 20)], [g(body, 11), g(body, 21)], [g(body, 12), g(body, 22)], h);
    // clip boundary only when clipping is on (280 = 1); else the whole picture
    let clipped = gi(body, 280).unwrap_or(0) == 1;
    let frame = match (clipped, super::fills::wipeout(body)) {
        (true, Some(Kind::Fill(f))) => f.path,
        _ => {
            let mut p = PathData::default();
            for (k, (x, y)) in [(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)].into_iter().enumerate() {
                let (x, y) = m.apply(x, y);
                if k == 0 { p.move_to(x, y) } else { p.line_to(x, y) }
            }
            p.close();
            p
        }
    };
    let def = body.iter().find(|t| t.0 == 340).map(|t| hex(t.1)).unwrap_or(0);
    if def != 0 { r.objs.image_defs.push((com.props.handle, def)); }
    let pct = |c: i32, d: i32| gi(body, c).unwrap_or(d).clamp(0, 100) as u8;
    Some(ImageEnt {
        m, size: [w, h], frame,
        show: gi(body, 70).unwrap_or(1) & 1 != 0,
        brightness: pct(281, 50), contrast: pct(282, 50), fade: pct(283, 0),
        file: String::new(), picture: None,
    }.into())
}

impl Reader {
    /// IMAGEDEF: keep the file name (group 1).
    pub(crate) fn imagedef(&mut self, body: &[Tag]) {
        let h = body.iter().find(|t| t.0 == 5).map(|t| hex(t.1)).unwrap_or(0);
        if let Some(f) = body.iter().find(|t| t.0 == 1) { self.objs.image_files.insert(h, s(f.1)); }
    }
}

/// Give each IMAGE the file name of its IMAGEDEF.
pub fn fill_files(r: &mut Reader) {
    let wanted = std::mem::take(&mut r.objs.image_defs);
    if wanted.is_empty() { return; }
    let by_ent: std::collections::HashMap<u64, String> = wanted.iter().filter_map(|(e, d)| r.objs.image_files.get(d).map(|f| (*e, f.clone()))).collect();
    for b in r.b.blocks_mut() {
        for e in b.ents.iter_mut() {
            if let Kind::Image(img) = &mut e.kind { if let Some(f) = by_ent.get(&e.props.handle) { img.file = f.clone(); } }
        }
    }
}
