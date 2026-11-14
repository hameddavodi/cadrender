//! TABLES section: LAYER, LTYPE, STYLE, DIMSTYLE, BLOCK_RECORD.

use crate::reader::Reader;
use crate::tags::{f, hex, i, raw, s, trim, Tag};
use cadrender_core::linetype::{LtPart, LtWhat};
use cadrender_core::scene::tables::{DimStyle, Linetype, TextStyle};

impl Reader {
    pub(crate) fn table_entry(&mut self, kind: &[u8], body: &[Tag]) {
        match kind {
            b"LAYER" => self.layer(body),
            b"LTYPE" => self.linetype(body),
            b"STYLE" => {
                let mut st = TextStyle::new("");
                let mut family = String::new();
                for &(c, v) in body {
                    match c {
                        2 => st.name = s(v), 5 => st.handle = hex(v), 3 => st.font = s(v), 4 => st.bigfont = s(v),
                        40 => st.height = f(v), 41 => st.width_factor = f(v), 50 => st.oblique = f(v), 70 => st.flags = i(v),
                        1000 if family.is_empty() => family = s(v), // ACAD xdata: TrueType family name
                        _ => {}
                    }
                }
                if st.width_factor <= 0.0 || st.width_factor > 100.0 { st.width_factor = 1.0; }
                if st.font.is_empty() && !family.is_empty() { st.font = family; }
                if !st.name.is_empty() { self.b.add_text_style(st); }
            }
            b"DIMSTYLE" => {
                let mut d = DimStyle { scale: 1.0, arrow_size: 2.5, text_height: 2.5, ..DimStyle::default() };
                for &(c, v) in body {
                    match c { 2 => d.name = s(v), 105 => d.handle = hex(v), 40 => d.scale = f(v), 41 => d.arrow_size = f(v), 140 => d.text_height = f(v), _ => {} }
                    d.raw.push((c, raw(v)));
                }
                self.b.dim_styles.push(d);
            }
            b"BLOCK_RECORD" => {
                let (mut name, mut handle) = (String::new(), 0);
                for &(c, v) in body { match c { 2 => name = s(v), 5 => handle = hex(v), _ => {} } }
                if handle != 0 {
                    let id = self.b.block(&name);
                    self.b.block_mut(id).handle = handle;
                    self.rec_by_handle.insert(handle, name);
                }
            }
            _ => {}
        }
    }

    /// LTYPE: dash pattern, plus text and shape elements (complex linetypes).
    fn linetype(&mut self, body: &[Tag]) {
        let mut lt = Linetype::default();
        // per element: (flags, shape number, scale, rotation, x, y, text)
        let mut cur: Option<(i32, u16, f64, f64, f64, f64, String)> = None;
        let mut parts = vec![];
        let flush = |cur: &mut Option<(i32, u16, f64, f64, f64, f64, String)>, elem: usize, parts: &mut Vec<_>| {
            if let Some(e) = cur.take() { if e.0 & 6 != 0 { parts.push((elem, e)); } }
        };
        for &(c, v) in body {
            match c {
                2 => lt.name = s(v),
                5 => lt.handle = hex(v),
                49 => { flush(&mut cur, lt.pattern.len().wrapping_sub(1), &mut parts); lt.pattern.push(f(v)); cur = Some((0, 0, 1.0, 0.0, 0.0, 0.0, String::new())); }
                74 => if let Some(e) = cur.as_mut() { e.0 = i(v); lt.complex |= i(v) & 6 != 0; },
                75 => if let Some(e) = cur.as_mut() { e.1 = i(v).clamp(0, 65535) as u16; },
                46 => if let Some(e) = cur.as_mut() { e.2 = f(v); },
                50 => if let Some(e) = cur.as_mut() { e.3 = f(v); },
                44 => if let Some(e) = cur.as_mut() { e.4 = f(v); },
                45 => if let Some(e) = cur.as_mut() { e.5 = f(v); },
                9 => if let Some(e) = cur.as_mut() { e.6 = raw(v); },
                _ => {}
            }
        }
        flush(&mut cur, lt.pattern.len().wrapping_sub(1), &mut parts);
        for (elem, (flags, code, scale, rot, x, y, text)) in parts {
            let what = if flags & 2 != 0 {
                let Some(face_id) = self.b.fonts.face_for("") else { continue };
                let face = &mut self.b.fonts.faces[face_id as usize];
                let laid = cadrender_core::text::layout::layout(face, &[cadrender_core::text::mtext::plain(&text)], 0.0, 1.0);
                if laid.glyphs.is_empty() { continue; }
                LtWhat::Text { face: face_id, glyphs: laid.glyphs, cap: face.cap_height }
            } else if code == 0 {
                continue; // no shape (seen in "DOT2" of some files)
            } else {
                // SHX shape files (ltypeshp.shx ...) are not free: drawn as a wave
                self.b.skip("LTYPE_SHAPE");
                LtWhat::Shape { code }
            };
            lt.parts.push(LtPart { elem, what, scale: if scale == 0.0 { 1.0 } else { scale }, rot: rot.to_radians(), absolute: flags & 1 != 0, off: [x, y] });
        }
        if lt.parts.iter().any(|p| matches!(p.what, LtWhat::Shape { .. })) {
            self.b.warn(format!("linetype {:?} uses SHX shapes: drawn as a wave", lt.name));
        }
        self.b.add_linetype(lt);
    }

    fn layer(&mut self, body: &[Tag]) {
        let (mut name, mut handle, mut xdict): (&[u8], u64, u64) = (b"", 0, 0);
        let (mut color, mut tc, mut lw, mut flags, mut lt, mut plot): (i32, Option<u32>, i16, i32, &[u8], bool) = (7, None, -3, 0, b"CONTINUOUS", true);
        let mut in_xdict = false;
        // layer transparency lives in XDATA: 1001 AcCmTransparency, 1071 value
        let (mut in_transp, mut alpha) = (false, 255u8);
        for &(c, v) in body {
            match c {
                102 => in_xdict = trim(v).eq_ignore_ascii_case(b"{ACAD_XDICTIONARY"),
                360 if in_xdict => xdict = hex(v),
                2 => name = v, 5 => handle = hex(v), 62 => color = i(v), 420 => tc = Some(i(v) as u32 & 0xff_ffff),
                370 => lw = i(v).clamp(-3, 211) as i16, 70 => flags = i(v), 6 => lt = v, 290 => plot = i(v) != 0,
                1001 => in_transp = trim(v) == b"AcCmTransparency",
                1071 if in_transp => { let t = crate::tags::long(v); if t & 0x0200_0000 != 0 { alpha = (t & 0xff) as u8; } in_transp = false; }
                _ => {}
            }
        }
        let id = self.b.layer(&s(name));
        let ltype = self.b.linetype_id(&s(lt));
        let l = self.b.layer_mut(id);
        l.off = color < 0;
        l.color = color.abs().clamp(1, 255);
        l.true_color = tc;
        l.lw = lw;
        l.frozen = flags & 1 != 0;
        l.plot = plot;
        l.ltype = ltype;
        l.handle = handle;
        l.alpha = alpha;
        if xdict != 0 { self.layer_xdict.push((id, xdict)); }
    }
}
