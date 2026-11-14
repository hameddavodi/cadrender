//! `cadrender._cadrender`: the compiled part of the `cadrender` Python package.
//!
//! The public API is in Python (`python/cadrender/__init__.py`); it wraps this
//! module. Everything heavy (reading the file, drawing, PNG encoding, writing
//! files) runs here with the GIL released.

mod batch;
mod doc;
mod errors;
mod options;
mod render;
mod svg;

use cadrender::{load_fonts, Document, FontChoice, OpenError};
use doc::PyDocument;
use pyo3::prelude::*;
use std::path::PathBuf;

/// Open a drawing (DXF now; DWG through the same call once the library reads it).
#[pyfunction]
#[pyo3(signature = (path, font=None))]
fn open(py: Python<'_>, path: PathBuf, font: Option<String>) -> PyResult<PyDocument> {
    let choice = font.as_deref().map(FontChoice::parse).unwrap_or(FontChoice::Auto);
    let res = py.detach(|| -> Result<Document, (String, String)> {
        let fonts = load_fonts(&choice).map_err(|e| ("OptionsError".to_string(), format!("font: {e}")))?;
        Document::open(&path, fonts).map_err(|e| {
            let class = match &e {
                OpenError::Unsupported(_) => "UnsupportedFormatError",
                _ if !path.exists() => "DrawingNotFoundError",
                _ => "OpenError",
            };
            (class.to_string(), format!("{}: {e}", path.display()))
        })
    });
    match res {
        Ok(d) => Ok(PyDocument::new(d)),
        Err((class, msg)) => Err(errors::err(py, &class, msg)),
    }
}

/// Names accepted by the `preset` option.
#[pyfunction]
fn presets() -> Vec<&'static str> { cadrender::RenderOptions::PRESETS.to_vec() }

/// All option names (keyword arguments of `render`).
#[pyfunction]
fn option_names() -> Vec<&'static str> { options::KEYS.to_vec() }

#[pymodule]
fn _cadrender(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyDocument>()?;
    m.add_function(wrap_pyfunction!(open, m)?)?;
    m.add_function(wrap_pyfunction!(presets, m)?)?;
    m.add_function(wrap_pyfunction!(option_names, m)?)?;
    m.add("SVG_AVAILABLE", svg::READY)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
