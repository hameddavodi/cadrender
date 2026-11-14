//! Geometry tests on small hand-made DXF files (tests/data, made by gen_geometry.py):
//! entities, linetypes, transparency and viewports.

use cadrender_core::entity::{Entity, Kind};
use cadrender_core::geom::{Aff, BBox, PathData};
use cadrender_core::linetype::LtWhat;
use cadrender_core::render::batch::Batcher;
use cadrender_core::render::walk::{Inherit, Walker};
use cadrender_core::render::Prepared;
use cadrender_core::sink::{FillRule, RecordingSink};
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
fn read(name: &str) -> Scene { cadrender_dxf::read_file(data(name), Fonts::with_fallback(Face::new("boxes", Box::new(Boxes)))).expect(name) }

fn by_handle(s: &Scene, h: u64) -> Option<&Entity> { s.blocks.iter().flat_map(|b| b.ents.iter()).find(|e| e.props.handle == h) }
fn bb(e: &Entity) -> (f64, f64, f64, f64) { (e.bbox.x0, e.bbox.y0, e.bbox.x1, e.bbox.y1) }
fn segments(p: &PathData) -> usize { p.verbs.iter().filter(|&&v| v != cadrender_core::geom::M && v != cadrender_core::geom::Z).count() }
fn stroke_path(e: &Entity) -> &PathData { match &e.kind { Kind::Stroke(s) => &s.path, k => panic!("not a stroke: {}", k.name()) } }

/// Walk the model space with `m` (drawing units -> pixels) into a recording sink.
fn walk_model(s: &Scene, opts: &RenderOptions, m: Aff, only: Option<u64>) -> (cadrender_core::Stats, RecordingSink) {
    let prep = Prepared::new(s, opts);
    let view = prep.view(s, opts, None);
    let mut w = Walker {
        scene: s, opts, prep: &prep, view: &view,
        out: Batcher::new(RecordingSink::default()),
        clip: BBox::new(0.0, 0.0, 2000.0, 2000.0),
        paper_scale: 1.0, dash_scale: m.scale_factor(), paper_units_per_mm: 1.0, stats: Default::default(), warnings: Default::default(),
    };
    let ents: Vec<Entity> = s.model_block().ents.iter().filter(|e| only.is_none_or(|h| e.props.handle == h)).cloned().collect();
    w.walk(&ents, None, &m, &Inherit::top(), 0);
    let st = w.stats;
    (st, w.out.finish())
}
fn full() -> RenderOptions { RenderOptions { lod: cadrender_core::Lod::full(), ..RenderOptions::default() } }

#[test]
fn point_and_defpoints() {
    let s = read("entities.dxf");
    assert_eq!((s.header.pdmode, s.header.pdsize), (34, 2.0));
    assert!(matches!(by_handle(&s, 0x50).unwrap().kind, Kind::Point(_)));
    assert!(by_handle(&s, 0x51).is_none(), "points on Defpoints are never plotted");
    assert_eq!(s.stats.skipped.get("POINT(defpoints)"), Some(&1));
    // PDMODE 34 = plus + circle, 2 units: drawn as a symbol (2 lines + circle)
    let (st, sink) = walk_model(&s, &full(), Aff::scale(10.0, 10.0), Some(0x50));
    assert_eq!(st.drawn, 1);
    assert!(sink.draws[0].segments >= 6, "{:?}", sink.draws);
    assert!(cadrender_core::entity::point::symbol(1).is_none());
    assert!(cadrender_core::entity::point::symbol(0).is_none());
}

#[test]
fn ray_and_xline_are_cut_to_the_view() {
    let s = read("entities.dxf");
    for h in [0x52, 0x53] {
        let e = by_handle(&s, h).unwrap();
        assert!(matches!(e.kind, Kind::XLine(_)));
        // never framed: the clean-rule frame box is None
        let opts = RenderOptions::preset("preview").unwrap();
        assert!(Prepared::new(&s, &opts).frame_bbox(e).is_none());
    }
    // the view is 0..2000 px; the XLINE at x = 0 drawn with an offset is inside
    let (st, sink) = walk_model(&s, &full(), Aff::translate(100.0, 100.0), Some(0x53));
    assert_eq!(st.drawn, 1);
    assert_eq!(sink.draws.len(), 1);
    let (st, _) = walk_model(&s, &full(), Aff::translate(-100.0, 100.0), Some(0x53));
    assert_eq!(st.drawn, 0, "outside the view: culled");
}

