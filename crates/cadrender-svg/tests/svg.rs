//! SVG output: structure tests on the sink, and a golden file for a small drawing.
//!
//! Update the golden file after an intended change with
//! `UPDATE_GOLDEN=1 cargo test -p cadrender-svg`.

use cadrender_core::color::Rgba;
use cadrender_core::geom::Aff;
use cadrender_core::sink::{FillRule, GradientPaint, GradientShape, IRect, Paint, PaintKind, Sink, TextRun};
use cadrender_core::text::layout::layout;
use cadrender_core::text::{load_fonts, FontChoice};
use cadrender_core::{render_sheet, Lod, Prepared, RenderOptions};
use cadrender_svg::{auto_precision, SvgConfig, SvgSink, TextMode};

fn data(name: &str) -> std::path::PathBuf { std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name) }

const BLACK_1PX: Paint<'static> = Paint { color: Rgba::BLACK, kind: PaintKind::Stroke { width: 1.0, dash: None } };

fn s(v: Vec<u8>) -> String { String::from_utf8(v).expect("SVG is UTF-8") }

#[test]
fn same_style_shares_one_class_rule() {
    let mut k = SvgSink::page(&SvgConfig { precision: Some(1), ..Default::default() }, 100, 50, Rgba::WHITE);
    k.move_to(0.0, 0.0); k.line_to(10.0, 0.0); k.draw(&BLACK_1PX);
    k.move_to(0.0, 5.0); k.line_to(10.0, 5.0); k.draw(&BLACK_1PX);
    k.move_to(1.0, 1.0); k.line_to(2.0, 1.0); k.line_to(2.0, 2.0); k.close();
    k.draw(&Paint { color: Rgba::gray(128), kind: PaintKind::Fill { rule: FillRule::EvenOdd } });
    let out = s(k.finish());
    assert!(out.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"50\" viewBox=\"0 0 100 50\""));
    assert_eq!(out.matches(".s000000-16{").count(), 1, "{out}");
    assert_eq!(out.matches("class=\"s000000-16\"").count(), 2);
    assert!(out.contains(".e808080{fill:#808080;fill-rule:evenodd}"));
    assert!(out.contains("<path class=\"e808080\" d=\"M1 1h1v1z\"/>"));
    assert!(out.contains("<rect width=\"100%\" height=\"100%\" fill=\"#fff\"/>"));
    assert!(out.ends_with("</svg>\n"));
}

#[test]
fn moves_alone_and_transparent_paint_write_nothing() {
    let mut k = SvgSink::page(&SvgConfig::default(), 10, 10, Rgba::TRANSPARENT);
    k.move_to(1.0, 1.0); k.draw(&BLACK_1PX);
    k.move_to(1.0, 1.0); k.line_to(5.0, 5.0); k.draw(&Paint { color: Rgba::TRANSPARENT, ..BLACK_1PX });
    assert!(k.body().is_empty());
    assert!(!s(k.finish()).contains("<rect"), "transparent page has no background rect");
}

#[test]
fn dashes_are_written_in_pixels() {
    let dash = [6.0f32, 3.0];
    let mut k = SvgSink::page(&SvgConfig::default(), 10, 10, Rgba::WHITE);
    k.move_to(0.0, 0.0); k.line_to(9.0, 0.0);
    k.draw(&Paint { color: Rgba::BLACK, kind: PaintKind::Stroke { width: 2.0, dash: Some(&dash) } });
    assert!(s(k.body().to_vec()).contains("<path class=\"s000000-32\" stroke-dasharray=\"6 3\" d="));
}

#[test]
fn regions_become_nested_svg_and_clip_paths_get_unique_ids() {
    let cfg = SvgConfig { precision: Some(0), ..Default::default() };
    let mut page = SvgSink::page(&cfg, 200, 100, Rgba::WHITE);
    let rect = IRect { x: 10, y: 20, w: 50, h: 40 };
    let mut a = page.region(rect);
    a.move_to(0.0, 0.0); a.line_to(5.0, 5.0);
    a.draw(&Paint { color: Rgba::rgb(0xff0000), kind: PaintKind::Stroke { width: 3.0, dash: None } });
    page.composite(a, rect, None);
    page.composite(page.region(rect), rect, None); // empty: nothing written
    let poly = [[0.0f32, 0.0], [50.0, 0.0], [25.0, 40.0]];
    for _ in 0..2 {
        let mut b = page.region(rect);
        b.move_to(0.0, 0.0); b.line_to(9.0, 9.0); b.draw(&BLACK_1PX);
        page.composite(b, rect, Some(&poly));
    }
    let out = s(page.finish());
    assert_eq!(out.matches("<svg x=\"10\" y=\"20\" width=\"50\" height=\"40\">").count(), 3);
    assert!(out.contains(".sff0000-48{"), "styles of a region reach the page");
    assert!(out.contains("<clipPath id=\"c0\"><path d=\"M0 0h50l-25 40z\"/></clipPath>\n<g clip-path=\"url(#c0)\">"));
    assert!(out.contains("id=\"c1\""));
}

