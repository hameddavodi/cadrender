//! End to end: DXF file -> Document -> PNG bytes -> pixels.

use cadrender::{Document, Fonts, RenderOptions};

fn data(name: &str) -> std::path::PathBuf { std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../cadrender-dxf/tests/data").join(name) }

fn decode(png_bytes: &[u8]) -> (u32, u32, Vec<u8>, png::ColorType) {
    let dec = png::Decoder::new(png_bytes);
    let mut r = dec.read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    (info.width, info.height, buf, info.color_type)
}

#[test]
fn viewport_sheet_to_png() {
    let doc = Document::open(data("viewport.dxf"), Fonts::new()).unwrap();
    assert_eq!(doc.sheets(), vec!["Sheet A"]);
    let opts = RenderOptions::preset("preview").unwrap();
    let prep = doc.prepare(&opts);
    let img = doc.render_png("Sheet A", &opts, &prep).unwrap();
    let (w, h, px, ct) = decode(&img.png);
    assert_eq!((w, h), (img.width, img.height));
    assert_eq!(ct, png::ColorType::Grayscale);
    let at = |x: u32, y: u32| px[(y * w + x) as usize];
    // WALLS line (model y=0) seen through the viewport at paper y=100
    let s = 2000.0 / (297.0 * 1.1);
    let (x, y_walls, y_hidden) = ((150.0 + 14.85) * s, (220.5 - 100.0) * s, (220.5 - 110.0) * s);
    let dark = (-2..=2).map(|d| at(x as u32, (y_walls as i32 + d) as u32)).min().unwrap();
    assert!(dark < 200, "WALLS line is drawn ({dark})");
    let light = (-2..=2).map(|d| at(x as u32, (y_hidden as i32 + d) as u32)).min().unwrap();
    assert_eq!(light, 255, "HIDDEN is frozen in the viewport");
}

#[test]
fn unknown_sheet_is_an_error_not_a_crash() {
    let doc = Document::open(data("viewport.dxf"), Fonts::new()).unwrap();
    let opts = RenderOptions::default();
    let prep = doc.prepare(&opts);
    let rows = doc.render_many(&["Sheet A".to_string(), "missing".to_string()], &opts, &prep, |n, r| (n.to_string(), r.is_ok()));
    assert_eq!(rows, vec![("Sheet A".to_string(), true), ("missing".to_string(), false)]);
}

#[test]
fn true_color_png_is_rgb() {
    let doc = Document::open(data("viewport.dxf"), Fonts::new()).unwrap();
    let opts = RenderOptions { width: 300, ..RenderOptions::default() };
    let prep = doc.prepare(&opts);
    let img = doc.render_png("Sheet A", &opts, &prep).unwrap();
    let (w, _, _, ct) = decode(&img.png);
    assert_eq!((w, ct), (300, png::ColorType::Rgb));
}