#[test]
fn faces_and_meshes_as_wireframe() {
    let s = read("entities.dxf");
    // 3DFACE triangle (corner 4 = corner 3), edge 2 hidden: 2 edges
    let f = by_handle(&s, 0x54).unwrap();
    assert_eq!(segments(stroke_path(f)), 2);
    assert_eq!(bb(f), (0.0, 50.0, 10.0, 60.0));
    // polyface: 2 triangles; the shared diagonal is hidden in both (negative index): 4 edges
    let pf = by_handle(&s, 0x40).unwrap();
    assert_eq!(segments(stroke_path(pf)), 4, "{:?}", stroke_path(pf));
    assert_eq!(bb(pf), (100.0, 0.0, 110.0, 10.0));
    // polygon mesh 2 x 3: 2 rows of 2 segments + 3 columns of 1 segment
    let pm = by_handle(&s, 0x41).unwrap();
    assert_eq!(segments(stroke_path(pm)), 7);
    // MESH: 2 triangles, shared edge once: 5 edges
    let me = by_handle(&s, 0x42).unwrap();
    assert_eq!(segments(stroke_path(me)), 5);
    assert_eq!(bb(me), (300.0, 0.0, 310.0, 10.0));
}

#[test]
fn image_frame_and_trace() {
    let s = read("entities.dxf");
    let im = by_handle(&s, 0x55).unwrap();
    // 200 x 100 pixels of 0.1 units at (400, 0)
    let b = bb(im);
    assert!((b.0 - 400.0).abs() < 1e-6 && (b.2 - 420.0).abs() < 1e-6 && (b.3 - 10.0).abs() < 1e-6, "{b:?}");
    assert!(matches!(by_handle(&s, 0x56).unwrap().kind, Kind::Fill(_)));
}

#[test]
fn attdef_rules() {
    let s = read("entities.dxf");
    let att = &s.blocks[s.blocks.iter().position(|b| b.name == "ATT").unwrap()];
    // in the block: the line and only the constant, visible attribute ("fixed")
    let handles: Vec<u64> = att.ents.iter().map(|e| e.props.handle).collect();
    assert_eq!(handles, vec![0x30, 0x32]);
    let Kind::Text(t) = &att.ents[1].kind else { panic!() };
    assert_eq!(t.glyphs.len(), "fixed".len());
    // in model space the tag is shown
    let Kind::Text(t) = &by_handle(&s, 0x57).unwrap().kind else { panic!() };
    assert_eq!(t.glyphs.len(), "TOPTAG".len());
}

#[test]
fn complex_linetypes() {
    let s = read("linetypes.dxf");
    let gas = s.linetypes.iter().find(|l| l.name == "GAS").unwrap();
    assert!(gas.complex);
    assert_eq!(gas.parts.len(), 1);
    assert_eq!(gas.parts[0].elem, 1);
    assert!(matches!(&gas.parts[0].what, LtWhat::Text { glyphs, .. } if glyphs.len() == 3));
    let bat = s.linetypes.iter().find(|l| l.name == "ISOLAMENTO").unwrap();
    assert_eq!(bat.parts.len(), 2);
    assert!(matches!(bat.parts[1].what, LtWhat::Shape { code: 134 }));
    assert!((bat.parts[1].rot - std::f64::consts::PI).abs() < 1e-9);
    assert_eq!(s.stats.skipped.get("LTYPE_SHAPE"), Some(&2));
    assert!(s.stats.warnings.iter().any(|w| w.contains("ISOLAMENTO")));

    // GAS line, 100 units at 20 px per unit: dashes (one stroke batch) and the text (fills)
    let (st, sink) = walk_model(&s, &full(), Aff::scale(20.0, 20.0), Some(0x70));
    assert_eq!(st.drawn, 1);
    assert!(sink.draws.iter().any(|d| d.stroke_width.is_some() && !d.dashed && d.segments > 100), "{:?}", sink.draws);
    assert!(sink.draws.iter().any(|d| d.fill_rule == Some(FillRule::NonZero)));
    assert!(st.glyphs >= 3 * 100, "about 105 repeats of 3 letters: {st:?}");
    // insulation line: a wave, one lobe (8 segments) per shape, 2 shapes per 4 units (scale 10)
    let (_, sink) = walk_model(&s, &full(), Aff::scale(2.0, 2.0), Some(0x71));
    let segs: usize = sink.draws.iter().map(|d| d.segments).sum();
    assert!(segs >= 25 * 2 * 9, "{segs}");
    // too small to see: drawn as a plain solid line
    let (_, sink) = walk_model(&s, &full(), Aff::scale(0.01, 0.01), Some(0x71));
    assert_eq!(sink.draws.len(), 1);
}

