//! `Document.render_all`: many sheets in parallel, GIL released.
//!
//! Sheets run on rayon threads. The optional `on_sheet` callback is called
//! from those threads; each call takes the GIL for a moment. If the callback
//! raises (or Ctrl-C arrives), sheets not started yet are cancelled and the
//! exception is raised after the running ones finish.

use crate::doc::{self, PyDocument};
use crate::render::{self, round3, Format, SheetResult};
use crate::{errors, options};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use rayon::prelude::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Instant;

pub struct Job<'a> {
    pub out_dir: PathBuf,
    pub stem: String,
    pub formats: Vec<Format>,
    pub sheets: Option<Vec<String>>,
    pub skip_existing: bool,
    pub threads: Option<usize>,
    pub on_sheet: Option<Py<PyAny>>,
    pub options: Option<&'a Bound<'a, PyDict>>,
}

/// Run the job. Returns the summary dict.
pub fn render_all<'py>(py: Python<'py>, d: &PyDocument, job: Job<'_>) -> PyResult<Bound<'py, PyDict>> {
    let t0 = Instant::now();
    let opts = options::from_dict(py, job.options)?;
    if job.formats.is_empty() { return Err(errors::options(py, "formats is empty; use for example (\"png\",)")); }
    if job.threads == Some(0) { return Err(errors::options(py, "threads must be at least 1 (or None for all cores)")); }
    std::fs::create_dir_all(&job.out_dir).map_err(|e| errors::err(py, "RenderError", format!("{}: {e}", job.out_dir.display())))?;

    // Sheets in tab order; names asked for but not in the file become errors.
    let all: Vec<String> = d.doc.sheets().into_iter().map(String::from).collect();
    let (names, mut results): (Vec<String>, Vec<SheetResult>) = match &job.sheets {
        None => (all, Vec::new()),
        Some(want) => (
            all.iter().filter(|s| want.contains(s)).cloned().collect(),
            want.iter().filter(|w| !all.contains(w)).map(|w| SheetResult::failed(w, "no such sheet in this file".into())).collect(),
        ),
    };
    let files = |name: &str| -> Vec<PathBuf> { job.formats.iter().map(|f| job.out_dir.join(cadrender::sheet_file_name(&job.stem, name, f.ext()))).collect() };
    let mut todo = Vec::new();
    for n in &names {
        let fs = files(n);
        if job.skip_existing && fs.iter().all(|p| p.exists()) {
            results.push(SheetResult { name: n.clone(), ok: true, skipped: true, files: fs, ..Default::default() });
        } else {
            todo.push((n.clone(), fs));
        }
    }

    let abort = AtomicBool::new(false);
    let cb_error: Mutex<Option<PyErr>> = Mutex::new(None);
    let (prepare_ms, wall_ms, rendered) = py.detach(|| -> PyResult<_> {
        let tp = Instant::now();
        let prep = d.prepared(&opts);
        let prepare_ms = tp.elapsed().as_secs_f64() * 1e3;
        let tw = Instant::now();
        let work = || -> Vec<SheetResult> {
            todo.par_iter().map(|(name, fs)| {
                if abort.load(Ordering::Relaxed) { return SheetResult::failed(name, "cancelled".into()); }
                let r = render::render_files(&d.doc, name, &job.formats, fs, &opts, &prep);
                if let Some(cb) = &job.on_sheet { call_back(cb, &r, &abort, &cb_error); }
                r
            }).collect()
        };
        let rendered = match job.threads {
            None => work(),
            Some(n) => rayon::ThreadPoolBuilder::new().num_threads(n).build()
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("thread pool: {e}")))?
                .install(work),
        };
        Ok((prepare_ms, tw.elapsed().as_secs_f64() * 1e3, rendered))
    })?;
    if let Some(e) = cb_error.into_inner().unwrap_or_else(|e| e.into_inner()) { return Err(e); }
    results.extend(rendered);
    // Back to tab order (asked-for names that do not exist go last).
    results.sort_by_key(|r| names.iter().position(|n| *n == r.name).unwrap_or(usize::MAX));
    summary(py, d, &job, results, prepare_ms, wall_ms, t0)
}

/// Call `on_sheet(name, result_dict)` with the GIL. Errors stop the job.
fn call_back(cb: &Py<PyAny>, r: &SheetResult, abort: &AtomicBool, cb_error: &Mutex<Option<PyErr>>) {
    if abort.load(Ordering::Relaxed) { return; }
    Python::attach(|py| {
        let res = py.check_signals().and_then(|_| render::to_dict(py, r)).and_then(|dict| cb.call1(py, (r.name.as_str(), dict)).map(drop));
        if let Err(e) = res {
            abort.store(true, Ordering::Relaxed);
            let mut slot = cb_error.lock().unwrap_or_else(|e| e.into_inner());
            if slot.is_none() { *slot = Some(e); }
        }
    });
}

fn summary<'py>(py: Python<'py>, d: &PyDocument, job: &Job<'_>, results: Vec<SheetResult>, prepare_ms: f64, wall_ms: f64, t0: Instant) -> PyResult<Bound<'py, PyDict>> {
    let out = PyDict::new(py);
    out.set_item("file", d.doc.path.to_string_lossy())?;
    out.set_item("out_dir", job.out_dir.to_string_lossy())?;
    out.set_item("formats", job.formats.iter().map(|f| f.ext()).collect::<Vec<_>>())?;
    out.set_item("threads", job.threads.unwrap_or_else(rayon::current_num_threads))?;
    out.set_item("parse_ms", round3(d.doc.parse_ms))?;
    out.set_item("prepare_ms", round3(prepare_ms))?;
    out.set_item("render_wall_ms", round3(wall_ms))?;
    out.set_item("total_ms", round3(t0.elapsed().as_secs_f64() * 1e3))?;
    let done: Vec<&str> = results.iter().filter(|r| r.ok && !r.skipped).map(|r| r.name.as_str()).collect();
    let skipped: Vec<&str> = results.iter().filter(|r| r.skipped).map(|r| r.name.as_str()).collect();
    let failed = PyDict::new(py);
    for r in results.iter().filter(|r| !r.ok) { failed.set_item(&r.name, r.error.as_deref().unwrap_or(""))?; }
    out.set_item("done", done)?;
    out.set_item("skipped", skipped)?;
    out.set_item("failed", failed)?;
    let rows = results.iter().map(|r| render::to_dict(py, r)).collect::<PyResult<Vec<_>>>()?;
    out.set_item("sheets", rows)?;
    Ok(out)
}

/// Parse the format names (shared with `Document.render_all`).
pub fn formats(py: Python<'_>, names: &[String]) -> PyResult<Vec<Format>> {
    let mut v = Vec::new();
    for n in names { let f = doc::format(py, n)?; if !v.contains(&f) { v.push(f); } }
    Ok(v)
}
