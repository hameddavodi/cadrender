//! The main loop: sections, then one object at a time.

use crate::objects::Objects;
use crate::tags::{f, hex, i, s, trim, Tag, Tags};
use crate::DxfError;
use cadrender_core::scene::meta::{InsertRec, TextRec};
use cadrender_core::scene::tables::{BlockId, Handle, LayerId};
use cadrender_core::text::Fonts;
use cadrender_core::{Scene, SceneBuilder};
use std::collections::HashMap;

pub struct Reader {
    pub b: SceneBuilder,
    /// BLOCK_RECORD handle -> block name.
    pub rec_by_handle: HashMap<Handle, String>,
    /// LAYER -> its extension dictionary (per-viewport overrides live there).
    pub layer_xdict: Vec<(LayerId, Handle)>,
    pub objs: Objects,
    /// The entity being read is in a block definition (not model or paper space).
    pub block_def: bool,
    /// ATTDEF handle -> its groups (MULTILEADER block attributes need them).
    pub attdefs: HashMap<Handle, Vec<(i32, Vec<u8>)>>,
    /// `defined[block]`: its BLOCK record was read (a block can be used, and
    /// even listed in BLOCK_RECORD, before its definition comes).
    defined: Vec<bool>,
    /// Work that needs the whole file (see `at_end`).
    deferred: Vec<Deferred>,
    /// Metadata of the entity just read; the main loop adds its owner block.
    pub meta: Option<(crate::entities::Common, MetaRec)>,
}

/// A metadata record whose owner is not known yet (`owner` is set by the main loop).
pub enum MetaRec { Insert(InsertRec), Text(TextRec) }

/// A step run after the last section, before the scene is built.
pub type Deferred = Box<dyn FnOnce(&mut Reader)>;

#[derive(PartialEq, Clone, Copy)]
enum Section { None, Header, Tables, Blocks, Entities, Objects, Other }

impl Reader {
    pub fn new(fonts: Fonts) -> Reader {
        Reader { b: SceneBuilder::new(fonts), rec_by_handle: HashMap::new(), layer_xdict: vec![], objs: Objects::default(), block_def: false, attdefs: HashMap::new(), defined: vec![], deferred: vec![], meta: None }
    }

    /// The BLOCK record of this block was read already.
    pub fn is_defined(&self, id: BlockId) -> bool { self.defined.get(id as usize).copied().unwrap_or(false) }

    /// Run `f` after the whole file is read (for example: a DIMENSION whose
    /// block is defined later in the file, or never).
    pub fn at_end(&mut self, f: Deferred) { self.deferred.push(f); }

    /// Block id of a BLOCK_RECORD handle.
    pub fn block_of_record(&mut self, h: Handle) -> Option<BlockId> {
        let name = self.rec_by_handle.get(&h)?.clone();
        Some(self.b.block(&name))
    }

    pub fn read(mut self, buf: &[u8]) -> Result<Scene, DxfError> {
        let mut t = Tags::new(buf);
        let mut body: Vec<Tag> = Vec::with_capacity(256);
        let mut section = Section::None;
        let mut sections_seen = 0u32;
        let mut cur_block: Option<BlockId> = None;
        let model = self.b.model();
        while let Some((c, v)) = t.next() {
            if c != 0 {
                if section == Section::Header && c == 9 { self.header_var(trim(v), &mut t); }
                else if sections_seen == 0 && c != 999 && t.pos() > 4096 { break; }
                continue;
            }
            let kind = trim(v);
            match kind {
                b"SECTION" => {
                    sections_seen += 1;
                    section = match t.peek() {
                        Some((2, n)) => { t.next(); match trim(n) { b"HEADER" => Section::Header, b"TABLES" => Section::Tables, b"BLOCKS" => Section::Blocks, b"ENTITIES" => Section::Entities, b"OBJECTS" => Section::Objects, _ => Section::Other } }
                        _ => Section::Other,
                    };
                    continue;
                }
                b"ENDSEC" => { section = Section::None; continue; }
                b"EOF" => break,
                _ => {}
            }
            match section {
                Section::Tables => { t.body(&mut body); self.table_entry(kind, &body); }
                Section::Blocks | Section::Entities => {
                    t.body(&mut body);
                    match kind {
                        b"BLOCK" => {
                            let (mut name, mut base, mut owner) = (String::new(), [0.0; 2], 0);
                            for &(c, v) in &body { match c { 2 => name = s(v), 10 => base[0] = f(v), 20 => base[1] = f(v), 330 if owner == 0 => owner = hex(v), _ => {} } }
                            if name.is_empty() { if let Some(n) = self.rec_by_handle.get(&owner) { name = n.clone(); } }
                            let id = self.b.block(&name);
                            let blk = self.b.block_mut(id);
                            blk.base = base;
                            if blk.handle == 0 { blk.handle = owner; }
                            if self.defined.len() <= id as usize { self.defined.resize(id as usize + 1, false); }
                            self.defined[id as usize] = true;
                            cur_block = Some(id);
                        }
                        b"ENDBLK" => cur_block = None,
                        _ => {
                            let in_entities = section == Section::Entities;
                            self.block_def = !in_entities && cur_block.is_some_and(|b| {
                                let n = self.b.block_name(b).to_ascii_lowercase();
                                !(n.starts_with("*model_space") || n.starts_with("*paper_space"))
                            });
                            let target = |r: &mut Reader, com: &crate::entities::Common| if in_entities {
                                match r.block_of_record(com.owner) {
                                    Some(b) => b,
                                    None => if com.paper { r.b.paper() } else { model },
                                }
                            } else { cur_block.unwrap_or(model) };
                            if let Some((ent, com)) = crate::entities::read(&mut self, kind, &body, &mut t) {
                                let block = target(&mut self, &com);
                                self.b.push(block, ent);
                            }
                            if let Some((com, rec)) = self.meta.take() {
                                let owner = target(&mut self, &com);
                                match rec {
                                    MetaRec::Insert(r) => self.b.record_insert(InsertRec { owner, ..r }),
                                    MetaRec::Text(r) => self.b.record_text(TextRec { owner, ..r }),
                                }
                            }
                        }
                    }
                }
                Section::Objects => { t.body(&mut body); self.object(kind, &body); }
                _ => { t.body(&mut body); if kind == b"ACDSRECORD" { self.acds_record(&body); } }
            }
        }
        if sections_seen == 0 { return Err(DxfError::NotDxf("no SECTION found".into())); }
        for f in std::mem::take(&mut self.deferred) { f(&mut self); }
        self.finish_objects();
        Ok(self.b.build())
    }

    fn header_var(&mut self, name: &[u8], t: &mut Tags) {
        let Some((code, v)) = t.peek() else { return };
        if code == 0 || code == 9 { return; }
        let h = &mut self.b.header;
        match name {
            b"$ACADVER" => h.version = s(v),
            b"$LTSCALE" => { let x = f(v); h.ltscale = if x > 0.0 { x } else { 1.0 }; }
            b"$PSLTSCALE" => h.psltscale = i(v) != 0,
            b"$INSUNITS" => h.insunits = i(v),
            b"$MEASUREMENT" => h.measurement = i(v),
            b"$PDMODE" => h.pdmode = i(v),
            b"$PDSIZE" => h.pdsize = f(v),
            _ => return,
        }
        t.next();
    }
}
