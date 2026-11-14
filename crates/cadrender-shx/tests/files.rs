//! Tests on the hand-made files in tests/data and on the built-in fonts.

use cadrender_shx::{BuiltinFont, FontFace, ShxError, ShxFont, ShxKind, ShxStyleFont};

fn data(name: &str) -> ShxFont {
    let p = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/").to_string() + name;
    ShxFont::from_file(p).unwrap()
}

fn lines(f: &ShxFont, ch: char) -> Vec<Vec<[f32; 2]>> {
    f.glyph_for_char(ch)
        .unwrap()
        .polylines()
        .map(|l| l.to_vec())
        .collect()
}

#[test]
fn test_font_header_and_glyphs() {
    let f = data("test_font.shx");
    assert_eq!(f.kind(), ShxKind::Shapes);
    assert!(f.is_font());
    let info = f.info().unwrap();
    assert_eq!(info.name, "cadrender test font");
    assert_eq!((info.above, info.below, info.modes), (10, 3, 2));
    assert!(f.issues().is_empty(), "{:?}", f.issues());

    assert_eq!(
        lines(&f, 'A'),
        vec![
            vec![
                [0.0, 0.0],
                [5.0, 5.0],
                [10.0, 10.0],
                [15.0, 5.0],
                [20.0, 0.0]
            ],
            vec![[5.0, 5.0], [15.0, 5.0]],
        ]
    );
    assert_eq!(f.glyph_for_char('A').unwrap().advance(), 20.0);
    assert_eq!(
        lines(&f, 'H'),
        vec![
            vec![[0.0, 0.0], [0.0, 10.0]],
            vec![[6.0, 10.0], [6.0, 0.0]],
            vec![[0.0, 5.0], [6.0, 5.0]]
        ]
    );
    assert_eq!(f.glyph_for_char(' ').unwrap().advance(), 6.0);
    assert!(f.glyph_for_char(' ').unwrap().is_empty());
    // C = two H subshapes
    let c = f.glyph_for_char('C').unwrap();
    assert_eq!(c.polyline_count(), 6);
    assert_eq!(c.advance(), 20.0);
    // é is a normal shape font byte 233 (Windows-1252 = Latin-1 here)
    let e = f.glyph_for_char('é').unwrap();
    assert_eq!(e.polyline_count(), 4);
    // degree has no 0xB0 shape: falls back to the old number 127
    assert_eq!(f.slot_for_char('°'), f.slot_for_code(127));
    assert!(f.glyph_for_char('Z').is_none());
}

#[test]
fn test_font_vertical_code() {
    let f = data("test_font.shx");
    let s = f.slot_for_char('B').unwrap();
    assert_eq!(f.glyph(s).end_point(), [4.0, 0.0]);
    assert_eq!(f.glyph(s).polyline_count(), 1);
    let v = f.glyph_vertical(s);
    assert_eq!(v.end_point(), [4.0, 5.0]);
    assert_eq!(v.points(), &[[0.0, 0.0], [0.0, 10.0], [0.0, 15.0]]);
}

#[test]
fn shape_file_with_names() {
    let f = data("test_shapes.shx");
    assert_eq!(f.kind(), ShxKind::Shapes);
    assert!(!f.is_font());
    assert!(f.info().is_none());
    let s = f.slot_by_name("zig").unwrap();
    assert_eq!(f.code_of(s), 132);
    assert_eq!(f.shape_name(s), "ZIG");
    assert_eq!(f.glyph(s).points(), &[[0.0, 0.0], [2.0, 2.0], [4.0, 0.0]]);
    let c = f.slot_by_name("CIRC1").unwrap();
    assert!(f.glyph(c).polyline_count() == 1 && f.glyph(c).points().len() > 16);
}

#[test]
fn unifont_file() {
    let f = data("test_unifont.shx");
    assert_eq!(f.kind(), ShxKind::Unifont);
    let info = f.info().unwrap();
    assert_eq!(info.name, "cadrender test unifont");
    assert_eq!((info.above, info.below), (8, 2));
    assert_eq!(f.shape_count(), 4);
    assert_eq!(f.glyph_for_char('一').unwrap().advance(), 12.0);
    let ring = f.glyph_for_char('Å').unwrap();
    assert_eq!(ring.advance(), 10.0); // the subshape 'A' moved the pen
    assert_eq!(ring.polyline_count(), 2);
    assert!(f.glyph_for_char('⌀').is_some()); // through 0x2205
}

#[test]
fn extended_big_font() {
    let f = data("test_bigfont.shx");
    assert_eq!(f.kind(), ShxKind::BigFont);
    let info = f.info().unwrap();
    assert!(info.extended_big);
    assert_eq!((info.above, info.char_width), (16, 16));
    assert_eq!(f.escape_ranges(), &[(0x81, 0x9F)]);
    let g = f.glyph(f.slot_for_code(0x8140).unwrap());
    let l: Vec<_> = g.polylines().collect();
    assert_eq!(
        l,
        vec![
            &[[0.0, 8.0], [16.0, 8.0]][..],
            &[[8.0, 0.0], [8.0, 16.0]][..]
        ]
    );
    assert_eq!(g.advance(), 16.0); // from the font info, the pen did not move

    // mixed into a normal font: raw code \M+ goes to the big font, scaled
    let prim = BuiltinFont::Simplex.font();
    let style = ShxStyleFont::new(prim, Some(&f));
    let id = style.glyph_id_for_code(0x8140).unwrap();
    assert!((style.glyph_scale(id) - 21.0 / 16.0).abs() < 1e-6);
    assert!((style.advance(id) - 21.0).abs() < 1e-4);
    assert_eq!(style.glyph_id('A'), prim.glyph_id('A'));
}

