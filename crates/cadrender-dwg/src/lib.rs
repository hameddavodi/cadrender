//! cadrender-dwg: an own DWG reader for cadrender.
//!
//! Status: R2010 / R2013 / R2018 (AC1024 / AC1027 / AC1032) files are read.
//! R2007 (AC1021) files are read too (own file structure, see
//! `container_r21`). R2004 (AC1018) shares the R2010 file structure; its
//! objects use the R2007 frame without a string stream (version checks).
//! See README.md for details, validation and speed numbers.
//!
//! ```no_run
//! let d = cadrender_dwg::read_file("drawing.dwg", &Default::default()).unwrap();
//! println!("{} objects, LTSCALE {}", d.objects.len(), d.header.ltscale);
//! ```

pub mod bits;
pub mod classes;
pub mod container;
pub mod container_r21;
pub mod crc;
pub mod crc64;
pub mod drawing;
pub mod error;
pub mod header;
pub mod lz77;
pub mod lz77_r21;
pub mod model;
pub mod objects;
pub mod objmap;
pub mod rs;
pub mod streams;

// object types for the Scene adapter
pub mod acis;
pub mod image;
pub mod dimstyle;
pub mod extra;
pub mod mleader;
pub mod tables2;

pub use container::Version;
pub use dimstyle::{DimStyle, VPort};
pub use extra::*;
pub use mleader::*;
pub use tables2::{XRecord, XValue};
pub use drawing::{read_bytes, read_file, Drawing, ReadOptions, TypeStats};
pub use error::{DwgError, Result};
pub use model::*;
pub use streams::{Color, EntityColor};
