//! OBJECTS section: LAYOUT (tabs), SORTENTSTABLE (draw order), DICTIONARY and
//! XRECORD (per-viewport layer overrides), MLEADERSTYLE.

use crate::reader::Reader;
use crate::tags::{f, hex, i, raw, s, trim, Tag};
use cadrender_core::geom::BBox;
use cadrender_core::scene::PaperSetup;
use cadrender_core::color::EntColor;
use cadrender_core::scene::tables::{Handle, LayerVpOverride, MLeaderStyle};
use std::collections::HashMap;

/// Owner handle and (name, handle) entries.
type Dict = (Handle, Vec<(String, Handle)>);
/// Block record handle (0 if missing), owner dictionary handle, (entity, sort handle) pairs.
type SortTable = (Handle, Handle, Vec<(Handle, Handle)>);

#[derive(Default)]
pub struct Objects {
    /// LAYOUT: (name, tab order, block record handle, layout handle, paper setup).
    layouts: Vec<(String, i32, Handle, Handle, PaperSetup)>,
    /// SORTENTSTABLE: (block record handle or 0, owner dictionary handle, pairs).
    sort_tables: Vec<SortTable>,
    /// DICTIONARY handle -> (owner handle, entries name -> handle).
    dicts: HashMap<Handle, Dict>,
    /// XRECORD handle -> payload groups we care about.
    xrecords: HashMap<Handle, Vec<(i32, i64)>>,
    /// MLEADERSTYLE handle -> raw groups.
    mleader_styles: Vec<(Handle, Vec<(i32, String)>)>,
    /// WIPEOUTVARIABLES group 70 (WIPEOUTFRAME): 0 off, 1 shown and plotted, 2 shown only.
    wipeout_frame: Option<i32>,
    /// REGION/3DSOLID/BODY handles whose ACIS data is in ACDSDATA, and that data (SAB).
    pub(crate) acis_pending: std::collections::HashSet<Handle>,
    pub(crate) acis_blobs: HashMap<Handle, Vec<u8>>,
    /// IMAGE handle -> IMAGEDEF handle, and IMAGEDEF handle -> file name.
    pub(crate) image_defs: Vec<(Handle, Handle)>,
    pub(crate) image_files: HashMap<Handle, String>,
}

/// Owner handle: the first 330 outside a 102 group.
fn owner(body: &[Tag]) -> Handle {
    let mut in_group = false;
    for &(c, v) in body {
        if c == 102 { in_group = trim(v).first() == Some(&b'{'); continue; }
        if !in_group && c == 330 { return hex(v); }
    }
    0
}

impl Reader {
    pub(crate) fn object(&mut self, kind: &[u8], body: &[Tag]) {
        match kind {
            b"LAYOUT" => {
                let (mut name, mut order, mut rec, mut sub) = (String::new(), 0, 0u64, 0u8);
                let handle = body.iter().find(|t| t.0 == 5).map(|t| hex(t.1)).unwrap_or(0);
                let mut p = PaperSetup::default();
                let (mut lim, mut size) = ([f64::NAN; 4], [0.0f64; 2]);
                for &(c, v) in body {
                    if c == 100 { sub = match trim(v) { b"AcDbPlotSettings" => 1, b"AcDbLayout" => 2, _ => 0 }; continue; }
                    match (sub, c) {
                        (1, 40..=43) => p.margins_mm[(c - 40) as usize] = f(v),
                        (1, 44) => size[0] = f(v),
                        (1, 45) => size[1] = f(v),
                        (1, 72) => p.units_per_mm = if i(v) == 0 { 1.0 / 25.4 } else { 1.0 },
                        (2, 1) => name = s(v),
                        (2, 70) => p.psltscale = Some(i(v) & 1 != 0),
                        (2, 71) => order = i(v),
                        (2, 10) => lim[0] = f(v), (2, 20) => lim[1] = f(v), (2, 11) => lim[2] = f(v), (2, 21) => lim[3] = f(v),
                        (2, 330) => rec = hex(v),
                        _ => {}
                    }
                }
                if size[0] > 0.0 && size[1] > 0.0 { p.size_mm = Some((size[0], size[1])); }
                let l = BBox::new(lim[0], lim[1], lim[2], lim[3]);
                if !l.is_empty() && l.w() > 0.0 && l.h() > 0.0 && l.w() < 1e7 && l.h() < 1e7 { p.limits = Some(l); }
                self.objs.layouts.push((name, order, rec, handle, p));
            }
            b"SORTENTSTABLE" => {
                let mut in_group = false;
                let (mut dict_owner, mut rec, mut after_marker) = (0, 0, false);
                let mut pairs = vec![];
                let mut pending: Option<Handle> = None;
                for &(c, v) in body {
                    match c {
                        102 => in_group = trim(v).first() == Some(&b'{'),
                        330 if in_group && dict_owner == 0 => dict_owner = hex(v),
                        100 => after_marker = trim(v) == b"AcDbSortentsTable",
                        330 if after_marker && !in_group && rec == 0 => rec = hex(v),
                        331 if after_marker => pending = Some(hex(v)),
                        5 if after_marker => if let Some(e) = pending.take() { pairs.push((e, hex(v))); },
                        _ => {}
                    }
                }
                self.objs.sort_tables.push((rec, dict_owner, pairs));
            }
            b"DICTIONARY" => {
                let handle = body.iter().find(|t| t.0 == 5).map(|t| hex(t.1)).unwrap_or(0);
                let mut entries = vec![];
                let mut name: Option<String> = None;
                for &(c, v) in body {
                    match c { 3 => name = Some(s(v)), 350 | 360 => if let Some(n) = name.take() { entries.push((n, hex(v))); }, _ => {} }
                }
                self.objs.dicts.insert(handle, (owner(body), entries));
            }
            b"XRECORD" => {
                let handle = body.iter().find(|t| t.0 == 5).map(|t| hex(t.1)).unwrap_or(0);
                let vals: Vec<(i32, i64)> = body.iter().filter_map(|&(c, v)| match c {
                    335 | 343 => Some((c, hex(v) as i64)),
                    420 | 91 | 440 => Some((c, crate::tags::long(v))),
                    _ => None,
                }).collect();
                if vals.iter().any(|v| v.0 == 335) { self.objs.xrecords.insert(handle, vals); }
            }
            b"IMAGEDEF" => self.imagedef(body),
            b"WIPEOUTVARIABLES" => self.objs.wipeout_frame = body.iter().find(|t| t.0 == 70).map(|t| i(t.1)),
            b"MLEADERSTYLE" => {
                let handle = body.iter().find(|t| t.0 == 5).map(|t| hex(t.1)).unwrap_or(0);
                self.objs.mleader_styles.push((handle, body.iter().map(|&(c, v)| (c, raw(v))).collect()));
            }
            _ => {}
        }
    }

