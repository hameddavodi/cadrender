//! Non-graphic objects: LAYOUT (tabs), SORTENTSTABLE (draw order),
//! per-viewport layer overrides (XRECORDs in the layer's extension
//! dictionary), DIMSTYLE and MLEADERSTYLE. Same rules as
//! `cadrender-dxf/src/objects.rs`.

use crate::tables::Ctx;
use cadrender_core::color::EntColor;
use cadrender_core::entity::frame::FrameEnt;
use cadrender_core::entity::{Entity, Kind};
use cadrender_core::geom::BBox;
use cadrender_core::scene::tables::{DimStyle, Handle, LayerVpOverride, MLeaderStyle};
use cadrender_core::scene::PaperSetup;
use cadrender_dwg::{Layout, ObjectKind};
use std::collections::HashMap;

/// Paper setup of a LAYOUT, like the DXF reader reads groups 40-45, 72, 70, 10-21.
pub fn paper_setup(l: &Layout) -> PaperSetup {
    let mut p = PaperSetup { margins_mm: l.margins, ..PaperSetup::default() };
    p.units_per_mm = if l.plot_paper_units == 0 { 1.0 / 25.4 } else { 1.0 };
    p.psltscale = Some(l.flags & 1 != 0);
    if l.paper_width > 0.0 && l.paper_height > 0.0 { p.size_mm = Some((l.paper_width, l.paper_height)); }
    let lim = BBox::new(l.limmin[0], l.limmin[1], l.limmax[0], l.limmax[1]);
    if !lim.is_empty() && lim.w() > 0.0 && lim.h() > 0.0 && lim.w() < 1e7 && lim.h() < 1e7 { p.limits = Some(lim); }
    p
}

