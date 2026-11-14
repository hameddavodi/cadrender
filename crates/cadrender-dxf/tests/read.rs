//! Reader tests on small hand-made DXF files (tests/data, made by gen.py).

use cadrender_core::entity::Kind;
use cadrender_core::geom::PathData;
use cadrender_core::render::Prepared;
use cadrender_core::sink::RecordingSink;
use cadrender_core::text::{Face, Fonts, GlyphSource};
use cadrender_core::{render_sheet, RenderOptions, Scene};

fn data(name: &str) -> std::path::PathBuf { std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name) }

/// A font where every glyph is a box: tests do not depend on system fonts.
struct Boxes;
impl GlyphSource for Boxes {
    fn cap_height(&self) -> f64 { 700.0 }
    fn space_advance(&self) -> f64 { 300.0 }
    fn glyph(&self, _: char) -> Option<(PathData, f64)> {
        let mut p = PathData::default();
        p.move_to(0.0, 0.0); p.line_to(500.0, 0.0); p.line_to(500.0, 700.0); p.line_to(0.0, 700.0); p.close();
        Some((p, 600.0))
    }
}
fn fonts() -> Fonts { Fonts::with_fallback(Face::new("boxes", Box::new(Boxes))) }
fn read(name: &str) -> Scene { cadrender_dxf::read_file(data(name), fonts()).expect(name) }

fn bbox(s: &Scene) -> (f64, f64, f64, f64) { let b = s.model_block().bbox; (b.x0, b.y0, b.x1, b.y1) }
fn near(a: (f64, f64, f64, f64), b: (f64, f64, f64, f64)) -> bool { (a.0 - b.0).abs() + (a.1 - b.1).abs() + (a.2 - b.2).abs() + (a.3 - b.3).abs() < 1e-6 }

#[test]
fn line() {
    let s = read("line.dxf");
    assert_eq!(s.model_block().ents.len(), 1);
    assert_eq!(s.header.version, "AC1032");
    assert!(near(bbox(&s), (0.0, 0.0, 100.0, 50.0)));
    assert!(s.layouts.is_empty());
}

#[test]
fn block_insert_uses_base_point_and_scale() {
    let s = read("block_insert.dxf");
    // block line (1,1)-(2,1), base (1,1), inserted at (10,10) scale 2 -> (10,10)-(12,10)
    assert!(near(bbox(&s), (10.0, 10.0, 12.0, 10.0)), "{:?}", bbox(&s));
}

#[test]
fn nested_insert_and_minsert() {
    let s = read("nested_insert.dxf");
    let m = s.model_block();
    assert_eq!(m.depth, 2);
    // OUTER rotated 90 degrees: lines at x=0, y = 5..6 and 10..11; MINSERT 3 x 2 at (100,0): x 100..121, y 0..20
    assert_eq!(m.expanded, 2 + 6);
    assert!(near(bbox(&s), (0.0, 0.0, 121.0, 20.0)), "{:?}", bbox(&s));
}

#[test]
fn viewport_layout_and_frozen_layer() {
    for file in ["viewport.dxf", "viewport_libredwg.dxf"] {
        let s = read(file);
        assert_eq!(s.sheet_names(), vec!["Sheet A"], "{file}");
        let l = &s.layouts[0];
        assert_eq!(l.viewports.len(), 1, "{file}: the paper viewport is not drawn");
        let vp = s.viewports(l).next().unwrap();
        assert_eq!(vp.handle, 0x201);
        let hidden = s.layers.iter().position(|x| x.name == "HIDDEN").unwrap() as u32;
        assert_eq!(vp.frozen_layers, vec![hidden], "{file}");
        // linetype table and layer linetype
        assert_eq!(s.linetypes[0].pattern, vec![10.0, -5.0]);
    }
}

fn draws(s: &Scene, opts: &RenderOptions) -> (cadrender_core::Stats, RecordingSink) {
    let prep = Prepared::new(s, opts);
    let r = render_sheet::<RecordingSink>(s, "Sheet A", opts, &prep, &()).unwrap();
    (r.stats, r.sink)
}

#[test]
fn render_viewport_respects_freeze_and_clean_rules() {
    let s = read("viewport.dxf");
    let preview = RenderOptions::preset("preview").unwrap();
    let (st, sink) = draws(&s, &preview);
    // paper: 4 frame lines (title-block line dropped); viewport: WALLS line (HIDDEN frozen)
    assert_eq!(st.drawn, 5, "{st:?}");
    assert_eq!(sink.regions.len(), 1);
    let none = RenderOptions::preset("none").unwrap();
    let (st, _) = draws(&s, &none);
    assert_eq!(st.drawn, 6, "without clean rules the title-block line is drawn");
    // framing: page is 2000 px wide, the sheet is 297 x 210 plus margins
    let prep = Prepared::new(&s, &preview);
    let r = render_sheet::<RecordingSink>(&s, "Sheet A", &preview, &prep, &()).unwrap();
    assert_eq!(r.width, 2000);
    assert_eq!(r.height, (2000.0f64 * 210.0 * 1.1 / (297.0 * 1.1)).ceil() as u32);
    assert!(render_sheet::<RecordingSink>(&s, "nope", &preview, &prep, &()).is_err());
}

