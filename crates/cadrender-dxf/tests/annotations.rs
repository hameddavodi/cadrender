//! Annotations: MULTILEADER, MTEXT formatting, masks, dimensions, draw order.
//! Small hand-made DXFs written inline.

use cadrender_core::entity::Kind;
use cadrender_core::geom::PathData;
use cadrender_core::text::{Face, Fonts, GlyphSource};
use cadrender_core::Scene;

/// Every glyph is a 600 x 700 box, space 300.
struct Boxes;
impl GlyphSource for Boxes {
    fn cap_height(&self) -> f64 { 700.0 }
    fn space_advance(&self) -> f64 { 300.0 }
    fn glyph(&self, _: char) -> Option<(PathData, f64)> {
        let mut p = PathData::default();
        p.move_to(0.0, 0.0); p.line_to(600.0, 0.0); p.line_to(600.0, 700.0); p.close();
        Some((p, 600.0))
    }
}

fn read(groups: &[(i32, &str)]) -> Scene {
    let mut s = String::new();
    for (c, v) in groups { s.push_str(&format!("{c}\n{v}\n")); }
    cadrender_dxf::read_bytes(s.as_bytes(), Fonts::with_fallback(Face::new("boxes", Box::new(Boxes)))).unwrap()
}

/// SECTION wrapper.
fn sec<'a>(name: &'a str, body: &[(i32, &'a str)]) -> Vec<(i32, &'a str)> {
    let mut g = vec![(0, "SECTION"), (2, name)];
    g.extend_from_slice(body);
    g.push((0, "ENDSEC"));
    g
}

fn file(parts: &[Vec<(i32, &'static str)>]) -> Scene {
    let mut g: Vec<(i32, &str)> = parts.concat();
    g.push((0, "EOF"));
    read(&g)
}

/// Kinds in the block an INSERT entity points to.
fn sub_kinds(s: &Scene, k: &Kind) -> Vec<&'static str> {
    let Kind::Insert(i) = k else { panic!("not an insert: {}", k.name()) };
    s.blocks[i.block as usize].ents.iter().map(|e| e.kind.name()).collect()
}

/// A text MULTILEADER as ODA writes it: one leader line (2 vertices + connection
/// point), dogleg 8 long to the right, default arrow 1.5, frame, page-colour mask.
fn mleader_text() -> Vec<(i32, &'static str)> {
    vec![
        (0, "MULTILEADER"), (5, "AA"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbMLeader"), (270, "2"),
        (300, "CONTEXT_DATA{"), (40, "1.0"), (10, "18.0"), (20, "10.0"), (30, "0.0"), (41, "3.0"), (140, "1.5"), (145, "2.0"),
        (290, "1"), (304, "HELLO\\PWORLD"), (11, "0.0"), (21, "0.0"), (31, "1.0"), (340, "0"),
        (12, "20.0"), (22, "10.0"), (32, "0.0"), (13, "1.0"), (23, "0.0"), (33, "0.0"), (42, "0.0"), (43, "0.0"), (44, "0.0"), (45, "1.0"),
        (170, "1"), (90, "-1073741824"), (171, "4"), (172, "5"), (91, "-1056964608"), (141, "1.5"), (92, "0"), (291, "1"), (292, "1"), (296, "0"),
        (302, "LEADER{"), (290, "1"), (291, "1"), (10, "10.0"), (20, "10.0"), (30, "0.0"), (11, "1.0"), (21, "0.0"), (31, "0.0"), (90, "0"), (40, "8.0"),
        (304, "LEADER_LINE{"), (10, "0.0"), (20, "0.0"), (30, "0.0"), (10, "5.0"), (20, "5.0"), (30, "0.0"), (91, "0"), (305, "}"),
        (271, "0"), (303, "}"), (301, "}"),
        (340, "0"), (90, "0"), (170, "1"), (91, "-1056964608"), (171, "-2"), (290, "1"), (291, "1"), (41, "8.0"), (42, "1.5"),
        (172, "2"), (292, "1"), (45, "1.0"),
    ]
}

#[test]
fn mleader_with_text() {
    let s = file(&[sec("ENTITIES", &mleader_text())]);
    let m = s.model_block();
    assert_eq!(m.ents.len(), 1);
    // leader line, arrow, dogleg, frame, text
    assert_eq!(sub_kinds(&s, &m.ents[0].kind), vec!["Stroke", "Fill", "Stroke", "Stroke", "Text"]);
    let Kind::Insert(i) = &m.ents[0].kind else { unreachable!() };
    let b = &s.blocks[i.block as usize];
    // the leader runs from the arrow tip (0,0) to the connection point (10,10)
    let line = b.ents[0].bbox;
    assert!((line.x0, line.y0, line.x1, line.y1) == (0.0, 0.0, 10.0, 10.0), "{line:?}");
    // the arrow tip is at (0,0); its body (1.5 long) lies along the leader, toward (5,5)
    let a = b.ents[1].bbox;
    assert!(a.x0.abs() < 1e-9 && a.y0.abs() < 1e-9 && a.x1 > 1.0 && a.x1 < 1.3, "{a:?}");
    // dogleg: (10,10) -> (18,10)
    let d = b.ents[2].bbox;
    assert!((d.x0, d.x1, d.y0) == (10.0, 18.0, 10.0), "{d:?}");
    // text: middle-left attachment at (20,10), two lines, mask in page colour
    let Kind::Text(t) = &b.ents[4].kind else { unreachable!() };
    let mk = t.mask.as_ref().expect("mask");
    assert_eq!(mk.color, None);
    let tb = b.ents[4].bbox;
    assert!(tb.x0 < 20.0 && tb.x0 > 18.0, "{tb:?}");
    assert!(tb.y0 < 10.0 - 3.0 && tb.y1 > 10.0 + 3.0, "{tb:?}");
}

fn mtext(attach: &'static str, text: &'static str, extra: &[(i32, &'static str)]) -> cadrender_core::entity::text::TextEnt {
    let mut g = vec![(0, "MTEXT"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbMText"), (10, "0.0"), (20, "0.0"), (30, "0.0"), (40, "7.0"), (71, attach), (1, text)];
    g.extend_from_slice(extra);
    let s = file(&[sec("ENTITIES", &g)]);
    let Kind::Text(t) = &s.model_block().ents[0].kind else { panic!() };
    (**t).clone()
}

#[test]
fn mtext_mask_uses_column_width_and_border() {
    // height 7 = 700 font units at scale 0.01; width 20 -> column 2000 font units
    let t = mtext("1", "ab", &[(41, "20.0"), (90, "1"), (63, "3"), (45, "1.5")]);
    let mk = t.mask.as_ref().unwrap();
    assert_eq!(mk.color, Some(cadrender_core::color::EntColor::aci(3)));
    // border: (1.5 - 1) x text height all round
    assert_eq!((mk.rect.x0, mk.rect.x1), (-350.0, 2350.0));
    assert_eq!((mk.rect.y0, mk.rect.y1), (-350.0, 1050.0));
    // page colour when bit 2 is set
    assert_eq!(mtext("1", "ab", &[(90, "3")]).mask.unwrap().color, None);
}

#[test]
fn mtext_centred_lines_and_formatting() {
    // attachment 5 = middle centre: each line is centred on the insertion point
    let t = mtext("5", "abc\\Pa", &[]);
    let x = |i: usize| t.m.apply(t.glyphs[i].x as f64 + 300.0, 0.0).0;
    assert!(((x(0) + x(2)) / 2.0).abs() < 1e-9 && x(3).abs() < 1e-9, "{} {} {}", x(0), x(2), x(3));
    assert!(t.rich.is_none());
    // a coloured word makes runs
    let t = mtext("1", "a{\\C1;b}", &[]);
    assert_eq!(t.rich.as_ref().unwrap().runs.len(), 2);
}

fn wipeout_file(frame: &'static str) -> Scene {
    let ents = sec("ENTITIES", &[
        (0, "WIPEOUT"), (5, "1A"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbWipeout"), (90, "0"),
        (10, "-0.5"), (20, "-0.5"), (30, "0.0"), (11, "10.0"), (21, "0.0"), (31, "0.0"), (12, "0.0"), (22, "10.0"), (32, "0.0"),
        (13, "1.0"), (23, "1.0"), (70, "7"), (71, "1"), (91, "2"), (14, "-0.5"), (24, "-0.5"), (14, "0.5"), (24, "0.5"),
        (0, "LINE"), (5, "1B"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbLine"), (10, "0.0"), (20, "0.0"), (30, "0.0"), (11, "1.0"), (21, "1.0"), (31, "0.0"),
    ]);
    let objs = sec("OBJECTS", &[(0, "WIPEOUTVARIABLES"), (5, "V1"), (100, "AcDbWipeoutVariables"), (70, frame)]);
    file(&[ents, objs])
}

#[test]
fn wipeout_frame_follows_wipeoutframe() {
    let kinds = |s: &Scene| s.model_block().ents.iter().map(|e| e.kind.name()).collect::<Vec<_>>();
    // 1 = shown and plotted: the outline right after the mask, same handle
    let s = wipeout_file("1");
    assert_eq!(kinds(&s), vec!["Fill", "Frame", "Stroke"]);
    assert_eq!(s.model_block().ents[1].props.handle, 0x1A);
    assert_eq!(s.model_block().ents[1].bbox, s.model_block().ents[0].bbox);
    // 0 = off, 2 = shown on screen only: no frame in a plot
    assert_eq!(kinds(&wipeout_file("0")), vec!["Fill", "Stroke"]);
    assert_eq!(kinds(&wipeout_file("2")), vec!["Fill", "Stroke"]);
}

#[test]
fn sortents_moves_wipeout_with_its_frame() {
    // file order: wipeout (1A), line (1B); SORTENTS puts the wipeout on top (sort handle 99)
    let tables = sec("TABLES", &[
        (0, "TABLE"), (2, "BLOCK_RECORD"),
        (0, "BLOCK_RECORD"), (5, "1F"), (100, "AcDbSymbolTableRecord"), (100, "AcDbBlockTableRecord"), (2, "*Model_Space"),
        (0, "ENDTAB"),
    ]);
    let ents = sec("ENTITIES", &[
        (0, "WIPEOUT"), (5, "1A"), (330, "1F"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbWipeout"), (90, "0"),
        (10, "-0.5"), (20, "-0.5"), (30, "0.0"), (11, "10.0"), (21, "0.0"), (31, "0.0"), (12, "0.0"), (22, "10.0"), (32, "0.0"),
        (13, "1.0"), (23, "1.0"), (70, "7"), (71, "1"), (91, "2"), (14, "-0.5"), (24, "-0.5"), (14, "0.5"), (24, "0.5"),
        (0, "LINE"), (5, "1B"), (330, "1F"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbLine"), (10, "0.0"), (20, "0.0"), (30, "0.0"), (11, "1.0"), (21, "1.0"), (31, "0.0"),
    ]);
    let objs = sec("OBJECTS", &[
        (0, "SORTENTSTABLE"), (5, "50"), (100, "AcDbSortentsTable"), (330, "1F"), (331, "1A"), (5, "99"),
        (0, "WIPEOUTVARIABLES"), (5, "51"), (100, "AcDbWipeoutVariables"), (70, "1"),
    ]);
    let s = file(&[tables, ents, objs]);
    let m = s.model_block();
    let order: Vec<(&str, u64)> = m.ents.iter().map(|e| (e.kind.name(), e.props.handle)).collect();
    assert_eq!(order, vec![("Stroke", 0x1B), ("Fill", 0x1A), ("Frame", 0x1A)]);
}

#[test]
fn mleader_style_values() {
    let s = file(&[sec("OBJECTS", &[
        (0, "DICTIONARY"), (5, "D1"), (330, "0"), (100, "AcDbDictionary"), (3, "Standard"), (350, "S1"),
        (0, "MLEADERSTYLE"), (5, "S1"), (330, "D1"), (100, "AcDbMLeaderStyle"), (170, "2"), (173, "2"), (91, "-1023410175"),
        (44, "2.5"), (43, "8.0"), (45, "3.5"), (292, "1"), (142, "10.0"),
    ])]);
    let st = &s.mleader_styles[0];
    assert_eq!(st.name, "Standard");
    assert_eq!((st.content, st.leader_type, st.frame), (2, 2, true));
    assert_eq!((st.arrow_size, st.dogleg_len, st.text_height, st.scale), (2.5, 8.0, 3.5, 10.0));
    assert_eq!(st.line_color, Some(cadrender_core::color::EntColor::aci(1)));
}

#[test]
fn mleader_with_block_and_attribute() {
    let tables = sec("TABLES", &[
        (0, "TABLE"), (2, "BLOCK_RECORD"),
        (0, "BLOCK_RECORD"), (5, "B1"), (100, "AcDbSymbolTableRecord"), (100, "AcDbBlockTableRecord"), (2, "TAG"),
        (0, "BLOCK_RECORD"), (5, "B2"), (100, "AcDbSymbolTableRecord"), (100, "AcDbBlockTableRecord"), (2, "_DOT"),
        (0, "ENDTAB"),
    ]);
    let blocks = sec("BLOCKS", &[
        (0, "BLOCK"), (330, "B1"), (8, "0"), (2, "TAG"), (70, "2"), (10, "1.0"), (20, "0.0"), (30, "0.0"),
        (0, "CIRCLE"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbCircle"), (10, "1.0"), (20, "0.0"), (30, "0.0"), (40, "2.0"),
        (0, "ATTDEF"), (5, "AD"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbText"), (10, "1.0"), (20, "0.0"), (30, "0.0"), (40, "1.0"), (1, "X"),
        (72, "1"), (11, "1.0"), (21, "0.0"), (31, "0.0"), (100, "AcDbAttributeDefinition"), (3, "Tag?"), (2, "T"), (70, "0"), (74, "2"),
        (0, "ENDBLK"), (8, "0"),
        (0, "BLOCK"), (330, "B2"), (8, "0"), (2, "_DOT"), (70, "0"), (10, "0.0"), (20, "0.0"), (30, "0.0"),
        (0, "CIRCLE"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbCircle"), (10, "0.0"), (20, "0.0"), (30, "0.0"), (40, "0.5"),
        (0, "ENDBLK"), (8, "0"),
    ]);
    let ml = sec("ENTITIES", &[
        (0, "MULTILEADER"), (5, "AB"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbMLeader"), (270, "2"),
        (300, "CONTEXT_DATA{"), (40, "1.0"), (10, "10.0"), (20, "0.0"), (30, "0.0"), (41, "2.0"), (140, "2.0"), (145, "1.0"),
        (290, "0"), (296, "1"), (341, "B1"), (14, "0.0"), (24, "0.0"), (34, "1.0"), (15, "20.0"), (25, "0.0"), (35, "0.0"),
        (16, "2.0"), (26, "2.0"), (36, "2.0"), (46, "0.0"), (93, "-1056964608"),
        (302, "LEADER{"), (290, "1"), (291, "0"), (10, "10.0"), (20, "0.0"), (30, "0.0"), (11, "1.0"), (21, "0.0"), (31, "0.0"), (90, "0"), (40, "0.0"),
        (304, "LEADER_LINE{"), (10, "0.0"), (20, "0.0"), (30, "0.0"), (91, "0"), (305, "}"), (303, "}"), (301, "}"),
        (340, "0"), (170, "1"), (91, "-1056964608"), (171, "-2"), (290, "1"), (291, "0"), (342, "B2"), (42, "2.0"),
        (172, "1"), (344, "B1"), (93, "-1056964608"), (10, "2.0"), (20, "2.0"), (30, "2.0"), (43, "0.0"),
        (330, "AD"), (177, "1"), (44, "0.0"), (302, "42"), (45, "1.0"),
    ]);
    let s = file(&[tables, blocks, ml]);
    let m = s.model_block();
    assert_eq!(m.ents.len(), 1);
    // leader line, arrow block, content block, attribute text
    assert_eq!(sub_kinds(&s, &m.ents[0].kind), vec!["Stroke", "Insert", "Insert", "Text"]);
    let Kind::Insert(i) = &m.ents[0].kind else { unreachable!() };
    let b = &s.blocks[i.block as usize];
    // content: base point (1,0) of TAG lands on (20,0), scale 2 -> circle box 16..24
    let c = b.ents[2].bbox;
    assert!((c.x0 - 16.0).abs() < 1e-9 && (c.x1 - 24.0).abs() < 1e-9, "{c:?}");
    // attribute "42": centred on the circle centre (20,0), 2 high
    let Kind::Text(t) = &b.ents[3].kind else { unreachable!() };
    assert_eq!(t.glyphs.len(), 2);
    let tb = b.ents[3].bbox;
    assert!(((tb.x0 + tb.x1) / 2.0 - 20.0).abs() < 1e-6, "{tb:?}");
    assert!((tb.y1 - tb.y0) > 2.0 && (tb.y1 - tb.y0) < 4.0, "{tb:?}");
}

/// A linear dimension 0..10 with no block, style ISO (arrow 2.5, text 2.5, 1 decimal, comma, DIMTFILL 1).
fn dim_without_block(xdata: &[(i32, &'static str)]) -> Scene {
    let tables = sec("TABLES", &[
        (0, "TABLE"), (2, "DIMSTYLE"),
        (0, "DIMSTYLE"), (105, "D1"), (100, "AcDbSymbolTableRecord"), (100, "AcDbDimStyleTableRecord"), (2, "ISO"),
        (40, "1.0"), (41, "2.5"), (42, "0.5"), (44, "1.0"), (140, "2.5"), (271, "1"), (278, "44"), (69, "1"),
        (0, "ENDTAB"),
    ]);
    let mut d = vec![
        (0, "DIMENSION"), (5, "2D"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbDimension"), (2, ""), (3, "ISO"),
        (10, "10.0"), (20, "5.0"), (30, "0.0"), (11, "5.0"), (21, "6.0"), (31, "0.0"), (70, "0"), (1, ""),
        (100, "AcDbAlignedDimension"), (13, "0.0"), (23, "0.0"), (33, "0.0"), (14, "10.0"), (24, "0.0"), (34, "0.0"),
        (100, "AcDbRotatedDimension"), (50, "0.0"),
    ];
    d.extend_from_slice(xdata);
    file(&[tables, sec("ENTITIES", &d)])
}

#[test]
fn dimension_without_block_is_generated_from_dimstyle() {
    let s = dim_without_block(&[]);
    let k = &s.model_block().ents[0].kind;
    // two extension lines, dimension line, two arrows, text
    assert_eq!(sub_kinds(&s, k), vec!["Stroke", "Stroke", "Stroke", "Fill", "Fill", "Text"]);
    let Kind::Insert(i) = k else { unreachable!() };
    let b = &s.blocks[i.block as usize];
    // extension line 1: from DIMEXO above the origin to DIMEXE past the dimension line
    let e = b.ents[0].bbox;
    assert!((e.y0 - 0.5).abs() < 1e-9 && (e.y1 - 6.0).abs() < 1e-9, "{e:?}");
    let dl = b.ents[2].bbox;
    assert_eq!((dl.x0, dl.x1, dl.y0), (0.0, 10.0, 5.0));
    // text "10,0" (1 decimal, comma) with a page-colour mask (DIMTFILL 1), centred on (5,6)
    let Kind::Text(t) = &b.ents[5].kind else { unreachable!() };
    assert_eq!(t.glyphs.len(), 4);
    assert_eq!(t.mask.as_ref().unwrap().color, None);
    let tb = b.ents[5].bbox;
    assert!(((tb.x0 + tb.x1) / 2.0 - 5.0).abs() < 1e-6, "{tb:?}");
    // XDATA override DIMTSZ = 1: ticks instead of arrows
    let s = dim_without_block(&[(1001, "ACAD"), (1000, "DSTYLE"), (1002, "{"), (1070, "142"), (1040, "1.0"), (1002, "}")]);
    assert_eq!(sub_kinds(&s, &s.model_block().ents[0].kind), vec!["Stroke", "Stroke", "Stroke", "Stroke", "Stroke", "Text"]);
}
