//! cadrender-core: the Scene model, entities, render options, the instancing
//! walk and the `Sink` trait (drawing backend interface).
//!
//! Readers (`cadrender-dxf`, later `cadrender-dwg`) build a [`Scene`] with
//! [`scene::SceneBuilder`]. Sinks (`cadrender-raster`, later SVG) implement
//! [`sink::Sink`]. [`render::render_sheet`] connects the two.
//! See `docs/architecture.md`.

pub mod color;
pub mod entity;
pub mod geom;
pub mod linetype;
pub mod options;
pub mod render;
pub mod scene;
pub mod sink;
pub mod text;

pub use color::{ColorPolicy, EntColor, Rgba};
pub use options::{CleanRules, Framing, Limits, Lineweights, LineweightPolicy, Lod, RenderOptions};
pub use render::{render_layout, render_sheet, render_sheet_strips, Prepared, RenderError, SheetRender, Stats};
pub use scene::{Scene, SceneBuilder};
pub use sink::Sink;