#[test]
fn hatch_solid_pattern_and_hole() {
    let s = read("hatch.dxf");
    let hs: Vec<_> = s.model_block().ents.iter().filter_map(|e| if let Kind::Hatch(h) = &e.kind { Some(h) } else { None }).collect();
    assert_eq!(hs.len(), 3);
    assert!(hs[0].solid);
    assert_eq!(hs[1].pattern.len(), 1);
    assert_eq!(hs[2].pattern[0].dashes, vec![0.5, -0.25]);
    // render the model through a fake layout: use the core walk directly
    let opts = RenderOptions { lod: cadrender_core::Lod::full(), ..RenderOptions::default() };
    let prep = Prepared::new(&s, &opts);
    let view = prep.view(&s, &opts, None);
    let mut w = cadrender_core::render::walk::Walker {
        scene: &s, opts: &opts, prep: &prep, view: &view,
        out: cadrender_core::render::batch::Batcher::new(RecordingSink::default()),
        clip: cadrender_core::geom::BBox::new(0.0, 0.0, 1000.0, 1000.0),
        paper_scale: 10.0, dash_scale: 10.0, paper_units_per_mm: 1.0, stats: Default::default(), warnings: Default::default(),
    };
    let m = cadrender_core::geom::Aff::scale(10.0, 10.0);
    let model = s.model_block();
    w.walk(&model.ents, None, &m, &cadrender_core::render::walk::Inherit::top(), 0);
    // 45-degree lines 2 units apart across a 10 x 10 square: about 10 lines
    assert!(w.stats.hatch_lines >= 8, "{:?}", w.stats);
    let sink = w.out.finish();
    assert!(sink.draws.iter().any(|d| d.fill_rule == Some(cadrender_core::sink::FillRule::EvenOdd)));
}

#[test]
fn libredwg_quirks() {
    let s = read("libredwg_quirks.dxf");
    let ents = &s.model_block().ents;
    let ins = ents.iter().find_map(|e| if let Kind::Insert(i) = &e.kind { Some(i) } else { None }).unwrap();
    // the junk "70 1" after AcDbXrecord does not hide the first ATTRIB; the second is really invisible
    assert_eq!(ins.attribs.len(), 1);
    let mt = ents.iter().find_map(|e| if let Kind::Text(t) = &e.kind { Some(t) } else { None }).unwrap();
    let mask = mt.mask.as_ref().expect("background mask");
    assert_eq!(mask.color, None, "page colour mask");
    // garbage fill scale -> default 1.5 -> margin 0.5 cap heights (700 font units)
    assert!((mask.rect.y1 - 700.0 * 1.5).abs() < 1e-6, "{:?}", mask.rect);
}

#[test]
fn not_dxf_and_binary() {
    assert!(cadrender_dxf::read_bytes(b"AutoCAD Binary DXF\r\n\x1a\0", Fonts::new()).is_err());
    assert!(cadrender_dxf::read_bytes(&[0x89u8; 10_000], Fonts::new()).is_err());
    // empty but valid
    let s = cadrender_dxf::read_bytes(b"  0\nSECTION\n  2\nENTITIES\n  0\nENDSEC\n  0\nEOF\n", Fonts::new()).unwrap();
    assert_eq!(s.model_block().ents.len(), 0);
}

#[test]
fn text_without_fonts_is_skipped_with_a_warning() {
    let s = cadrender_dxf::read_file(data("libredwg_quirks.dxf"), Fonts::new()).unwrap();
    assert!(s.stats.warnings.iter().any(|w| w.contains("no font")));
}

#[test]
fn layout_paper_setup_and_paper_framing() {
    let s = read("viewport.dxf");
    let p = &s.layouts[0].paper;
    assert_eq!(p.psltscale, Some(true));
    let l = p.limits.expect("limits");
    assert_eq!((l.x0, l.y0, l.x1, l.y1), (0.0, 0.0, 297.0, 210.0));
    let opts = RenderOptions { framing: cadrender_core::Framing::Paper, ..RenderOptions::default() };
    let prep = Prepared::new(&s, &opts);
    let r = render_sheet::<RecordingSink>(&s, "Sheet A", &opts, &prep, &()).unwrap();
    assert_eq!(r.height, (2000.0f64 * 210.0 / 297.0).ceil() as u32);
}

#[test]
fn mtext_stops_at_embedded_object() {
    let s = read("oda_mtext.dxf");
    let t = s.model_block().ents.iter().find_map(|e| if let Kind::Text(t) = &e.kind { Some(t) } else { None }).unwrap();
    // no rotation: the 11/21 after 101 must not become the direction
    assert!(t.m.0[1].abs() < 1e-12, "{:?}", t.m);
}

#[test]
fn text_with_the_builtin_liberation_font() {
    let fonts = cadrender_core::text::load_fonts(&cadrender_core::text::FontChoice::Liberation).unwrap();
    let s = cadrender_dxf::read_file(data("oda_mtext.dxf"), fonts).unwrap();
    let t = s.model_block().ents.iter().find_map(|e| if let Kind::Text(t) = &e.kind { Some(t) } else { None }).unwrap();
    assert_eq!(t.glyphs.len(), "Legend".len());
    assert_eq!(s.fonts.faces[0].name, "liberationsans-regular.ttf");
}
