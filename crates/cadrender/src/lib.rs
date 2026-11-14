//! cadrender: open a drawing once, render its sheets many times.
//!
//! This is the small API the CLI and the Python binding use. It joins the
//! reader (DXF or DWG), the core renderer and the sinks (PNG now).
//!
//! ```no_run
//! let fonts = cadrender::load_fonts(&cadrender::FontChoice::Auto).unwrap();
//! let doc = cadrender::Document::open("drawing.dxf", fonts).unwrap();
//! let opts = cadrender::RenderOptions::preset("preview").unwrap();
//! let prep = doc.prepare(&opts);
//! let img = doc.render_png(&doc.sheets()[0].to_string(), &opts, &prep).unwrap();
//! std::fs::write("sheet.png", &img.png).unwrap();
//! ```

pub use cadrender_core::text::{FontChoice, Fonts};
pub use cadrender_core::{ColorPolicy, Prepared, RenderOptions, Scene, Stats};
use cadrender_raster::{PngFormat, PngStream, RasterSink};
pub use cadrender_svg::{SvgConfig, TextMode as SvgText};

/// Pixels per strip when a picture is drawn in strips (one strip = about 16 MB grey).
pub const STRIP_PIXELS: u64 = 16_000_000;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Fonts by the font policy: one sans font for all text styles. Arial if
/// installed, else the built-in Liberation Sans (`FontChoice::Auto`); or always
/// Liberation Sans; or a given file. `Err` only if a given file cannot be read.
pub fn load_fonts(choice: &FontChoice) -> Result<Fonts, String> { cadrender_core::text::load_fonts(choice) }

#[derive(Debug)]
pub enum OpenError {
    Unsupported(String),
    Dxf(cadrender_dxf::DxfError),
    Dwg(cadrender_dwg_scene::cadrender_dwg::DwgError),
}

impl std::fmt::Display for OpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self { OpenError::Unsupported(s) => write!(f, "{s}"), OpenError::Dxf(e) => write!(f, "{e}"), OpenError::Dwg(e) => write!(f, "{e}") }
    }
}
impl std::error::Error for OpenError {}

/// One opened drawing.
pub struct Document {
    pub scene: Scene,
    pub path: PathBuf,
    /// Time to read the file and build the scene.
    pub parse_ms: f64,
    pub size_bytes: u64,
}

/// One rendered sheet.
pub struct SheetImage {
    pub name: String,
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub empty: bool,
    pub viewports: usize,
    pub stats: Stats,
    pub warnings: Vec<String>,
    /// Drawing time (walk + rasterise).
    pub render_ms: f64,
    /// PNG encoding time.
    pub encode_ms: f64,
}

/// One sheet rendered to SVG.
pub struct SvgImage {
    pub name: String,
    /// The whole SVG file (UTF-8).
    pub svg: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub empty: bool,
    pub viewports: usize,
    pub stats: Stats,
    pub warnings: Vec<String>,
    /// Walk + writing the SVG text.
    pub render_ms: f64,
}

impl Document {
    pub fn open(path: impl AsRef<Path>, fonts: Fonts) -> Result<Document, OpenError> {
        let path = path.as_ref();
        let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let t = Instant::now();
        // DWG: by extension, or by the "AC10" magic of a DWG file with another name
        let is_dwg = ext == "dwg" || (ext != "dxf" && std::fs::File::open(path).and_then(|mut f| {
            let mut m = [0u8; 4];
            std::io::Read::read_exact(&mut f, &mut m).map(|_| &m == b"AC10")
        }).unwrap_or(false));
        let mut scene = if is_dwg {
            cadrender_dwg_scene::read_file(path, fonts).map_err(OpenError::Dwg)?
        } else {
            cadrender_dxf::read_file(path, fonts).map_err(OpenError::Dxf)?
        };
        // IMAGE pictures: their files are found next to the drawing
        cadrender_image::load_scene(&mut scene, path.parent());
        let size_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        Ok(Document { scene, path: path.to_path_buf(), parse_ms: t.elapsed().as_secs_f64() * 1e3, size_bytes })
    }

    /// Sheet (tab) names in tab order.
    pub fn sheets(&self) -> Vec<&str> { self.scene.sheet_names() }

    /// Work shared by all sheets for these options. Make it once, pass it to every render.
    pub fn prepare(&self, opts: &RenderOptions) -> Prepared { Prepared::new(&self.scene, opts) }

