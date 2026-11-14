//! MULTILEADER -> a small anonymous block (leader lines, arrowheads, dogleg,
//! MTEXT or block content with attributes) drawn through an `InsertEnt`.
//! The part builders are `cadrender_core::entity::mleader` (shared with the
//! DXF reader); this file maps the decoded context data (the geometry as
//! AutoCAD last drew it, already scaled) into them.

use crate::entities::Com;
use crate::hatch::ent_color;
use crate::tables::Ctx;
use cadrender_core::entity::mleader as ml;
use cadrender_core::entity::{Entity, Kind, Props};
use cadrender_core::text::build::{MTextIn, DEFAULT_FILL_SCALE};
use cadrender_dwg::{EntityKind as K, MLeader, MLeaderText};

type P = [f64; 2];

impl Ctx<'_> {
    pub fn mleader(&mut self, m: &MLeader, com: &Com) -> Option<Kind> {
        let c = &m.context;
        let mut ents: Vec<Entity> = vec![];
        // the parts take the entity's layer; ByBlock parts inherit from the MULTILEADER
        let base = Props { handle: 0, ..com.props };
        let arrow = Some(c.arrow_size).filter(|s| *s > 0.0).unwrap_or(m.arrow_size * m.scale);
        let lines = ml::Lines {
            props: Props {
                color: ent_color(m.line_color),
                ltype: self.ltype_of.get(&m.line_type).copied().unwrap_or(base.ltype),
                lw: m.line_weight.clamp(-3, 211) as i16,
                ..base
            },
            path_type: m.path_type as i32,
            arrow: if arrow > 0.0 { arrow } else { 0.0 },
            arrow_block: self.block_of.get(&m.arrow_head).copied(),
        };
        for root in &c.roots {
            let conn = [root.connection[0], root.connection[1]];
            for line in &root.lines {
                let mut pts: Vec<P> = line.points.iter().map(|p| [p[0], p[1]]).collect();
                pts.push(conn);
                lines.line(pts, &mut ents);
            }
            if m.enable_landing && m.enable_dogleg && root.unknown {
                lines.dogleg(conn, [root.direction[0], root.direction[1]], root.landing_distance, &mut ents);
            }
        }
        if m.content_type == 2 { if let Some(t) = &c.text { self.mleader_text(m, t, base, &mut ents); } }
        if m.content_type == 1 {
            if let Some(bc) = &c.block {
                if let Some(&b) = self.block_of.get(&bc.record) {
                    let color = ent_color(bc.color);
                    let pos = [bc.location[0], bc.location[1]];
                    let scale = [bc.scale[0], bc.scale[1]];
                    let to_world = ml::block_part(&mut self.b, b, pos, bc.rotation, scale, Props { color, ..base }, &mut ents);
                    // attributes: the ATTDEF of the block, with the value from the MULTILEADER
                    for (h, value, _, _) in &m.block_attribs {
                        let d = self.d;
                        let Some(e) = d.get(*h).and_then(|o| o.entity()) else { continue };
                        let K::AttDef(a) = &e.kind else { continue };
                        if a.flags & 1 != 0 { continue; }
                        let ac = self.com(*h, e);
                        let mut t = a.text.clone();
                        t.value = value.clone();
                        let Some(te) = self.text_kind(&K::Text(t), &ac) else { continue };
                        ml::attrib(te, &to_world, ac.props.color, color, base, &mut ents);
                    }
                }
            }
        }
        ml::finish(&mut self.b, com.props.handle, ents)
    }

    /// MTEXT content, with its background mask and optional frame.
    fn mleader_text(&mut self, m: &MLeader, t: &MLeaderText, base: Props, ents: &mut Vec<Entity>) {
        let d = t.direction;
        let rot = if d[0] == 0.0 && d[1] == 0.0 { 0.0 } else { d[1].atan2(d[0]) };
        let sp = MTextIn {
            text: &t.label, height: m.context.text_height, pos: [t.location[0], t.location[1]], rot,
            width: if t.width > 1e-9 { t.width } else { 0.0 }, attach: t.attachment as i32, spacing: t.line_spacing_factor,
            mask: if t.bg_fill { Some(if t.bg_mask { None } else { Some(ent_color(t.bg_color)) }) } else { None },
            mask_scale: if t.bg_scale > 0.0 { t.bg_scale } else { DEFAULT_FILL_SCALE }, mirror: false,
        };
        let Some(te) = self.mtext_ent(t.style, &sp) else { return };
        let props = Props { color: ent_color(t.color), ..base };
        let frame = m.text_frame.then_some((m.context.landing_gap, Props { color: ent_color(m.line_color), ..base }));
        ml::text_part(&self.b, te, props, frame, ents);
    }
}
