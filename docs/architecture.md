# cadrender architecture

This page explains how cadrender is built and how to add to it.

Words used here:

- **Scene**: everything we need from one drawing file, in memory, read only.
- **Entity**: one thing in a drawing (a line, a text, a block reference ...).
- **Block**: a group of entities, stored once. Model space and every paper
  tab are blocks too.
- **INSERT**: a reference to a block, with a position, scale and rotation.
- **Walk**: going through the entities of a block, and into the blocks that
  INSERTs point to, with a matrix (a small table of numbers that moves,
  scales and turns coordinates).
- **Sink**: the part that makes the output (PNG pixels or SVG text).
- **Culling**: skipping something because its bounding box (the smallest
  rectangle around it) is outside the picture.

## 1. The crates

```
crates/cadrender-core     Scene, entities, options, the walk, the Sink trait, text layout
crates/cadrender-dxf      DXF file -> Scene
crates/cadrender-raster   Sink that draws pixels (own scanline rasteriser) + streaming PNG writer
crates/cadrender          small library API: Document::open / render_png / render_many
crates/cadrender-cli      the `cadrender` program
crates/cadrender-acis     ACIS SAT/SAB -> outline path (REGION, 3DSOLID, BODY)
crates/cadrender-image    finds and decodes IMAGE picture files (PNG, JPEG)
crates/cadrender-svg      Sink that writes SVG
crates/cadrender-dwg      DWG file (R2004-R2018) -> typed object model
crates/cadrender-dwg-scene  DWG object model -> Scene
crates/cadrender-shx      SHX font reader, font resolution, Hershey stroke fonts
crates/cadrender-py       the Python package (PyO3, abi3 wheels)
```

The workspace uses `members = ["crates/*"]`, so a new crate folder joins the
build by itself.

Data flow:

```
file.dxf --(cadrender-dxf: SceneBuilder)--------------> Scene  (once per file)
file.dwg --(cadrender-dwg -> cadrender-dwg-scene)--------^
                                              |
           RenderOptions --> Prepared  (once per file + options)
                                              |
          per sheet (rayon): render_sheet<S: Sink>   <- the walk
                                              |
                     S = RasterSink --> PNG bytes   (S = SvgSink --> SVG text)
```

Only `cadrender-core` knows the Scene types. Readers only call
`SceneBuilder`. Sinks only see device-space paths. So readers, sinks and
the core can change without touching each other.

## 2. The Scene (`cadrender-core/src/scene/`)

`Scene` is built once and then never changes. All sheets and viewports read
it at the same time from many threads.

| Field | What it holds |
|---|---|
| `header` | `$ACADVER`, `$LTSCALE`, `$PSLTSCALE`, `$INSUNITS`, `$MEASUREMENT` |
| `layers` | name, colour, lineweight, linetype, on/off, frozen, plot flag, **per-viewport overrides** (`vp_overrides`) |
| `linetypes` | dash pattern (`> 0` dash, `< 0` gap, `0` dot), complex flag |
| `text_styles` | font file, big font, height, width factor, oblique, chosen font face |
| `dim_styles` | DIMSCALE, DIMASZ, DIMTXT, plus all raw groups |
| `mleader_styles` | placeholder: name + raw groups (for the MULTILEADER work) |
| `blocks` | every block once: entities, base point, bounding box, expanded count, depth |
| `model` | id of the model space block |
| `layouts` | paper tabs **in tab order**: block, viewports to draw, paper setup (limits, units, PSLTSCALE) |
| `fonts` | font faces and their glyph outlines (stored once) |
| `stats` | entity counts, skipped types, reader warnings |
| `meta` | metadata, not drawn (`meta.rs`): INSERTs with their attributes (tag, plain value), TEXT / MTEXT / MULTILEADER texts as plain text; owner block, layer, handle, position |

Ids are plain numbers (`LayerId`, `BlockId`, `LinetypeId` = `u32`, index into
the vectors). Special linetype ids: `LT_BYLAYER`, `LT_BYBLOCK`, `LT_SOLID`.

**Draw order (SORTENTS).** The reader hands each SORTENTS table to the
builder (`add_sort_table`). `build()` sorts the entities of that block into
draw order and stores them in that order, so the walk reads memory in order
(fast). `Block::file_order` keeps the way back to file order, for the option
`draw_order: false`.

