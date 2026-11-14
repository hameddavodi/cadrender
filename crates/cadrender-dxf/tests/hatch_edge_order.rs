//! HATCH edge loops whose edges are out of order or stored backwards.
//! Small hand-made DXF written inline.

use cadrender_core::entity::Kind;
use cadrender_core::geom;
use cadrender_core::text::Fonts;

/// One solid HATCH with one edge loop of straight edges `(x0, y0, x1, y1)`.
fn hatch_path(lines: &[(f64, f64, f64, f64)]) -> geom::PathData {
    let mut g: Vec<(i32, String)> = [
        (0, "SECTION"), (2, "ENTITIES"), (0, "HATCH"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbHatch"),
        (10, "0"), (20, "0"), (30, "0"), (210, "0"), (220, "0"), (230, "1"), (2, "SOLID"), (70, "1"), (71, "0"), (91, "1"),
        (92, "1"),
    ].iter().map(|&(c, v)| (c, v.to_string())).collect();
    g.push((93, lines.len().to_string()));
    for &(x0, y0, x1, y1) in lines {
        g.extend([(72, "1".into()), (10, x0.to_string()), (20, y0.to_string()), (11, x1.to_string()), (21, y1.to_string())]);
    }
    g.extend([(97, "0".into()), (75, "0".into()), (76, "1".into()), (98, "0".into()), (0, "ENDSEC".into()), (0, "EOF".into())]);
    let text: String = g.iter().map(|(c, v)| format!("{c}\n{v}\n")).collect();
    let s = cadrender_dxf::read_bytes(text.as_bytes(), Fonts::new()).unwrap();
    let Kind::Hatch(h) = &s.model_block().ents[0].kind else { panic!("not a hatch") };
    h.path.clone()
}

#[test]
fn shuffled_and_reversed_edges_make_one_clean_loop() {
    // square 0..10: bottom, top (backwards), right (backwards), left
    let p = hatch_path(&[(0.0, 0.0, 10.0, 0.0), (0.0, 10.0, 10.0, 10.0), (10.0, 10.0, 10.0, 0.0), (0.0, 10.0, 0.0, 0.0)]);
    assert_eq!(p.pts, vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0], [0.0, 0.0]]);
    assert_eq!(p.verbs.iter().filter(|&&v| v == geom::M).count(), 1);
}

#[test]
fn edges_in_order_are_unchanged() {
    let p = hatch_path(&[(0.0, 0.0, 10.0, 0.0), (10.0, 0.0, 10.0, 10.0), (10.0, 10.0, 0.0, 10.0), (0.0, 10.0, 0.0, 0.0)]);
    assert_eq!(p.pts, vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0], [0.0, 0.0]]);
}
