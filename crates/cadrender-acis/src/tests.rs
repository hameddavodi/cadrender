use super::*;
use cadrender_core::geom::BBox;

fn data(name: &str) -> Vec<u8> {
    std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data").join(name)).unwrap()
}

fn close(a: BBox, b: (f64, f64, f64, f64)) -> bool {
    (a.x0 - b.0).abs() < 1e-6 && (a.y0 - b.1).abs() < 1e-6 && (a.x1 - b.2).abs() < 1e-6 && (a.y1 - b.3).abs() < 1e-6
}

#[test]
fn cube_sat_and_sab_give_the_same_outline() {
    for name in ["cube.sat", "cube.sab"] {
        let o = outline(&data(name)).expect(name);
        assert_eq!((o.edges, o.skipped), (12, 0), "{name}");
        // body transform moves the -1..1 cube to (10, 20)
        assert!(close(o.path.bbox(), (9.0, 19.0, 11.0, 21.0)), "{name}: {:?}", o.path.bbox());
    }
}

const HEAD: &str = "700 0 1 0\n@8 cadrtest @12 ACIS 32.0 NT @24 Sat Nov 14 08:58:12 2026\n1 9.9999999999999995e-007 1e-010\n";

/// A full circle (one edge, one vertex) and a half arc of radius 5.
fn circles(half_sense: &str) -> String {
    format!("{HEAD}body $-1 -1 $-1 $-1 $-1 $-1 #
edge $-1 -1 $-1 $2 0 $2 6.2831853 $-1 $4 forward @7 unknown #
vertex $-1 -1 $-1 $1 $3 #
point $-1 -1 $-1 15 0 0 #
ellipse-curve $-1 -1 $-1 10 0 0 0 0 1 5 0 0 1 I I #
edge $-1 -1 $-1 $6 0 $7 3.1415926 $-1 $10 {half_sense} @7 unknown #
vertex $-1 -1 $-1 $5 $8 #
vertex $-1 -1 $-1 $5 $9 #
point $-1 -1 $-1 35 0 0 #
point $-1 -1 $-1 25 0 0 #
ellipse-curve $-1 -1 $-1 30 0 0 0 0 1 5 0 0 1 I I #
edge $-1 -1 $-1 $6 0 $7 1 $-1 $12 forward @7 unknown #
intcurve-curve $-1 -1 $-1 forward {{ exactcur nubs 3 open 2 0 3 1 3 }} I I #
End-of-ACIS-data
")
}

#[test]
fn circle_and_arcs() {
    let o = outline(circles("forward").as_bytes()).unwrap();
    assert_eq!((o.edges, o.skipped), (2, 1), "the spline edge is counted, not drawn");
    let b = o.path.bbox();
    // full circle 5..15, upper half arc 25..35 going counter-clockwise from (35,0) to (25,0)
    assert!(close(b, (5.0, -5.0, 35.0, 5.0)), "{b:?}");
    let o = outline(circles("reversed").as_bytes()).unwrap();
    // a reversed edge is the lower half: the circle still reaches y = 5
    let mut arc = PathDataCheck::default();
    for p in &o.path.pts { if p[0] > 20.0 { arc.add(p[1]); } }
    assert!(arc.max < 1e-9 && arc.min < -4.9, "{arc:?}");
    let o = outline(circles("forward").as_bytes()).unwrap();
    let mut arc = PathDataCheck::default();
    for p in &o.path.pts { if p[0] > 20.0 { arc.add(p[1]); } }
    assert!(arc.min > -1e-9 && arc.max > 4.9, "{arc:?}");
}

#[derive(Debug)]
struct PathDataCheck { min: f64, max: f64 }
impl Default for PathDataCheck { fn default() -> Self { PathDataCheck { min: f64::MAX, max: f64::MIN } } }
impl PathDataCheck { fn add(&mut self, y: f64) { self.min = self.min.min(y); self.max = self.max.max(y); } }

#[test]
fn decipher_undoes_the_cipher() {
    let plain = "body $-1 -1 @7 unknown #";
    let enc: Vec<u8> = plain.bytes().map(|c| if c == b' ' { c } else { 159 - c }).collect();
    assert_eq!(decipher(&enc, false), plain);
    // 'A' is stored as '^', which DXF writes as "^ "
    let enc: Vec<u8> = b"^ ".to_vec();
    assert_eq!(decipher(&enc, true), "A");
    assert_eq!(decipher(b"^ ", false), "A ");
}

#[test]
fn garbage_is_safe() {
    assert!(outline(b"").is_none());
    assert!(outline(b"ACIS BinaryFile\x01\x02").is_none());
    let o = outline(format!("{HEAD}edge $-1 -1 $-1 $99 0 $-5 #\n").as_bytes()).unwrap();
    assert_eq!((o.edges, o.skipped), (0, 1));
}
