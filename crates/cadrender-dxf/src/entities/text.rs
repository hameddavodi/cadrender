//! TEXT, MTEXT and ATTRIB -> laid-out `TextEnt`.

use super::Common;
use crate::reader::Reader;
use crate::tags::{f, gf, gi, raw, s, trim, Tag};
use cadrender_core::color::EntColor;
use cadrender_core::entity::text::TextEnt;
use cadrender_core::text::build::{self, MTextIn, TextIn};
pub use cadrender_core::text::build::DEFAULT_FILL_SCALE;
use cadrender_core::text::mtext;

/// An MTEXT to lay out: from an MTEXT entity or from the text part of a MULTILEADER.
#[derive(Clone, Debug)]
pub struct MTextSpec {
    pub text: String,
    pub style: String,
    /// Text height; 0 = from the style.
    pub height: f64,
    pub pos: (f64, f64),
    /// Rotation in radians.
    pub rot: f64,
    /// Reference (wrap) width in drawing units, 0 = no wrap.
    pub width: f64,
    /// Attachment point 1..9 (1 = top left).
    pub attach: i32,
    /// Line spacing factor (1 = normal).
    pub spacing: f64,
    /// Background mask: `None` = no mask; `Some(None)` = page colour; `Some(Some(c))` = colour c.
    pub mask: Option<Option<EntColor>>,
    /// Border factor of the mask (1.5 = half a text height around).
    pub mask_scale: f64,
    /// Mirrored OCS (extrusion -Z).
    pub mirror: bool,
}

pub fn text(r: &mut Reader, kind: &[u8], body: &[Tag], com: &Common) -> Option<TextEnt> {
    // Cut at an embedded object (101) and at the junk XRECORD part LibreDWG
    // writes into some ATTRIBs: its "70 1" would hide the attribute.
    let body = match body.iter().position(|t| t.0 == 101 || (t.0 == 100 && trim(t.1) == b"AcDbXrecord")) { Some(p) => &body[..p], None => body };
    let g = |code: i32| gf(body, code);
    if kind == b"ATTRIB" && gi(body, 70).unwrap_or(0) & 1 != 0 { return None; }
    let style = body.iter().find(|t| t.0 == 7).map(|t| s(t.1)).unwrap_or_else(|| "STANDARD".into());
    if kind == b"MTEXT" {
        let mut text = String::new();
        for &(c, v) in body { if c == 3 { text.push_str(&raw(v)); } }
        if let Some(t) = body.iter().find(|t| t.0 == 1) { text.push_str(&raw(t.1)); }
        let text = mtext::caret_decode(&text).into_owned();
        let rot = if g(11).is_some() { g(21).unwrap_or(0.0).atan2(g(11).unwrap_or(1.0)) } else { g(50).unwrap_or(0.0).to_radians() };
        let spec = MTextSpec {
            text, style, height: g(40).unwrap_or(0.0), pos: (g(10).unwrap_or(0.0), g(20).unwrap_or(0.0)), rot,
            width: g(41).unwrap_or(0.0), attach: gi(body, 71).unwrap_or(1), spacing: g(44).unwrap_or(1.0),
            mask: mask_of(body), mask_scale: g(45).unwrap_or(DEFAULT_FILL_SCALE), mirror: com.ez < 0.0,
        };
        return mtext_ent(r, &spec);
    }
    let text = body.iter().find(|t| t.0 == 1).map(|t| mtext::caret_decode(&raw(t.1)).into_owned()).unwrap_or_default();
    let t = TextIn {
        value: &text, height: g(40).unwrap_or(0.0), width_factor: g(41).unwrap_or(0.0), oblique: g(51).unwrap_or(0.0),
        rotation: g(50).unwrap_or(0.0).to_radians(), ins: [g(10).unwrap_or(0.0), g(20).unwrap_or(0.0)],
        align: g(11).map(|x| [x, g(21).unwrap_or(0.0)]), ha: gi(body, 72).unwrap_or(0),
        va: gi(body, if kind == b"ATTRIB" { 74 } else { 73 }).unwrap_or(0), mirror: com.ez < 0.0,
    };
    build::text_ent(&mut r.b, &style, &t)
}

/// Lay out an MTEXT (the layout is `core::text::build::mtext_ent`).
pub fn mtext_ent(r: &mut Reader, sp: &MTextSpec) -> Option<TextEnt> {
    let input = MTextIn {
        text: &sp.text, height: sp.height, pos: [sp.pos.0, sp.pos.1], rot: sp.rot, width: sp.width,
        attach: sp.attach, spacing: sp.spacing, mask: sp.mask, mask_scale: sp.mask_scale, mirror: sp.mirror,
    };
    build::mtext_ent(&mut r.b, &sp.style, &input)
}

/// MTEXT background fill (group 90): bit 1 = fill colour (63/421), bit 2 = page colour.
fn mask_of(body: &[Tag]) -> Option<Option<EntColor>> {
    let bg = gi(body, 90).unwrap_or(0);
    if bg & 3 == 0 { return None; }
    if bg & 2 != 0 { return Some(None); }
    Some(Some(match body.iter().find(|t| t.0 == 421) {
        Some(t) => EntColor::true_color(crate::tags::long(t.1) as u32 & 0xff_ffff),
        None => EntColor::aci(body.iter().find(|t| t.0 == 63).map(|t| f(t.1) as i32).unwrap_or(256)),
    }))
}