#[test]
fn text_runs_become_text_elements() {
    let mut fonts = load_fonts(&FontChoice::Liberation).unwrap();
    let face = &mut fonts.faces[0];
    let laid = layout(face, &["a<b".to_string(), "x  y".to_string()], 0.0, 5.0 / 3.0);
    let face = &fonts.faces[0];
    let px = 10.0 / face.cap_height; // 10 px cap height
    let run = TextRun { face, glyphs: &laid.glyphs, m: Aff([px, 0.0, 0.0, -px, 5.0, 50.0]), color: Rgba::BLACK };
    let mut k = SvgSink::page(&SvgConfig { precision: Some(1), ..Default::default() }, 100, 100, Rgba::WHITE);
    assert!(k.text(&run));
    let body = s(k.body().to_vec());
    assert_eq!(body.matches("<text ").count(), 2, "one element per line: {body}");
    assert!(body.contains("<text class=\"f000000\" x=\"5\" y=\"50\" font-size=\"13.96\""), "{body}");
    assert!(body.contains(">a&lt;b</text>"));
    assert!(body.contains(">x  y</text>"), "two spaces kept: {body}");
    assert!(s(k.finish()).contains("text{font-family:Arial,\"Liberation Sans\",sans-serif"));

    // rotated text gets a matrix
    let r = Aff::rotate(0.5);
    let run = TextRun { m: Aff([px, 0.0, 0.0, -px, 5.0, 50.0]).mul(&r), ..run };
    let mut k = SvgSink::page(&SvgConfig { precision: Some(1), ..Default::default() }, 100, 100, Rgba::WHITE);
    assert!(k.text(&run));
    assert!(s(k.body().to_vec()).contains("transform=\"matrix(.8776 -.4794 .4794 .8776 5 50)\""));

    // a gap that is not whole spaces (a tab stop) starts a positioned piece
    let mut glyphs = laid.glyphs.clone();
    for g in glyphs.iter_mut().skip(1).take(2) { g.x += 1500.0; } // 2.6 spaces
    let run2 = TextRun { glyphs: &glyphs[..3], m: Aff([px, 0.0, 0.0, -px, 5.0, 50.0]), ..run };
    let mut k = SvgSink::page(&SvgConfig { precision: Some(1), ..Default::default() }, 100, 100, Rgba::WHITE);
    assert!(k.text(&run2));
    let body = s(k.body().to_vec());
    assert!(body.contains("><tspan x=\"5\">a</tspan><tspan x=\""), "{body}");
    assert!(body.contains("&lt;b</tspan></text>"), "{body}");

    // glyph mode: the sink says no, so the walk sends outlines
    let mut k = SvgSink::page(&SvgConfig { text: TextMode::Paths, ..Default::default() }, 100, 100, Rgba::WHITE);
    assert!(!k.text(&run));
    assert!(k.body().is_empty());
}

#[test]
fn precision_follows_size_and_lod_is_weaker() {
    assert_eq!(auto_precision(2000, 1000), 2);
    assert_eq!(auto_precision(600, 400), 3);
    assert_eq!(auto_precision(20000, 100), 1);
    let o = RenderOptions::default();
    let svg = SvgConfig::default().render_options(&o);
    assert!(svg.lod.enabled);
    assert_eq!(svg.lod.text_greek_px, o.lod.text_greek_px / 4.0);
    assert_eq!(svg.lod.dot_px, o.lod.dot_px / 4.0);
    let full = SvgConfig { full_detail: true, ..Default::default() }.render_options(&o);
    assert!(!full.lod.enabled);
    let same = SvgConfig { lod_zoom: 1.0, ..Default::default() }.render_options(&RenderOptions { lod: Lod::default(), ..o });
    assert_eq!(same.lod, Lod::default());
}

