//! Big pictures: strips give the same pixels as one piece, the width limit
//! holds, and a 10 000 px picture works.

use cadrender::{Document, Fonts, RenderOptions};
use cadrender_core::{render_sheet, render_sheet_strips};
use cadrender_raster::RasterSink;

fn data(name: &str) -> std::path::PathBuf { std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../cadrender-dxf/tests/data").join(name) }

/// Grey value of every pixel of a sink.
fn greys(s: &RasterSink) -> Vec<u8> {
    let mut v = Vec::with_capacity((s.width() * s.height()) as usize);
    for y in 0..s.height() { for x in 0..s.width() { v.push(s.pixel(x, y).unwrap()[1]); } }
    v
}

#[test]
fn strips_match_one_piece() {
    for file in ["viewport.dxf", "hatch.dxf", "nested_insert.dxf"] {
        let doc = Document::open(data(file), Fonts::new()).unwrap();
        let opts = RenderOptions { width: 900, ..RenderOptions::preset("preview").unwrap() };
        let prep = doc.prepare(&opts);
        for sheet in doc.sheets() {
            let whole = render_sheet::<RasterSink>(&doc.scene, sheet, &opts, &prep, &()).unwrap();
            let want = greys(&whole.sink);
            let mut got = vec![];
            let mut strips = 0;
            let r = render_sheet_strips::<RasterSink>(&doc.scene, sheet, &opts, &prep, &(), 37, |s, y0, h| {
                assert_eq!(y0 as usize, got.len() / 900);
                assert_eq!(h, whole.height);
                assert!(s.height() <= 37);
                got.extend(greys(&s));
                strips += 1;
            }).unwrap();
            assert_eq!((r.width, r.height), (whole.width, whole.height));
            assert_eq!(strips, whole.height.div_ceil(37));
            assert_eq!(got.len(), want.len());
            // the seams may differ by float rounding only
            let bad = got.iter().zip(&want).filter(|(a, b)| (**a as i32 - **b as i32).abs() > 8).count();
            assert!(bad * 10_000 <= want.len(), "{file} {sheet}: {bad} pixels differ");
            assert!(want.iter().any(|&g| g < 128), "{file} {sheet}: something is drawn");
        }
    }
}

#[test]
fn width_limit() {
    let doc = Document::open(data("viewport.dxf"), Fonts::new()).unwrap();
    let mut opts = RenderOptions::preset("preview").unwrap();
    let prep = doc.prepare(&opts);
    opts.width = 0;
    assert!(doc.render_png("Sheet A", &opts, &prep).is_err());
    opts.width = 100_001;
    assert!(doc.render_png("Sheet A", &opts, &prep).err().unwrap().contains("width"));
    // 10 000 px: drawn in strips, one valid PNG of the full size
    opts.width = 10_000;
    let img = doc.render_png("Sheet A", &opts, &prep).unwrap();
    assert!(img.height > 1_600, "more than one strip");
    let mut r = png::Decoder::new(&img.png[..]).read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).unwrap();
    assert_eq!((info.width, info.height), (10_000, img.height));
    assert!(buf.iter().any(|&g| g < 128), "lines are drawn");
}
