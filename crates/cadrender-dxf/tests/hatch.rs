//! HATCH tests on small hand-made DXF files (tests/data/gen_hatch.py).

use cadrender_core::color::{EntColor, Rgba};
use cadrender_core::entity::hatch::{GradientKind, HatchEnt};
use cadrender_core::entity::Kind;
use cadrender_core::geom::{self, Aff, BBox, PathData};
use cadrender_core::render::batch::Batcher;
use cadrender_core::render::walk::{Inherit, Walker};
use cadrender_core::render::{Prepared, Stats};
use cadrender_core::sink::{FillRule, RecordedDraw, RecordingSink};
use cadrender_core::text::Fonts;
use cadrender_core::{Lod, RenderOptions, Scene};

fn read(name: &str) -> Scene {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name);
    cadrender_dxf::read_file(p, Fonts::new()).expect(name)
}

fn hatches(s: &Scene) -> Vec<&HatchEnt> {
    s.model_block().ents.iter().filter_map(|e| if let Kind::Hatch(h) = &e.kind { Some(h) } else { None }).collect()
}

fn loops(p: &PathData) -> usize { p.verbs.iter().filter(|&&v| v == geom::M).count() }

/// Walk model space with matrix `m` into a recording sink.
fn render(s: &Scene, opts: &RenderOptions, m: Aff) -> (Vec<RecordedDraw>, Stats) {
    let prep = Prepared::new(s, opts);
    let view = prep.view(s, opts, None);
    let mut w = Walker {
        scene: s, opts, prep: &prep, view: &view,
        out: Batcher::new(RecordingSink::default()),
        clip: BBox::new(-100_000.0, -100_000.0, 100_000.0, 100_000.0),
        paper_scale: 1.0, dash_scale: 1.0, paper_units_per_mm: 1.0, stats: Default::default(), warnings: Default::default(),
    };
    let model = s.model_block();
    w.walk(&model.ents, None, &m, &Inherit::top(), 0);
    let stats = w.stats;
    (w.out.finish().draws, stats)
}

#[test]
fn islands_follow_the_hatch_style() {
    let s = read("hatch_islands.dxf");
    let h = hatches(&s);
    // normal: all 4 loops (even-odd); outer: the outer area only; ignore: the outer loop
    assert_eq!(h.iter().map(|h| loops(&h.path)).collect::<Vec<_>>(), vec![4, 2, 1]);
    // the loops are listed inside-out in the file: the outer pair must be kept
    let b = h[1].path.bbox();
    assert_eq!((b.x0, b.y0, b.x1, b.y1), (0.0, 0.0, 100.0, 100.0));
    for h in &h { assert!(h.solid); }
}

#[test]
fn every_edge_type() {
    let s = read("hatch_edges.dxf");
    let h = hatches(&s);
    assert_eq!(h.len(), 2);
    let b = h[0].path.bbox();
    assert!((b.x0, b.y0) == (0.0, 0.0) && (b.x1 - 12.0).abs() < 1e-9 && (b.y1 - 10.0).abs() < 1e-6, "{b:?}");
    assert_eq!(loops(&h[0].path), 1);
    let mut polys = vec![];
    geom::flatten(&h[0].path, 1e-4, &mut polys);
    let near = |x: f64, y: f64, d: f64| polys[0].iter().any(|p| (p[0] - x).hypot(p[1] - y) < d);
    // ellipse end, rational spline midpoint (on the circle around (10,8)), the bottom of the clockwise arc
    assert!(near(12.0, 2.0, 1e-9));
    assert!(near(10.0 + 2f64.sqrt(), 8.0 + 2f64.sqrt(), 1e-3));
    assert!(near(5.0, 5.0, 1e-3));
    // the reversed last line was turned: the loop goes on from (0,10) down to (0,0)
    assert_eq!(*polys[0].last().unwrap(), [0.0, 0.0]);
    // polyline loop with a bulge of 1: a half circle below the chord (20,0)-(30,0)
    let b = h[1].path.bbox();
    assert!((b.y0 + 5.0).abs() < 1e-3 && b.y1.abs() < 1e-9, "{b:?}");
}

