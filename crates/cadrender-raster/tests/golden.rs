//! Golden picture for the rasteriser: lines of every width class at many
//! angles, dashes, arcs, fills and a viewport region, compared with
//! `tests/data/golden_lines.png`.
//!
//! The tolerance is made for hairlines: a small change in rounding can move
//! an anti-aliased edge by a little, so a few pixels may differ a lot; the
//! rest must stay within a few grey levels. To accept a new look on purpose:
//! `CADRENDER_UPDATE_GOLDEN=1 cargo test -p cadrender-raster --test golden`.

use cadrender_core::color::Rgba;
use cadrender_core::sink::{FillRule, IRect, Paint, PaintKind, Sink};
use cadrender_raster::{PngFormat, RasterSink};

const W: u32 = 240;
const H: u32 = 160;

fn stroke(s: &mut RasterSink, c: Rgba, width: f32, dash: Option<&[f32]>) {
    s.draw(&Paint { color: c, kind: PaintKind::Stroke { width, dash } });
}

fn scene() -> RasterSink {
    let mut s = RasterSink::page(&(), W, H, Rgba::WHITE);
    // width classes 1, 1.19 (0.5 mm), 1.67 (0.7 mm), 2, 2.38 (1 mm), 4: horizontal, vertical, 30 degrees
    for (k, w) in [1.0f32, 1.19, 1.67, 2.0, 2.38, 4.0].iter().enumerate() {
        let y = 6.3 + k as f32 * 7.0;
        s.move_to(4.0, y); s.line_to(70.0, y);
        let x = 80.3 + k as f32 * 7.0;
        s.move_to(x, 4.0); s.line_to(x, 46.0);
        s.move_to(130.0 + k as f32 * 9.0, 4.0); s.line_to(150.0 + k as f32 * 9.0, 4.0 + 20.0 * 0.577);
        stroke(&mut s, Rgba::BLACK, *w, None);
    }
    // a fan of 1 px lines (anti-aliasing at every angle), grey
    for k in 0..24 {
        let a = k as f32 * std::f32::consts::PI / 24.0;
        s.move_to(40.0, 100.0); s.line_to(40.0 + 34.0 * a.cos(), 100.0 - 34.0 * a.sin());
    }
    stroke(&mut s, Rgba::gray(90), 1.0, None);
    // dashes and dots
    s.move_to(90.0, 60.5); s.line_to(230.0, 60.5);
    s.move_to(90.0, 66.0); s.line_to(230.0, 80.0);
    stroke(&mut s, Rgba::BLACK, 1.0, Some(&[6.0, 3.0, 1.0, 3.0]));
    // arcs as cubics (a circle and a half circle), and a closed rectangle
    let k = 0.5523f32 * 14.0;
    let (cx, cy) = (110.0, 110.0);
    s.move_to(cx + 14.0, cy);
    s.cubic_to(cx + 14.0, cy + k, cx + k, cy + 14.0, cx, cy + 14.0);
    s.cubic_to(cx - k, cy + 14.0, cx - 14.0, cy + k, cx - 14.0, cy);
    s.cubic_to(cx - 14.0, cy - k, cx - k, cy - 14.0, cx, cy - 14.0);
    s.cubic_to(cx + k, cy - 14.0, cx + 14.0, cy - k, cx + 14.0, cy);
    s.close();
    s.move_to(132.2, 92.2); s.line_to(170.2, 92.2); s.line_to(170.2, 128.2); s.line_to(132.2, 128.2); s.close();
    stroke(&mut s, Rgba::BLACK, 1.0, None);
    // fills: a triangle (non-zero) and a ring (even-odd)
    s.move_to(180.0, 90.0); s.line_to(230.0, 100.0); s.line_to(190.0, 130.0); s.close();
    s.draw(&Paint { color: Rgba::gray(160), kind: PaintKind::Fill { rule: FillRule::NonZero } });
    for r in [12.0f32, 6.0] {
        let (cx, cy, k) = (205.0, 140.0, 0.5523 * r);
        s.move_to(cx + r, cy);
        s.cubic_to(cx + r, cy + k, cx + k, cy + r, cx, cy + r);
        s.cubic_to(cx - k, cy + r, cx - r, cy + k, cx - r, cy);
        s.cubic_to(cx - r, cy - k, cx - k, cy - r, cx, cy - r);
        s.cubic_to(cx + k, cy - r, cx + r, cy - k, cx + r, cy);
        s.close();
    }
    s.draw(&Paint { color: Rgba::BLACK, kind: PaintKind::Fill { rule: FillRule::EvenOdd } });
    // a viewport region with a white mask over a line, then a line on top
    let rect = IRect { x: 8, y: 136, w: 150, h: 20 };
    let mut reg = s.region(rect);
    reg.move_to(10.0, 2.0); reg.line_to(60.0, 2.0); reg.line_to(60.0, 18.0); reg.line_to(10.0, 18.0); reg.close();
    reg.draw(&Paint { color: Rgba::WHITE, kind: PaintKind::Fill { rule: FillRule::NonZero } });
    reg.move_to(0.0, 4.0); reg.line_to(150.0, 6.0);
    stroke(&mut reg, Rgba::BLACK, 1.19, None);
    s.move_to(0.0, 146.0); s.line_to(240.0, 146.0);
    stroke(&mut s, Rgba::gray(120), 1.0, None);
    s.composite(reg, rect, None);
    s
}

#[test]
fn golden_lines() {
    let s = scene();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/golden_lines.png");
    let png = s.png(PngFormat::Gray).unwrap();
    if std::env::var_os("CADRENDER_UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &png).unwrap();
        return;
    }
    let want = std::fs::read(&path).expect("golden picture; make it with CADRENDER_UPDATE_GOLDEN=1");
    let mut r = png::Decoder::new(&want[..]).read_info().unwrap();
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).unwrap();
    assert_eq!((info.width, info.height, info.color_type), (W, H, png::ColorType::Grayscale));
    let (mut small, mut big) = (0, 0);
    for y in 0..H {
        for x in 0..W {
            let d = (s.pixel(x, y).unwrap()[0] as i32 - buf[(y * W + x) as usize] as i32).abs();
            if d > 48 { big += 1 } else if d > 4 { small += 1 }
        }
    }
    let n = (W * H) as usize;
    assert!(big * 1000 <= n, "{big} pixels differ a lot");
    assert!(small * 100 <= n, "{small} pixels differ a little");
}

/// The properties the golden picture is about, checked directly.
#[test]
fn line_classes_have_their_ink() {
    let s = scene();
    let g = |x: u32, y: u32| s.pixel(x, y).unwrap()[0] as u32;
    // ink of one column across each horizontal line = its width
    let ink = |y0: u32| (y0 - 3..y0 + 4).map(|y| 255 - g(30, y)).sum::<u32>() as f32 / 255.0;
    for (k, w) in [1.0f32, 1.19, 1.67, 2.0, 2.38, 4.0].iter().enumerate() {
        let y0 = (6.3 + k as f32 * 7.0) as u32;
        let i = ink(y0);
        // the ink curve adds a little to partly covered pixels, never takes away
        assert!(i >= w - 0.05 && i <= w + 0.45, "width {w}: ink {i}");
    }
    // 1 px line: one black row
    assert_eq!(g(30, 6), 0);
    assert_eq!((g(30, 5), g(30, 7)), (255, 255));
    // the region mask hides the grey line, the region's own line is on top
    assert_eq!(g(8 + 35, 146), 255, "masked");
    assert!(g(8 + 100, 146) < 200, "grey line outside the mask");
}
