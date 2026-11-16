//! Metadata (`Scene::meta`) from DWG: the same records as from the DXF.
//! tests/data/attributes.dwg is ODA's DWG of cadrender-dxf/tests/data/attributes.dxf
//! (made by gen_attributes.py; ODA File Converter, DXF -> DWG 2018).

use cadrender_core::scene::Scene;
use cadrender_core::text::Fonts;
use std::path::Path;

fn dwg() -> Scene {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/attributes.dwg");
    cadrender_dwg_scene::read_file(p, Fonts::new()).expect("attributes.dwg")
}

fn dxf() -> Scene {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../cadrender-dxf/tests/data/attributes.dxf");
    cadrender_dxf::read_file(p, Fonts::new()).expect("attributes.dxf")
}

/// (sheet, block, layer, handle, position, attributes)
type InsertRow = (String, String, String, u64, [f64; 2], Vec<(String, String)>);
/// (sheet, type, layer, handle, position, text)
type TextRow = (String, &'static str, String, u64, [f64; 2], String);

/// Every INSERT record as plain values.
fn inserts(s: &Scene) -> Vec<InsertRow> {
    s.meta.inserts.iter().map(|r| (
        s.owner_name(r.owner).into_owned(), s.block_name(r.block).into_owned(), s.layers[r.layer as usize].name.clone(),
        r.handle, r.pos, r.attribs.clone(),
    )).collect()
}

/// Every text record as plain values.
fn texts(s: &Scene) -> Vec<TextRow> {
    s.meta.texts.iter().map(|t| (
        s.owner_name(t.owner).into_owned(), t.kind.name(), s.layers[t.layer as usize].name.clone(), t.handle, t.pos, t.text.clone(),
    )).collect()
}

#[test]
fn dwg_attributes_match_the_dxf() {
    let (a, b) = (dwg(), dxf());
    // NOTES is split into group 3 lines in the DXF. ODA ignores group 3 in an
    // ATTRIB, so its DWG keeps only the last part: compare it apart.
    let notes = |s: &Scene| s.meta.inserts[1].attribs.iter().find(|a| a.0 == "NOTES").map(|a| a.1.clone()).unwrap();
    assert!(notes(&b).starts_with("Long note: part 01") && notes(&b).ends_with(&notes(&a)));
    assert_eq!(notes(&a), "alue part 20 of the long value");
    let without_notes = |s: &Scene| {
        let mut v = inserts(s);
        for r in &mut v { r.5.retain(|a| a.0 != "NOTES"); }
        v
    };
    assert_eq!(inserts(&a).len(), 4);
    assert_eq!(without_notes(&a), without_notes(&b));
    let a01 = &a.meta.inserts[1];
    assert_eq!(a.owner_name(a01.owner), "A01");
    assert!(a01.attribs.iter().any(|(t, v)| t == "SITE" && v == "Città di prova"));
    assert!(a01.attribs.iter().any(|(t, v)| t == "DESCRIPTION" && v == "First line\nSecond line"));
}

#[test]
fn dwg_texts_match_the_dxf() {
    let (a, b) = (dwg(), dxf());
    assert_eq!(texts(&a).len(), 4);
    assert_eq!(texts(&a), texts(&b));
}