#[test]
fn gradient_is_read_and_drawn_as_bands() {
    let s = read("hatch_gradient.dxf");
    let h = hatches(&s);
    let g = h[0].gradient.as_ref().expect("gradient");
    assert_eq!(g.kind, GradientKind::Spherical);
    assert_eq!(g.colors, [EntColor::true_color(0xFF0000), EntColor::true_color(0xFFFF00)]);
    let opts = RenderOptions::default();
    let (draws, _) = render(&s, &opts, Aff::scale(4.0, -4.0));
    let fills: Vec<Rgba> = draws.iter().filter(|d| d.fill_rule == Some(FillRule::EvenOdd)).map(|d| d.color).collect();
    assert!(fills.len() > 10, "{}", fills.len());
    // drawn from the outside in: red (colour 1) first, yellow (colour 2) at the centre last
    assert_eq!((fills[0].r, fills[0].b), (255, 0));
    assert!(fills[0].g < 30 && fills[fills.len() - 1].g > 225, "{:?} {:?}", fills[0], fills[fills.len() - 1]);
    // monochrome: grey bands
    let mono = RenderOptions::preset("preview").unwrap();
    let (draws, _) = render(&s, &mono, Aff::scale(4.0, -4.0));
    assert!(draws.iter().filter(|d| d.fill_rule.is_some()).all(|d| d.color.is_gray()));
}

#[test]
fn dense_pattern_becomes_a_tint_only_with_lod() {
    let s = read("hatch_dense.dxf");
    let lod = RenderOptions::default();
    // 1 px per unit: lines 0.1 px apart -> one tint fill, no lines for that hatch
    let (draws, st) = render(&s, &lod, Aff::scale(1.0, -1.0));
    assert!(st.hatch_dense >= 1);
    let tint = draws.iter().find(|d| d.fill_rule == Some(FillRule::EvenOdd)).expect("tint");
    assert_eq!(tint.color.a, 255, "1 px pen, 0.1 px apart: full ink");
    let full = RenderOptions { lod: Lod::full(), ..RenderOptions::default() };
    let (_, st_full) = render(&s, &full, Aff::scale(1.0, -1.0));
    assert_eq!(st_full.hatch_dense, 0);
    assert!(st_full.hatch_lines > st.hatch_lines + 900, "{} vs {}", st_full.hatch_lines, st.hatch_lines);
}

#[test]
fn dots_mirror_and_double() {
    let s = read("hatch_dense.dxf");
    let h = hatches(&s);
    // dots: 10 x 10 grid inside a 100 x 100 square
    assert_eq!(h[1].pattern[0].dashes, vec![0.0, -10.0]);
    let full = RenderOptions { lod: Lod::full(), ..RenderOptions::default() };
    let only = |i: usize| { let mut s2 = read("hatch_dense.dxf"); let e = s2.model_block().ents[i].clone(); set_only(&mut s2, e); s2 };
    let (_, st) = render(&only(1), &full, Aff::scale(1.0, -1.0));
    assert!((90..=121).contains(&st.hatch_lines), "{}", st.hatch_lines);
    // a mirrored hatch (extrusion -Z): 45 degrees becomes 135 degrees
    let a = h[2].pattern[0].angle.to_degrees().rem_euclid(360.0);
    assert!((a - 135.0).abs() < 1e-9, "{a}");
    assert!(h[2].path.bbox().x0 >= 400.0);
    // double user hatch: a second family at 90 degrees
    assert_eq!(h[3].pattern.len(), 2);
    assert!((h[3].pattern[1].angle.to_degrees() - 90.0).abs() < 1e-9);
}

/// Keep only entity `e` in model space.
fn set_only(s: &mut Scene, e: cadrender_core::entity::Entity) {
    let id = s.model as usize;
    s.blocks[id].ents = vec![e];
    s.blocks[id].file_order = None;
}
