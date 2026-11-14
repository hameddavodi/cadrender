//! MULTILEADER parts, shared by the DXF and DWG readers. A MULTILEADER is
//! drawn as a small anonymous block (leader lines, arrowheads, dogleg, MTEXT
//! or block content) through an `InsertEnt`. A reader maps its raw fields
//! into [`Lines`] and the content values and calls the part builders in this
//! order: [`Lines::line`] / [`Lines::dogleg`] per leader, then
//! [`text_part`] or [`block_part`] (+ [`attrib`] per attribute), then [`finish`].

use super::fill::FillEnt;
use super::insert::InsertEnt;
use super::stroke::StrokeEnt;
use super::text::TextEnt;
use super::{Entity, Kind, Props};
use crate::color::EntColor;
use crate::geom::{Aff, PathData};
use crate::scene::builder::SceneBuilder;
use crate::scene::tables::{BlockId, Handle};

type P = [f64; 2];

/// How the leader lines of one MULTILEADER are drawn.
#[derive(Clone, Copy, Debug)]
pub struct Lines {
    /// Props of lines, arrowheads, dogleg and text frame.
    pub props: Props,
    /// Leader type: 0 = invisible, 1 = straight, 2 = spline.
    pub path_type: i32,
    /// Arrowhead size in drawing units (0 = none).
    pub arrow: f64,
    /// Arrowhead block; `None` = AutoCAD's default closed triangle.
    pub arrow_block: Option<BlockId>,
}

impl Lines {
    /// One leader line: points from the arrow tip on, the connection point last.
    pub fn line(&self, mut pts: Vec<P>, ents: &mut Vec<Entity>) {
        pts.dedup_by(|a, b| (a[0] - b[0]).abs() + (a[1] - b[1]).abs() < 1e-9);
        if pts.len() < 2 || !pts.iter().all(|p| p[0].is_finite() && p[1].is_finite()) { return; }
        if self.path_type != 0 {
            let mut path = PathData::default();
            if self.path_type == 2 { spline_through(&pts, &mut path) } else { polyline(&pts, &mut path) }
            ents.push(Entity::new(self.props, StrokeEnt { path, width: 0.0 }));
        }
        if self.arrow > 0.0 {
            let (tip, next) = (pts[0], pts[1]);
            let ang = (tip[1] - next[1]).atan2(tip[0] - next[0]);
            match self.arrow_block {
                Some(b) => {
                    let am = Aff::translate(tip[0], tip[1]).mul(&Aff::rotate(ang)).mul(&Aff::scale(self.arrow, self.arrow));
                    ents.push(Entity::new(self.props, InsertEnt::new(b, am)));
                }
                None => ents.push(Entity::new(self.props, closed_arrow(tip, ang, self.arrow))),
            }
        }
    }

    /// The landing ("dogleg"): from the connection point `len` along `dir`.
    /// The reader decides if the leader has one (landing and dogleg flags).
    pub fn dogleg(&self, conn: P, dir: P, len: f64, ents: &mut Vec<Entity>) {
        let n = dir[0].hypot(dir[1]);
        if self.path_type == 0 || len <= 0.0 || n <= 1e-12 { return; }
        let mut path = PathData::default();
        path.move_to(conn[0], conn[1]);
        path.line_to(conn[0] + dir[0] / n * len, conn[1] + dir[1] / n * len);
        ents.push(Entity::new(self.props, StrokeEnt { path, width: 0.0 }));
    }
}

fn polyline(pts: &[P], path: &mut PathData) {
    path.move_to(pts[0][0], pts[0][1]);
    for p in &pts[1..] { path.line_to(p[0], p[1]); }
}

/// Smooth curve through the points (Catmull-Rom as cubic Béziers).
fn spline_through(pts: &[P], path: &mut PathData) {
    if pts.len() < 3 { return polyline(pts, path); }
    path.move_to(pts[0][0], pts[0][1]);
    let n = pts.len();
    for k in 0..n - 1 {
        let p0 = pts[k.saturating_sub(1)];
        let (p1, p2) = (pts[k], pts[k + 1]);
        let p3 = pts[(k + 2).min(n - 1)];
        let c1 = [p1[0] + (p2[0] - p0[0]) / 6.0, p1[1] + (p2[1] - p0[1]) / 6.0];
        let c2 = [p2[0] - (p3[0] - p1[0]) / 6.0, p2[1] - (p3[1] - p1[1]) / 6.0];
        path.cubic_to(c1[0], c1[1], c2[0], c2[1], p2[0], p2[1]);
    }
}

/// AutoCAD's default arrowhead: a filled triangle, length = size, width = size / 3.
pub fn closed_arrow(tip: P, ang: f64, size: f64) -> FillEnt {
    let m = Aff::translate(tip[0], tip[1]).mul(&Aff::rotate(ang)).mul(&Aff::scale(size, size));
    let mut path = PathData::default();
    path.move_to(0.0, 0.0);
    path.line_to(-1.0, 1.0 / 6.0);
    path.line_to(-1.0, -1.0 / 6.0);
    path.close();
    path.transform(&m);
    FillEnt { path, background: false }
}

