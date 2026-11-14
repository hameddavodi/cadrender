//! DIMENSION. Normally drawn through its ready-made anonymous block (`*D..`),
//! which already holds lines, arrows and the text with its DIMTFILL mask.
//! When the block is missing or empty, simple geometry is made from the
//! DIMSTYLE (plus the per-entity overrides in the ACAD/DSTYLE XDATA):
//! linear, aligned, radius and diameter dimensions.

use super::mleader::closed_arrow;
use super::text::{mtext_ent, MTextSpec};
use super::Common;
use crate::reader::Reader;
use crate::tags::{find, gf, gi, i, raw, s, trim, Tag};
use cadrender_core::color::EntColor;
use cadrender_core::entity::insert::InsertEnt;
use cadrender_core::entity::stroke::StrokeEnt;
use cadrender_core::entity::{Entity, Kind, Props};
use cadrender_core::geom::{Aff, PathData};
use cadrender_core::scene::tables::{BlockId, DimStyle};

type P = [f64; 2];

pub fn dimension(r: &mut Reader, body: &[Tag], com: &Common) -> Option<Kind> {
    let name = find(body, 2).map(s).unwrap_or_default();
    if !name.is_empty() {
        let id = r.b.block(&name);
        if !r.b.block_mut(id).ents.is_empty() { return super::insert::insert(r, b"DIMENSION", body, com, vec![]); }
        if !r.is_defined(id) {
            // The block may come later in the file (ODA writes paper-space tabs
            // before many `*D` blocks): draw through it, and decide at the end.
            let ins = super::insert::insert(r, b"DIMENSION", body, com, vec![]);
            let (com, body) = (*com, body.iter().map(|&(c, v)| (c, v.to_vec())).collect::<Vec<_>>());
            r.at_end(Box::new(move |r| fill_if_empty(r, id, &com, &body)));
            return ins;
        }
    }
    let ents = fallback(r, body, com);
    if ents.is_empty() { return None; }
    let id = r.b.block(&format!("*DIMGEN{:X}_{}", com.props.handle, r.b.stats.counts.get("DIMENSION").copied().unwrap_or(0)));
    for e in ents { r.b.push(id, e); }
    Some(InsertEnt::new(id, Aff::ID).into())
}

/// Simple geometry from the DIMSTYLE, for a DIMENSION without a usable block.
fn fallback(r: &mut Reader, body: &[Tag], com: &Common) -> Vec<Entity> {
    let st = Style::of(r, body);
    let mut ents = vec![];
    generate(r, body, com, &st, &mut ents);
    ents
}

/// End of the file: a DIMENSION whose block is still empty gets the simple
/// geometry, written into that block (its INSERT already points there, with
/// the identity matrix, so the geometry stays in place). When two DIMENSIONs
/// share one missing block, the first one fills it.
fn fill_if_empty(r: &mut Reader, block: BlockId, com: &Common, body: &[(i32, Vec<u8>)]) {
    if !r.b.block_mut(block).ents.is_empty() { return; }
    for e in fallback(r, &own_tags(body), com) { r.b.push(block, e); }
}

fn own_tags(body: &[(i32, Vec<u8>)]) -> Vec<Tag<'_>> { body.iter().map(|(c, v)| (*c, v.as_slice())).collect() }

/// DIMSTYLE values with the entity overrides applied.
struct Style { ds: Option<DimStyle>, ovr: Vec<(i32, String)> }

impl Style {
    fn of(r: &Reader, body: &[Tag]) -> Style {
        let sname = find(body, 3).map(s).unwrap_or_else(|| "STANDARD".into());
        let ds = r.b.dim_styles.iter().find(|d| d.name.eq_ignore_ascii_case(&sname)).cloned();
        // XDATA: 1001 ACAD, 1000 DSTYLE, 1002 {, then (1070 code, value) pairs, 1002 }
        let mut ovr = vec![];
        let mut on = false;
        let mut code: Option<i32> = None;
        for &(c, v) in body {
            match c {
                1000 if trim(v) == b"DSTYLE" => on = true,
                1002 if on && trim(v) == b"}" => on = false,
                1070 if on && code.is_none() => code = Some(i(v)),
                1000..=1071 if on => if let Some(k) = code.take() { ovr.push((k, raw(v))) },
                _ => {}
            }
        }
        Style { ds, ovr }
    }
    fn num(&self, code: i32, default: f64) -> f64 {
        self.ovr.iter().find(|o| o.0 == code).and_then(|o| o.1.trim().parse().ok())
            .or_else(|| self.ds.as_ref().and_then(|d| d.num(code))).unwrap_or(default)
    }
    fn scale(&self) -> f64 { let s = self.num(40, 1.0); if s > 0.0 { s } else { 1.0 } }

    /// The measurement as text: DIMLFAC, DIMDEC, DIMDSEP, DIMZIN (trailing zeros).
    fn format(&self, value: f64) -> String {
        let dec = self.num(271, 2.0).clamp(0.0, 8.0) as usize;
        let mut t = format!("{:.*}", dec, value * self.num(144, 1.0));
        if (self.num(78, 0.0) as i32) & 8 != 0 && t.contains('.') { t = t.trim_end_matches('0').trim_end_matches('.').to_string(); }
        let sep = self.num(278, 46.0) as u32;
        if sep != 46 { if let Some(c) = char::from_u32(sep) { t = t.replace('.', &c.to_string()); } }
        t
    }
}

