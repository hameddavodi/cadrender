//! TEXT, MTEXT and ATTRIB -> laid-out `TextEnt`.
//!
//! The layout itself is `cadrender_core::text::build` (shared with the DXF
//! reader); this file only maps typed DWG values into its input structs.
//! DWG angles are radians; DWG strings keep control characters raw, so there
//! is no caret decoding here.

use crate::entities::Com;
use crate::tables::{deg, Ctx};
use cadrender_core::color::EntColor;
use cadrender_core::entity::text::TextEnt;
use cadrender_core::text::build::{self, MTextIn, TextIn};
use cadrender_dwg::{Color, EntityKind as K, MText, ObjectKind, XValue};

impl Ctx<'_> {
    /// TEXT, MTEXT or ATTRIB entity -> text; `None` if empty or invisible.
    pub fn text_kind(&mut self, k: &K, com: &Com) -> Option<TextEnt> {
        let (style, mut tin) = match k {
            K::Text(t) => (t.style, text_in(t)),
            K::Attrib(a) => { if a.flags & 1 != 0 { return None; } (a.text.style, text_in(&a.text)) }
            K::MText(m) => return self.mtext_kind(m, None, com),
            _ => return None,
        };
        tin.mirror = com.ez < 0.0;
        let name = self.style_name_of(style);
        build::text_ent(&mut self.b, &name, &tin)
    }

    /// Text style name of a style handle ("STANDARD" if unknown).
    pub(crate) fn style_name_of(&self, style: u64) -> String {
        self.style_name.get(&style).cloned().unwrap_or_else(|| "STANDARD".into())
    }

    /// MTEXT; `rt` replaces the stored text (see `rich_text`).
    pub fn mtext_kind(&mut self, m: &MText, rt: Option<&str>, com: &Com) -> Option<TextEnt> {
        let sp = MTextIn {
            text: rt.unwrap_or(&m.value), height: m.text_height, pos: [m.insertion[0], m.insertion[1]],
            rot: m.x_axis[1].atan2(m.x_axis[0]), width: m.rect_width, attach: m.attachment as i32,
            spacing: m.linespacing_factor, mask: mask_color(m), mask_scale: m.background_scale, mirror: com.ez < 0.0,
        };
        self.mtext_ent(m.style, &sp)
    }

    /// R2007 files keep the R2008+ form of an MTEXT's text (with paragraph
    /// formats) in an "ACAD_MTEXT_2008_RT" XRECORD of its extension
    /// dictionary (groups 3 then 1, like MTEXT). ODA's DXF uses that text.
    pub fn rich_text(&self, xdict: Option<u64>) -> Option<String> {
        let dict = match &self.d.get(xdict?)?.kind { ObjectKind::Dictionary(x) => x, _ => return None };
        let h = dict.entries.iter().find(|(n, _)| n == "ACAD_MTEXT_2008_RT")?.1;
        let ObjectKind::XRecord(x) = &self.d.get(h)?.kind else { return None };
        let mut out = String::new();
        for code in [3, 1] {
            for (c, v) in &x.items { if *c == code { if let XValue::Str(s) = v { out.push_str(s); } } }
        }
        (!out.is_empty()).then_some(out)
    }

    /// MTEXT with inline formatting (layout: `core::text::build::mtext_ent`).
    pub(crate) fn mtext_ent(&mut self, style: u64, sp: &MTextIn) -> Option<TextEnt> {
        let name = self.style_name_of(style);
        build::mtext_ent(&mut self.b, &name, sp)
    }
}

fn text_in(t: &cadrender_dwg::Text) -> TextIn<'_> {
    // A DWG leaves the alignment point out (data flag 0x02) when it is the
    // origin; ODA writes 11/21 = 0,0 then, and AutoCAD aligns on (0, 0).
    TextIn {
        value: &t.value, height: t.height, width_factor: t.width_factor, oblique: deg(t.oblique),
        rotation: t.rotation, ins: t.insertion, align: t.alignment.or(Some([0.0, 0.0])), ha: t.halign as i32, va: t.valign as i32,
        mirror: false,
    }
}

/// MTEXT background fill: bit 1 = fill colour, bit 2 = page colour.
fn mask_color(m: &MText) -> Option<Option<EntColor>> {
    let bg = m.background_flags;
    if bg & 3 == 0 { return None; }
    if bg & 2 != 0 { return Some(None); }
    Some(Some(match m.background_color {
        Some(Color::Rgb(rgb)) => EntColor::true_color(rgb),
        Some(Color::Index(i)) => EntColor::aci(i as i32),
        Some(Color::ByBlock) => EntColor::BYBLOCK,
        _ => EntColor::BYLAYER,
    }))
}
