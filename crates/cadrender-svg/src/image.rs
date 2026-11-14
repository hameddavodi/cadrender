//! IMAGE pictures as `<image>` with the file embedded as a data URI.
//!
//! Why a data URI and not a link to the file: the SVG then stands alone
//! (it can be mailed, served or opened anywhere), and the IMAGEDEF path is
//! usually a path on the drafter's machine that means nothing elsewhere.
//! The PNG/JPEG bytes are embedded as they are (no re-encoding).
//! Brightness, contrast, grey and fade become one SVG filter; the clip
//! polygon a `<clipPath>` (in page pixels, so it sits on a `<g>` around the
//! transformed image).

use crate::num::{write_num, PathWriter};
use cadrender_core::sink::ImageDraw;

pub(crate) fn write(b: &mut Vec<u8>, id: u32, dec: u8, img: &ImageDraw<'_>) -> bool {
    let pic = img.pic;
    if pic.encoded.is_empty() || pic.w == 0 || pic.h == 0 || img.clip.len() < 3 { return false; }
    let mut p = PathWriter::new(dec);
    for (i, q) in img.clip.iter().enumerate() { if i == 0 { p.move_to(q[0], q[1]) } else { p.line_to(q[0], q[1]) } }
    p.close();
    b.extend_from_slice(format!("<clipPath id=\"i{id}\"><path d=\"").as_bytes());
    b.extend_from_slice(&p.d);
    b.extend_from_slice(b"\"/></clipPath>\n");
    let filter = write_filter(b, id, img);
    b.extend_from_slice(format!("<g clip-path=\"url(#i{id})\"><image").as_bytes());
    if filter { b.extend_from_slice(format!(" filter=\"url(#f{id})\"").as_bytes()); }
    b.extend_from_slice(format!(" width=\"{}\" height=\"{}\" preserveAspectRatio=\"none\" transform=\"matrix(", pic.w, pic.h).as_bytes());
    for (k, v) in img.m.0.iter().enumerate() { if k > 0 { b.push(b' '); } write_num(b, *v, 6); }
    b.extend_from_slice(b")\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" xlink:href=\"data:");
    b.extend_from_slice(pic.mime.as_bytes());
    b.extend_from_slice(b";base64,");
    base64(b, &pic.encoded);
    b.extend_from_slice(b"\"/></g>\n");
    true
}

/// The filter for brightness/contrast, grey and fade; false if none is needed.
fn write_filter(b: &mut Vec<u8>, id: u32, img: &ImageDraw<'_>) -> bool {
    let (k, d) = img.linear();
    let bc = (k - 1.0).abs() > 1e-4 || d.abs() > 1e-4;
    let f = img.fade.min(100) as f32 / 100.0;
    if !bc && !img.gray && f == 0.0 { return false; }
    b.extend_from_slice(format!("<filter id=\"f{id}\" color-interpolation-filters=\"sRGB\">").as_bytes());
    let funcs = |b: &mut Vec<u8>, k: [f32; 3], d: [f32; 3]| {
        b.extend_from_slice(b"<feComponentTransfer>");
        for (c, name) in ["R", "G", "B"].iter().enumerate() {
            b.extend_from_slice(format!("<feFunc{name} type=\"linear\" slope=\"").as_bytes());
            write_num(b, k[c] as f64, 4);
            b.extend_from_slice(b"\" intercept=\"");
            write_num(b, d[c] as f64, 4);
            b.extend_from_slice(b"\"/>");
        }
        b.extend_from_slice(b"</feComponentTransfer>");
    };
    if bc { funcs(b, [k; 3], [d; 3]); }
    if img.gray { b.extend_from_slice(b"<feColorMatrix type=\"saturate\" values=\"0\"/>"); }
    if f > 0.0 {
        let bg = img.background;
        let c = [bg.r, bg.g, bg.b].map(|v| v as f32 / 255.0 * f);
        funcs(b, [1.0 - f; 3], c);
    }
    b.extend_from_slice(b"</filter>\n");
    true
}

/// Standard base64 (with padding).
fn base64(out: &mut Vec<u8>, data: &[u8]) {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    out.reserve(data.len().div_ceil(3) * 4);
    for c in data.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63]);
        out.push(T[(n >> 12) as usize & 63]);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] } else { b'=' });
        out.push(if c.len() > 2 { T[n as usize & 63] } else { b'=' });
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn base64_matches_rfc4648() {
        for (i, o) in [("", ""), ("f", "Zg=="), ("fo", "Zm8="), ("foo", "Zm9v"), ("foobar", "Zm9vYmFy")] {
            let mut v = vec![];
            super::base64(&mut v, i.as_bytes());
            assert_eq!(String::from_utf8(v).unwrap(), o);
        }
    }
}