fn pt(body: &[Tag], x: i32) -> Option<P> { Some([gf(body, x)?, gf(body, x + 10).unwrap_or(0.0)]) }
fn sub(a: P, b: P) -> P { [a[0] - b[0], a[1] - b[1]] }
fn add(a: P, b: P, k: f64) -> P { [a[0] + b[0] * k, a[1] + b[1] * k] }
fn unit(a: P) -> P { let n = a[0].hypot(a[1]); if n > 1e-12 { [a[0] / n, a[1] / n] } else { [1.0, 0.0] } }
fn dot(a: P, b: P) -> f64 { a[0] * b[0] + a[1] * b[1] }

fn line(a: P, b: P, props: Props, out: &mut Vec<Entity>) {
    let mut p = PathData::default();
    p.move_to(a[0], a[1]);
    p.line_to(b[0], b[1]);
    out.push(Entity::new(props, StrokeEnt { path: p, width: 0.0 }));
}

/// Arrowhead at `tip`, its body toward `from`: closed filled, or a tick when DIMTSZ > 0.
fn arrow(st: &Style, tip: P, from: P, props: Props, out: &mut Vec<Entity>) {
    let sc = st.scale();
    let d = unit(sub(tip, from));
    let tsz = st.num(142, 0.0) * sc;
    if tsz > 0.0 {
        let k = [(d[0] - d[1]) * tsz / 2.0, (d[1] + d[0]) * tsz / 2.0];
        return line(sub(tip, k), add(tip, k, 1.0), props, out);
    }
    let size = st.num(41, 2.5) * sc;
    if size > 0.0 { out.push(Entity::new(props, closed_arrow(tip, d[1].atan2(d[0]), size))); }
}

fn generate(r: &mut Reader, body: &[Tag], com: &Common, st: &Style, out: &mut Vec<Entity>) {
    let props = Props { handle: 0, color: EntColor::BYBLOCK, ..com.props };
    let sc = st.scale();
    let kind = gi(body, 70).unwrap_or(0) & 7;
    let (Some(p10), mid) = (pt(body, 10), pt(body, 11)) else { return };
    let (value, pos, dir, prefix) = match kind {
        0 | 1 => {
            let (Some(p1), Some(p2)) = (pt(body, 13), pt(body, 14)) else { return };
            let d = if kind == 1 { unit(sub(p2, p1)) } else { let a = gf(body, 50).unwrap_or(0.0).to_radians(); [a.cos(), a.sin()] };
            // feet of the extension lines on the dimension line (through p10)
            let foot = |p: P| add(p10, d, dot(sub(p, p10), d));
            let (a1, a2) = (foot(p1), foot(p2));
            let (exo, exe) = (st.num(42, 0.625) * sc, st.num(44, 1.25) * sc);
            for (p, a) in [(p1, a1), (p2, a2)] {
                let n = sub(a, p);
                if n[0].hypot(n[1]) > 1e-9 { let u = unit(n); line(add(p, u, exo), add(a, u, exe), props, out); }
            }
            line(a1, a2, props, out);
            arrow(st, a1, a2, props, out);
            arrow(st, a2, a1, props, out);
            (dot(sub(a2, a1), d).abs(), mid.unwrap_or(add(a1, sub(a2, a1), 0.5)), d, "")
        }
        3 | 4 => {
            let Some(p15) = pt(body, 15) else { return };
            let v = sub(p15, p10);
            let len = v[0].hypot(v[1]);
            let pos = mid.unwrap_or(add(p10, v, 0.5));
            if kind == 3 { line(p10, p15, props, out); arrow(st, p10, p15, props, out); arrow(st, p15, p10, props, out); }
            else { line(pos, p15, props, out); arrow(st, p15, p10, props, out); }
            (len, pos, unit(v), if kind == 3 { "\u{2300}" } else { "R" })
        }
        _ => return,
    };
    let mut text = find(body, 1).map(raw).unwrap_or_default();
    let meas = format!("{prefix}{}", st.format(value));
    if text.trim().is_empty() && text != " " { text = "<>".into(); }
    let text = text.replace("<>", &meas);
    // readable: the text never runs upside down
    let mut rot = dir[1].atan2(dir[0]);
    if rot > std::f64::consts::FRAC_PI_2 + 1e-9 { rot -= std::f64::consts::PI; } else if rot <= -std::f64::consts::FRAC_PI_2 + 1e-9 { rot += std::f64::consts::PI; }
    let style = st.ds.as_ref().and_then(|d| d.handle_of(340)).and_then(|h| r.b.text_style_by_handle(h)).map(|s| s.name.clone()).unwrap_or_else(|| "STANDARD".into());
    let tfill = st.num(69, 0.0) as i32;
    let spec = MTextSpec {
        text, style, height: st.num(140, 2.5) * sc, pos: (pos[0], pos[1]), rot, width: 0.0, attach: 5, spacing: 1.0,
        mask: match tfill { 1 => Some(None), 2 => Some(Some(EntColor::aci(st.num(70, 0.0) as i32))), _ => None },
        mask_scale: 1.0 + st.num(147, 0.625) / st.num(140, 2.5).max(1e-9), mirror: false,
    };
    if let Some(te) = mtext_ent(r, &spec) { out.push(Entity::new(props, te)); }

}
