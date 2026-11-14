//! Python errors. The classes are written in Python (`cadrender/_errors.py`),
//! because some of them have two parents (`SheetNotFoundError` is also a
//! `KeyError`, `OptionsError` is also a `ValueError`). Rust only raises them.

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyType;

/// Make an exception of class `cadrender._errors.<name>` with one message.
pub fn err(py: Python<'_>, name: &str, msg: impl Into<String>) -> PyErr {
    let msg = msg.into();
    let class = py.import("cadrender._errors").and_then(|m| m.getattr(name)).and_then(|c| c.cast_into::<PyType>().map_err(PyErr::from));
    match class {
        Ok(c) => PyErr::from_type(c, (msg,)),
        // The Python part is missing (should not happen in a real install).
        Err(_) => PyRuntimeError::new_err(format!("{name}: {msg}")),
    }
}

pub fn options(py: Python<'_>, msg: impl Into<String>) -> PyErr { err(py, "OptionsError", msg) }
