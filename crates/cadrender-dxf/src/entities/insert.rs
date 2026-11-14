//! INSERT (with MINSERT rows/columns and ATTRIBs), DIMENSION and ACAD_TABLE.
//! DIMENSION and ACAD_TABLE are drawn through their ready-made anonymous blocks.

use super::Common;
use crate::reader::Reader;
use crate::tags::{find, gf, gi, s, Tag};
use cadrender_core::entity::insert::InsertEnt;
use cadrender_core::entity::{Entity, Kind};
use cadrender_core::geom::Aff;

pub fn insert(r: &mut Reader, kind: &[u8], body: &[Tag], com: &Common, attribs: Vec<Entity>) -> Option<Kind> {
    let bname = s(find(body, 2)?);
    if bname.is_empty() { return None; }
    let block = r.b.block(&bname);
    let g = |c: i32| gf(body, c).unwrap_or(0.0);
    let m = if kind == b"DIMENSION" { Aff::ID } else {
        let (mut sx, mut sy) = (gf(body, 41).unwrap_or(1.0), gf(body, 42).unwrap_or(1.0));
        if kind == b"ACAD_TABLE" { sx = 1.0; sy = 1.0; }
        if sx == 0.0 { sx = 1.0; }
        if sy == 0.0 { sy = 1.0; }
        let m = Aff::translate(g(10), g(20)).mul(&Aff::rotate(g(50).to_radians())).mul(&Aff::scale(sx, sy));
        match com.ocs() { Some(o) => o.mul(&m), None => m }
    };
    let mut ins = InsertEnt::new(block, m);
    if kind == b"INSERT" {
        let cols = gi(body, 70).unwrap_or(1).clamp(1, 10_000) as u32;
        let rows = gi(body, 71).unwrap_or(1).clamp(1, 10_000) as u32;
        ins.set_grid(cols, rows, g(44), g(45));
    }
    ins.attribs = attribs;
    Some(ins.into())
}
