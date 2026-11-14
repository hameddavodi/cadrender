//! One test per shape code, on fonts built in memory.

use cadrender_shx::{Glyph, ShxFont, ShxKind};

/// Build a "shapes 1.0" font with above 10, below 2, modes 2.
fn font(shapes: &[(u16, &[u8])]) -> ShxFont {
    let mut recs: Vec<(u16, Vec<u8>)> = vec![(0, b"t\0\x0a\x02\x02\x00".to_vec())];
    for (n, code) in shapes {
        let mut r = vec![0u8]; // empty name
        r.extend_from_slice(code);
        recs.push((*n, r));
    }
    recs.sort_by_key(|r| r.0);
    let mut b = b"AutoCAD-86 shapes 1.0\r\n\x1a".to_vec();
    b.extend_from_slice(&recs[0].0.to_le_bytes());
    b.extend_from_slice(&recs.last().unwrap().0.to_le_bytes());
    b.extend_from_slice(&(recs.len() as u16).to_le_bytes());
    for (n, r) in &recs {
        b.extend_from_slice(&n.to_le_bytes());
        b.extend_from_slice(&(r.len() as u16).to_le_bytes());
    }
    for (_, r) in &recs {
        b.extend_from_slice(r);
    }
    b.extend_from_slice(b"EOF");
    let f = ShxFont::from_bytes(b).unwrap();
    assert!(f.issues().is_empty(), "{:?}", f.issues());
    f
}

fn glyph_of(code: &[u8]) -> Glyph {
    let f = font(&[(65, code)]);
    f.glyph(f.slot_for_code(65).unwrap()).clone()
}

fn lines(g: &Glyph) -> Vec<Vec<[f32; 2]>> {
    g.polylines().map(|l| l.to_vec()).collect()
}

