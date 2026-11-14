//! The facade's SVG API.

use cadrender::{load_fonts, Document, FontChoice, RenderOptions, SvgConfig};

fn data(name: &str) -> std::path::PathBuf { std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../cadrender-dxf/tests/data").join(name) }

#[test]
fn viewport_sheet_to_svg() {
    let doc = Document::open(data("viewport.dxf"), load_fonts(&FontChoice::Liberation).unwrap()).unwrap();
    let mut opts = RenderOptions::preset("preview").unwrap();
    opts.width = 400;
    let prep = doc.prepare(&opts);
    let img = doc.render_svg("Sheet A", &opts, &prep, &SvgConfig::default()).unwrap();
    let text = String::from_utf8(img.svg).unwrap();
    assert!(text.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"400\""));
    assert!(text.contains("<svg x="), "the viewport is a nested svg");
    assert_eq!((img.width, img.viewports), (400, 1));
    // same size as the PNG of the same sheet
    let png = doc.render_png("Sheet A", &opts, &prep).unwrap();
    assert_eq!((png.width, png.height), (img.width, img.height));
}

#[test]
fn many_sheets_and_errors() {
    let doc = Document::open(data("viewport.dxf"), load_fonts(&FontChoice::Liberation).unwrap()).unwrap();
    let opts = RenderOptions::preset("none").unwrap();
    let prep = doc.prepare(&opts);
    let names = vec!["Sheet A".to_string(), "Nope".to_string()];
    let res = doc.render_many_svg(&names, &opts, &prep, &SvgConfig::default(), |_, r| r.map(|i| i.svg.len()));
    assert!(res[0].as_ref().is_ok_and(|n| *n > 100));
    assert!(res[1].is_err());
}
