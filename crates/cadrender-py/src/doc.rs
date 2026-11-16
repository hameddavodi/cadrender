//! `cadrender._cadrender.Document`: one opened drawing.
//!
//! The Python class `cadrender.Document` wraps this one and adds the nice
//! keyword arguments. The scene is read once and shared by all renders.

use crate::render::{self, Format, SheetResult};
use crate::{batch, errors, options, svg};
use cadrender::{Document, Prepared, RenderOptions};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[pyclass(frozen, name = "Document", module = "cadrender._cadrender")]
pub struct PyDocument {
    pub doc: Arc<Document>,
    /// `Prepared` for the last options used (clean flags, colour table).
    /// Most callers render all sheets with the same options, so one entry is enough.
    prep: Mutex<Option<(String, Arc<Prepared>)>>,
}

impl PyDocument {
    pub fn new(doc: Document) -> PyDocument { PyDocument { doc: Arc::new(doc), prep: Mutex::new(None) } }

    /// `Prepared` for these options, from the cache when the options match.
    pub fn prepared(&self, opts: &RenderOptions) -> Arc<Prepared> {
        let mut key = opts.clone();
        key.width = 0; // the width does not change `Prepared`
        let key = format!("{key:?}");
        let mut slot = self.prep.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((k, p)) = slot.as_ref() { if *k == key { return p.clone(); } }
        let p = Arc::new(self.doc.prepare(opts));
        *slot = Some((key, p.clone()));
        p
    }

    /// A metadata filter: a sheet, "Model" or a block name.
    fn check_owner(&self, py: Python<'_>, sheet: &str) -> PyResult<()> {
        let sc = &self.doc.scene;
        if sheet == "Model" || (0..sc.blocks.len()).any(|b| sc.block_name(b as u32) == sheet) { return Ok(()); }
        self.check_sheet(py, sheet)
    }

    pub fn check_sheet(&self, py: Python<'_>, sheet: &str) -> PyResult<()> {
        if self.doc.sheets().contains(&sheet) { return Ok(()); }
        Err(errors::err(py, "SheetNotFoundError", format!("no sheet {sheet:?} in {}; sheets: {:?}", self.doc.path.display(), self.doc.sheets())))
    }
}

/// Check a format name. SVG raises `NotImplementedError` until the switch in `svg.rs` is on.
pub fn format(py: Python<'_>, s: &str) -> PyResult<Format> {
    let f = Format::parse(s).ok_or_else(|| errors::options(py, format!("unknown format {s:?}; use \"png\" or \"svg\"")))?;
    if f == Format::Svg && !svg::READY { return Err(pyo3::exceptions::PyNotImplementedError::new_err(svg::NOT_READY_MSG)); }
    Ok(f)
}

fn render_error(py: Python<'_>, r: &SheetResult) -> PyErr {
    errors::err(py, "RenderError", format!("sheet {:?}: {}", r.name, r.error.as_deref().unwrap_or("unknown error")))
}

#[pymethods]
impl PyDocument {
    /// Sheet (layout tab) names in tab order. Model space is not a sheet.
    #[getter]
    fn sheets(&self) -> Vec<String> { self.doc.sheets().into_iter().map(String::from).collect() }

    #[getter]
    fn path(&self) -> PathBuf { self.doc.path.clone() }

