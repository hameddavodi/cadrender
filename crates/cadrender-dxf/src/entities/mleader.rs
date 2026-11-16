//! MULTILEADER -> a small anonymous block (leader lines, arrowheads, dogleg,
//! MTEXT or block content with attributes) drawn through an `InsertEnt`.
//!
//! The values come from the entity's CONTEXT_DATA, which holds the geometry
//! as AutoCAD last drew it (already scaled), and from the per-entity values
//! after it (ODA writes every style value there, so MLEADERSTYLE is only a
//! fallback for files that leave them out).

use super::text::{mtext_ent, MTextSpec, DEFAULT_FILL_SCALE};
use super::Common;
use crate::reader::{MetaRec, Reader};
use crate::tags::{f, hex, i, raw, trim, Tag};
use cadrender_core::color::EntColor;
use cadrender_core::entity::mleader as ml;
pub(crate) use cadrender_core::entity::mleader::closed_arrow;
use cadrender_core::entity::{Entity, Kind, Props};
use cadrender_core::scene::meta::{self, TextKind, TextRec};
use cadrender_core::scene::tables::Handle;
use cadrender_core::text::mtext::caret_decode;

type P = [f64; 2];

/// One leader: its lines (vertices from the arrow tip on) and the landing.
#[derive(Default, Debug)]
struct Leader { conn: Option<P>, dogleg_dir: P, dogleg_len: f64, has_dogleg: bool, lines: Vec<Vec<P>> }

/// What the entity says, after parsing.
#[derive(Default, Debug)]
pub(crate) struct ML {
    leaders: Vec<Leader>,
    // context
    arrow_size: Option<f64>,
    landing_gap: f64,
    has_text: bool,
    text: String,
    text_style: Handle,
    text_pos: P,
    text_dir: Option<P>,
    text_h: f64,
    text_w: f64,
    spacing: f64,
    text_color: Option<EntColor>,
    attach: i32,
    bg_on: bool,
    bg_page: bool,
    bg_color: Option<EntColor>,
    bg_scale: f64,
    has_block: bool,
    block: Handle,
    block_pos: P,
    block_scale: P,
    block_rot: f64,
    block_color: Option<EntColor>,
    // entity
    style: Handle,
    leader_type: i32,
    line_color: Option<EntColor>,
    ltype: Handle,
    lw: Option<i16>,
    landing: bool,
    dogleg: bool,
    dogleg_len: f64,
    arrow_block: Handle,
    ent_arrow_size: Option<f64>,
    content: i32,
    frame: bool,
    scale: f64,
    /// Block attributes: (ATTDEF handle, value).
    attribs: Vec<(Handle, String)>,
}

/// Parse the groups. Sections: 300 CONTEXT_DATA{ .. 301 }, 302 LEADER{ .. 303 }, 304 LEADER_LINE{ .. 305 }.
pub(crate) fn parse(body: &[Tag]) -> ML {
    let mut m = ML { leader_type: 1, landing: true, dogleg: true, scale: 1.0, spacing: 1.0, attach: 1, content: 2, block_scale: [1.0, 1.0], ..Default::default() };
    let (mut ctx, mut in_leader, mut in_line) = (false, false, false);
    let mut started = false;
    let mut pending_attdef: Option<Handle> = None;
    let col = |v: &[u8]| EntColor::from_raw(i(v));
    for &(c, v) in body {
        if !started { started = c == 100 && trim(v) == b"AcDbMLeader"; continue; }
        match c {
            300 => { ctx = true; continue; }
            301 => { ctx = false; continue; }
            302 if ctx && trim(v) == b"LEADER{" => { in_leader = true; m.leaders.push(Leader::default()); continue; }
            303 if in_leader => { in_leader = false; continue; }
            304 if in_leader && trim(v) == b"LEADER_LINE{" => { in_line = true; if let Some(l) = m.leaders.last_mut() { l.lines.push(vec![]); } continue; }
            305 if in_line => { in_line = false; continue; }
            _ => {}
        }
        if in_line {
            let line = m.leaders.last_mut().and_then(|l| l.lines.last_mut());
            if let Some(line) = line { match c { 10 => line.push([f(v), 0.0]), 20 => if let Some(p) = line.last_mut() { p[1] = f(v) }, _ => {} } }
        } else if in_leader {
            let Some(l) = m.leaders.last_mut() else { continue };
            match c {
                10 => l.conn = Some([f(v), 0.0]),
                20 => if let Some(p) = l.conn.as_mut() { p[1] = f(v) },
                11 => l.dogleg_dir[0] = f(v),
                21 => l.dogleg_dir[1] = f(v),
                40 => l.dogleg_len = f(v),
                291 => l.has_dogleg = i(v) != 0,
                _ => {}
            }
        } else if ctx {
            match c {
                140 => m.arrow_size = Some(f(v)),
                145 => m.landing_gap = f(v),
                290 => m.has_text = i(v) != 0,
                304 => m.text = caret_decode(&raw(v)).into_owned(),
                340 => m.text_style = hex(v),
                12 => m.text_pos[0] = f(v),
                22 => m.text_pos[1] = f(v),
                13 => m.text_dir = Some([f(v), 0.0]),
                23 => if let Some(d) = m.text_dir.as_mut() { d[1] = f(v) },
                41 => m.text_h = f(v),
                43 => m.text_w = f(v),
                45 => m.spacing = f(v),
                90 => m.text_color = col(v),
                171 => m.attach = i(v),
                91 => m.bg_color = col(v),
                141 => m.bg_scale = f(v),
                291 => m.bg_on = i(v) != 0,
                292 => m.bg_page = i(v) != 0,
                296 => m.has_block = i(v) != 0,
                341 => m.block = hex(v),
                15 => m.block_pos[0] = f(v),
                25 => m.block_pos[1] = f(v),
                16 => m.block_scale[0] = f(v),
                26 => m.block_scale[1] = f(v),
                46 => m.block_rot = f(v),
                93 => m.block_color = col(v),
                _ => {}
            }
        } else {
            match c {
                340 => m.style = hex(v),
                170 => m.leader_type = i(v),
                91 => m.line_color = col(v),
                341 => m.ltype = hex(v),
                171 => m.lw = Some(i(v).clamp(-3, 211) as i16),
                290 => m.landing = i(v) != 0,
                291 => m.dogleg = i(v) != 0,
                41 => m.dogleg_len = f(v),
                342 => m.arrow_block = hex(v),
                42 => m.ent_arrow_size = Some(f(v)),
                172 => m.content = i(v),
                292 => m.frame = i(v) != 0,
                45 => m.scale = f(v),
                330 => pending_attdef = Some(hex(v)),
                302 => if let Some(h) = pending_attdef.take() { m.attribs.push((h, raw(v))) }, // decoded by text::text
                _ => {}
            }
        }
    }
    m
}

