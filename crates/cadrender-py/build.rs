//! Link flags for a Python extension module, so that a plain
//! `cargo build --workspace` works on macOS without libpython (Python's own
//! symbols are found when the module is imported). maturin adds the same flags.
fn main() {
    pyo3_build_config::add_extension_module_link_args();
}
