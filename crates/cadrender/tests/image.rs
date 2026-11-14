//! IMAGE end to end: the picture file is found next to the drawing
//! (tests/data/pics/red_blue.png, 8 x 4 px, left half red, right half blue),
//! drawn in PNG and embedded in SVG. The second IMAGE's file is missing:
//! only its frame is drawn.

use cadrender::{Document, Fonts, RenderOptions, SvgConfig};

fn doc() -> Document { Document::open(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/image.dxf"), Fonts::new()).unwrap() }

#[test]
fn picture_in_png_and_frame_for_missing_file() {
    let d = doc();
    assert_eq!(d.scene.stats.skipped.get("IMAGE(file missing)"), Some(&1));
    assert!(d.scene.stats.warnings.iter().any(|w| w.contains("missing.jpg")));
    let opts = RenderOptions { framing: cadrender_core::Framing::Paper, ..RenderOptions::default() };
    let prep = d.prepare(&opts);
    let img = d.render_png("Sheet", &opts, &prep).unwrap();
    let dec = png::Decoder::new(&img.png[..]);
    let mut r = dec.read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).unwrap();
    assert_eq!(info.color_type, png::ColorType::Rgb, "a colour picture makes the page colour");
    // paper 200 x 50 units -> 2000 x 500 px: 10 px per unit; picture 80 x 40 units at (0, 0)
    let s = info.width as f64 / 200.0;
    let at = |x: f64, y: f64| { let (px, py) = ((x * s) as usize, ((50.0 - y) * s) as usize); let i = (py * info.width as usize + px) * 3; [buf[i], buf[i + 1], buf[i + 2]] };
    assert_eq!(at(20.0, 20.0), [255, 0, 0], "left half red");
    assert_eq!(at(60.0, 20.0), [0, 0, 255], "right half blue");
    // missing picture: white inside its frame
    assert_eq!(at(140.0, 20.0), [255, 255, 255]);
}

#[test]
fn picture_in_svg_as_data_uri() {
    let d = doc();
    let opts = RenderOptions::default();
    let prep = d.prepare(&opts);
    let svg = d.render_svg("Sheet", &opts, &prep, &SvgConfig::default()).unwrap();
    let text = String::from_utf8(svg.svg).unwrap();
    assert_eq!(text.matches("<image").count(), 1);
    assert!(text.contains("xlink:href=\"data:image/png;base64,iVBORw0KGgo"));
    assert!(text.contains("clip-path=\"url(#i"));
}