**Coordinates are `f64`** everywhere in the scene and the walk. Only the last
step (the sink) gets `f32` pixels. Far-away drawings do not jitter.

### `SceneBuilder`

The only way to fill a Scene. A reader calls:

- `layer(name)`, `layer_mut(id)`, `add_linetype`, `linetype_id(name)`,
  `add_text_style`, `dim_styles.push`, `mleader_styles.push`
- `block(name)` (creates it if needed: a block may be used before it is
  defined), `block_mut(id)`, `model()`, `paper()`
- `push(block, entity)`
- `add_layout(name, order, block, handle, paper_setup)`, `add_sort_table`
- `count`, `skip`, `warn` (statistics)
- `record_insert`, `record_text` (metadata, see below)
- `build()`: resolves frozen-layer handles, computes all bounding boxes
  bottom-up (with an explicit stack, so deep or cyclic block trees are safe),
  applies draw order, picks the viewports of each tab, sorts tabs.

The DWG reader will use the same builder. Nothing in the builder is DXF specific.

**Metadata** (`scene/meta.rs`). Attribute values and text content are kept
in `Scene::meta`, a side table, not in the entities: the walk never reads it
and `Entity` stays small (a laid-out `TextEnt` keeps only glyphs). Readers
call `record_insert` / `record_text` with plain values (`meta::plain_text`,
`meta::plain_mtext`: `\U+XXXX` and `%%d` decoded, MTEXT codes removed, the
0x80..0x9F bytes of old ANSI files read as Windows-1252). Hidden attributes
are kept, invisible texts are not; layer state and clean rules do not matter.
`build()` sorts the records by owner (model space, sheets in tab order, other
blocks), so DXF and DWG give the same order. `Scene::owner_name` turns an
owner block into a sheet name, `"Model"` or the block's name; anonymous
blocks are named by their handle (`Block::meta_name`, `*U#2F1A`), because
writers number them differently. `Document::sheet_info / attributes / texts`
(and the Python methods) read this table.

## 3. Entities (`cadrender-core/src/entity/`)

An `Entity` is:

- `props: Props`: handle, layer, colour, lineweight, linetype, linetype scale, transparency
- `bbox`: bounding box in block coordinates (set by `build()`)
- `kind: Kind`: the shape data

`Kind` is an enum with **one variant per module**:

| Variant | Module | Used for |
|---|---|---|
| `Stroke` | `stroke.rs` | LINE, ARC, CIRCLE, ELLIPSE, (LW)POLYLINE, SPLINE, LEADER |
| `Fill` | `fill.rs` | SOLID, TRACE, WIPEOUT (mask in page colour) |
| `Hatch` | `hatch.rs` | HATCH (solid or real pattern lines, tint when too dense) |
| `Text` | `text.rs` | TEXT, MTEXT, ATTRIB (laid out once; MTEXT background mask) |
| `Insert` | `insert.rs` | INSERT (+ MINSERT grid, ATTRIBs), DIMENSION and ACAD_TABLE (through their blocks) |
| `Viewport` | `viewport.rs` | VIEWPORT (draws nothing itself; clip polygon, non-plan views) |
| `Point` | `point.rs` | POINT (`$PDMODE`/`$PDSIZE`; points on Defpoints are left out) |
| `Pline` | `pline.rs` | dashed (LW)POLYLINE without "linetype generation": pattern per segment |
| `XLine` | `xline.rs` | RAY, XLINE (cut to the view, not used for framing) |
| `Frame` | `frame.rs` | WIPEOUT frame (hidden by `RenderOptions::wipeout_frames = false`) |
| `Image` | `image.rs` | IMAGE: picture (`Sink::image`) inside its clip frame, then the frame |

3DFACE, MESH, polyface and polygon meshes (wireframe, seen from above),
REGION / 3DSOLID / BODY (their ACIS edges, seen from above, via
`cadrender-acis`), HELIX and constant ATTDEFs are plain `Stroke`/`Text`
entities made by the reader. IMAGE pictures are loaded after reading by
`cadrender-image` (called in `Document::open`, which knows the drawing's
folder); a missing file leaves only the frame. Complex linetypes (text, SHX shapes) are drawn
by `cadrender-core/src/linetype.rs`.

