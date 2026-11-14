//! Header and tables: linetypes, layers, text styles, dimension styles,
//! block records. Same values and defaults as `cadrender-dxf/src/tables.rs`.

use cadrender_core::scene::tables::{BlockId, Handle, LayerId, Linetype, LinetypeId, TextStyle};
use cadrender_core::linetype::{LtPart, LtWhat};
use cadrender_core::text::Fonts;
use cadrender_core::{Scene, SceneBuilder};
use cadrender_dwg::{lineweight_from_raw, Color, Drawing, ObjectKind};
use std::collections::HashMap;

/// Adapter state: the builder plus handle -> id maps.
pub struct Ctx<'a> {
    pub d: &'a Drawing,
    pub b: SceneBuilder,
    pub layer_of: HashMap<Handle, LayerId>,
    pub block_of: HashMap<Handle, BlockId>,
    /// LTYPE handle -> linetype id as the DXF reader would resolve its name.
    pub ltype_of: HashMap<Handle, LinetypeId>,
    /// STYLE handle -> style name.
    pub style_name: HashMap<Handle, String>,
    /// VIEWPORT handle -> 1-based position in its layout block.
    pub vp_pos: HashMap<Handle, i32>,
    /// The entity being converted is in a block definition (ATTDEF rule).
    pub block_def: bool,
    /// LAYER -> its extension dictionary (per-viewport overrides live there).
    pub layer_xdict: Vec<(LayerId, Handle)>,
}

/// Entries of a table control object, in table order.
fn entries(d: &Drawing, control: Handle) -> Vec<Handle> {
    match d.get(control).map(|o| &o.kind) {
        Some(ObjectKind::Control(c)) => c.entries.iter().chain(c.extra.iter()).copied().collect(),
        _ => vec![],
    }
}

/// Radians -> degrees (DXF keeps degrees, the Scene follows the DXF values).
pub fn deg(rad: f64) -> f64 { rad.to_degrees() }

