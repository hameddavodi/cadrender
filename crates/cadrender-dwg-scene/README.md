# cadrender-dwg-scene

Turns a DWG `Drawing` (from `cadrender-dwg`) into the same `Scene` that
`cadrender-dxf` makes from ODA's DXF of that file. `Document::open` and the
CLI use it for `.dwg` files (or files that start with `AC10`).

Why a separate crate: the DWG reader stays standalone (no renderer
dependency), this crate has the same role as `cadrender-dxf` (reader ->
Scene).

## What it fills

- Header (LTSCALE, PSLTSCALE, INSUNITS, PDMODE, PDSIZE), layers (with
  transparency from the "AcCmTransparency" EED and the per-viewport
  overrides from the ADSK_XREC_LAYER_* XRECORDs), linetypes (with text and
  shape parts), text styles, dimstyles, multileader styles.
- Blocks, model space, layouts in tab order with paper setup, viewports
  (status = position among the layout's viewports, as ODA writes it; clip,
  view direction, frozen layers), SORTENTS.
- Every entity kind the DXF reader draws, with the same rules: DWG angles
  are radians, mirrored OCS, fit-point-only splines get a real cubic
  interpolation, non-finite geometry is skipped, hatch "not closed" loops
  are joined like ODA does (only when the hatch has no closed loop),
  MULTILEADER from its context data, WIPEOUT
  frames when WIPEOUTFRAME = 1, R2007 MTEXT uses the
  ACAD_MTEXT_2008_RT text.
- HELIX is drawn from its helix values, not from the spline stored in the
  DWG: ODA writes its own spline to the DXF (a cubic through the helix
  every 1/8 turn), and the dashes of a linetype only match on the same
  curve.

`src/text.rs` and `src/mleader.rs` copy the layout rules of
`cadrender-dxf/src/entities/{text,mleader}.rs`; keep them in step.

## Checking against the DXF

- scenediff modes: `TAB=<name>` (only that tab and what its viewports
  show), `TREE=<hex handle>` (an INSERT and the differing entities in its
  block), `AT=x,y` (model space entities at a point).

- `cargo run --release -p cadrender-dwg-scene --example scenediff -- a.dwg a.dxf`
  compares both scenes entity by entity (by handle). `DUMP=<hex handle>`
  prints one entity from both sides.
- `CADRENDER_DWG_SAMPLES=/path/to/dwgs cargo test -p cadrender-dwg-scene -- --ignored`
  takes a folder with your own `.dwg` files and checks tabs and tables
  against the ODA File Converter DXF of each file, looked up in a
  `dxf_oda` folder next to it (`/path/to/dxf_oda/<name>.dxf`).
