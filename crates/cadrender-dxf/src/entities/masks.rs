//! Masks after reading: WIPEOUT frames.
//!
//! A WIPEOUT is a fill in the page colour (`FillEnt { background: true }`).
//! With WIPEOUTFRAME = 1 AutoCAD also plots its outline in the entity's colour
//! and lineweight. The setting lives in the OBJECTS section (read last), so
//! the frames are added in one pass at the end: a stroke right after each
//! wipeout, with the same handle so the SORTENTS order keeps them together.

use crate::reader::Reader;
use cadrender_core::entity::frame::FrameEnt;
use cadrender_core::entity::{Entity, Kind};

fn is_wipeout(e: &Entity) -> bool { matches!(&e.kind, Kind::Fill(f) if f.background) }

pub fn wipeout_frames(r: &mut Reader) {
    for b in r.b.blocks_mut() {
        if !b.ents.iter().any(is_wipeout) { continue; }
        let mut out = Vec::with_capacity(b.ents.len() + 8);
        for e in b.ents.drain(..) {
            let frame = match &e.kind {
                Kind::Fill(f) if f.background => Some(Entity::new(e.props, FrameEnt { path: f.path.clone() })),
                _ => None,
            };
            out.push(e);
            if let Some(fr) = frame { out.push(fr); }
        }
        b.ents = out;
    }
}
