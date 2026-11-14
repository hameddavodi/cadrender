//! TEXT, MTEXT and ATTRIB, laid out once when the scene is built.

use super::{Entity, EntityKind, PrepareCx};
use crate::color::EntColor;
use crate::geom::{Aff, BBox, PathData};
use crate::render::batch::StrokeKey;
use crate::render::walk::{Visit, Walker};
use crate::sink::Sink;
use crate::text::layout::Glyph;
use crate::text::mtext::Rich;
use crate::text::{Face, FaceId};

#[derive(Clone, Debug)]
pub struct TextEnt {
    pub face: FaceId,
    pub glyphs: Vec<Glyph>,
    /// Font units -> block coordinates (position, rotation, height, width factor, oblique).
    pub m: Aff,
    /// Cap height in drawing units.
    pub cap_h: f64,
    /// Width of the widest line in font units.
    pub width: f64,
    /// Box of the laid-out text in font units (also the mask box without margin).
    pub frame: BBox,
    /// Background mask (MTEXT background fill), in font units.
    pub mask: Option<Box<TextMask>>,
    /// MTEXT inline formatting (sizes, colours, lines). `None` = plain text (fast path).
    pub rich: Option<Box<Rich>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextMask {
    /// Mask rectangle in font units (already with the border margin).
    pub rect: BBox,
    /// `None` = page background colour, else this colour.
    pub color: Option<EntColor>,
}

impl EntityKind for TextEnt {
    fn prepare(&mut self, _: &PrepareCx) -> BBox {
        let mut b = self.frame;
        if let Some(mk) = &self.mask { b.union(&mk.rect); }
        b.transformed(&self.m)
    }

    fn draw<S: Sink>(&self, e: &Entity, v: &Visit, w: &mut Walker<'_, S>) {
        let Some(face) = w.scene.fonts.faces.get(self.face as usize) else { return };
        let (st, _) = w.style(e, v.layer, &v.inh, 0.0);
        let mm = v.m.mul(&self.m);
        let cap_px = self.cap_h * v.scale;
        let lod = w.opts.lod;
        if lod.enabled && cap_px < lod.text_hide_px { w.stats.tiny += 1; return; }
        if let (Some(mk), true) = (&self.mask, w.opts.text_masks) {
            let color = match mk.color { None => w.opts.background, Some(c) => w.color(c, v.layer, &v.inh) };
            let r = mk.rect;
            let mut p = PathData::default();
            p.move_to(r.x0, r.y0); p.line_to(r.x1, r.y0); p.line_to(r.x1, r.y1); p.line_to(r.x0, r.y1); p.close();
            w.out.fill_now(&p, &mm, color);
        }
        let ch = face.cap_height;
        if lod.enabled && cap_px < lod.text_greek_px {
            // "greeking": unreadable text becomes a grey bar
            let wd = self.glyphs.last().map(|g| g.x as f64).unwrap_or(0.0) + ch;
            let (x0, y0) = mm.apply(0.0, ch * 0.5);
            let (x1, y1) = mm.apply(wd, ch * 0.5);
            w.out.set_stroke(StrokeKey { color: st.color.with_alpha(110), dash: crate::render::batch::NO_DASH, ..st }.with_width_px((cap_px * 0.8).max(1.0)));
            w.out.seg(x0 as f32, y0 as f32, x1 as f32, y1 as f32);
            w.stats.tiny += 1;
            return;
        }
        if let Some(rich) = &self.rich { return self.draw_rich(rich, face, st, &mm, v, w); }
        if S::TEXT_RUNS {
            w.out.flush(); // keep the draw order
            if w.out.sink.text(&crate::sink::TextRun { face, glyphs: &self.glyphs, m: mm, color: st.color }) { w.stats.glyphs += self.glyphs.len() as u64; w.stats.drawn += 1; return; }
        }
        if face.filled { w.out.set_fill(st.color); } else { w.out.set_stroke(st.solid()); }
        for g in &self.glyphs {
            if let Some(gd) = face.glyphs.get(g.id as usize) {
                if gd.path.is_empty() { continue; }
                let gm = mm.mul(&Aff::translate(g.x as f64, g.y as f64));
                w.out.push(&gd.path, &gm);
            }
        }
        w.stats.glyphs += self.glyphs.len() as u64;
        w.stats.drawn += 1;
    }
}

impl TextEnt {
    /// Formatted MTEXT: one transform and colour per run, then the lines.
    fn draw_rich<S: Sink>(&self, rich: &Rich, face: &Face, st: StrokeKey, mm: &Aff, v: &Visit, w: &mut Walker<'_, S>) {
        let color_of = |c: Option<EntColor>, w: &Walker<'_, S>| c.map_or(st.color, |c| w.color(c, v.layer, &v.inh));
        if S::TEXT_RUNS {
            // sinks that write real text (SVG) get the runs, with resolved colours
            let run_colors: Vec<_> = rich.runs.iter().map(|r| color_of(r.color, w)).collect();
            let deco_colors: Vec<_> = rich.deco.iter().map(|d| color_of(d.color, w)).collect();
            w.out.flush(); // keep the draw order
            let run = crate::sink::TextRun { face, glyphs: &self.glyphs, m: *mm, color: st.color };
            if w.out.sink.rich_text(&run, &crate::sink::RichText { rich, run_colors: &run_colors, deco_colors: &deco_colors }) {
                w.stats.glyphs += self.glyphs.len() as u64;
                w.stats.drawn += 1;
                return;
            }
        }
        let mut start = 0usize;
        for r in &rich.runs {
            let end = (r.end as usize).min(self.glyphs.len());
            let col = color_of(r.color, w);
            let rm = Aff([r.sx as f64, 0.0, r.k as f64, r.sy as f64, 0.0, 0.0]);
            // fake bold: the outline stroked again, about 1/25 of the height wide
            let passes: &[bool] = if r.bold { &[false, true] } else { &[false] };
            for &outline in passes {
                if outline {
                    let px = (self.cap_h * v.scale * r.sy as f64 * 0.04).max(0.5);
                    w.out.set_stroke(StrokeKey { color: col, ..st.solid() }.with_width_px(px));
                } else if face.filled { w.out.set_fill(col); } else { w.out.set_stroke(StrokeKey { color: col, ..st.solid() }); }
                for g in &self.glyphs[start..end] {
                    if let Some(gd) = face.glyphs.get(g.id as usize) {
                        if gd.path.is_empty() { continue; }
                        w.out.push(&gd.path, &mm.mul(&Aff::translate(g.x as f64, g.y as f64)).mul(&rm));
                    }
                }
            }
            start = end;
        }
        for d in &rich.deco {
            let mut p = PathData::default();
            let (x0, y0, x1, y1) = (d.x0 as f64, d.y0 as f64, d.x1 as f64, d.y1 as f64);
            p.move_to(x0, y0); p.line_to(x1, y0); p.line_to(x1, y1); p.line_to(x0, y1); p.close();
            w.out.set_fill(color_of(d.color, w));
            w.out.push(&p, mm);
        }
        w.stats.glyphs += self.glyphs.len() as u64;
        w.stats.drawn += 1;
    }
}