#[test]
fn golden_small_drawing() {
    let fonts = load_fonts(&FontChoice::Liberation).unwrap();
    let scene = cadrender_dxf::read_file(data("svg_golden.dxf"), fonts).unwrap();
    let mut opts = RenderOptions::preset("none").unwrap();
    opts.width = 600;
    opts.parallel_viewports = false;
    let cfg = SvgConfig::default();
    let o = cfg.render_options(&opts);
    let prep = Prepared::new(&scene, &o);
    let r = render_sheet::<SvgSink>(&scene, "Sheet A", &o, &prep, &cfg).unwrap();
    let got = s(r.sink.finish());
    let path = data("svg_golden.svg");
    if std::env::var_os("UPDATE_GOLDEN").is_some() { std::fs::write(&path, &got).unwrap(); }
    let want = std::fs::read_to_string(&path).expect("golden file; make it with UPDATE_GOLDEN=1");
    assert_eq!(got, want, "SVG changed; check it, then UPDATE_GOLDEN=1 cargo test -p cadrender-svg");
    // what the golden file must show
    for part in ["<svg x=", "stroke-dasharray=", ">A&amp;B &lt;1&gt;</text>", "transform=\"matrix(", ">Model</text>", "fill-rule:evenodd"] {
        assert!(got.contains(part), "missing {part}");
    }
}

/// Render "Sheet R" of `svg_rich.dxf` (formatted MTEXT and gradient hatches) to SVG.
fn render_rich(cfg: &SvgConfig) -> String {
    let fonts = load_fonts(&FontChoice::Liberation).unwrap();
    let scene = cadrender_dxf::read_file(data("svg_rich.dxf"), fonts).unwrap();
    let mut opts = RenderOptions::preset("none").unwrap();
    opts.width = 900;
    opts.color = cadrender_core::ColorPolicy::TrueColor;
    opts.parallel_viewports = false;
    let o = cfg.render_options(&opts);
    let prep = Prepared::new(&scene, &o);
    s(render_sheet::<SvgSink>(&scene, "Sheet R", &o, &prep, cfg).unwrap().sink.finish())
}

#[test]
fn formatted_mtext_becomes_text_elements() {
    let out = render_rich(&SvgConfig::default());
    // runs that differ only in colour or weight share one <text> per line:
    // a <tspan> per run, the space before a run kept for copy and paste
    let line = between(&out, "<text class=\"f000000\" x=\"40.909\" y=\"96.489\"", "</text>");
    assert!(line.contains(">Plain</tspan>"), "{line}");
    assert!(line.contains(" class=\"fff0000\" ") && line.contains("> red</tspan>"), "{line}");
    assert!(line.contains(" font-weight=\"bold\" ") && line.contains("> bold</tspan>"), "{line}");
    assert!(out.contains(">A &amp; B &lt;tag&gt;</tspan>"));
    assert!(out.contains(" class=\"f0000ff\" ") && out.contains("> blue</tspan>"));
    // width factor and oblique go into the matrix of their run
    assert!(out.contains("transform=\"matrix(.5 0 0 "), "narrow run: {out}");
    assert!(out.contains(">narrow</text>") && out.contains(">slant</text>") && out.contains(">ital</text>"));
    // stacked fractions: top and bottom are separate, smaller texts
    for part in [">1</text>", ">2</text>", ">3</text>", ">/</text>", ">4</text>", ">+0.1</text>", ">-0.2</text>"] {
        assert!(out.contains(part), "missing {part}");
    }
    // the only non-zero fills left are the line boxes (underline + overline, one fraction bar)
    assert_eq!(out.matches("<path class=\"f").count(), 2, "glyph outlines left: {out}");
    // glyph mode still writes outlines
    let paths = render_rich(&SvgConfig { text: TextMode::Paths, ..Default::default() });
    assert!(!paths.contains("<text"));
    assert!(paths.matches("<path class=\"f").count() > 2);
}

#[test]
fn golden_rich_drawing() {
    let got = render_rich(&SvgConfig::default());
    let path = data("svg_rich.svg");
    if std::env::var_os("UPDATE_GOLDEN").is_some() { std::fs::write(&path, &got).unwrap(); }
    let want = std::fs::read_to_string(&path).expect("golden file; make it with UPDATE_GOLDEN=1");
    assert_eq!(got, want, "SVG changed; check it, then UPDATE_GOLDEN=1 cargo test -p cadrender-svg");
}

/// The text between `start` and the next `end` after it.
fn between<'a>(s: &'a str, start: &str, end: &str) -> &'a str {
    let i = s.find(start).unwrap_or_else(|| panic!("no {start}")) + start.len();
    &s[i..i + s[i..].find(end).unwrap()]
}

