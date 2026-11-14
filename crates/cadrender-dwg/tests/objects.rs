//! Per-object tests with tiny hand-built byte fixtures (no real drawings needed).
mod common;

use cadrender_dwg::objects::{decode_object, ClassTable};
use cadrender_dwg::{Color, EntityKind, LtypeRef, ObjectKind, Version};
use common::{entity_common, object, BitWriter};

fn decode(buf: &[u8]) -> cadrender_dwg::Result<(cadrender_dwg::Object, cadrender_dwg::objects::DecodeInfo)> {
    decode_object(buf, 0, Version::R2018, &ClassTable::new(&[]), true)
}

fn line_fixture() -> Vec<u8> {
    let mut d = BitWriter::new();
    entity_common(&mut d, 0x13, 0x2A5);
    d.bit(true);
    d.rd(1.5); // start x
    d.bb(3).rd(10.0); // end x (DD, full RD)
    d.rd(2.5); // start y
    d.dd_default(); // end y = start y
    d.bit(true); // thickness 0
    d.bit(true); // extrusion 0,0,1
    let mut h = BitWriter::new();
    h.h(5, 0x10); // layer
    object(&d, &BitWriter::new(), &h)
}

#[test]
fn line_decodes_and_uses_all_bits() {
    let buf = line_fixture();
    let (o, info) = decode(&buf).unwrap();
    assert_eq!(o.handle, 0x2A5);
    assert_eq!(o.type_code, 0x13);
    assert!(info.crc_ok);
    assert_eq!(info.data_left, 0);
    assert!(info.handles_left < 8);
    let e = o.entity().unwrap();
    assert_eq!(e.common.layer, 0x10);
    assert_eq!(e.common.ltype, LtypeRef::ByLayer);
    assert_eq!(e.common.color.color, Color::ByLayer);
    assert_eq!(e.common.lineweight_mm100(), -1);
    match &e.kind {
        EntityKind::Line(l) => {
            assert_eq!(l.start, [1.5, 2.5, 0.0]);
            assert_eq!(l.end, [10.0, 2.5, 0.0]);
            assert_eq!(l.extrusion, [0.0, 0.0, 1.0]);
        }
        k => panic!("not a line: {k:?}"),
    }
}

#[test]
fn circle_with_true_color() {
    // Common data written by hand here, with an RGB colour.
    let mut d = BitWriter::new();
    d.bb(0).rc(0x12).h(0, 0x30).bs(0).bit(false).bb(2).bl(0).bit(true).bit(false);
    d.bs(0x8000u16 as i16).bl(0xC2FF8000u32 as i32); // ENC: true colour
    d.bd(1.0).bb(0).bb(0).rc(0).bb(0).bit(false).bit(false).bit(false).bs(0).rc(5);
    d.bd(3.0).bd(4.0).bd(0.0); // center
    d.bd(2.0); // radius
    d.bit(true).bit(true); // thickness, extrusion
    let mut h = BitWriter::new();
    h.h(5, 0x10);
    let (o, info) = decode(&object(&d, &BitWriter::new(), &h)).unwrap();
    assert_eq!(info.data_left, 0);
    let e = o.entity().unwrap();
    assert_eq!(e.common.color.color, Color::Rgb(0xFF8000));
    assert_eq!(e.common.lineweight_mm100(), 18);
    match &e.kind {
        EntityKind::Circle(c) => {
            assert_eq!(c.center, [3.0, 4.0, 0.0]);
            assert_eq!(c.radius, 2.0);
        }
        k => panic!("not a circle: {k:?}"),
    }
}

#[test]
fn book_color_reads_no_rgb_and_takes_a_handle() {
    let mut d = BitWriter::new();
    d.bb(0).rc(0x1B).h(0, 0x31).bs(0).bit(false).bb(2).bl(0).bit(true).bit(false);
    d.bs(0xC000u16 as i16); // ENC: book colour, no RGB follows
    d.bd(1.0).bb(0).bb(0).rc(0).bb(0).bit(false).bit(false).bit(false).bs(0).rc(29);
    d.bd(1.0).bd(2.0).bd(0.0); // POINT position
    d.bit(true).bit(true).bd(0.0);
    let mut h = BitWriter::new();
    h.h(5, 0x99); // DBCOLOR
    h.h(5, 0x10); // layer
    let (o, info) = decode(&object(&d, &BitWriter::new(), &h)).unwrap();
    assert_eq!(info.data_left, 0);
    let e = o.entity().unwrap();
    assert_eq!(e.common.color_book, Some(0x99));
    assert_eq!(e.common.layer, 0x10);
}