    /// Facts about the file: version, size, parse time, counts, reader warnings.
    #[getter]
    fn info<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let doc = &self.doc;
        let sc = &doc.scene;
        let d = PyDict::new(py);
        d.set_item("path", doc.path.to_string_lossy())?;
        d.set_item("size_bytes", doc.size_bytes)?;
        d.set_item("parse_ms", render::round3(doc.parse_ms))?;
        d.set_item("version", &sc.header.version)?;
        d.set_item("insunits", sc.header.insunits)?;
        d.set_item("measurement", sc.header.measurement)?;
        d.set_item("ltscale", sc.header.ltscale)?;
        d.set_item("psltscale", sc.header.psltscale)?;
        d.set_item("font", sc.fonts.faces.first().map(|f| f.name.as_str()).unwrap_or(""))?;
        d.set_item("sheets", doc.sheets().len())?;
        d.set_item("entities", sc.entity_count())?;
        d.set_item("blocks", sc.blocks.len())?;
        d.set_item("layers", sc.layers.len())?;
        d.set_item("model_expanded", sc.model_block().expanded)?;
        d.set_item("entity_counts", &sc.stats.counts)?;
        d.set_item("skipped_types", &sc.stats.skipped)?;
        d.set_item("reader_warnings", &sc.stats.warnings)?;
        Ok(d)
    }

    /// Sheets in tab order: name, order and paper settings.
    fn sheet_info<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyList>> {
        let out = PyList::empty(py);
        for s in self.doc.sheet_info() {
            let p = s.paper;
            let paper = PyDict::new(py);
            paper.set_item("units_per_mm", p.units_per_mm)?;
            paper.set_item("size_mm", p.size_mm)?;
            paper.set_item("margins_mm", (p.margins_mm[0], p.margins_mm[1], p.margins_mm[2], p.margins_mm[3]))?;
            paper.set_item("limits", p.limits.map(|b| (b.x0, b.y0, b.x1, b.y1)))?;
            paper.set_item("psltscale", p.psltscale)?;
            let d = PyDict::new(py);
            d.set_item("sheet", s.name)?;
            d.set_item("order", s.order)?;
            d.set_item("paper", paper)?;
            out.append(d)?;
        }
        Ok(out)
    }

    /// INSERTs with attributes. See `cadrender.Document.attributes`.
    #[pyo3(signature = (sheet=None))]
    fn attributes<'py>(&self, py: Python<'py>, sheet: Option<&str>) -> PyResult<Bound<'py, PyList>> {
        if let Some(s) = sheet { self.check_owner(py, s)?; }
        let out = PyList::empty(py);
        for a in self.doc.attributes(sheet) {
            let values = PyDict::new(py);
            for (tag, value) in a.values { values.set_item(tag, value)?; }
            let d = PyDict::new(py);
            d.set_item("sheet", a.sheet.as_ref())?;
            d.set_item("block", a.block.as_ref())?;
            d.set_item("layer", a.layer)?;
            d.set_item("handle", format!("{:X}", a.handle))?;
            d.set_item("position", (a.pos[0], a.pos[1]))?;
            d.set_item("values", values)?;
            out.append(d)?;
        }
        Ok(out)
    }

    /// TEXT, MTEXT and MULTILEADER texts. See `cadrender.Document.texts`.
    #[pyo3(signature = (sheet=None, layers=None))]
    fn texts<'py>(&self, py: Python<'py>, sheet: Option<&str>, layers: Option<Vec<String>>) -> PyResult<Bound<'py, PyList>> {
        if let Some(s) = sheet { self.check_owner(py, s)?; }
        let layers: Option<Vec<&str>> = layers.as_ref().map(|l| l.iter().map(String::as_str).collect());
        let out = PyList::empty(py);
        for t in self.doc.texts(sheet, layers.as_deref()) {
            let d = PyDict::new(py);
            d.set_item("sheet", t.sheet.as_ref())?;
            d.set_item("layer", t.layer)?;
            d.set_item("type", t.kind)?;
            d.set_item("handle", format!("{:X}", t.handle))?;
            d.set_item("text", t.text)?;
            d.set_item("position", (t.pos[0], t.pos[1]))?;
            out.append(d)?;
        }
        Ok(out)
    }

    /// Render one sheet and return the file bytes (PNG or SVG).
    #[pyo3(signature = (sheet, format, options=None))]
    fn render<'py>(&self, py: Python<'py>, sheet: &str, format: &str, options: Option<&Bound<'py, PyDict>>) -> PyResult<Bound<'py, PyBytes>> {
        let fmt = self::format(py, format)?;
        let opts = options::from_dict(py, options)?;
        self.check_sheet(py, sheet)?;
        let prep = py.detach(|| self.prepared(&opts));
        let mut r = SheetResult { name: sheet.into(), ..Default::default() };
        let out = py.detach(|| render::render_bytes(&self.doc, sheet, fmt, &opts, &prep, &mut r));
        match out {
            Ok(bytes) => Ok(PyBytes::new(py, &bytes)),
            Err(e) => { r.error = Some(e); Err(render_error(py, &r)) }
        }
    }

    /// Render one sheet into a file. Returns the sheet's result dict.
    #[pyo3(signature = (sheet, path, format, options=None))]
    fn render_to<'py>(&self, py: Python<'py>, sheet: &str, path: PathBuf, format: &str, options: Option<&Bound<'py, PyDict>>) -> PyResult<Bound<'py, PyDict>> {
        let fmt = self::format(py, format)?;
        let opts = options::from_dict(py, options)?;
        self.check_sheet(py, sheet)?;
        let r = py.detach(|| {
            let prep = self.prepared(&opts);
            render::render_files(&self.doc, sheet, &[fmt], std::slice::from_ref(&path), &opts, &prep)
        });
        if !r.ok { return Err(render_error(py, &r)); }
        render::to_dict(py, &r)
    }

    /// Render many sheets in parallel into `out_dir`. See `cadrender.Document.render_all`.
    #[pyo3(signature = (out_dir, stem, formats, options=None, threads=None, sheets=None, skip_existing=false, on_sheet=None))]
    #[allow(clippy::too_many_arguments)]
    fn render_all<'py>(
        &self, py: Python<'py>, out_dir: PathBuf, stem: String, formats: Vec<String>, options: Option<&Bound<'py, PyDict>>,
        threads: Option<usize>, sheets: Option<Vec<String>>, skip_existing: bool, on_sheet: Option<Py<PyAny>>,
    ) -> PyResult<Bound<'py, PyDict>> {
        let formats = batch::formats(py, &formats)?;
        batch::render_all(py, self, batch::Job { out_dir, stem, formats, sheets, skip_existing, threads, on_sheet, options })
    }

    fn __repr__(&self) -> String {
        format!("<cadrender.Document {:?}: {} sheets, {}>", self.doc.path.display().to_string(), self.doc.sheets().len(), self.doc.scene.header.version)
    }
}
