//! Metadata (`Scene::meta`): block attributes and texts, on
//! tests/data/attributes.dxf (made by gen_attributes.py) and small inline files.

use cadrender_core::geom::PathData;
use cadrender_core::scene::{Scene, TextKind};
use cadrender_core::text::{Face, Fonts, GlyphSource};

/// Every glyph is a box: tests do not depend on system fonts.
struct Boxes;
impl GlyphSource for Boxes {
    fn cap_height(&self) -> f64 { 700.0 }
    fn space_advance(&self) -> f64 { 300.0 }
    fn glyph(&self, _: char) -> Option<(PathData, f64)> {
        let mut p = PathData::default();
        p.move_to(0.0, 0.0); p.line_to(500.0, 0.0); p.line_to(500.0, 700.0); p.close();
        Some((p, 600.0))
    }
}
fn fonts() -> Fonts { Fonts::with_fallback(Face::new("boxes", Box::new(Boxes))) }

fn read(name: &str) -> Scene {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name);
    cadrender_dxf::read_file(path, fonts()).expect(name)
}

/// (sheet, block, values)
type Rec = (String, String, Vec<(String, String)>);

/// Every INSERT with attributes, in scene order.
fn inserts(s: &Scene) -> Vec<Rec> {
    s.meta.inserts.iter().map(|r| (s.owner_name(r.owner).into_owned(), s.block_name(r.block).into_owned(), r.attribs.clone())).collect()
}

fn values(v: &[(&str, &str)]) -> Vec<(String, String)> { v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect() }

fn long_value() -> String {
    let parts: Vec<String> = (1..=20).map(|i| format!("part {i:02} of the long value")).collect();
    format!("Long note: {}", parts.join(" "))
}

#[test]
fn attributes_per_sheet() {
    let s = read("attributes.dxf");
    let long = long_value();
    let want = vec![
        ("Model".to_string(), "ROOM".to_string(), values(&[("NAME", "Kitchen")])),
        ("A01".into(), "TITLE_BLOCK".into(), values(&[
            ("PROJECT", "P-001"), ("CLIENT", "Example Client Ltd"), ("SITE", "Città di prova"), ("DRAWING", "Ground floor plan"),
            ("SCALE", "1:100"), ("NOTES", &long), ("DESCRIPTION", "First line\nSecond line"), ("SHEET_NO", "A01"),
        ])),
        ("A02".into(), "TITLE_BLOCK".into(), values(&[
            ("PROJECT", "P-001"), ("CLIENT", "Example Client Ltd"), ("DRAWING", "Section A-A"), ("SCALE", "1:50"),
            ("SHEET_NO", "A02"), ("REVISION", "B - angle 45°"),
        ])),
        // nested: the TAG insert is inside the DETAIL block (reported by block name)
        ("DETAIL".into(), "TAG".into(), values(&[("ID", "D-1")])),
    ];
    assert_eq!(inserts(&s), want);
    let a01 = &s.meta.inserts[1];
    assert_eq!(a01.pos, [200.0, 10.0]);
    assert_eq!(s.layers[a01.layer as usize].name, "0");
    assert_ne!(a01.handle, 0);
}

#[test]
fn texts_per_sheet() {
    let s = read("attributes.dxf");
    let got: Vec<(String, &str, &str, &str)> = s.meta.texts.iter()
        .map(|t| (s.owner_name(t.owner).into_owned(), t.kind.name(), s.layers[t.layer as usize].name.as_str(), t.text.as_str())).collect();
    let got: Vec<(&str, &str, &str, &str)> = got.iter().map(|g| (g.0.as_str(), g.1, g.2, g.3)).collect();
    assert_eq!(got, vec![
        ("Model", "TEXT", "0", "Model text"),
        ("Model", "MULTILEADER", "0", "Leader note"),
        ("A01", "TEXT", "NOTES", "Sheet note"),
        ("A02", "MTEXT", "0", "Bold heading\nSecond line"),
    ]);
    assert_eq!(s.meta.texts[2].pos, [20.0, 20.0]);
    assert!(s.meta.texts.iter().all(|t| t.handle != 0));
    assert_eq!(s.meta.texts.iter().filter(|t| t.kind == TextKind::MText).count(), 1);
}

#[test]
fn attributes_do_not_change_the_drawing() {
    // attributes are still drawn as before, hidden ones are not
    let s = read("attributes.dxf");
    let a01 = s.layout("A01").unwrap();
    let blk = &s.blocks[a01.block as usize];
    let n: usize = blk.ents.iter().map(|e| match &e.kind { cadrender_core::entity::Kind::Insert(i) => i.attribs.len(), _ => 0 }).sum();
    assert_eq!(n, 8);
    let a02 = s.layout("A02").unwrap();
    let blk = &s.blocks[a02.block as usize];
    let n: usize = blk.ents.iter().map(|e| match &e.kind { cadrender_core::entity::Kind::Insert(i) => i.attribs.len(), _ => 0 }).sum();
    assert_eq!(n, 5, "the hidden REVISION is not drawn");
}

/// An old (AC1015) file: ANSI bytes, not UTF-8; the value is split into group 3 + 1.
#[test]
fn old_file_code_page_and_group_3() {
    let mut f: Vec<u8> = vec![];
    let mut g = |c: i32, v: &[u8]| { f.extend_from_slice(format!("{c}\n").as_bytes()); f.extend_from_slice(v); f.push(b'\n'); };
    for (c, v) in [(0, &b"SECTION"[..]), (2, b"HEADER"), (9, b"$ACADVER"), (1, b"AC1015"), (0, b"ENDSEC"), (0, b"SECTION"), (2, b"ENTITIES"),
        (0, b"INSERT"), (5, b"2A"), (8, b"0"), (66, b"1"), (2, b"TB"), (10, b"1.5"), (20, b"2.5"),
        (0, b"ATTRIB"), (5, b"2B"), (8, b"0"), (10, b"0"), (20, b"0"), (40, b"1"), (3, b"l\x92area "), (1, b"\x80 5 ^J"), (2, b"NOTE"), (70, b"0"),
        (0, b"SEQEND"), (0, b"ENDSEC"), (0, b"EOF")] { g(c, v); }
    let s = cadrender_dxf::read_bytes(&f, fonts()).unwrap();
    assert_eq!(inserts(&s), vec![("Model".to_string(), "TB".to_string(), values(&[("NOTE", "l’area € 5")]))]);
    assert_eq!(s.meta.inserts[0].handle, 0x2A);
    assert_eq!(s.meta.inserts[0].pos, [1.5, 2.5]);
}
