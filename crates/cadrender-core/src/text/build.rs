//! TEXT / ATTRIB and MTEXT -> laid-out [`TextEnt`], shared by the DXF and DWG
//! readers. A reader only maps its raw fields into [`TextIn`] or [`MTextIn`]
//! (angles in radians, oblique in degrees, DXF caret escapes already decoded)
//! and calls [`text_ent`] or [`mtext_ent`] with the text style name.

use super::{layout, mtext};
use crate::color::EntColor;
use crate::entity::text::{TextEnt, TextMask};
use crate::geom::{Aff, BBox};
use crate::scene::builder::SceneBuilder;

/// Line spacing of MTEXT and multi-line text, as a multiple of the cap height.
pub const LINE_SPACING: f64 = 5.0 / 3.0;

/// Default MTEXT background border factor (some writers store garbage here).
pub const DEFAULT_FILL_SCALE: f64 = 1.5;

/// A TEXT or ATTRIB, named after its DXF groups.
#[derive(Clone, Debug, Default)]
pub struct TextIn<'a> {
    /// 1: the text (caret escapes decoded).
    pub value: &'a str,
    /// 40: height; 0 = from the style.
    pub height: f64,
    /// 41: width factor; 0 or out of range = 1.
    pub width_factor: f64,
    /// 51: oblique angle in degrees.
    pub oblique: f64,
    /// 50: rotation in radians.
    pub rotation: f64,
    /// 10/20: insertion point.
    pub ins: [f64; 2],
    /// 11/21: alignment point, if the entity has one.
    pub align: Option<[f64; 2]>,
    /// 72: horizontal alignment.
    pub ha: i32,
    /// 73 (74 for ATTRIB): vertical alignment.
    pub va: i32,
    /// Mirrored OCS (extrusion -Z).
    pub mirror: bool,
}

/// An MTEXT to lay out: from an MTEXT entity, a MULTILEADER or a dimension.
#[derive(Clone, Debug, Default)]
pub struct MTextIn<'a> {
    /// The text with inline codes (caret escapes decoded).
    pub text: &'a str,
    /// Text height; 0 = from the style.
    pub height: f64,
    pub pos: [f64; 2],
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

/// Face, height, width factor and oblique of a text style (by name).
/// Warns and returns `None` when no font at all is loaded.
pub fn style_font(b: &mut SceneBuilder, name: &str) -> Option<(u16, f64, f64, f64)> {
    let (face, h, wf, obl) = match b.text_style(name) {
        Some(st) => (st.face, st.height, st.width_factor, st.oblique),
        None => (None, 0.0, 1.0, 0.0),
    };
    match face.or_else(|| b.fonts.face_for("")) {
        Some(f) => Some((f, h, wf, obl)),
        None => { b.warn("no font available: texts are not drawn"); None }
    }
}

/// Font units -> block coordinates; `None` if not finite.
#[allow(clippy::too_many_arguments)]
fn place(p: [f64; 2], rot: f64, oblique: f64, sx: f64, sy: f64, ax: f64, ay: f64, mirror: bool) -> Option<Aff> {
    let mut m = Aff::translate(p[0], p[1]).mul(&Aff::rotate(rot));
    if oblique.abs() > 1e-6 && oblique.abs() < 85.0 { m = m.mul(&Aff::shear_x(oblique.to_radians())); }
    m = m.mul(&Aff::scale(sx, sy)).mul(&Aff::translate(ax, ay));
    if mirror { m = Aff::scale(-1.0, 1.0).mul(&m); }
    m.0.iter().all(|v| v.is_finite()).then_some(m)
}

/// TEXT or ATTRIB -> text; `None` if empty, without a font or not finite.
pub fn text_ent(b: &mut SceneBuilder, style: &str, t: &TextIn) -> Option<TextEnt> {
    if t.value.trim().is_empty() { return None; }
    if !(t.ins[0].is_finite() && t.ins[1].is_finite()) { return None; }
    let (face_id, style_h, _, _) = style_font(b, style)?;
    let h = if t.height > 0.0 { t.height } else if style_h > 0.0 { style_h } else { 1.0 };
    if !(h > 0.0 && h < 1e9) { return None; }
    // Font policy (see text/mod.rs): keep each style's width factor and oblique.
    // TEXT and ATTRIB carry their own copy (groups 41 and 51).
    let wf = if t.width_factor > 0.0 && t.width_factor < 100.0 { t.width_factor } else { 1.0 };
    let face = &mut b.fonts.faces[face_id as usize];
    let cap = face.cap_height;
    let s_em = h / cap; // font units -> drawing units
    let laid = layout::layout(face, &[mtext::plain(t.value)], 0.0, LINE_SPACING);
    if laid.glyphs.is_empty() { return None; }
    let width = laid.width;
    let (mut ax, mut ay, mut p) = (0.0, 0.0, t.ins);
    // Aligned (3) and Fit (5) keep the insertion point.
    if let Some(al) = t.align.filter(|_| (t.ha != 0 || t.va != 0) && t.ha != 3 && t.ha != 5) {
        p = al;
        ax = match t.ha { 1 | 4 => -width / 2.0, 2 => -width, _ => 0.0 };
        ay = match t.va { 1 => cap * 0.25, 2 => -cap / 2.0, 3 => -cap, _ => 0.0 };
        if t.ha == 4 { ay = -cap / 2.0; }
    }
    let m = place(p, t.rotation, t.oblique, s_em * wf, s_em, ax, ay, t.mirror)?;
    let frame = BBox::new(0.0, -0.35 * cap, width, cap * 1.25);
    Some(TextEnt { face: face_id, glyphs: laid.glyphs, m, cap_h: h, width, frame, mask: None, rich: None })
}