impl<'a> Ctx<'a> {
    pub fn new(d: &'a Drawing, fonts: Fonts) -> Ctx<'a> {
        Ctx {
            d, b: SceneBuilder::new(fonts),
            layer_of: HashMap::new(), block_of: HashMap::new(), ltype_of: HashMap::new(),
            style_name: HashMap::new(), layer_xdict: vec![], vp_pos: HashMap::new(), block_def: false,
        }
    }

    pub fn finish(self) -> Scene { self.b.build() }

    pub fn header(&mut self) {
        let h = &self.d.header;
        let sh = &mut self.b.header;
        sh.version = self.d.version.magic().to_string();
        sh.ltscale = if h.ltscale > 0.0 { h.ltscale } else { 1.0 };
        sh.psltscale = h.psltscale;
        sh.insunits = h.insunits as i32;
        sh.pdmode = h.pdmode as i32;
        sh.pdsize = h.pdsize;
    }

    /// All objects of the table, in table order (falls back to handle order
    /// when the control object is missing).
    fn table<T>(&self, control: Handle, pick: impl Fn(&'a ObjectKind) -> Option<&'a T>) -> Vec<(Handle, &'a T)> {
        let d = self.d;
        let mut out: Vec<(Handle, &T)> = entries(d, control).into_iter().filter_map(|h| Some((h, pick(&d.get(h)?.kind)?))).collect();
        if out.is_empty() {
            out = d.objects.iter().filter_map(|o| Some((o.handle, pick(&o.kind)?))).collect();
        }
        out
    }

    pub fn linetypes(&mut self) {
        let hs = self.d.header.handles.clone();
        for (h, lt) in self.table(hs.ltype_control, |k| if let ObjectKind::Ltype(l) = k { Some(l) } else { None }) {
            let pattern = lt.dashes.iter().map(|x| x.length).collect();
            let complex = lt.dashes.iter().any(|x| x.flags & 6 != 0);
            let parts = self.lt_parts(&lt.name, &lt.dashes);
            self.b.add_linetype(Linetype { name: lt.name.clone(), handle: h, pattern, complex, parts });
        }
        // after all are added: the id an entity gets for this handle
        let names: Vec<(Handle, String)> = self.d.objects.iter().filter_map(|o| match &o.kind { ObjectKind::Ltype(l) => Some((o.handle, l.name.clone())), _ => None }).collect();
        for (h, n) in names { let id = self.b.linetype_id(&n); self.ltype_of.insert(h, id); }
    }

    pub fn layers(&mut self) {
        let hs = self.d.header.handles.clone();
        for (h, l) in self.table(hs.layer_control, |k| if let ObjectKind::Layer(l) = k { Some(l) } else { None }) {
            let id = self.b.layer(&l.name);
            let ltype = self.ltype_of.get(&l.ltype).copied().unwrap_or(cadrender_core::scene::tables::LT_SOLID);
            let (color, tc) = match l.color { Color::Index(i) => (i as i32, None), Color::Rgb(rgb) => (7, Some(rgb)), _ => (7, None) };
            let sl = self.b.layer_mut(id);
            sl.off = !l.on;
            sl.color = color.clamp(1, 255);
            sl.true_color = tc;
            sl.lw = lineweight_from_raw(l.lineweight_raw).clamp(-3, 211);
            sl.frozen = l.frozen;
            sl.plot = l.plot;
            sl.ltype = ltype;
            sl.handle = h;
            sl.alpha = layer_alpha(self.d, &l.eed);
            self.layer_of.insert(h, id);
            if let Some(x) = self.d.get(h).and_then(|o| o.xdict) { self.layer_xdict.push((id, x)); }
        }
    }

    pub fn text_styles(&mut self) {
        let hs = self.d.header.handles.clone();
        for (h, s) in self.table(hs.style_control, |k| if let ObjectKind::Style(s) = k { Some(s) } else { None }) {
            if s.name.is_empty() { continue; }
            let mut st = TextStyle::new(&s.name);
            st.handle = h;
            st.font = s.font_file.clone();
            st.bigfont = s.bigfont_file.clone();
            st.height = s.fixed_height;
            st.width_factor = if s.width_factor <= 0.0 || s.width_factor > 100.0 { 1.0 } else { s.width_factor };
            st.oblique = deg(s.oblique);
            st.flags = (s.is_shape_file as i32) | ((s.vertical as i32) << 2);
            self.style_name.insert(h, s.name.clone());
            self.b.add_text_style(st);
        }
    }

    pub fn block_records(&mut self) {
        let hs = self.d.header.handles.clone();
        for (h, bh) in self.table(hs.block_control, |k| if let ObjectKind::BlockHeader(b) = k { Some(b) } else { None }) {
            let name = if bh.name.is_empty() { format!("*BLOCK_{h:X}") } else { bh.name.clone() };
            // two records with one name (should not happen): keep them apart
            let id = match self.b.find_block(&name) {
                Some(id) if self.b.block_mut(id).handle != 0 && self.b.block_mut(id).handle != h => self.b.block(&format!("{name}#{h:X}")),
                _ => self.b.block(&name),
            };
            let blk = self.b.block_mut(id);
            blk.handle = h;
            blk.base = [bh.base_point[0], bh.base_point[1]];
            self.block_of.insert(h, id);
        }
    }
}

/// Layer transparency: EED of app "AcCmTransparency", group 1071.
fn layer_alpha(d: &Drawing, eed: &[(Handle, Vec<u8>)]) -> u8 {
    for (app, bytes) in eed {
        let Some(ObjectKind::AppId(name)) = d.get(*app).map(|o| &o.kind) else { continue };
        if name != "AcCmTransparency" { continue; }
        let t = cadrender_dwg::eed_items(bytes).into_iter().find(|i| i.0 == 1071).and_then(|i| i.1.int());
        if let Some(t) = t { if t & 0x0200_0000 != 0 { return (t & 0xff) as u8; } }
    }
    255
}

impl Ctx<'_> {
    /// Text and shape elements of a complex linetype (like the DXF reader).
    fn lt_parts(&mut self, name: &str, dashes: &[cadrender_dwg::LtypeDash]) -> Vec<LtPart> {
        let mut parts = vec![];
        for (elem, x) in dashes.iter().enumerate() {
            if x.flags & 6 == 0 { continue; }
            let what = if x.flags & 2 != 0 {
                let Some(face_id) = self.b.fonts.face_for("") else { continue };
                let face = &mut self.b.fonts.faces[face_id as usize];
                let text = x.text.clone().unwrap_or_default();
                let laid = cadrender_core::text::layout::layout(face, &[cadrender_core::text::mtext::plain(&text)], 0.0, 1.0);
                if laid.glyphs.is_empty() { continue; }
                LtWhat::Text { face: face_id, glyphs: laid.glyphs, cap: face.cap_height }
            } else if x.shape_number == 0 {
                continue;
            } else {
                self.b.skip("LTYPE_SHAPE");
                LtWhat::Shape { code: x.shape_number as u16 }
            };
            parts.push(LtPart { elem, what, scale: if x.scale == 0.0 { 1.0 } else { x.scale }, rot: x.rotation, absolute: x.flags & 1 != 0, off: x.offset });
        }
        if parts.iter().any(|p| matches!(p.what, LtWhat::Shape { .. })) {
            self.b.warn(format!("linetype {name:?} uses SHX shapes: drawn as a wave"));
        }
        parts
    }
}