#[test]
fn truncated_file_keeps_complete_shapes() {
    let f = data("broken_truncated.shx");
    assert!(!f.issues().is_empty());
    assert!(f.shape_count() > 0);
    // every shape that was kept can be parsed without a panic
    f.parse_all();
}

#[test]
fn garbage_is_rejected_cleanly() {
    assert_eq!(
        ShxFont::from_bytes(b"hello".to_vec()).unwrap_err(),
        ShxError::NotShx
    );
    assert_eq!(
        ShxFont::from_bytes(b"AutoCAD-86 shapes 1.0\r\n\x1a".to_vec()).unwrap_err(),
        ShxError::NoShapes
    );
    assert!(matches!(
        ShxFont::from_file("/no/such/file.shx"),
        Err(ShxError::Io(_))
    ));
    // random bytes after a valid header: never panic
    let mut seed = 12345u32;
    for len in 0..300 {
        let mut b = b"AutoCAD-86 shapes 1.0\r\n\x1a".to_vec();
        for _ in 0..len {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            b.push((seed >> 16) as u8);
        }
        for kind in [&b"shapes"[..], b"unifont", b"bigfont"] {
            let mut c = b.clone();
            c[11..17].copy_from_slice(&kind[..6]);
            if kind.len() == 7 {
                c.insert(17, kind[6]);
            }
            if let Ok(f) = ShxFont::from_bytes(c) {
                f.parse_all();
            }
        }
    }
}

#[test]
fn every_byte_mutation_of_a_real_font_is_safe() {
    let base = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/test_font.shx"
    ))
    .unwrap();
    for i in 24..base.len() {
        for v in [0u8, 7, 9, 13, 14, 0x80, 0xFF] {
            let mut b = base.clone();
            b[i] = v;
            if let Ok(f) = ShxFont::from_bytes(b) {
                f.parse_all();
                for s in 0..f.shape_count() as u32 {
                    f.glyph_vertical(s);
                }
            }
        }
    }
}

#[test]
fn builtin_fonts_are_complete() {
    for b in [BuiltinFont::Simplex, BuiltinFont::Duplex] {
        let f = b.font();
        assert!(f.issues().is_empty(), "{:?}", f.issues());
        let info = f.info().unwrap();
        assert_eq!((info.above, info.below), (21, 7));
        for c in (32u8..127)
            .map(char::from)
            .chain("àèéìòùÀÈÉÌÒÙçÇñÑäöüÄÖÜßøØ°±²³·⌀∅".chars())
        {
            if c == 'ß' {
                continue; // not in the font: the layout shows '?'
            }
            let g = f
                .glyph_for_char(c)
                .unwrap_or_else(|| panic!("{b:?} misses {c:?}"));
            assert!(g.advance() > 0.0, "{b:?} {c:?}");
            assert!(c == ' ' || !g.is_empty(), "{b:?} {c:?} has no ink");
        }
        // capital letters are 21 units tall, sit on the baseline
        let h = f.glyph_for_char('H').unwrap();
        assert_eq!(h.bbox()[1], 0.0);
        assert_eq!(h.bbox()[3], 21.0);
        // accents sit above the letter
        let e = f.glyph_for_char('é').unwrap();
        assert!(e.bbox()[3] > f.glyph_for_char('e').unwrap().bbox()[3] + 1.0);
        // legacy AutoCAD numbers
        assert_eq!(
            f.glyph(f.slot_for_code(127).unwrap()).bbox(),
            f.glyph_for_char('°').unwrap().bbox()
        );
    }
    // Roman Simplex advance widths (Hershey): 'A' 18, 'i' 8, space 16
    let f = BuiltinFont::Simplex.font();
    assert_eq!(f.glyph_for_char('A').unwrap().advance(), 18.0);
    assert_eq!(f.glyph_for_char('i').unwrap().advance(), 8.0);
    assert_eq!(f.glyph_for_char(' ').unwrap().advance(), 16.0);
}

#[test]
fn font_is_shared_by_threads() {
    let f = std::sync::Arc::new(ShxFont::from_bytes(BuiltinFont::Duplex.bytes().to_vec()).unwrap());
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let f = f.clone();
            std::thread::spawn(move || {
                let mut sum = 0.0;
                for c in "The quick brown fox 0123456789".chars() {
                    sum += f.glyph_for_char(c).unwrap().advance();
                }
                sum
            })
        })
        .collect();
    let sums: Vec<f32> = handles.into_iter().map(|h| h.join().unwrap()).collect();
    assert!(sums.windows(2).all(|w| w[0] == w[1]));
}