Each kind implements the trait `EntityKind`:

```rust
pub trait EntityKind {
    fn prepare(&mut self, cx: &PrepareCx) -> BBox;            // once, at build: bounding box
    fn draw<S: Sink>(&self, e: &Entity, v: &Visit, w: &mut Walker<'_, S>);
    fn block_refs(&self, out: &mut Vec<BlockId>) {}          // blocks it uses (default: none)
    fn expanded(&self, cx: &PrepareCx) -> u64 { 1 }          // statistics
    fn depth(&self, cx: &PrepareCx) -> u32 { 0 }
    fn clean_bbox(...) -> Option<BBox> { Some(e.bbox) }      // framing after clean rules
}
```

The list of kinds is written **once**, in the `entity_kinds!` macro call in
`entity/mod.rs`. The macro makes the enum and all dispatch functions. There
are no other big `match` blocks over kinds.

**Keep `Entity` small.** The walk touches millions of entities per sheet. A
test fails if `Entity` grows over 160 bytes. Big, rarely drawn kinds are
boxed (`Text(Box<TextEnt>)`, `Viewport(Box<ViewportEnt>)`), and so are rare
parts of common kinds (MINSERT grid, text mask).

## 4. The walk (`cadrender-core/src/render/`)

`render_sheet` (in `render/mod.rs`) does one tab:

1. **Frame.** `Framing::Extents` (default, same as ezdxf + matplotlib): the
   box of visible paper-space entities, plus what each viewport really shows,
   plus a 5 % margin. `Framing::Paper`: the layout limits (the paper).
2. **Paper space**: walk the tab's block with the paper -> pixel matrix.
3. **Viewports**: for each viewport, a new region sink of the viewport's
   size; walk model space with `page matrix x viewport matrix`. Viewports are
   independent, so they run in parallel (rayon). Then `composite` puts each
   region on the page.

`Walker::walk` (in `render/walk.rs`) is the instancing walk. For each entity:

1. layer: layer "0" inside a block takes the INSERT's layer;
2. hidden? (off, frozen, frozen in this viewport, or removed by clean rules: one array lookup);
3. bounding box -> device pixels; **cull** if outside the clip (at every level:
   a culled INSERT skips its whole block tree);
4. `tiny` flag if smaller than the level-of-detail dot size;
5. `kind.draw(...)`.

An INSERT draws itself by calling `walk` again on its block, with
`parent matrix x insert matrix` and the inherited style (BYBLOCK colour,
lineweight, linetype). Blocks are never copied. Depth is limited
(`Limits::max_depth`), and so is the number of visits per sheet
(`Limits::max_visits`); both give a warning, not a crash.

Helpers a kind uses: `w.style(e, layer, inherit, min_px)` (colour, width,
dash), `w.color(..)`, `w.stroke(path, m, key)`, `w.out.fill_now(..)`,
`w.out.set_fill / set_stroke / push / seg`, `w.dot(..)`, `w.walk(..)`,
`w.stats`, `w.opts`, `w.prep`, `w.scene`.

**Batching** (`render/batch.rs`): `Batcher` sits between the walk and the
sink. Shapes with the same `StrokeKey` (colour, width, dash) go into one path;
one `draw` call per batch. Even-odd fills (hatches, masks) are drawn at once
(`fill_now`), because overlapping even-odd fills would cancel.

**`Prepared`** (`render/prep.rs`): work done once per (scene, options) and
shared by all sheets: clean rules turned into flags per layer and per block,
the 256 ACI colours through the colour policy, block boxes after cleaning.
`Prepared::view(..)` gives the layer table of one view (paper or one
viewport), with per-viewport frozen layers and layer overrides applied.

## 5. The Sink trait (`cadrender-core/src/sink.rs`)

```rust
pub trait Sink: Send + Sync + Sized {
    type Config: Sync;
    fn page(cfg: &Self::Config, w: u32, h: u32, background: Rgba) -> Self;
    fn region(&self, rect: IRect) -> Self;              // empty sink for one viewport
    fn composite(&mut self, region: Self, rect: IRect, clip: Option<&[[f32; 2]]>);
    fn move_to / line_to / quad_to / cubic_to / close(..);  // build a path, device pixels
    fn draw(&mut self, paint: &Paint);                  // draw the built path, start a new one
    fn discard(&mut self);
}
```

