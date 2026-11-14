//! The one switch for SVG output.
//!
//! SVG comes from `Document::render_svg` in `crates/cadrender`, with the
//! default `SvgConfig` (real `<text>`, weaker level of detail than PNG).

use cadrender::{Document, Prepared, RenderOptions, SvgConfig};

/// `true` once `render` below really makes SVG.
pub const READY: bool = true;

/// Render one sheet to SVG bytes (UTF-8 text).
pub fn render(doc: &Document, sheet: &str, opts: &RenderOptions, prep: &Prepared) -> Result<Vec<u8>, String> {
    doc.render_svg_isolated(sheet, opts, prep, &SvgConfig::default()).map(|img| img.svg)
}

pub const NOT_READY_MSG: &str = "SVG output is not available in this build of cadrender yet (the SVG renderer is still being built); use format=\"png\"";