    /// Render one sheet to PNG bytes. Big pictures are drawn and encoded in
    /// strips of about `STRIP_PIXELS` pixels, so memory stays bounded.
    pub fn render_png(&self, sheet: &str, opts: &RenderOptions, prep: &Prepared) -> Result<SheetImage, String> {
        let t = Instant::now();
        let fmt = PngFormat::for_options(opts);
        let rows = (STRIP_PIXELS / opts.width.max(1) as u64).clamp(64, u32::MAX as u64) as u32;
        let mut enc: Option<std::io::Result<PngStream<Vec<u8>>>> = None;
        let mut encode = std::time::Duration::ZERO;
        let r = cadrender_core::render_sheet_strips::<RasterSink>(&self.scene, sheet, opts, prep, &(), rows, |strip, _, height| {
            let t = Instant::now();
            let e = enc.get_or_insert_with(|| PngStream::new(Vec::new(), strip.width(), height, fmt));
            if let Ok(s) = e { if let Err(x) = s.write_canvas(&strip.canvas) { *e = Err(x); } }
            encode += t.elapsed();
        }).map_err(|e| e.to_string())?;
        let t2 = Instant::now();
        let png = enc.ok_or("no picture")?.and_then(|s| s.finish()).map_err(|e| format!("PNG encoding: {e}"))?;
        encode += t2.elapsed();
        let total = t.elapsed();
        Ok(SheetImage {
            name: sheet.to_string(), png, width: r.width, height: r.height, empty: r.empty, viewports: r.viewports,
            stats: r.stats, warnings: r.warnings, render_ms: (total - encode).as_secs_f64() * 1e3, encode_ms: encode.as_secs_f64() * 1e3,
        })
    }

    /// Like `render_png`, but a panic inside the renderer becomes an error for
    /// this sheet only.
    pub fn render_png_isolated(&self, sheet: &str, opts: &RenderOptions, prep: &Prepared) -> Result<SheetImage, String> {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.render_png(sheet, opts, prep))) {
            Ok(r) => r,
            Err(p) => Err(format!("renderer crashed: {}", p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "unknown panic".into()))),
        }
    }

    /// Render one sheet to SVG. `opts` are the PNG options; `svg` makes the
    /// level of detail weaker (a viewer can zoom) or turns it off. The same
    /// `prep` as for PNG works.
    pub fn render_svg(&self, sheet: &str, opts: &RenderOptions, prep: &Prepared, svg: &SvgConfig) -> Result<SvgImage, String> {
        let t = Instant::now();
        let o = svg.render_options(opts);
        let r = cadrender_core::render_sheet::<cadrender_svg::SvgSink>(&self.scene, sheet, &o, prep, svg).map_err(|e| e.to_string())?;
        let bytes = r.sink.finish();
        Ok(SvgImage { name: sheet.to_string(), svg: bytes, width: r.width, height: r.height, empty: r.empty, viewports: r.viewports, stats: r.stats, warnings: r.warnings, render_ms: t.elapsed().as_secs_f64() * 1e3 })
    }

    /// Like `render_svg`, but a panic becomes an error for this sheet only.
    pub fn render_svg_isolated(&self, sheet: &str, opts: &RenderOptions, prep: &Prepared, svg: &SvgConfig) -> Result<SvgImage, String> {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.render_svg(sheet, opts, prep, svg))) {
            Ok(r) => r,
            Err(p) => Err(format!("renderer crashed: {}", p.downcast_ref::<&str>().map(|s| s.to_string()).or_else(|| p.downcast_ref::<String>().cloned()).unwrap_or_else(|| "unknown panic".into()))),
        }
    }

    /// Render many sheets to SVG in parallel, like `render_many`.
    pub fn render_many_svg<R: Send>(&self, sheets: &[String], opts: &RenderOptions, prep: &Prepared, svg: &SvgConfig, f: impl Fn(&str, Result<SvgImage, String>) -> R + Sync) -> Vec<R> {
        use rayon::prelude::*;
        sheets.par_iter().map(|s| f(s, self.render_svg_isolated(s, opts, prep, svg))).collect()
    }

    /// Render many sheets in parallel (rayon). `f` is called for each finished
    /// sheet (from worker threads) and its result is returned, in tab order.
    pub fn render_many<R: Send>(&self, sheets: &[String], opts: &RenderOptions, prep: &Prepared, f: impl Fn(&str, Result<SheetImage, String>) -> R + Sync) -> Vec<R> {
        use rayon::prelude::*;
        sheets.par_iter().map(|s| f(s, self.render_png_isolated(s, opts, prep))).collect()
    }
}

/// File name for a sheet picture: `<stem>__<sheet>.<ext>`, with path separators
/// in the sheet name replaced.
pub fn sheet_file_name(stem: &str, sheet: &str, ext: &str) -> String {
    let clean: String = sheet.chars().map(|c| if matches!(c, '/' | '\\' | '\0') { '_' } else { c }).collect();
    format!("{stem}__{clean}.{ext}")
}

#[cfg(test)]
mod tests {
    #[test]
    fn file_names() {
        assert_eq!(super::sheet_file_name("a", "01/02", "png"), "a__01_02.png");
    }
}