/// MTEXT with inline formatting -> text; `None` if empty, without a font or not finite.
pub fn mtext_ent(b: &mut SceneBuilder, style: &str, sp: &MTextIn) -> Option<TextEnt> {
    if sp.text.trim().is_empty() || !(sp.pos[0].is_finite() && sp.pos[1].is_finite()) { return None; }
    let (face_id, style_h, wf, oblique) = style_font(b, style)?;
    let h = if sp.height > 0.0 { sp.height } else if style_h > 0.0 { style_h } else { 1.0 };
    if !(h > 0.0 && h < 1e9) { return None; }
    // MTEXT takes width factor and oblique from its style (ezdxf ignores them; AutoCAD does not).
    let face = &mut b.fonts.faces[face_id as usize];
    let cap = face.cap_height;
    let s_em = h / cap; // font units -> drawing units
    let wrap = sp.width.max(0.0) / (s_em * wf);
    let line_factor = LINE_SPACING * if sp.spacing > 0.0 && sp.spacing < 10.0 { sp.spacing } else { 1.0 };
    // lines follow the attachment (left / centre / right) unless \pxq says otherwise
    let att = sp.attach.clamp(1, 9) - 1;
    let toks = mtext::parse(sp.text, h, wf, oblique);
    let laid = mtext::layout_rich(face, &toks, wrap, line_factor, wf, oblique, (att % 3) as u8);
    if laid.glyphs.is_empty() { return None; }
    let (width, col, top, block_h) = (laid.width, laid.col, laid.top, laid.block_h);
    // anchor offset in font units (layout: first baseline at y = 0, lines go down;
    // the column box is [0, col] wide, `top` above the first baseline)
    let ax = match att % 3 { 1 => -col / 2.0, 2 => -col, _ => 0.0 };
    let ay = match att / 3 { 0 => -top, 1 => -top + block_h / 2.0, _ => -top + block_h };
    let m = place(sp.pos, sp.rot, oblique, s_em * wf, s_em, ax, ay, sp.mirror)?;
    let frame = BBox::new(laid.ink_x.0, top - block_h - 0.35 * cap, laid.ink_x.1, top + 0.25 * cap);
    let mask = sp.mask.map(|color| {
        let scale = if (1.0..=5.0).contains(&sp.mask_scale) { sp.mask_scale } else { DEFAULT_FILL_SCALE };
        Box::new(TextMask { rect: BBox::new(0.0, top - block_h, col, top).grow(cap * (scale - 1.0)), color })
    });
    Some(TextEnt { face: face_id, glyphs: laid.glyphs, m, cap_h: h, width, frame, mask, rich: laid.rich })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{load_fonts, FontChoice, Fonts};

    fn builder() -> SceneBuilder { SceneBuilder::new(load_fonts(&FontChoice::Liberation).unwrap()) }

    #[test]
    fn text_alignment_moves_to_the_alignment_point() {
        let mut b = builder();
        let left = text_ent(&mut b, "STANDARD", &TextIn { value: "AB", height: 2.0, ins: [10.0, 5.0], ..Default::default() }).unwrap();
        assert_eq!(left.m.apply(0.0, 0.0), (10.0, 5.0));
        // centred (72 = 1) on 11/21: the middle of the text is at the alignment point
        let t = TextIn { value: "AB", height: 2.0, ins: [10.0, 5.0], align: Some([20.0, 5.0]), ha: 1, ..Default::default() };
        let c = text_ent(&mut b, "STANDARD", &t).unwrap();
        let mid = c.m.apply(c.width / 2.0, 0.0);
        assert!((mid.0 - 20.0).abs() < 1e-9 && (mid.1 - 5.0).abs() < 1e-9, "{mid:?}");
        // Fit (5) keeps the insertion point
        let f = text_ent(&mut b, "STANDARD", &TextIn { ha: 5, ..t }).unwrap();
        assert_eq!(f.m.apply(0.0, 0.0), (10.0, 5.0));
    }

    #[test]
    fn empty_or_broken_text_is_none() {
        let mut b = builder();
        assert!(text_ent(&mut b, "STANDARD", &TextIn { value: "  ", ..Default::default() }).is_none());
        assert!(text_ent(&mut b, "STANDARD", &TextIn { value: "A", ins: [f64::NAN, 0.0], ..Default::default() }).is_none());
        assert!(text_ent(&mut b, "STANDARD", &TextIn { value: "A", rotation: f64::INFINITY, ..Default::default() }).is_none());
        assert!(mtext_ent(&mut b, "STANDARD", &MTextIn { text: "", ..Default::default() }).is_none());
        // no font at all: a warning, no text
        let mut nf = SceneBuilder::new(Fonts::new());
        assert!(text_ent(&mut nf, "STANDARD", &TextIn { value: "A", ..Default::default() }).is_none());
    }

    #[test]
    fn mtext_lines_and_mask() {
        let mut b = builder();
        let sp = MTextIn { text: "A\\PB", height: 1.0, attach: 1, mask: Some(None), mask_scale: 9.0, ..Default::default() };
        let te = mtext_ent(&mut b, "STANDARD", &sp).unwrap();
        assert_eq!(te.glyphs.len(), 2);
        // a broken border factor falls back to the default 1.5
        let cap = b.fonts.faces[te.face as usize].cap_height;
        let mask = te.mask.unwrap();
        assert!((mask.rect.x0 + cap * (DEFAULT_FILL_SCALE - 1.0)).abs() < 1e-9, "{:?}", mask.rect);
    }
}
