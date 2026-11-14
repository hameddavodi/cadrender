//! Write an SVG specimen of an SHX font (built-in or a file) to look at.
//!
//! cargo run --example specimen -- out.svg [font.shx]

use cadrender_shx::{layout_text, BuiltinFont, HAlign, Polylines, ShxFont, TextLayout, TextParams};

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "specimen.svg".into());
    let owned;
    let fonts: Vec<(&str, &ShxFont)> = match args.next() {
        Some(p) => {
            owned = ShxFont::from_file(&p).expect("read font");
            vec![("file", &owned)]
        }
        None => vec![
            ("simplex", BuiltinFont::Simplex.font()),
            ("duplex", BuiltinFont::Duplex.font()),
        ],
    };
    let lines = [
        "ABCDEFGHIJKLMNOPQRSTUVWXYZ",
        "abcdefghijklmnopqrstuvwxyz 0123456789",
        "!\"#$%&'()*+,-./:;<=>?@[\\]^_`{|}~",
        "àèéìòù ÀÈÉÌÒÙ çÇ ñÑ äöü ÄÖÜ øØ Å å",
        "%%c120  90%%d  %%p0.05  m\u{B2} m\u{B3}  a\u{B7}b",
        "%%uUnderline%%u %%oOverline%%o %%kStrike%%k",
    ];
    let mut svg = String::new();
    let mut y = 0.0;
    let mut layout = TextLayout::new();
    let mut poly = Polylines::default();
    let mut body = String::new();
    for (name, font) in &fonts {
        for (i, text) in lines.iter().enumerate() {
            y += 14.0;
            let p = TextParams {
                height: 8.0,
                oblique_deg: if i == 5 { 15.0 } else { 0.0 },
                halign: HAlign::Left,
                ..Default::default()
            };
            layout_text(*font, text, &p, &mut layout);
            poly.clear();
            layout.stroke_polylines(*font, &mut poly);
            for l in poly.iter() {
                body.push_str("<polyline points=\"");
                for pt in l {
                    body.push_str(&format!("{:.3},{:.3} ", 10.0 + pt[0], y - pt[1]));
                }
                body.push_str("\"/>\n");
            }
        }
        body.push_str(&format!("<!-- {name} -->\n"));
        y += 8.0;
    }
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"1000\" height=\"{}\" viewBox=\"0 0 340 {}\">\n<rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n<g fill=\"none\" stroke=\"black\" stroke-width=\"0.4\" stroke-linecap=\"round\" stroke-linejoin=\"round\">\n",
        (y + 10.0) * 1000.0 / 340.0,
        y + 10.0
    ));
    svg.push_str(&body);
    svg.push_str("</g></svg>\n");
    std::fs::write(&out, svg).expect("write svg");
    println!("wrote {out}");
}