pub fn mleader(r: &mut Reader, body: &[Tag], com: &Common) -> Option<Kind> {
    let m = parse(body);
    let mut ents: Vec<Entity> = vec![];
    // the parts take the entity's layer; ByBlock parts inherit from the MULTILEADER
    let base = Props { handle: 0, ..com.props };
    let lines = ml::Lines {
        props: Props {
            color: m.line_color.unwrap_or(EntColor::BYBLOCK),
            ltype: r.b.linetype_by_handle(m.ltype).unwrap_or(base.ltype),
            lw: m.lw.unwrap_or(base.lw),
            ..base
        },
        path_type: m.leader_type,
        arrow: m.arrow_size.filter(|s| *s > 0.0).or(m.ent_arrow_size.map(|s| s * m.scale)).unwrap_or(0.0),
        arrow_block: if m.arrow_block != 0 { r.block_of_record(m.arrow_block) } else { None },
    };
    for l in &m.leaders {
        for verts in &l.lines {
            let mut pts = verts.clone();
            if let Some(c) = l.conn { pts.push(c); }
            lines.line(pts, &mut ents);
        }
        if let (Some(c), true) = (l.conn, m.landing && m.dogleg && l.has_dogleg) { lines.dogleg(c, l.dogleg_dir, l.dogleg_len, &mut ents); }
    }
    if m.content == 2 && m.has_text {
        let rec = TextRec { owner: 0, kind: TextKind::MLeader, layer: com.props.layer, handle: com.props.handle, pos: [m.text_pos[0], m.text_pos[1]], text: meta::plain_mtext(&m.text) };
        r.meta = Some((*com, MetaRec::Text(rec)));
        text_part(r, &m, base, &lines, &mut ents);
    }
    if m.content == 1 && m.has_block { block_part(r, &m, base, &mut ents); }
    ml::finish(&mut r.b, com.props.handle, ents)
}

/// MTEXT content, with its background mask and optional frame.
fn text_part(r: &mut Reader, m: &ML, base: Props, lines: &ml::Lines, ents: &mut Vec<Entity>) {
    let style = r.b.text_style_by_handle(m.text_style).map(|s| s.name.clone()).unwrap_or_else(|| "STANDARD".into());
    let rot = m.text_dir.map_or(0.0, |d| d[1].atan2(d[0]));
    let spec = MTextSpec {
        text: m.text.clone(), style, height: m.text_h, pos: (m.text_pos[0], m.text_pos[1]), rot,
        width: if m.text_w > 1e-9 { m.text_w } else { 0.0 }, attach: m.attach, spacing: m.spacing,
        mask: if m.bg_on { Some(if m.bg_page { None } else { m.bg_color }) } else { None },
        mask_scale: if m.bg_scale > 0.0 { m.bg_scale } else { DEFAULT_FILL_SCALE }, mirror: false,
    };
    let Some(te) = mtext_ent(r, &spec) else { return };
    let props = Props { color: m.text_color.unwrap_or(EntColor::BYBLOCK), ..base };
    ml::text_part(&r.b, te, props, m.frame.then_some((m.landing_gap, Props { color: lines.props.color, ..base })), ents);
}

/// Block content and its attributes.
fn block_part(r: &mut Reader, m: &ML, base: Props, ents: &mut Vec<Entity>) {
    let Some(b) = r.block_of_record(m.block) else { return };
    let color = m.block_color.unwrap_or(EntColor::BYBLOCK);
    let to_world = ml::block_part(&mut r.b, b, m.block_pos, m.block_rot, m.block_scale, Props { color, ..base }, ents);
    // attributes: the ATTDEF of the block, with the value from the MULTILEADER
    for (h, value) in &m.attribs {
        let Some(def) = r.attdefs.get(h).cloned() else { continue };
        let mut tags: Vec<Tag> = def.iter().filter(|t| t.0 != 1).map(|t| (t.0, t.1.as_slice())).collect();
        tags.push((1, value.as_bytes()));
        let ac = super::common(r, &tags);
        let Some(te) = super::text::text(r, b"ATTRIB", &tags, &ac) else { continue };
        ml::attrib(te, &to_world, ac.props.color, color, base, ents);
    }
}

/// Remember an ATTDEF (it is not drawn) for MULTILEADER block attributes.
pub fn keep_attdef(r: &mut Reader, body: &[Tag], com: &Common) {
    if com.props.handle == 0 { return; }
    r.attdefs.insert(com.props.handle, body.iter().map(|&(c, v)| (c, v.to_vec())).collect());
}