fn near(a: [f32; 2], b: [f32; 2]) -> bool {
    (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3
}

fn dist(p: [f32; 2], c: [f32; 2]) -> f32 {
    ((p[0] - c[0]).powi(2) + (p[1] - c[1]).powi(2)).sqrt()
}

#[test]
fn code0_ends_the_shape() {
    let g = glyph_of(&[0x10, 0, 0x10, 0x10]);
    assert_eq!(lines(&g), vec![vec![[0.0, 0.0], [1.0, 0.0]]]);
    assert_eq!(g.advance(), 1.0);
}

#[test]
fn codes1_2_pen_down_and_up() {
    // pen starts down; 2 lifts it, 1 lowers it again
    let g = glyph_of(&[0x10, 2, 0x10, 1, 0x10, 0]);
    assert_eq!(
        lines(&g),
        vec![vec![[0.0, 0.0], [1.0, 0.0]], vec![[2.0, 0.0], [3.0, 0.0]]]
    );
    assert_eq!(g.advance(), 3.0);
}

#[test]
fn vector_bytes_all_16_directions() {
    let want: [[f32; 2]; 16] = [
        [1.0, 0.0],
        [1.0, 0.5],
        [1.0, 1.0],
        [0.5, 1.0],
        [0.0, 1.0],
        [-0.5, 1.0],
        [-1.0, 1.0],
        [-1.0, 0.5],
        [-1.0, 0.0],
        [-1.0, -0.5],
        [-1.0, -1.0],
        [-0.5, -1.0],
        [0.0, -1.0],
        [0.5, -1.0],
        [1.0, -1.0],
        [1.0, -0.5],
    ];
    for dir in 0..16u8 {
        for len in [1u8, 2, 15] {
            let g = glyph_of(&[(len << 4) | dir, 0]);
            let e = g.end_point();
            let w = [
                want[dir as usize][0] * len as f32,
                want[dir as usize][1] * len as f32,
            ];
            assert!(near(e, w), "dir {dir:X} len {len}: {e:?} != {w:?}");
            assert_eq!(g.polyline_count(), 1);
        }
    }
}

#[test]
fn codes3_4_scale_is_cumulative() {
    // *2, *3 => 6; /3 => 2; /2 => 1
    let g = glyph_of(&[4, 2, 4, 3, 0x10, 3, 3, 0x10, 3, 2, 0x10, 0]);
    let e = g.end_point();
    assert!(near(e, [6.0 + 2.0 + 1.0, 0.0]), "{e:?}");
    // scale applies to 8 displacements too
    let g = glyph_of(&[3, 2, 8, 4, 2, 0]);
    assert!(near(g.end_point(), [2.0, 1.0]));
    // a zero factor is ignored
    let g = glyph_of(&[3, 0, 0x10, 0]);
    assert!(near(g.end_point(), [1.0, 0.0]));
}

#[test]
fn codes5_6_push_pop() {
    // push at (1,0), go up, pop back, go right: two polylines
    let g = glyph_of(&[0x10, 5, 0x14, 6, 0x10, 0]);
    assert_eq!(
        lines(&g),
        vec![
            vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
            vec![[1.0, 0.0], [2.0, 0.0]]
        ]
    );
    // pop on an empty stack is ignored; deep pushes do not panic
    let mut code = vec![6u8];
    code.extend(std::iter::repeat_n(5u8, 20));
    code.extend(std::iter::repeat_n(6u8, 20));
    code.push(0);
    let g = glyph_of(&code);
    assert!(g.is_empty());
}

#[test]
fn code7_subshape_shapes_font() {
    let f = font(&[(66, &[0x20, 0]), (65, &[2, 0x10, 1, 7, 66, 7, 66, 0])]);
    let g = f.glyph(f.slot_for_code(65).unwrap());
    // pen state carries into the subshape; position continues after it
    assert_eq!(lines(g), vec![vec![[1.0, 0.0], [3.0, 0.0], [5.0, 0.0]]]);
    assert_eq!(g.advance(), 5.0);
    // missing subshape is skipped; a self reference stops at the depth limit
    let f = font(&[(65, &[7, 99, 0x10, 0]), (66, &[0x10, 7, 66, 0])]);
    assert_eq!(f.glyph(f.slot_for_code(65).unwrap()).advance(), 1.0);
    let g = f.glyph(f.slot_for_code(66).unwrap());
    assert!(g.advance() >= 10.0 && g.advance() <= 12.0);
}

#[test]
fn code7_subshape_fan_out_is_bounded() {
    // each level calls itself 8 times: 8^10 calls without a work limit
    let code = [
        0x10, 7, 65, 7, 65, 7, 65, 7, 65, 7, 65, 7, 65, 7, 65, 7, 65, 0,
    ];
    let t = std::time::Instant::now();
    let g = glyph_of(&code);
    assert!(g.advance() > 0.0);
    assert!(t.elapsed().as_secs_f64() < 2.0);
}

#[test]
fn code8_single_displacement_signed() {
    let g = glyph_of(&[8, 0xF6, 3, 8, 127, 0x80, 0]); // (-10,3) then (127,-128)
    assert_eq!(
        lines(&g),
        vec![vec![[0.0, 0.0], [-10.0, 3.0], [117.0, -125.0]]]
    );
}

#[test]
fn code9_displacements_until_zero_pair() {
    let g = glyph_of(&[9, 3, 1, 3, 2, 2, 0xFD, 0, 0, 0x10, 0]);
    assert_eq!(
        lines(&g),
        vec![vec![
            [0.0, 0.0],
            [3.0, 1.0],
            [6.0, 3.0],
            [8.0, 0.0],
            [9.0, 0.0]
        ]]
    );
}

#[test]
fn code10_octant_arc_full_circle_and_quarters() {
    // full circle radius 5 from 0 degrees: centre (-5, 0)
    let g = glyph_of(&[10, 5, 0x00, 0]);
    assert!(near(g.end_point(), [0.0, 0.0]));
    for p in g.points() {
        assert!((dist(*p, [-5.0, 0.0]) - 5.0).abs() < 1e-3);
    }
    assert!(near([g.bbox()[0], g.bbox()[3]], [-10.0, 5.0]));
    // CCW, start octant 0, 2 octants: quarter to the top
    let g = glyph_of(&[10, 4, 0x02, 0]);
    assert!(near(g.end_point(), [-4.0, 4.0]), "{:?}", g.end_point());
    // spec example: clockwise, start octant 3, 2 octants (byte = 0x80 | 0x32)
    let g = glyph_of(&[10, 1, 0xB2, 0]);
    let a0 = 135f32.to_radians();
    let c = [-a0.cos(), -a0.sin()];
    let a1 = 45f32.to_radians();
    let want = [c[0] + a1.cos(), c[1] + a1.sin()];
    assert!(near(g.end_point(), want), "{:?} {:?}", g.end_point(), want);
    // the arc goes over the top (clockwise from 135 to 45 degrees)
    assert!(g.bbox()[3] > c[1] + 0.99);
}

#[test]
fn code11_fractional_arc_spec_example() {
    // 55..95 degrees radius 3: 11,(56,28,0,3,012)
    let g = glyph_of(&[11, 56, 28, 0, 3, 0x12, 0]);
    let a0 = (45.0 + 56.0 * 45.0 / 256.0f64).to_radians();
    let a1 = (90.0 + 28.0 * 45.0 / 256.0f64).to_radians();
    let c = [-3.0 * a0.cos(), -3.0 * a0.sin()];
    let want = [
        (c[0] + 3.0 * a1.cos()) as f32,
        (c[1] + 3.0 * a1.sin()) as f32,
    ];
    assert!(near(g.end_point(), want), "{:?} {:?}", g.end_point(), want);
    let cc = [c[0] as f32, c[1] as f32];
    for p in g.points() {
        assert!((dist(*p, cc) - 3.0).abs() < 1e-3);
    }
    // high radius byte: radius = 1 * 256 + 0, end offset 0 = full last octant
    let g = glyph_of(&[11, 0, 0, 1, 0, 0x02, 0]);
    assert!(near(g.end_point(), [-256.0, 256.0]), "{:?}", g.end_point());
    // clockwise version of a quarter
    let g = glyph_of(&[11, 0, 0, 0, 10, 0x82, 0]);
    assert!(near(g.end_point(), [-10.0, -10.0]), "{:?}", g.end_point());
}

#[test]
fn code12_bulge_arc() {
    // semicircle (bulge 127) to (10,0): counter-clockwise, goes below the chord
    let g = glyph_of(&[12, 10, 0, 127, 0]);
    assert!(near(g.end_point(), [10.0, 0.0]));
    assert!((g.bbox()[1] + 5.0).abs() < 1e-3, "{:?}", g.bbox());
    for p in g.points() {
        assert!((dist(*p, [5.0, 0.0]) - 5.0).abs() < 1e-3);
    }
    // negative bulge: clockwise, goes above
    let g = glyph_of(&[12, 10, 0, 0x81, 0]);
    assert!((g.bbox()[3] - 5.0).abs() < 1e-3);
    // bulge 0 is a straight line
    let g = glyph_of(&[12, 3, 4, 0, 0]);
    assert_eq!(lines(&g), vec![vec![[0.0, 0.0], [3.0, 4.0]]]);
    // small bulge: sagitta = bulge/127 * chord / 2
    let g = glyph_of(&[12, 20, 0, 0xC1, 0]); // -63
    let s = 63.0 / 127.0 * 10.0;
    assert!((g.bbox()[3] - s).abs() < 0.02, "{:?} {s}", g.bbox());
}

#[test]
fn code13_polyarc() {
    // spec example 13,(0,5,127),(0,5,-127),(0,0): an S shape
    let g = glyph_of(&[13, 0, 5, 127, 0, 5, 0x81, 0, 0, 0x10, 0]);
    assert!(near(g.end_point(), [1.0, 10.0]));
    let b = g.bbox();
    assert!(
        (b[0] + 2.5).abs() < 1e-3 && (b[2] - 2.5).abs() < 1e-3,
        "{b:?}"
    );
    // straight segment with bulge 0 inside a polyarc
    let g = glyph_of(&[13, 4, 0, 0, 0, 4, 0, 0, 0, 0]);
    assert_eq!(lines(&g), vec![vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0]]]);
}

