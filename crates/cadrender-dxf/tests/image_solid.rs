//! IMAGE, REGION / 3DSOLID / BODY outlines and the WIPEOUT frame option.
//! Small hand-made DXFs written inline.

use cadrender_core::entity::{Entity, Kind};
use cadrender_core::geom::{Aff, BBox, PathData};
use cadrender_core::render::batch::Batcher;
use cadrender_core::render::walk::{Inherit, Walker};
use cadrender_core::render::Prepared;
use cadrender_core::sink::RecordingSink;
use cadrender_core::text::{Face, Fonts, GlyphSource};
use cadrender_core::{RenderOptions, Scene};

struct NoGlyphs;
impl GlyphSource for NoGlyphs {
    fn cap_height(&self) -> f64 { 700.0 }
    fn space_advance(&self) -> f64 { 300.0 }
    fn glyph(&self, _: char) -> Option<(PathData, f64)> { None }
}
fn fonts() -> Fonts { Fonts::with_fallback(Face::new("none", Box::new(NoGlyphs))) }

fn text(groups: &[(i32, &str)]) -> String {
    let mut s = String::new();
    for (c, v) in groups { s.push_str(&format!("{c}\n{v}\n")); }
    s
}
fn sec<'a>(name: &'a str, body: &[(i32, &'a str)]) -> Vec<(i32, &'a str)> {
    let mut g = vec![(0, "SECTION"), (2, name)];
    g.extend_from_slice(body);
    g.push((0, "ENDSEC"));
    g
}
fn file(parts: &[Vec<(i32, &str)>]) -> Scene {
    let mut g: Vec<(i32, &str)> = parts.concat();
    g.push((0, "EOF"));
    cadrender_dxf::read_bytes(text(&g).as_bytes(), fonts()).unwrap()
}

/// Walk the model space with `m` into a recording sink.
fn walk_model(s: &Scene, opts: &RenderOptions, m: Aff) -> (cadrender_core::Stats, RecordingSink) {
    let prep = Prepared::new(s, opts);
    let view = prep.view(s, opts, None);
    let mut w = Walker {
        scene: s, opts, prep: &prep, view: &view,
        out: Batcher::new(RecordingSink::default()),
        clip: BBox::new(0.0, 0.0, 2000.0, 2000.0),
        paper_scale: 1.0, dash_scale: m.scale_factor(), paper_units_per_mm: 1.0, stats: Default::default(), warnings: Default::default(),
    };
    let ents: Vec<Entity> = s.model_block().ents.clone();
    w.walk(&ents, None, &m, &Inherit::top(), 0);
    let st = w.stats;
    (st, w.out.finish())
}

fn wipeout_scene() -> Scene {
    let ents = sec("ENTITIES", &[
        (0, "WIPEOUT"), (5, "1A"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbWipeout"), (90, "0"),
        (10, "-0.5"), (20, "-0.5"), (30, "0.0"), (11, "10.0"), (21, "0.0"), (31, "0.0"), (12, "0.0"), (22, "10.0"), (32, "0.0"),
        (13, "1.0"), (23, "1.0"), (70, "7"), (71, "1"), (91, "2"), (14, "-0.5"), (24, "-0.5"), (14, "0.5"), (24, "0.5"),
    ]);
    let objs = sec("OBJECTS", &[(0, "WIPEOUTVARIABLES"), (5, "V1"), (100, "AcDbWipeoutVariables"), (70, "1")]);
    file(&[ents, objs])
}

#[test]
fn wipeout_frames_option_hides_frames() {
    let s = wipeout_scene();
    assert!(matches!(s.model_block().ents[1].kind, Kind::Frame(_)));
    let m = Aff::scale(10.0, 10.0);
    // default: as the file says (frame on): one fill + one stroke
    let (_, sink) = walk_model(&s, &RenderOptions::default(), m);
    let d = sink.all_draws();
    assert_eq!(d.iter().filter(|d| d.fill_rule.is_some()).count(), 1);
    assert_eq!(d.iter().filter(|d| d.stroke_width.is_some()).count(), 1);
    // off: only the mask
    let off = RenderOptions { wipeout_frames: false, ..RenderOptions::default() };
    let (_, sink) = walk_model(&s, &off, m);
    let d = sink.all_draws();
    assert_eq!(d.iter().filter(|d| d.fill_rule.is_some()).count(), 1);
    assert_eq!(d.iter().filter(|d| d.stroke_width.is_some()).count(), 0);
}

fn cipher(s: &str) -> String {
    // AutoCAD's SAT cipher: 159 - c (space kept); '^' is written "^ " in DXF
    let mut o = String::new();
    for c in s.chars() {
        if c == ' ' { o.push(' '); continue; }
        let e = (159 - c as u8) as char;
        o.push(e);
        if e == '^' { o.push(' '); }
    }
    o
}

fn acis_data(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../cadrender-acis/tests/data").join(name)).unwrap()
}

fn stroke_bbox(s: &Scene, h: u64) -> BBox {
    let e = s.model_block().ents.iter().find(|e| e.props.handle == h).expect("entity");
    match &e.kind { Kind::Stroke(st) => st.path.bbox(), k => panic!("not a stroke: {}", k.name()) }
}

#[test]
fn region_with_sat_text() {
    let sat = String::from_utf8(acis_data("cube.sat")).unwrap();
    let lines: Vec<String> = sat.lines().map(cipher).collect();
    let mut g: Vec<(i32, &str)> = vec![(0, "REGION"), (5, "2A"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbModelerGeometry"), (70, "1")];
    for l in &lines {
        // long lines go on in group 3
        let (a, b) = l.split_at(l.len().min(100));
        g.push((1, a));
        if !b.is_empty() { g.push((3, b)); }
    }
    let s = file(&[sec("ENTITIES", &g)]);
    let b = stroke_bbox(&s, 0x2A);
    assert!((b.x0 - 9.0).abs() < 1e-9 && (b.y1 - 21.0).abs() < 1e-9, "{b:?}");
}

#[test]
fn solid_with_sab_in_acdsdata() {
    let hex: String = acis_data("cube.sab").iter().map(|b| format!("{b:02X}")).collect();
    let chunks: Vec<&str> = (0..hex.len()).step_by(254).map(|i| &hex[i..(i + 254).min(hex.len())]).collect();
    let ents = sec("ENTITIES", &[
        (0, "3DSOLID"), (5, "2B"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbModelerGeometry"), (290, "1"), (2, "{00000000-0000-0000-0000-000000000000}"),
        (0, "REGION"), (5, "2C"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbModelerGeometry"), (290, "1"),
    ]);
    let mut acds: Vec<(i32, &str)> = vec![(0, "ACDSRECORD"), (90, "1"), (2, "AcDbDs::ID"), (280, "10"), (320, "2B"), (2, "ASM_Data"), (280, "15"), (94, "1")];
    for c in &chunks { acds.push((310, c)); }
    let s = file(&[ents, sec("ACDSDATA", &acds)]);
    let b = stroke_bbox(&s, 0x2B);
    assert!((b.x0 - 9.0).abs() < 1e-9 && (b.y1 - 21.0).abs() < 1e-9, "{b:?}");
    // the region without data is left out and counted
    assert!(s.model_block().ents.iter().all(|e| e.props.handle != 0x2C));
    assert_eq!(s.stats.skipped.get("ACIS(no data)"), Some(&1));
}