/// MTEXT content (already laid out, see `text::build::mtext_ent`) with its
/// optional frame: the text box plus `landing_gap` all round, drawn with `frame`.
pub fn text_part(b: &SceneBuilder, te: TextEnt, props: Props, frame: Option<(f64, Props)>, ents: &mut Vec<Entity>) {
    if let Some((gap, fprops)) = frame {
        let cap = b.fonts.faces[te.face as usize].cap_height;
        let (x0, w, y0, y1) = (te.frame.x0, te.frame.x1, te.frame.y0 + 0.35 * cap, te.frame.y1 - 0.25 * cap);
        let (sx, sy) = (te.m.0[0].hypot(te.m.0[1]), te.m.0[2].hypot(te.m.0[3]));
        let (gx, gy) = (gap / sx.max(1e-12), gap / sy.max(1e-12));
        let mut path = PathData::default();
        path.move_to(x0 - gx, y0 - gy);
        path.line_to(w + gx, y0 - gy);
        path.line_to(w + gx, y1 + gy);
        path.line_to(x0 - gx, y1 + gy);
        path.close();
        path.transform(&te.m);
        ents.push(Entity::new(fprops, StrokeEnt { path, width: 0.0 }));
    }
    ents.push(Entity::new(props, te));
}

/// Block content: an insert of `block` (scale 0 counts as 1). Returns the
/// matrix that takes the block's ATTDEF positions to the MULTILEADER block,
/// for [`attrib`].
pub fn block_part(b: &mut SceneBuilder, block: BlockId, pos: P, rot: f64, scale: P, props: Props, ents: &mut Vec<Entity>) -> Aff {
    let (sx, sy) = (if scale[0] == 0.0 { 1.0 } else { scale[0] }, if scale[1] == 0.0 { 1.0 } else { scale[1] });
    let im = Aff::translate(pos[0], pos[1]).mul(&Aff::rotate(rot)).mul(&Aff::scale(sx, sy));
    ents.push(Entity::new(props, InsertEnt::new(block, im)));
    let bb = b.block_mut(block).base;
    im.mul(&Aff::translate(-bb[0], -bb[1]))
}

/// One attribute of block content: the ATTDEF laid out with the value from the
/// MULTILEADER (`te`), moved by `to_world`. A ByBlock ATTDEF colour takes the
/// block content colour.
pub fn attrib(mut te: TextEnt, to_world: &Aff, attdef_color: EntColor, block_color: EntColor, base: Props, ents: &mut Vec<Entity>) {
    te.m = to_world.mul(&te.m);
    let color = if attdef_color == EntColor::BYBLOCK { block_color } else { attdef_color };
    ents.push(Entity::new(Props { color, ..base }, te));
}

/// The parts -> an anonymous block and the insert that draws it; `None` if empty.
pub fn finish(b: &mut SceneBuilder, handle: Handle, ents: Vec<Entity>) -> Option<Kind> {
    if ents.is_empty() { return None; }
    let name = format!("*MLEADER{:X}_{}", handle, b.stats.counts.get("MULTILEADER").copied().unwrap_or(0));
    let id = b.block(&name);
    for e in ents { b.push(id, e); }
    Some(InsertEnt::new(id, Aff::ID).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::Fonts;

    fn lines(path_type: i32, arrow: f64) -> Lines { Lines { props: Props::default(), path_type, arrow, arrow_block: None } }

    #[test]
    fn line_dedups_and_skips_broken_points() {
        let mut ents = vec![];
        // the same point twice is one point: too short, nothing drawn
        lines(1, 1.0).line(vec![[1.0, 1.0], [1.0, 1.0]], &mut ents);
        lines(1, 1.0).line(vec![[0.0, 0.0], [f64::NAN, 1.0]], &mut ents);
        assert!(ents.is_empty());
        // a line and its arrowhead; invisible leader type 0 keeps the arrow only
        lines(1, 1.0).line(vec![[0.0, 0.0], [5.0, 0.0]], &mut ents);
        assert_eq!(ents.len(), 2);
        lines(0, 1.0).line(vec![[0.0, 0.0], [5.0, 0.0]], &mut ents);
        lines(2, 0.0).line(vec![[0.0, 0.0], [5.0, 0.0], [5.0, 5.0]], &mut ents);
        assert_eq!(ents.len(), 4);
    }

    #[test]
    fn dogleg_needs_length_direction_and_visible_leader() {
        let mut ents = vec![];
        lines(1, 0.0).dogleg([0.0, 0.0], [1.0, 0.0], 0.0, &mut ents);
        lines(1, 0.0).dogleg([0.0, 0.0], [0.0, 0.0], 2.0, &mut ents);
        lines(0, 0.0).dogleg([0.0, 0.0], [1.0, 0.0], 2.0, &mut ents);
        assert!(ents.is_empty());
        lines(1, 0.0).dogleg([0.0, 0.0], [3.0, 0.0], 2.0, &mut ents);
        assert_eq!(ents.len(), 1);
    }

    #[test]
    fn block_part_and_attrib_colours() {
        let mut b = SceneBuilder::new(Fonts::new());
        let blk = b.block("ARROWBLK");
        b.block_mut(blk).base = [1.0, 0.0];
        let mut ents = vec![];
        let to_world = block_part(&mut b, blk, [10.0, 0.0], 0.0, [0.0, 2.0], Props::default(), &mut ents);
        // scale 0 counts as 1; the block base point goes to the content position
        assert_eq!(to_world.apply(1.0, 0.0), (10.0, 0.0));
        assert_eq!(to_world.apply(2.0, 1.0), (11.0, 2.0));
        let te = || TextEnt { face: 0, glyphs: vec![], m: Aff::ID, cap_h: 1.0, width: 0.0, frame: crate::geom::BBox::new(0.0, 0.0, 0.0, 0.0), mask: None, rich: None };
        let red = EntColor::aci(1);
        attrib(te(), &to_world, EntColor::BYBLOCK, red, Props::default(), &mut ents);
        attrib(te(), &to_world, EntColor::aci(3), red, Props::default(), &mut ents);
        assert_eq!((ents[1].props.color, ents[2].props.color), (red, EntColor::aci(3)));
        assert!(finish(&mut b, 0x2A, ents).is_some());
        assert!(finish(&mut b, 0x2B, vec![]).is_none());
    }
}
