//! Document::sheet_info / attributes / texts (data: cadrender-dxf/tests/data/attributes.dxf).

use cadrender::{Document, Fonts};

fn doc() -> Document {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../cadrender-dxf/tests/data/attributes.dxf");
    Document::open(p, Fonts::new()).unwrap()
}

#[test]
fn sheets_attributes_and_texts() {
    let d = doc();
    let sheets: Vec<&str> = d.sheet_info().iter().map(|s| s.name).collect();
    assert_eq!(sheets, d.sheets());
    let all: Vec<String> = d.attributes(None).iter().map(|a| format!("{}/{}", a.sheet, a.block)).collect();
    assert_eq!(all, vec!["Model/ROOM", "A01/TITLE_BLOCK", "A02/TITLE_BLOCK", "DETAIL/TAG"]);
    let a01 = d.attributes(Some("A01"));
    assert_eq!(a01.len(), 1);
    assert!(a01[0].values.iter().any(|(t, v)| t == "SHEET_NO" && v == "A01"));
    assert!(d.attributes(Some("nope")).is_empty());
    let notes: Vec<&str> = d.texts(None, Some(&["Notes"])).iter().map(|t| t.text).collect();
    assert_eq!(notes, vec!["Sheet note"]);
    assert_eq!(d.texts(Some("A02"), None)[0].kind, "MTEXT");
    assert_eq!(d.texts(None, None).len(), 4);
}