fn lwpolyline_fixture() -> Vec<u8> {
    let mut d = BitWriter::new();
    entity_common(&mut d, 0x4D, 0x40);
    d.bs(0x200 | 16); // closed, has bulges
    d.bl(3); // vertices
    d.bl(3); // bulges
    d.rd(0.0).rd(0.0); // first vertex
    d.bb(3).rd(5.0).dd_default(); // (5, 0)
    d.dd_default().bb(3).rd(5.0); // (5, 5)
    d.bd(0.0).bd(1.0).bd(0.0); // bulges
    let mut h = BitWriter::new();
    h.h(5, 0x10);
    object(&d, &BitWriter::new(), &h)
}

#[test]
fn lwpolyline_vertices_and_bulges() {
    let (o, info) = decode(&lwpolyline_fixture()).unwrap();
    assert_eq!(info.data_left, 0);
    match &o.entity().unwrap().kind {
        EntityKind::LwPolyline(p) => {
            assert!(p.closed());
            let pts: Vec<_> = p.vertices.iter().map(|v| v.point).collect();
            assert_eq!(pts, vec![[0.0, 0.0], [5.0, 0.0], [5.0, 5.0]]);
            assert_eq!(p.vertices[1].bulge, 1.0);
        }
        k => panic!("not a lwpolyline: {k:?}"),
    }
}

fn layer_fixture() -> Vec<u8> {
    let mut d = BitWriter::new();
    d.bb(0).rc(0x33).h(0, 0x10).bs(0); // LAYER, handle, no EED
    d.bl(0).bit(true).bit(false); // reactors, xdic missing, DS
    d.bs(0); // xref dependent
    d.bs(2 | (5 << 5)); // off, lineweight index 5
    d.bs(0).bl(0xC3000001u32 as i32).rc(0); // CMC: ACI 1
    let mut s = BitWriter::new();
    s.tu("Walls");
    let mut h = BitWriter::new();
    h.h(4, 0x2); // owner (layer control)
    h.h(5, 0); // xref block
    h.h(5, 0xF); // plotstyle
    h.h(5, 0); // material
    h.h(5, 0x16); // linetype
    h.h(5, 0); // unknown (R2013+)
    object(&d, &s, &h)
}

#[test]
fn layer_reads_name_from_string_stream() {
    let (o, info) = decode(&layer_fixture()).unwrap();
    assert_eq!(info.data_left, 0);
    assert!(info.handles_left < 8);
    assert_eq!(o.owner, 0x2);
    match &o.kind {
        ObjectKind::Layer(l) => {
            assert_eq!(l.name, "Walls");
            assert!(!l.on);
            assert_eq!(l.lineweight_raw, 5);
            assert_eq!(l.color, Color::Index(1));
            assert_eq!(l.ltype, 0x16);
        }
        k => panic!("not a layer: {k:?}"),
    }
}

#[test]
fn r2004_layer_has_8bit_name_xref_bits_and_no_material() {
    // R2004: name in the data stream (T), xref data as B BS B, no material handle
    let mut d = BitWriter::new();
    d.bl(0).bit(true); // reactors, xdic missing (no DS bit before R2013)
    d.t8("Walls");
    d.bit(false).bs(0).bit(false); // 64-flag, xref index + 1, xref dependent
    d.bs(2 | (5 << 5)); // off, lineweight index 5
    d.bs(0).bl(0xC3000001u32 as i32).rc(0); // CMC: ACI 1
    let mut h = BitWriter::new();
    h.h(4, 0x2); // owner (layer control)
    h.h(5, 0); // xref block
    h.h(5, 0xF); // plotstyle
    h.h(5, 0x16); // linetype
    let buf = common::object_r2004(0x33, 0x10, &d, &h);
    let (o, info) = decode_object(&buf, 0, Version::R2004, &ClassTable::new(&[]), true).unwrap();
    assert_eq!(info.data_left, 0);
    assert!(info.handles_left < 8);
    assert!(info.crc_ok);
    match &o.kind {
        ObjectKind::Layer(l) => {
            assert_eq!(l.name, "Walls");
            assert!(!l.on);
            assert_eq!(l.lineweight_raw, 5);
            assert_eq!(l.color, Color::Index(1));
            assert_eq!(l.ltype, 0x16);
        }
        k => panic!("not a layer: {k:?}"),
    }
}

#[test]
fn bad_crc_is_reported_not_fatal() {
    let mut buf = line_fixture();
    let n = buf.len();
    buf[n - 10] ^= 0xff; // the CRC bytes sit before the 8 padding bytes
    let (_, info) = decode(&buf).unwrap();
    assert!(!info.crc_ok);
}

/// Every truncation and every single-bit flip of every fixture must give
/// an object or an error, never a panic.
#[test]
fn damaged_fixtures_never_panic() {
    for f in [line_fixture(), lwpolyline_fixture(), layer_fixture()] {
        for cut in 0..f.len() {
            let _ = decode(&f[..cut]);
        }
        for i in 0..(f.len() - 8) * 8 {
            let mut g = f.clone();
            g[i / 8] ^= 0x80 >> (i % 8);
            let _ = decode(&g);
        }
    }
}
