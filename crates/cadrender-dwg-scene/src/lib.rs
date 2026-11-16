//! cadrender-dwg-scene: turn a [`cadrender_dwg::Drawing`] into a
//! [`cadrender_core::Scene`].
//!
//! Goal: the same Scene the DXF reader (`cadrender-dxf`) builds from ODA's
//! DXF of the same drawing. So the rules here copy the DXF reader's rules
//! (units, defaults, what is skipped), only the input is typed DWG data
//! instead of group codes. DWG-only facts are handled here:
//!
//! - angles are radians (DXF uses degrees for ARC, TEXT, INSERT, STYLE ...);
//! - SPLINEs that store only fit points get a real interpolating curve;
//! - non-finite coordinates (NaN lines in some files) are skipped;
//! - colour-book colours are already resolved by the DWG reader;
//! - VIEWPORT status (DXF 68) is not stored in DWG: it is the position in
//!   the layout's viewport list, like ODA writes it.
//!
//! Why a separate crate (not `cadrender-dwg/src/scene.rs`): the DWG reader
//! stays a standalone library with no renderer dependency (its tools and
//! tests build fast and alone), the adapter sits next to `cadrender-dxf` in
//! the same role (reader -> Scene), and work on the DWG container (R2007)
//! and on this adapter do not touch the same files.

mod acis;
mod entities;
mod image;
mod hatch;
mod kinds;
mod meta;
mod mleader;
mod objects;
mod shapes;
mod spline;
mod tables;
mod text;

use cadrender_core::text::Fonts;
use cadrender_core::Scene;
use cadrender_dwg::{Drawing, DwgError, ReadOptions};
use std::path::Path;

pub use cadrender_dwg;

/// Read a DWG file into a Scene. `fonts` are used to lay out text (face 0 is the fallback).
pub fn read_file(path: impl AsRef<Path>, fonts: Fonts) -> Result<Scene, DwgError> {
    let d = cadrender_dwg::read_file(path, &read_options())?;
    Ok(build_scene(&d, fonts))
}

/// Read DWG bytes into a Scene.
pub fn read_bytes(buf: &[u8], fonts: Fonts) -> Result<Scene, DwgError> {
    let d = cadrender_dwg::read_bytes(buf, &read_options())?;
    Ok(build_scene(&d, fonts))
}

/// Options for rendering: parallel decode, CRC checks off (a bad CRC is only
/// a warning anyway, and the checks cost time).
fn read_options() -> ReadOptions {
    ReadOptions { check_crc: false, ..ReadOptions::default() }
}

/// Build the Scene of a decoded drawing.
pub fn build_scene(d: &Drawing, fonts: Fonts) -> Scene {
    let mut cx = tables::Ctx::new(d, fonts);
    cx.header();
    cx.linetypes();
    cx.layers();
    cx.text_styles();
    cx.dim_styles();
    cx.block_records();
    cx.index_viewports();
    cx.entities();
    cx.layouts();
    cx.sort_tables();
    cx.layer_overrides();
    cx.mleader_styles();
    cx.wipeout_frames();
    cx.finish()
}