#[test]
fn polyline_linetype_generation() {
    let s = read("linetypes.dxf");
    // flag 128 off and a dashed layer linetype: one sub-path per segment
    let Kind::Pline(p) = &by_handle(&s, 0x72).unwrap().kind else { panic!("expected a pline") };
    assert_eq!(p.segs.verbs.iter().filter(|&&v| v == cadrender_core::geom::M).count(), 3);
    assert!(matches!(by_handle(&s, 0x73).unwrap().kind, Kind::Stroke(_)), "flag 128: one continuous pattern");
    // drawn: the two long segments (100 px, pattern 20 + 10 px) get 4 dashes
    // each, stretched so they start and end with a dash; the short one
    // (2 px < one pattern) is solid. All as solid strokes: 2 x 4 dashes + 1 line.
    let (_, sink) = walk_model(&s, &full(), Aff::scale(2.0, 2.0), Some(0x72));
    assert_eq!(sink.draws.len(), 1, "{:?}", sink.draws);
    assert!(!sink.draws[0].dashed);
    assert_eq!(sink.draws[0].segments, (2 * 4 + 1) * 2);
}

#[test]
fn linetype_does_not_scale_with_the_block() {
    let s = read("linetypes.dxf");
    // block line 100 units inserted at 0.001: 0.1 units long. AutoCAD keeps the
    // pattern (15 units = 300 px here) in world size: the short line is one dash.
    let (_, sink) = walk_model(&s, &full(), Aff::scale(20.0, 20.0), Some(0x74));
    assert_eq!(sink.draws.len(), 1);
    assert!(sink.draws[0].dashed, "{:?}", sink.draws);
}

#[test]
fn layer_and_entity_transparency() {
    let s = read("linetypes.dxf");
    let glass = s.layers.iter().find(|l| l.name == "GLASS").unwrap();
    assert_eq!(glass.alpha, 76);
    assert_eq!(s.layers.iter().find(|l| l.name == "DASHL").unwrap().alpha, 255);
    let (_, sink) = walk_model(&s, &full(), Aff::scale(2.0, 2.0), Some(0x75));
    assert_eq!(sink.draws[0].color.a, 76);
    let (_, sink) = walk_model(&s, &full(), Aff::scale(2.0, 2.0), Some(0x76));
    assert_eq!(sink.draws[0].color.a, 127);
}

#[test]
fn viewports_clip_views_and_overrides() {
    let s = read("viewports.dxf");
    let l = &s.layouts[0];
    let vps: Vec<_> = s.viewports(l).collect();
    // the edge-on view (0x94) shows nothing of a 2D drawing and is not drawn
    assert_eq!(vps.iter().map(|v| v.handle).collect::<Vec<_>>(), vec![0x91, 0x92, 0x93]);
    // clip polygon from the L-shaped polyline
    let clipped = vps[0];
    let c = clipped.clip.as_ref().expect("clip polygon");
    assert!(c.len() >= 6, "{c:?}");
    let r = clipped.visible_rect();
    assert_eq!((r.x0, r.y0, r.x1, r.y1), (0.0, 0.0, 100.0, 100.0));
    // view from below: x is mirrored
    let below = vps[1].matrix();
    assert!(below.det() < 0.0);
    let (x, _) = below.apply(10.0, 0.0);
    assert!((x - 190.0).abs() < 1e-9, "{x}");
    // twist 90: model +x goes to paper +y
    let tw = vps[2].matrix();
    let (a, b) = (tw.apply(0.0, 0.0), tw.apply(10.0, 0.0));
    assert!((b.0 - a.0).abs() < 1e-9 && (b.1 - a.1 - 10.0).abs() < 1e-9, "{a:?} {b:?}");

    // per-viewport overrides of layer DASHL in viewport 0x93: colour 3, lineweight 0.50 mm
    let opts = RenderOptions { color: cadrender_core::ColorPolicy::TrueColor, ..RenderOptions::default() };
    let prep = Prepared::new(&s, &opts);
    let li = s.layers.iter().position(|x| x.name == "DASHL").unwrap();
    let plain = prep.view(&s, &opts, Some(vps[0])).layers[li];
    let ovr = prep.view(&s, &opts, Some(vps[2])).layers[li];
    assert_ne!(plain.color, ovr.color);
    assert_eq!(ovr.color, prep.aci[3]);
    assert_eq!((plain.lw, ovr.lw), (-3, 50));

    // the whole sheet renders, the clipped viewport gets its polygon
    let prep = Prepared::new(&s, &full());
    let r = render_sheet::<RecordingSink>(&s, "Sheet A", &full(), &prep, &()).unwrap();
    assert_eq!(r.viewports, 3);
}
