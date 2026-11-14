//! Micro benchmark (no extra crates). Run with:
//! `cargo bench --manifest-path crates/cadrender-shx/Cargo.toml`
//!
//! Measures: cold parse of a whole font, layout of typical drawing texts, and
//! layout + polyline output into reused buffers.

use std::hint::black_box;
use std::time::Instant;

use cadrender_shx::{layout_text, BuiltinFont, HAlign, Polylines, ShxFont, TextLayout, TextParams};

fn time<F: FnMut()>(label: &str, iters: u32, per: u32, mut f: F) {
    // warm up
    for _ in 0..iters / 10 + 1 {
        f();
    }
    let t = Instant::now();
    for _ in 0..iters {
        f();
    }
    let ns = t.elapsed().as_nanos() as f64 / (iters as f64 * per as f64);
    println!("{label:<48} {ns:>10.1} ns/op");
}

fn main() {
    let bytes = BuiltinFont::Simplex.bytes();
    time(
        "parse file + all glyphs (simplex, 162 shapes)",
        2_000,
        1,
        || {
            let f = ShxFont::from_bytes(bytes.to_vec()).unwrap();
            f.parse_all();
            black_box(&f);
        },
    );

    let font = BuiltinFont::Simplex.font();
    let texts = [
        "GROUND FLOOR PLAN",
        "%%c120",
        "H=2.70",
        "Level +3.45 m",
        "Plant room",
        "SECTION A-A'",
        "Scale 1:50",
        "90%%d",
        "Brick wall, 30 cm thick",
        "%%uNOTE%%u: check on site",
    ];
    let p = TextParams {
        height: 2.5,
        width_factor: 0.8,
        oblique_deg: 15.0,
        halign: HAlign::Center,
        ..Default::default()
    };
    let mut layout = TextLayout::new();
    let n = texts.len() as u32;
    time("layout_text (per text, avg 17 chars)", 200_000, n, || {
        for t in &texts {
            layout_text(font, t, &p, &mut layout);
            black_box(&layout);
        }
    });

    let mut out = Polylines::default();
    time(
        "layout_text + stroke_polylines (per text)",
        100_000,
        n,
        || {
            for t in &texts {
                layout_text(font, t, &p, &mut layout);
                out.clear();
                layout.stroke_polylines(font, &mut out);
                black_box(&out);
            }
        },
    );

    let chars: u32 = texts.iter().map(|t| t.chars().count() as u32).sum();
    time(
        "glyph lookup + cached glyph (per char)",
        500_000,
        chars,
        || {
            for t in &texts {
                for c in t.chars() {
                    black_box(font.glyph_for_char(c));
                }
            }
        },
    );
}
