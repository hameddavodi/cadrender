//! Metadata for `Scene::meta`: INSERT attributes and TEXT / MTEXT /
//! MULTILEADER content, with the same rules as the DXF reader (hidden
//! attributes are kept, invisible texts are not; values are plain text).

use crate::tables::Ctx;
use cadrender_core::scene::meta::{plain_mtext, plain_text, InsertRec, TextKind, TextRec};
use cadrender_core::scene::tables::{BlockId, Handle};
use cadrender_dwg::EntityKind as K;

impl Ctx<'_> {
    /// Keep the metadata of entity `h` in block `owner`, if it has any.
    pub fn record_meta(&mut self, owner: BlockId, h: Handle) {
        let d = self.d;
        let Some(o) = d.get(h) else { return };
        let Some(e) = o.entity() else { return };
        match &e.kind {
            K::Insert(i) if i.attribs.is_empty() => return,
            K::Insert(_) | K::Text(_) | K::MText(_) | K::MLeader(_) => {}
            _ => return,
        }
        let com = self.com(h, e);
        let at = |p: [f64; 2]| [if com.ez < 0.0 { -p[0] } else { p[0] }, p[1]];
        let (layer, handle) = (com.props.layer, h);
        let text = |kind, pos, text| TextRec { owner, kind, layer, handle, pos, text };
        match &e.kind {
            K::Insert(i) => {
                let Some(&block) = self.block_of.get(&i.block_header) else { return };
                let mut attribs = vec![];
                for &a in &i.attribs {
                    let Some(K::Attrib(a)) = d.get(a).and_then(|x| x.entity()).map(|x| &x.kind) else { continue };
                    if a.tag.trim().is_empty() { continue; }
                    // R2018 multi-line attribute: the text is in the embedded MTEXT
                    let multi = a.mtext.as_ref().map(|m| plain_mtext(&m.value)).filter(|v| !v.is_empty());
                    attribs.push((a.tag.trim().to_string(), multi.unwrap_or_else(|| plain_text(&a.text.value))));
                }
                self.b.record_insert(InsertRec { owner, block, layer, handle, pos: at([i.insertion[0], i.insertion[1]]), attribs });
            }
            _ if e.common.invisible => {}
            K::Text(t) => self.b.record_text(text(TextKind::Text, at(t.insertion), plain_text(&t.value))),
            K::MText(m) => {
                let rt = self.rich_text(o.xdict);
                self.b.record_text(text(TextKind::MText, at([m.insertion[0], m.insertion[1]]), plain_mtext(rt.as_deref().unwrap_or(&m.value))));
            }
            K::MLeader(m) if m.content_type == 2 => {
                if let Some(t) = &m.context.text { self.b.record_text(text(TextKind::MLeader, [t.location[0], t.location[1]], plain_mtext(&t.label))); }
            }
            _ => {}
        }
    }
}
