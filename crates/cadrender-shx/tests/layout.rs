//! Layout tests on the hand-made test font (above 10, below 3) and the
//! built-in Simplex font (above 21, below 7).

use cadrender_shx::{
    layout_text, BuiltinFont, DecorationKind, FontFace, HAlign, Polylines, ShxFont, TextLayout,
    TextParams, VAlign,
};

fn test_font() -> ShxFont {
    ShxFont::from_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/test_font.shx"
    ))
    .unwrap()
}

fn run(font: &dyn FontFace, text: &str, p: TextParams) -> TextLayout {
    let mut l = TextLayout::new();
    layout_text(font, text, &p, &mut l);
    l
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

#[test]
fn advance_and_scale() {
    let f = test_font();
    // A (20) + H (10) + space (6) + H (10) = 46 font units; height 5 => scale 0.5
    let l = run(
        &f,
        "AH H",
        TextParams {
            height: 5.0,
            ..Default::default()
        },
    );
    assert_eq!(l.glyphs.len(), 4);
    assert_eq!(
        l.glyphs.iter().map(|g| g.x).collect::<Vec<_>>(),
        vec![0.0, 20.0, 30.0, 36.0]
    );
    assert_eq!(l.advance, 46.0);
    assert!(close(l.width(), 23.0));
    // the H cap top (10 units) lands at the text height
    let mut out = Polylines::default();
    l.stroke_polylines(&f, &mut out);
    let max_y = out.points.iter().map(|p| p[1]).fold(f64::MIN, f64::max);
    assert!(close(max_y, 5.0));
}

#[test]
fn width_factor_and_oblique() {
    let f = test_font();
    let l = run(
        &f,
        "H",
        TextParams {
            height: 10.0,
            width_factor: 0.5,
            oblique_deg: 45.0,
            ..Default::default()
        },
    );
    assert!(close(l.width(), 5.0));
    let mut out = Polylines::default();
    l.stroke_polylines(&f, &mut out);
    // left stem of H: (0,0)-(0,10) in font units; scale 1 in y, 0.5 in x; 45 deg slant
    let first: Vec<_> = out.iter().next().unwrap().to_vec();
    assert!(close(first[0][0], 0.0) && close(first[0][1], 0.0));
    assert!(
        close(first[1][0], 10.0) && close(first[1][1], 10.0),
        "{first:?}"
    );
}

#[test]
fn horizontal_alignments() {
    let f = test_font();
    let p = |halign| TextParams {
        height: 10.0,
        halign,
        ..Default::default()
    };
    let b = |h| run(&f, "H", p(h)).bounds();
    assert!(close(b(HAlign::Left)[0], 0.0));
    assert!(close(b(HAlign::Center)[0], -5.0) && close(b(HAlign::Center)[2], 5.0));
    assert!(close(b(HAlign::Right)[2], 0.0));
    // Middle: centred in x, middle between descender (-3) and cap (10) in y
    let m = b(HAlign::Middle);
    assert!(close(m[0], -5.0));
    assert!(close(m[1] + m[3], 0.0), "{m:?}");
}

#[test]
fn vertical_alignments() {
    let f = test_font();
    let p = |valign| TextParams {
        height: 10.0,
        valign,
        ..Default::default()
    };
    let y_off = |v| run(&f, "H", p(v)).transform.f;
    assert!(close(y_off(VAlign::Baseline), 0.0));
    assert!(close(y_off(VAlign::Bottom), 3.0));
    assert!(close(y_off(VAlign::Middle), -5.0));
    assert!(close(y_off(VAlign::Top), -10.0));
}

#[test]
fn aligned_and_fit() {
    let f = test_font();
    // natural width at height 10: H = 10 units => 10
    let fit = run(
        &f,
        "H",
        TextParams {
            height: 10.0,
            halign: HAlign::Fit,
            fit_width: 30.0,
            ..Default::default()
        },
    );
    assert!(close(fit.width(), 30.0));
    assert!(close(fit.transform.d, 1.0)); // height unchanged
    let al = run(
        &f,
        "H",
        TextParams {
            height: 10.0,
            halign: HAlign::Aligned,
            fit_width: 30.0,
            ..Default::default()
        },
    );
    assert!(close(al.width(), 30.0));
    assert!(close(al.transform.d, 3.0)); // height grows with the width
                                         // without a fit width the text keeps its natural size
    let none = run(
        &f,
        "H",
        TextParams {
            height: 10.0,
            halign: HAlign::Fit,
            ..Default::default()
        },
    );
    assert!(close(none.width(), 10.0));
}

#[test]
fn mirrored_text() {
    let f = test_font();
    let l = run(
        &f,
        "H",
        TextParams {
            height: 10.0,
            backward: true,
            upside_down: true,
            ..Default::default()
        },
    );
    let b = l.bounds();
    assert!(close(b[2], 0.0) && close(b[0], -10.0));
    assert!(close(b[1], -10.0) && close(b[3], 3.0));
}

#[test]
fn underline_overline_toggles() {
    let f = test_font();
    let l = run(
        &f,
        "%%uHH%%u H%%oH",
        TextParams {
            height: 10.0,
            ..Default::default()
        },
    );
    assert_eq!(l.glyphs.len(), 5);
    assert_eq!(l.decorations.len(), 2);
    let u = l.decorations[0];
    assert_eq!(u.kind, DecorationKind::Underline);
    assert_eq!((u.x0, u.x1), (0.0, 20.0));
    assert!(u.y < 0.0);
    let o = l.decorations[1]; // overline left open: closes at the end
    assert_eq!(o.kind, DecorationKind::Overline);
    assert_eq!((o.x0, o.x1), (36.0, 46.0));
    assert!(o.y > 10.0);
    let mut out = Polylines::default();
    l.stroke_polylines(&f, &mut out);
    assert_eq!(out.len(), 4 * 3 + 2); // the space has no ink
}

#[test]
fn special_characters_and_missing_glyphs() {
    let f = test_font();
    // %%d -> degree -> old number 127 in this font
    let l = run(&f, "%%d", TextParams::default());
    assert_eq!(l.glyphs[0].id, f.slot_for_code(127).unwrap());
    // missing 'Z' -> '?'
    let l = run(&f, "Z", TextParams::default());
    assert_eq!(l.glyphs[0].id, f.slot_for_code(63).unwrap());
    // %%065 -> raw code 65 = 'A'
    let l = run(&f, "%%065", TextParams::default());
    assert_eq!(l.glyphs[0].id, f.slot_for_code(65).unwrap());
    // line breaks are dropped in TEXT
    assert_eq!(run(&f, "H\r\nH", TextParams::default()).glyphs.len(), 2);
    // \U+ escape
    let l = run(&f, "\\U+0041", TextParams::default());
    assert_eq!(l.glyphs[0].id, f.slot_for_code(65).unwrap());
}

#[test]
fn builtin_specials() {
    let f = BuiltinFont::Simplex.font();
    let l = run(
        f,
        "%%c50 90%%d %%p0.5",
        TextParams {
            height: 2.5,
            ..Default::default()
        },
    );
    assert!(!l.glyphs.is_empty());
    let q = f.slot_for_char('?').unwrap();
    assert!(
        l.glyphs.iter().all(|g| g.id != q),
        "a special code became '?'"
    );
    // typographic quotes fall back to ASCII ones
    let l = run(f, "l\u{2019}area \u{2013} m\u{B2}", TextParams::default());
    assert!(l.glyphs.iter().all(|g| g.id != q));
}

#[test]
fn reuse_does_not_grow() {
    let f = BuiltinFont::Simplex.font();
    let mut l = TextLayout::new();
    let mut out = Polylines::default();
    let p = TextParams::default();
    layout_text(f, "GROUND FLOOR PLAN %%c120", &p, &mut l);
    l.stroke_polylines(f, &mut out);
    let (cap_g, cap_p) = (l.glyphs.capacity(), out.points.capacity());
    for _ in 0..100 {
        layout_text(f, "GROUND FLOOR PLAN %%c120", &p, &mut l);
        out.clear();
        l.stroke_polylines(f, &mut out);
    }
    assert_eq!(l.glyphs.capacity(), cap_g);
    assert_eq!(out.points.capacity(), cap_p);
}