#[test]
fn code14_vertical_only_skips_each_command_kind() {
    // horizontal text skips the command after 14 (with all its bytes)
    let cases: Vec<Vec<u8>> = vec![
        vec![14, 8, 5, 5],
        vec![14, 9, 1, 1, 2, 2, 0, 0],
        vec![14, 10, 3, 0x02],
        vec![14, 11, 1, 2, 0, 3, 0x12],
        vec![14, 12, 4, 4, 50],
        vec![14, 13, 1, 1, 10, 2, 2, 20, 0, 0],
        vec![14, 3, 2],
        vec![14, 0x44],
        vec![14, 2],
        vec![14, 7, 66],
    ];
    for c in cases {
        let mut code = c.clone();
        code.extend_from_slice(&[0x10, 0]);
        let f = font(&[(65, &code), (66, &[0x30, 0])]);
        let s = f.slot_for_code(65).unwrap();
        let h = f.glyph(s);
        assert!(
            near(h.end_point(), [1.0, 0.0]),
            "{c:?}: {:?}",
            h.end_point()
        );
        let v = f.glyph_vertical(s);
        assert!(
            !near(v.end_point(), [1.0, 0.0]) || c[1] == 2,
            "{c:?} vertical"
        );
    }
    // 14 followed by the end code does not run past the end
    let g = glyph_of(&[0x10, 14, 0, 0x10, 0x10]);
    assert!(near(g.end_point(), [1.0, 0.0]));
}

#[test]
fn code15_and_truncated_code_are_harmless() {
    assert!(near(glyph_of(&[15, 0x10, 0]).end_point(), [1.0, 0.0]));
    for code in [
        &[8u8][..],
        &[8, 1],
        &[9, 1, 1, 2],
        &[10, 1],
        &[11, 1, 2, 3],
        &[12, 1, 1],
        &[13, 1, 1],
        &[3],
        &[7],
    ] {
        let g = glyph_of(code);
        assert!(g.points().len() <= 4, "{code:?}");
    }
}

#[test]
fn unifont_subshape_two_bytes() {
    let mut b = b"AutoCAD-86 unifont 1.0\r\n\x1a".to_vec();
    b.extend_from_slice(&3u32.to_le_bytes());
    let info = b"u\0\x0a\x02\x00\x00\x00\x00";
    b.extend_from_slice(&(info.len() as u16).to_le_bytes());
    b.extend_from_slice(info);
    let rec = |n: u16, code: &[u8], b: &mut Vec<u8>| {
        b.extend_from_slice(&n.to_le_bytes());
        b.extend_from_slice(&((code.len() + 1) as u16).to_le_bytes());
        b.push(0);
        b.extend_from_slice(code);
    };
    rec(0x4E00, &[0x20, 0], &mut b);
    rec(0x4E01, &[7, 0x4E, 0x00, 7, 0x00, 0x4E, 0], &mut b); // second one is byte-swapped
    let f = ShxFont::from_bytes(b).unwrap();
    assert_eq!(f.kind(), ShxKind::Unifont);
    let g = f.glyph(f.slot_for_char('\u{4E01}').unwrap());
    assert!(near(g.end_point(), [4.0, 0.0]));
}