`Paint` is a colour plus `Stroke { width, dash }` or `Fill { rule }`. Text
arrives as glyph outlines (fills), or as strokes for line fonts (SHX).

Optional parts, all off by default (a sink that does not use them compiles
and works as before; the switches are constants, so the PNG path pays nothing):

- `TEXT_RUNS` + `text(&TextRun)` / `rich_text(&TextRun, &RichText)`: texts
  and formatted MTEXT as laid-out runs, for sinks that write real text (SVG).
  `RichText` has the MTEXT runs (size, width factor, oblique, bold) and the
  line boxes, with colours already resolved. Return `false` to get outlines.
- `GRADIENTS` + `fill_gradient(&GradientPaint)`: hatch gradients as one
  smooth linear or radial gradient (device pixels, stops 0..1) for the path
  just built. Without it a sink gets bands of solid colour.
- `image(&ImageDraw)`: an IMAGE picture with its pixel -> device matrix, a
  clip polygon, brightness/contrast/fade and the grey flag. Return `false`
  (the default) and only the frame is drawn. PNG: sampled pixels; SVG:
  `<image>` with the file bytes as a data URI.

The walk is generic over `S: Sink`, so every sink gets its own fast,
specialised copy of the walk. To add a sink, implement the trait in a new
crate; nothing in the walk changes. `RecordingSink` (in `sink.rs`) records
draws and is handy in tests.

## 6. Options (`cadrender-core/src/options.rs`)

`RenderOptions`: `width` (px), `dpi`, `framing`, `margin`, `max_height`,
`color` (`MonochromeLight` = grey print-preview look, or `TrueColor`), `background`,
`lineweight` (policy `Paper` = true plot width / `Preview` = ezdxf's
mm-as-points / `Hairline`, plus `default_lw` and `min_px`), `lod`
(level-of-detail limits; `Lod::full()` turns them off), `clean`, `limits`,
`draw_order`, `text_masks`, `wipeout_frames` (false = never draw WIPEOUT frames;
true = as the file says), `parallel_viewports`.

**Clean rules are data** (`CleanRules`): layer names and block-name
keywords. Matching ignores case and separators (`Title_Block-Text` =
`title block text`). VIEWPORTs are never removed; ATTRIBs on removed
layers are removed. Presets: `RenderOptions::preset("preview")` (grey look,
paper lineweights, the `Defpoints` layer left out) and `"none"`. The CLI
uses `preview` by default (`--preset none` to change).

## 7. The DXF reader (`cadrender-dxf`)

One pass over a memory-mapped file (`tags.rs` reads code/value pairs as
byte slices; nothing is copied until needed). `reader.rs` has the main loop
by section; `tables.rs`, `objects.rs` and `entities/*.rs` handle the parts.

Main input: DXF as AutoCAD and ODA write it (AC1032). DXF written by
LibreDWG's `dwg2dxf` also works, with these work-arounds:

- VIEWPORT status 0 everywhere -> all viewports on except the first (the paper viewport);
- garbage MTEXT `box_fill_scale` -> values outside 1..5 become 1.5;
- ATTRIBs with a junk `AcDbXrecord` part whose `70 1` would hide them -> the
  record is cut at that marker;
- SORTENTS tables without the block-record handle -> found through the owner dictionary.

Every entity record is cut at an embedded object (`101`), because ODA
repeats group codes there (without the cut, the MTEXT direction is read
wrong).

## 8. Errors

- A file that is not DXF, or binary DXF: `DxfError`; a broken or unsupported
  DWG: `DwgError`. The CLI exits with code 1.
- Each sheet is separate: `Document::render_many` catches a panic in one
  sheet and reports it as that sheet's error. Other sheets go on. The CLI
  JSON lists `"ok": false, "error": ...` for it.
- Bad numbers in the file become 0 or a safe default; loops on angles use
  `rem_euclid`, so garbage cannot hang the reader.

## 9. How to add a new entity type

Example: MULTILEADER.