#[test]
fn hatch_gradients_become_svg_gradients() {
    let out = render_rich(&SvgConfig::default());
    // 9 hatches: LINEAR, CYLINDER, INVCYLINDER, CURVED, INVCURVED are linear; the 4 spheres radial
    assert_eq!(out.matches("<linearGradient ").count(), 5, "{out}");
    assert_eq!(out.matches("<radialGradient ").count(), 4);
    assert_eq!(out.matches("<path fill=\"url(#g").count(), 9);
    assert!(!out.contains("class=\"e"), "no colour bands left");
    // ids are unique
    for i in 0..9 { assert_eq!(out.matches(&format!("id=\"g{i}\"")).count(), 1); }
    // LINEAR: blue -> grey half way -> yellow, along x (angle 0)
    let lin = between(&out, "<linearGradient id=\"g0\"", "</linearGradient>");
    assert!(lin.contains("y1=\"0\"") && lin.contains("y2=\"0\""), "{lin}");
    assert!(lin.contains("<stop offset=\"0\" stop-color=\"#00f\"/><stop offset=\".5\" stop-color=\"#808080\"/><stop offset=\"1\" stop-color=\"#ff0\"/>"), "{lin}");
    // CYLINDER at 45 degrees: the axis goes right and up (device y down), light in the middle
    let cyl = between(&out, "<linearGradient id=\"g1\"", "</linearGradient>");
    let n: Vec<f64> = ["x1", "y1", "x2", "y2"].iter().map(|a| between(cyl, &format!(" {a}=\""), "\"").parse().unwrap()).collect();
    assert!((n[0] + n[1]).abs() < 0.01 && (n[2] + n[3]).abs() < 0.01 && n[2] > n[0], "{cyl}");
    assert!(cyl.contains("<stop offset=\".5\" stop-color=\"#ff0\"/>"));
    // CURVED gets 9 stops
    assert!(out.split("<linearGradient ").any(|g| g.split("</linearGradient>").next().unwrap().matches("<stop ").count() == 9));
    // SPHERICAL shifted: the light centre moves to the upper left of the box
    let sph = between(&out, "<radialGradient id=\"g3\"", "</radialGradient>");
    assert!(sph.contains("<stop offset=\"0\" stop-color=\"#ff0\"/><stop offset=\"1\" stop-color=\"#00f\"/>"), "{sph}");
}

#[test]
fn sinks_without_gradients_keep_the_colour_bands() {
    use cadrender_core::sink::RecordingSink;
    let fonts = load_fonts(&FontChoice::Liberation).unwrap();
    let scene = cadrender_dxf::read_file(data("svg_rich.dxf"), fonts).unwrap();
    let mut opts = RenderOptions::preset("none").unwrap();
    opts.width = 900;
    let prep = Prepared::new(&scene, &opts);
    let r = render_sheet::<RecordingSink>(&scene, "Sheet R", &opts, &prep, &()).unwrap();
    let bands = r.sink.all_draws().iter().filter(|d| d.fill_rule == Some(FillRule::EvenOdd)).count();
    assert!(bands > 9 * 10, "bands: {bands}");
}

#[test]
fn gradient_stops_keep_transparency() {
    let mut k = SvgSink::page(&SvgConfig { precision: Some(1), ..Default::default() }, 100, 100, Rgba::WHITE);
    k.move_to(0.0, 0.0); k.line_to(10.0, 0.0); k.line_to(10.0, 10.0); k.close();
    let stops = [(0.0, Rgba::rgb(0xff0000).with_alpha(128)), (1.0, Rgba::rgb(0x0000ff))];
    assert!(k.fill_gradient(&GradientPaint { shape: GradientShape::Radial { c: [5.0, 5.0], r: 7.5 }, stops: &stops }));
    let body = s(k.body().to_vec());
    assert_eq!(body, "<radialGradient id=\"g0\" gradientUnits=\"userSpaceOnUse\" cx=\"5\" cy=\"5\" r=\"7.5\"><stop offset=\"0\" stop-color=\"#f00\" stop-opacity=\".502\"/><stop offset=\"1\" stop-color=\"#00f\"/></radialGradient>\n<path fill=\"url(#g0)\" fill-rule=\"evenodd\" d=\"M0 0h10v10z\"/>\n");
    // an empty path writes nothing
    assert!(k.fill_gradient(&GradientPaint { shape: GradientShape::Radial { c: [5.0, 5.0], r: 7.5 }, stops: &stops }));
    assert_eq!(s(k.body().to_vec()).matches("<path").count(), 1);
}
