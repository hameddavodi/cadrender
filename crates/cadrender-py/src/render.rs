//! Render one sheet in one or more formats, without Python (runs with the
//! GIL released). The result is plain Rust data; `to_dict` turns it into a
//! Python dict afterwards.

use crate::svg;
use cadrender::{Document, Prepared, RenderOptions, Stats};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format { Png, Svg }

impl Format {
    pub fn parse(s: &str) -> Option<Format> {
        match s.to_ascii_lowercase().trim_start_matches('.') { "png" => Some(Format::Png), "svg" => Some(Format::Svg), _ => None }
    }
    pub fn ext(self) -> &'static str { match self { Format::Png => "png", Format::Svg => "svg" } }
}

/// What happened to one sheet.
#[derive(Default)]
pub struct SheetResult {
    pub name: String,
    pub ok: bool,
    pub skipped: bool,
    pub error: Option<String>,
    pub files: Vec<PathBuf>,
    pub width: u32,
    pub height: u32,
    pub empty: bool,
    pub viewports: usize,
    pub warnings: Vec<String>,
    pub stats: Stats,
    /// Total for this sheet: render + encode + write, all formats.
    pub ms: f64,
    pub render_ms: f64,
    pub encode_ms: f64,
    pub write_ms: f64,
}

impl SheetResult {
    pub fn failed(name: &str, error: String) -> SheetResult { SheetResult { name: name.into(), error: Some(error), ..Default::default() } }
}

/// Render `sheet` in one format to bytes. Panics in the renderer become `Err`.
pub fn render_bytes(doc: &Document, sheet: &str, fmt: Format, opts: &RenderOptions, prep: &Prepared, r: &mut SheetResult) -> Result<Vec<u8>, String> {
    match fmt {
        Format::Png => {
            let img = doc.render_png_isolated(sheet, opts, prep)?;
            r.width = img.width; r.height = img.height; r.empty = img.empty; r.viewports = img.viewports;
            r.render_ms += img.render_ms; r.encode_ms += img.encode_ms;
            r.stats = img.stats; r.warnings = img.warnings;
            Ok(img.png)
        }
        Format::Svg => {
            let t = Instant::now();
            let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| svg::render(doc, sheet, opts, prep)))
                .unwrap_or_else(|_| Err("SVG renderer crashed".into()));
            r.render_ms += t.elapsed().as_secs_f64() * 1e3;
            out
        }
    }
}

/// Render `sheet` in every format and write `files[i]` for `formats[i]`.
pub fn render_files(doc: &Document, sheet: &str, formats: &[Format], files: &[PathBuf], opts: &RenderOptions, prep: &Prepared) -> SheetResult {
    let t0 = Instant::now();
    let mut r = SheetResult { name: sheet.into(), ..Default::default() };
    for (fmt, path) in formats.iter().zip(files) {
        let bytes = match render_bytes(doc, sheet, *fmt, opts, prep, &mut r) {
            Ok(b) => b,
            Err(e) => { r.error = Some(e); break; }
        };
        let tw = Instant::now();
        if let Err(e) = write_atomic(path, &bytes) { r.error = Some(format!("{}: {e}", path.display())); break; }
        r.write_ms += tw.elapsed().as_secs_f64() * 1e3;
        r.files.push(path.clone());
    }
    r.ok = r.error.is_none();
    r.ms = t0.elapsed().as_secs_f64() * 1e3;
    r
}

/// Write to `<path>.tmp` first, then rename: a reader (or `skip_existing`)
/// never sees half a picture, even if the process is killed.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| { let _ = std::fs::remove_file(&tmp); })
}

/// One sheet's result as a Python dict.
pub fn to_dict<'py>(py: Python<'py>, r: &SheetResult) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("sheet", &r.name)?;
    d.set_item("ok", r.ok)?;
    d.set_item("skipped", r.skipped)?;
    d.set_item("error", r.error.as_deref())?;
    d.set_item("files", r.files.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>())?;
    d.set_item("width", r.width)?;
    d.set_item("height", r.height)?;
    d.set_item("empty", r.empty)?;
    d.set_item("viewports", r.viewports)?;
    d.set_item("warnings", &r.warnings)?;
    d.set_item("ms", round3(r.ms))?;
    d.set_item("render_ms", round3(r.render_ms))?;
    d.set_item("encode_ms", round3(r.encode_ms))?;
    d.set_item("write_ms", round3(r.write_ms))?;
    let s = &r.stats;
    let st = PyDict::new(py);
    for (k, v) in [("visited", s.visited), ("drawn", s.drawn), ("culled", s.culled), ("tiny", s.tiny), ("hatch_lines", s.hatch_lines), ("hatch_dense", s.hatch_dense), ("glyphs", s.glyphs), ("flushes", s.flushes)] {
        st.set_item(k, v)?;
    }
    d.set_item("stats", st)?;
    Ok(d)
}

pub fn round3(ms: f64) -> f64 { (ms * 1000.0).round() / 1000.0 }