1. **Core, new file** `crates/cadrender-core/src/entity/mleader.rs`:
   a struct (for example `MLeaderEnt { lines: Vec<PathData>, arrows: ..., text: Option<TextEnt>, block: Option<InsertEnt> }`)
   and `impl EntityKind for MLeaderEnt`. In `draw`, reuse the helpers
   (`w.style`, `w.stroke`, `w.out.fill_now`) and the other kinds
   (call `self.text.draw(e, v, w)` for its text part).
2. **Core, `entity/mod.rs`**: `pub mod mleader;` and one line in
   `entity_kinds! { ... MLeader(Box<mleader::MLeaderEnt>), }` (box it if it
   is bigger than about 80 bytes) plus a one-line `From` impl like the ones
   for `TextEnt`.
3. **Reader, new file** `crates/cadrender-dxf/src/entities/mleader.rs` with
   `pub fn mleader(r: &mut Reader, body: &[Tag], com: &Common) -> Option<Kind>`.
4. **Reader, `entities/mod.rs`**: `mod mleader;` and one line in the `match`
   in `read`: `b"MULTILEADER" => mleader::mleader(r, body, &com),`.
5. Tests: a small DXF in `crates/cadrender-dxf/tests/data/` (add it to
   `gen.py`) and a test in `tests/read.rs`.

That is two one-line edits in shared files (`entity/mod.rs`,
`entities/mod.rs`) and the rest in your own new files. If two people add a
kind at the same time, the merge conflict is one line.

Some entities do not need a new kind: if the shape is lines, fills and
text, the reader can build a small anonymous block and push an `InsertEnt`
(DIMENSION already works like this).

## 10. Fonts

Font policy: cadrender ships no AutoCAD or other non-free fonts. All text
styles (SHX and TrueType alike) use one sans font:
Arial if installed, else **Liberation Sans**, which is built into
`cadrender-core` (`text::LIBERATION_SANS`, file and OFL licence in
`crates/cadrender-core/fonts/`). Each style keeps its height, width factor,
oblique angle and alignment. Choose with `text::FontChoice` (`Auto`,
`Liberation`, `File(path)`); CLI `--font auto|liberation|FILE.ttf`; the JSON
summary says which font was used. `Fonts::face_for(font_file)` decides which
face a style uses: a face added under that file name, else face 0. So the
Hershey stroke fonts from `cadrender-shx` (a standalone crate today) can
plug in as a `GlyphSource`
with `filled() = false`, added with `Fonts::add(...)` before the scene is
built, only when the user asks for them.

## 11. Known gaps

- SHAPE, OLE2FRAME: not drawn (counted in `skipped_types`).
- REGION / 3DSOLID / BODY: only straight and circular/elliptic edges; spline
  edges are counted (`ACIS_SPLINE_EDGE`). No silhouettes of curved faces.
- MTEXT inline formatting is dropped (one font for all styles is the policy, not a gap).
- IMAGE: PNG and JPEG only (no TIFF/BMP); RASTERVARIABLES IMAGEFRAME is not
  read (the frame is always drawn, the AutoCAD default); inverted clips are
  drawn as normal clips.
- Meshes, 3DFACE and non-plan viewports: the scene is 2D (z is dropped), so a
  non-plan view is a parallel projection of the XY plane. Edge-on views are not drawn.
- SHX shapes in complex linetypes (ltypeshp.shx is not free) are drawn as a
  small wavy line; counted as `LTYPE_SHAPE` in `skipped_types` (once per linetype element).
- RAY/XLINE in paper space make the sheet frame huge when no clean rules are on
  (`--preset none`): framing uses the raw box then.
- Thin lines are snapped to whole pixel rows, so they are crisp and dark (see
  `docs/raster.md`). ezdxf/matplotlib snap with a half-pixel bias, so a crisp
  line is often one row away from the ezdxf line: the `pixel diff %` score is
  higher for this reason, not because lines are missing.
- MTEXT uses its text style's width factor and oblique angle (font policy).
  ezdxf does not, so MTEXT is narrower than in the ezdxf pictures when the
  style is narrow.
- Draw order is applied inside blocks too (AutoCAD does this; ezdxf does not).
  So some tabs differ from ezdxf on purpose (for example window details
  drawn over their WIPEOUT, as the drafter meant).
