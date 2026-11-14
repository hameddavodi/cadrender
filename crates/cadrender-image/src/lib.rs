//! cadrender-image: load the picture files of IMAGE entities.
//!
//! The IMAGEDEF file name is often a path on the drafter's machine
//! (`C:\...\photo.jpg`) or relative to the drawing (`.\IMAGES\photo.jpg`).
//! [`resolve`] tries, in order: the path as it is, the path relative to the
//! folder of the drawing, and the bare file name in that folder (names
//! compared without case, like Windows). PNG and JPEG are read.

use cadrender_core::entity::image::Picture;
use cadrender_core::entity::Kind;
use cadrender_core::Scene;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Biggest picture we decode (pixels): about 256 MB as RGBA.
const MAX_PIXELS: u64 = 64_000_000;

/// Find the file of an IMAGEDEF name. `dir` = folder of the drawing.
pub fn resolve(name: &str, dir: Option<&Path>) -> Option<PathBuf> {
    let name = name.trim();
    if name.is_empty() { return None; }
    let unix = name.replace('\\', "/");
    let p = PathBuf::from(&unix);
    if p.is_absolute() && p.is_file() { return Some(p); }
    let dir = dir?;
    let rel = unix.trim_start_matches("./");
    // a Windows drive path ("C:/...") is not relative to anything here
    let has_drive = rel.len() > 1 && rel.as_bytes()[1] == b':';
    if !has_drive && !rel.starts_with('/') {
        let q = dir.join(rel);
        if q.is_file() { return Some(q); }
        if let Some(q) = find_nocase(dir, rel) { return Some(q); }
    }
    let file = rel.rsplit('/').next()?;
    let q = dir.join(file);
    if q.is_file() { return Some(q); }
    find_nocase(dir, file)
}

/// `rel` below `dir`, each part compared without case.
fn find_nocase(dir: &Path, rel: &str) -> Option<PathBuf> {
    let mut cur = dir.to_path_buf();
    for part in rel.split('/').filter(|s| !s.is_empty() && *s != ".") {
        let want = part.to_lowercase();
        let hit = std::fs::read_dir(&cur).ok()?.flatten().find(|e| e.file_name().to_string_lossy().to_lowercase() == want)?;
        cur = hit.path();
    }
    cur.is_file().then_some(cur)
}

/// Decode a PNG or JPEG file.
pub fn load_file(path: &Path) -> Result<Picture, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    decode(bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// Decode PNG or JPEG bytes (found by their signature).
pub fn decode(bytes: Vec<u8>) -> Result<Picture, String> {
    if bytes.starts_with(b"\x89PNG") { decode_png(bytes) } else if bytes.starts_with(&[0xFF, 0xD8]) { decode_jpeg(bytes) } else { Err("not a PNG or JPEG file".into()) }
}

fn too_big(w: u32, h: u32) -> Result<(), String> {
    if w == 0 || h == 0 || w as u64 * h as u64 > MAX_PIXELS { Err(format!("picture size {w} x {h} not supported")) } else { Ok(()) }
}

fn decode_png(bytes: Vec<u8>) -> Result<Picture, String> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(&bytes[..]));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().map_err(|e| e.to_string())?;
    let (w, h) = (r.info().width, r.info().height);
    too_big(w, h)?;
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let n = w as usize * h as usize;
    let mut rgba = Vec::with_capacity(n * 4);
    let px = &buf[..info.buffer_size()];
    match info.color_type {
        png::ColorType::Grayscale => for &g in px.iter().take(n) { rgba.extend_from_slice(&[g, g, g, 255]); },
        png::ColorType::GrayscaleAlpha => for p in px.chunks_exact(2) { rgba.extend_from_slice(&[p[0], p[0], p[0], p[1]]); },
        png::ColorType::Rgb => for p in px.chunks_exact(3) { rgba.extend_from_slice(&[p[0], p[1], p[2], 255]); },
        png::ColorType::Rgba => rgba.extend_from_slice(px),
        png::ColorType::Indexed => return Err("indexed PNG not expanded".into()),
    }
    Ok(picture(w, h, rgba, bytes, "image/png"))
}

fn decode_jpeg(bytes: Vec<u8>) -> Result<Picture, String> {
    let mut d = jpeg_decoder::Decoder::new(std::io::Cursor::new(&bytes[..]));
    d.read_info().map_err(|e| e.to_string())?;
    let info = d.info().ok_or("no JPEG header")?;
    let (w, h) = (info.width as u32, info.height as u32);
    too_big(w, h)?;
    let px = d.decode().map_err(|e| e.to_string())?;
    let n = w as usize * h as usize;
    let mut rgba = Vec::with_capacity(n * 4);
    use jpeg_decoder::PixelFormat as F;
    match info.pixel_format {
        F::L8 => for &g in px.iter().take(n) { rgba.extend_from_slice(&[g, g, g, 255]); },
        F::L16 => for p in px.chunks_exact(2) { rgba.extend_from_slice(&[p[0], p[0], p[0], 255]); },
        F::RGB24 => for p in px.chunks_exact(3) { rgba.extend_from_slice(&[p[0], p[1], p[2], 255]); },
        // Adobe CMYK JPEGs store inverted values
        F::CMYK32 => for p in px.chunks_exact(4) {
            let k = p[3] as u32;
            rgba.extend_from_slice(&[(p[0] as u32 * k / 255) as u8, (p[1] as u32 * k / 255) as u8, (p[2] as u32 * k / 255) as u8, 255]);
        },
    }
    Ok(picture(w, h, rgba, bytes, "image/jpeg"))
}

fn picture(w: u32, h: u32, mut rgba: Vec<u8>, encoded: Vec<u8>, mime: &'static str) -> Picture {
    rgba.resize(w as usize * h as usize * 4, 255);
    let gray = rgba.chunks_exact(4).all(|p| p[0] == p[1] && p[1] == p[2]);
    Picture { w, h, rgba, gray, encoded, mime }
}

/// Load the pictures of all IMAGE entities of a scene. Each file is read
/// once. Missing or broken files are counted (`IMAGE(file missing)`) and
/// get a warning; those IMAGEs draw only their frame.
pub fn load_scene(scene: &mut Scene, drawing_dir: Option<&Path>) {
    if scene.stats.counts.get("IMAGE").copied().unwrap_or(0) == 0 { return; }
    let mut cache: HashMap<String, Option<Arc<Picture>>> = HashMap::new();
    let mut problems: Vec<String> = vec![];
    for b in scene.blocks.iter_mut() {
        for e in b.ents.iter_mut() {
            let Kind::Image(img) = &mut e.kind else { continue };
            let pic = cache.entry(img.file.clone()).or_insert_with(|| {
                let r = resolve(&img.file, drawing_dir).ok_or_else(|| format!("image file not found: {}", img.file)).and_then(|p| load_file(&p));
                match r { Ok(p) => Some(Arc::new(p)), Err(m) => { problems.push(m); None } }
            });
            img.picture = pic.clone();
        }
    }
    for m in problems {
        *scene.stats.skipped.entry("IMAGE(file missing)".into()).or_default() += 1;
        if scene.stats.warnings.len() < 100 { scene.stats.warnings.push(m); }
    }
}

#[cfg(test)]
mod tests;
