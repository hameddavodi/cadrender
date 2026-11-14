//! Block order: a DIMENSION may come before its `*D` block in the file (ODA
//! writes paper-space tabs in the BLOCKS section before many `*D` blocks).
//! Small hand-made DXFs written inline.

use cadrender_core::entity::Kind;
use cadrender_core::scene::Block;
use cadrender_core::text::Fonts;
use cadrender_core::Scene;

fn read(groups: &[(i32, &str)]) -> Scene {
    let mut s = String::new();
    for (c, v) in groups { s.push_str(&format!("{c}\n{v}\n")); }
    s.push_str("0\nEOF\n");
    cadrender_dxf::read_bytes(s.as_bytes(), Fonts::new()).unwrap()
}

fn block<'a>(s: &'a Scene, name: &str) -> &'a Block {
    s.blocks.iter().find(|b| b.name == name).unwrap_or_else(|| panic!("no block {name}"))
}

fn kinds(b: &Block) -> Vec<&'static str> { b.ents.iter().map(|e| e.kind.name()).collect() }

/// A linear dimension 0..10 at y = 5 that uses block `name`.
fn dim(handle: &'static str, name: &'static str) -> Vec<(i32, &'static str)> {
    vec![
        (0, "DIMENSION"), (5, handle), (100, "AcDbEntity"), (8, "0"), (100, "AcDbDimension"), (2, name),
        (10, "10.0"), (20, "5.0"), (30, "0.0"), (11, "5.0"), (21, "6.0"), (31, "0.0"), (70, "32"), (1, ""),
        (100, "AcDbAlignedDimension"), (13, "0.0"), (23, "0.0"), (33, "0.0"), (14, "10.0"), (24, "0.0"), (34, "0.0"),
        (100, "AcDbRotatedDimension"), (50, "0.0"),
    ]
}

fn block_def(name: &'static str, rec: &'static str, ents: &[(i32, &'static str)]) -> Vec<(i32, &'static str)> {
    let mut g = vec![(0, "BLOCK"), (5, "1"), (330, rec), (100, "AcDbEntity"), (8, "0"), (100, "AcDbBlockBegin"), (2, name), (70, "1"), (10, "0.0"), (20, "0.0"), (30, "0.0"), (3, name)];
    g.extend_from_slice(ents);
    g.extend_from_slice(&[(0, "ENDBLK"), (5, "2"), (330, rec), (100, "AcDbEntity"), (8, "0"), (100, "AcDbBlockEnd")]);
    g
}

/// Block A holds the DIMENSIONs; `*D1` (one LINE) is defined after A. All
/// blocks are listed in BLOCK_RECORD first, as ODA does. `*D9` never comes.
fn file(dims: &[Vec<(i32, &'static str)>], d1_ents: &[(i32, &'static str)]) -> Scene {
    let mut g = vec![(0, "SECTION"), (2, "TABLES"), (0, "TABLE"), (2, "BLOCK_RECORD")];
    for (h, n) in [("A0", "A"), ("D1", "*D1"), ("D9", "*D9")] {
        g.extend_from_slice(&[(0, "BLOCK_RECORD"), (5, h), (100, "AcDbSymbolTableRecord"), (100, "AcDbBlockTableRecord"), (2, n)]);
    }
    g.extend_from_slice(&[(0, "ENDTAB"), (0, "ENDSEC"), (0, "SECTION"), (2, "BLOCKS")]);
    g.extend(block_def("A", "A0", &dims.concat()));
    g.extend(block_def("*D1", "D1", d1_ents));
    g.extend_from_slice(&[(0, "ENDSEC"), (0, "SECTION"), (2, "ENTITIES")]);
    g.extend_from_slice(&[(0, "INSERT"), (5, "10"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbBlockReference"), (2, "A"), (10, "0.0"), (20, "0.0"), (30, "0.0")]);
    g.extend_from_slice(&[(0, "ENDSEC")]);
    read(&g)
}

const LINE: [(i32, &str); 11] = [
    (0, "LINE"), (5, "30"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbLine"),
    (10, "0.0"), (20, "5.0"), (30, "0.0"), (11, "10.0"), (21, "5.0"), (31, "0.0"),
];

fn insert_block<'a>(s: &'a Scene, k: &Kind) -> &'a Block {
    let Kind::Insert(i) = k else { panic!("not an insert: {}", k.name()) };
    &s.blocks[i.block as usize]
}

#[test]
fn dimension_before_its_block_draws_the_block() {
    let s = file(&[dim("20", "*D1")], &LINE);
    let a = block(&s, "A");
    assert_eq!(kinds(a), vec!["Insert"]);
    let d = insert_block(&s, &a.ents[0].kind);
    assert_eq!(d.name, "*D1");
    // the block's own LINE, not the generated geometry
    assert_eq!(kinds(d), vec!["Stroke"]);
    // and nothing was generated anywhere
    assert!(s.blocks.iter().all(|b| !b.name.starts_with("*DIMGEN")));
}

#[test]
fn dimension_with_block_that_never_comes_gets_generated_geometry() {
    let s = file(&[dim("20", "*D9")], &LINE);
    let d = insert_block(&s, &block(&s, "A").ents[0].kind);
    assert_eq!(d.name, "*D9");
    // two extension lines, dimension line, two arrows (no text: no font)
    let k = kinds(d);
    assert_eq!(&k[..5], &["Stroke", "Stroke", "Stroke", "Fill", "Fill"], "{k:?}");
    let dl = d.ents[2].bbox;
    assert_eq!((dl.x0, dl.x1, dl.y0), (0.0, 10.0, 5.0));
}

#[test]
fn dimension_with_block_defined_empty_gets_generated_geometry() {
    // *D1 is defined after A, but holds nothing: same as a missing block
    let s = file(&[dim("20", "*D1")], &[]);
    let d = insert_block(&s, &block(&s, "A").ents[0].kind);
    assert_eq!(&kinds(d)[..3], &["Stroke", "Stroke", "Stroke"]);
}

#[test]
fn two_dimensions_sharing_a_missing_block_fill_it_once() {
    let s = file(&[dim("20", "*D9"), dim("21", "*D9")], &LINE);
    let a = block(&s, "A");
    let (d0, d1) = (insert_block(&s, &a.ents[0].kind), insert_block(&s, &a.ents[1].kind));
    assert!(std::ptr::eq(d0, d1));
    assert!(d0.ents.len() >= 5 && d0.ents.len() <= 6, "{:?}", kinds(d0));
}