    /// Resolve handles between objects, once everything is read.
    pub(crate) fn finish_objects(&mut self) {
        crate::entities::acis_fill_pending(self);
        crate::entities::image_fill_files(self);
        let objs = std::mem::take(&mut self.objs);
        // tabs
        let mut seen_blocks = std::collections::HashSet::new();
        for (name, order, rec, handle, paper) in &objs.layouts {
            if name.eq_ignore_ascii_case("model") { continue; }
            match self.block_of_record(*rec) {
                Some(block) if seen_blocks.insert(block) => self.b.add_layout(name, *order, block, *handle, paper.clone()),
                Some(_) => self.b.warn(format!("layout {name:?} shares its block with another layout, skipped")),
                None => self.b.warn(format!("layout {name:?} has no block record, skipped")),
            }
        }
        // draw order. LibreDWG does not write the block record handle; then the
        // owner dictionary's owner is the block record.
        for (rec, dict, pairs) in objs.sort_tables {
            let block = self.block_of_record(rec).or_else(|| objs.dicts.get(&dict).and_then(|d| self.block_of_record(d.0)));
            match block {
                Some(b) => self.b.add_sort_table(b, pairs),
                None => self.b.warn("draw order table without a block, ignored"),
            }
        }
        // per-viewport layer overrides
        for (layer, xdict) in std::mem::take(&mut self.layer_xdict) {
            let Some((_, entries)) = objs.dicts.get(&xdict) else { continue };
            let mut ovr: Vec<LayerVpOverride> = vec![];
            for (key, h) in entries {
                let code = match key.as_str() {
                    "ADSK_XREC_LAYER_COLOR_OVR" => 420, "ADSK_XREC_LAYER_LINETYPE_OVR" => 343,
                    "ADSK_XREC_LAYER_LINEWT_OVR" => 91, "ADSK_XREC_LAYER_ALPHA_OVR" => 440, _ => continue,
                };
                let Some(vals) = objs.xrecords.get(h) else { continue };
                let vps = vals.iter().filter(|v| v.0 == 335).map(|v| v.1 as u64);
                let values = vals.iter().filter(|v| v.0 == code).map(|v| v.1);
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
        // WIPEOUT frames are plotted only when WIPEOUTFRAME = 1 (also the AutoCAD default)
        if objs.wipeout_frame.unwrap_or(1) == 1 { crate::entities::wipeout_frames(self); }
        // multileader style names come from their dictionary
        let names: HashMap<Handle, String> = objs.dicts.values().flat_map(|d| d.1.iter().map(|(n, h)| (*h, n.clone()))).collect();
        for (h, raw) in objs.mleader_styles {
            let mut st = MLeaderStyle { name: names.get(&h).cloned().unwrap_or_default(), handle: h, raw, ..Default::default() };
            st.parse();
            self.b.mleader_styles.push(st);
        }
    }
}