impl Ctx<'_> {
    pub fn layouts(&mut self) {
        let d = self.d;
        let mut seen = std::collections::HashSet::new();
        for o in &d.objects {
            let ObjectKind::Layout(l) = &o.kind else { continue };
            if l.name.eq_ignore_ascii_case("model") { continue; }
            match self.block_of.get(&l.block_record).copied() {
                Some(block) if seen.insert(block) => self.b.add_layout(&l.name, l.tab_order, block, o.handle, paper_setup(l)),
                Some(_) => self.b.warn(format!("layout {:?} shares its block with another layout, skipped", l.name)),
                None => self.b.warn(format!("layout {:?} has no block record, skipped", l.name)),
            }
        }
    }

    pub fn sort_tables(&mut self) {
        let d = self.d;
        for o in &d.objects {
            let ObjectKind::SortEnts(t) = &o.kind else { continue };
            // the owner dictionary's owner is the block record, if the table does not say
            let rec = if t.block_owner != 0 { t.block_owner } else { d.get(o.owner).map(|x| x.owner).unwrap_or(0) };
            let pairs: Vec<(Handle, Handle)> = t.entries.iter().map(|&(sort, ent)| (ent, sort)).collect();
            match self.block_of.get(&rec).copied() {
                Some(b) => self.b.add_sort_table(b, pairs),
                None => self.b.warn("draw order table without a block, ignored"),
            }
        }
    }

    /// Per-viewport layer overrides: XRECORDs named ADSK_XREC_LAYER_*_OVR in
    /// the layer's extension dictionary. Each holds (335 viewport, value) pairs.
    pub fn layer_overrides(&mut self) {
        let d = self.d;
        for (layer, xdict) in std::mem::take(&mut self.layer_xdict) {
            let Some(ObjectKind::Dictionary(dict)) = d.get(xdict).map(|o| &o.kind) else { continue };
            let mut ovr: Vec<LayerVpOverride> = vec![];
            for (key, h) in &dict.entries {
                let code = match key.as_str() {
                    "ADSK_XREC_LAYER_COLOR_OVR" => 420, "ADSK_XREC_LAYER_LINETYPE_OVR" => 343,
                    "ADSK_XREC_LAYER_LINEWT_OVR" => 91, "ADSK_XREC_LAYER_ALPHA_OVR" => 440, _ => continue,
                };
                let Some(ObjectKind::XRecord(x)) = d.get(*h).map(|o| &o.kind) else { continue };
                let vps = x.items.iter().filter(|v| v.0 == 335).filter_map(|v| v.1.handle());
                let values = x.items.iter().filter(|v| v.0 == code).filter_map(|v| v.1.int().or_else(|| v.1.handle().map(|h| h as i64)));
                for (vp, val) in vps.zip(values) {
                    let idx = match ovr.iter().position(|o| o.viewport == vp) { Some(i) => i, None => { ovr.push(LayerVpOverride { viewport: vp, ..Default::default() }); ovr.len() - 1 } };
                    let o = &mut ovr[idx];
                    match code {
                        420 => o.color = EntColor::from_raw(val as i32),
                        343 => o.ltype = self.b.linetype_by_handle(val as u64),
                        91 => o.lw = Some(val.clamp(-3, 211) as i16),
                        _ => if val & 0x0200_0000 != 0 { o.alpha = Some((val & 0xff) as u8) },
                    }
                }
            }
            self.b.layer_mut(layer).vp_overrides = ovr;
        }
    }

    /// DIMSTYLE: the few named values plus raw (DXF code, value) pairs, like
    /// the DXF reader keeps them.
    pub fn dim_styles(&mut self) {
        let d = self.d;
        for o in &d.objects {
            let ObjectKind::DimStyle(s) = &o.kind else { continue };
            let g = |c: i32| s.raw.iter().find(|r| r.0 == c).and_then(|r| r.1.parse::<f64>().ok());
            let mut ds = DimStyle { name: s.name.clone(), handle: o.handle, scale: 1.0, arrow_size: 2.5, text_height: 2.5, raw: s.raw.clone() };
            if let Some(v) = g(40) { ds.scale = v; }
            if let Some(v) = g(41) { ds.arrow_size = v; }
            if let Some(v) = g(140) { ds.text_height = v; }
            self.b.dim_styles.push(ds);
        }
    }

    /// MLEADERSTYLE: name from its dictionary entry, raw (DXF code, value) pairs.
    pub fn mleader_styles(&mut self) {
        let d = self.d;
        let mut names: HashMap<Handle, String> = HashMap::new();
        for o in &d.objects {
            if let ObjectKind::Dictionary(x) = &o.kind { for (n, h) in &x.entries { names.insert(*h, n.clone()); } }
        }
        for o in &d.objects {
            let ObjectKind::MLeaderStyle(s) = &o.kind else { continue };
            let mut st = MLeaderStyle { name: names.get(&o.handle).cloned().unwrap_or_default(), handle: o.handle, raw: s.raw_groups(), ..Default::default() };
            st.parse();
            self.b.mleader_styles.push(st);
        }
    }

    /// WIPEOUT frames: with WIPEOUTFRAME = 1 (WIPEOUTVARIABLES 70; also the
    /// AutoCAD default) the outline of each wipeout is plotted too. Same as
    /// `cadrender-dxf/src/entities/masks.rs`: a stroke right after each
    /// wipeout, with the same props and handle.
    pub fn wipeout_frames(&mut self) {
        let frame = self.d.objects.iter().find_map(|o| match o.kind { ObjectKind::WipeoutVariables { frame } => Some(frame), _ => None });
        if frame.unwrap_or(1) != 1 { return; }
        let is_wipeout = |e: &Entity| matches!(&e.kind, Kind::Fill(f) if f.background);
        for b in self.b.blocks_mut() {
            if !b.ents.iter().any(is_wipeout) { continue; }
            let mut out = Vec::with_capacity(b.ents.len() + 8);
            for e in b.ents.drain(..) {
                let fr = match &e.kind {
                    Kind::Fill(f) if f.background => Some(Entity::new(e.props, FrameEnt { path: f.path.clone() })),
                    _ => None,
                };
                out.push(e);
                if let Some(fr) = fr { out.push(fr); }
            }
            b.ents = out;
        }
    }
}
