//! cadrender-dxf: a fast, tolerant ASCII DXF reader that builds a
//! [`cadrender_core::Scene`].
//!
//! One pass over a memory-mapped file. Main input: DXF written by the ODA
//! File Converter. Second input: DXF written by LibreDWG, with its known
//! bugs worked around (VIEWPORT status 0, garbage MTEXT `box_fill_scale`,
//! broken ATTRIB records in some files).

mod entities;
mod objects;
mod reader;
mod tables;
pub mod tags;

use cadrender_core::text::Fonts;
use cadrender_core::Scene;
use std::path::Path;

#[derive(Debug)]
pub enum DxfError {
    Io(std::io::Error),
    /// Binary DXF is not supported yet.
    Binary,
    /// The file does not look like a DXF file.
    NotDxf(String),
}

impl std::fmt::Display for DxfError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DxfError::Io(e) => write!(f, "{e}"),
            DxfError::Binary => write!(f, "binary DXF is not supported"),
            DxfError::NotDxf(s) => write!(f, "not a DXF file: {s}"),
        }
    }
}
impl std::error::Error for DxfError {}
impl From<std::io::Error> for DxfError { fn from(e: std::io::Error) -> Self { DxfError::Io(e) } }

/// Read a DXF file into a Scene. `fonts` are used to lay out text (face 0 is the fallback).
pub fn read_file(path: impl AsRef<Path>, fonts: Fonts) -> Result<Scene, DxfError> {
    let file = std::fs::File::open(path.as_ref())?;
    // SAFETY: the map is read-only and lives only during this call. If another
    // process changes the file while we read it, we may read garbage values,
    // which the tolerant parser turns into skipped entities, not memory errors.
    let map = unsafe { memmap2::Mmap::map(&file)? };
    read_bytes(&map, fonts)
}

/// Read DXF from memory.
pub fn read_bytes(buf: &[u8], fonts: Fonts) -> Result<Scene, DxfError> {
    if buf.starts_with(b"AutoCAD Binary DXF") { return Err(DxfError::Binary); }
    reader::Reader::new(fonts).read(buf)
}
