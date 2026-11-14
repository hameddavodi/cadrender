# The raster sink (`cadrender-raster`)

How cadrender turns paths into PNG pixels, and why it is built this way.

## Why our own rasteriser

A profile of an earlier version (drawing with tiny-skia) showed that 42-59 % of the render time went
into tiny-skia's thin-line code. tiny-skia runs its full pixel pipeline for
every short run of 1-2 pixels, and a CAD picture is almost only such runs.
So thin lines, fills and pasting are now our own code. tiny-skia is only used
to make the outline of a thick line (its stroker), which we then fill.

## The parts

| File | What it does |
|---|---|
| `canvas.rs` | Pixel buffers and blending. 8-bit grey for a grey page, grey + alpha for a viewport region, RGBA when a colour shows up. |
| `hairline.rs` | Lines up to 2 px wide, drawn straight into the canvas. |
| `fill.rs` | Polygons with exact area coverage (the "cell" method of FreeType and AGG): text, hatches, masks, thick lines, clip polygons. |
| `path.rs` | The path between two `draw` calls, and curve flattening (0.2 px). |
| `pngout.rs` | PNG writer that takes the picture strip by strip ("up" filter, `fdeflate`). |

## Grey canvas

A page starts as 1 byte per pixel when its background is an opaque grey
(the grey `preview` look). That is 4 times less memory to clear, blend, paste and
encode than RGBA. The first time a colour that is not grey is drawn, the
canvas turns into RGBA by itself ("promotion"), so true colour still works.
A viewport region over a grey page is grey + alpha (2 bytes per pixel).

Each canvas remembers which part of each band of 16 rows was touched. Pasting
a region on the page only reads those parts, and skips empty pixels 8 at a time.

## Thin lines

- **Exact thickness.** For a mostly horizontal line, each pixel column gets
  the vertical cut through the line (`w / cos(angle)`), and each pixel in the
  column gets the part of the cut inside it. So a line of width `w` puts `w`
  pixels of ink per pixel of length, at any angle.
- **Snapping.** A straight segment that is horizontal or vertical (the ends
  differ by at most 0.25 px) is moved by at most half a pixel, so it covers
  whole pixel rows: the pixel centre for odd widths (1 px, 1.19 px, 3 px), the
  pixel edge for even widths (2 px). A 1 px line is one dark row instead of two
  half-grey rows. Curves are never moved; a curve end that touches a snapped
  point moves with it, so joints stay closed.
- **Ink curve.** Partly covered pixels get a little more ink:
  `ink = 1 - (1 - coverage)^1.6`. Lines at an angle then look as dark as
  snapped ones. Fully covered and empty pixels do not change.
- **Joints.** Inside a polyline each pixel column belongs to one segment only,
  so joints are not drawn twice (no beads on flattened arcs). Open ends get a
  square cap of half the width, like matplotlib, so corners made of separate
  LINEs are closed.
- **Lines thinner than 1 px** (only with a `min_px` below 1) are drawn 1 px
  wide with less ink (at least 50 %).
- **Exact horizontal and vertical lines** are drawn as runs (one `memset` per
  row), not pixel by pixel.

Thickness per lineweight class at 2000 px on an A1 sheet (paper rule): 0.13 to
0.35 mm are all 1 px (the `min_px` floor), 0.50 mm = 1.19 px (one black row and
two light rows), 0.70 mm = 1.67 px, 1.00 mm = 2.38 px. The test
`line_classes_have_their_ink` checks that each class gets its ink.

## Big pictures: strips

`render_sheet_strips` (core) draws a sheet in strips of full width; each strip
is a page of its own, and `Document::render_png` streams each strip into the
PNG writer. A strip has about 16 million pixels (`STRIP_PIXELS`), so at 2000 px
almost every sheet is one strip and nothing changes; at 10 000 px a sheet is
about 10 strips. Things just outside a strip can reach into it (thick lines),
so the walk culls with a wider window when there is more than one strip.

Viewport regions are pasted in viewport order with at most one region per
worker thread alive at a time.

## Measured

Apple M4, 320 sheets from 8 real-world architectural drawings, 2000 px wide,
best of 3, `cadrender render --all`. "Before" is the earlier general-purpose
2D drawing path (tiny-skia); "after" is this rasteriser.

| | before | after |
|---|---|---|
| 1 thread, whole run | 8.69 s | 3.04 s |
| 1 thread, render only (sum of sheets) | 6.80 s | 1.35 s |
| 1 thread, PNG encoding | 0.50 s | 0.34 s |
| 10 threads, whole run | 3.03 s | 1.75 s |

Big widths, 1 thread (peak memory includes the scene, about 350 MB for the
biggest drawing):

| | before | after |
|---|---|---|
| small drawing, 10 sheets, 6000 px | 0.57 s, 615 MB | 0.23 s, 104 MB |
| big drawing, 60 sheets, 6000 px | 5.6 s, 1.72 GB | 1.6 s, 374 MB |
| small drawing, 10 sheets, 10 000 px | 1.2 s, 1.77 GB | 0.42 s, 169 MB |
| big drawing, 60 sheets, 10 000 px | 12.8 s, 3.65 GB | 3.2 s, 408 MB |

With N threads, N sheets are drawn at once, so memory grows with N
(the big drawing at 10 000 px: 575 MB with 4 threads).

## Picture scores (30 sheets, against ezdxf + matplotlib)

| mean | before | after |
|---|---|---|
| match | 97.41 | 97.64 |
| edge F1 | 96.00 | 97.66 |
| missing % | 2.70 | 2.19 |
| extra % | 2.24 | 2.32 |
| SSIM | 72.39 | 71.17 |
| pixel diff % | 37.78 | 41.17 |

Lines are found better (match, edge F1, missing). SSIM and pixel diff are a
little worse on purpose: they compare pixel by pixel, and a crisp line one row
away from the ezdxf line counts fully. matplotlib snaps with a half-pixel bias
(`floor(v + 0.5) + 0.5` in a y-up frame). A test build that copied this bias
got SSIM back to the old value and half of the pixel diff, but it moves lines
by up to 1 px, so we keep the correct rounding. Two sheets drop more: one
only because the comparison tool chose a worse alignment (scale 1.0047,
correlation 0.711 instead of 1.0098, 0.731; the picture changed in 660 pixels
only), and one because close parallel diagonals are now darker (closer
to ezdxf's dark band, but counted as "extra").

Snapping all pens below 1.5 px to 1 px (as the hatch code does when level of
detail is on) gave no speed gain here (render 1.28 s both ways): this drawer's
cost is per column, so the two thin extra rows of a 1.19 px line are cheap.
So lines keep their true width class.

## A grid over model space

`render/index.rs` puts the boxes of the top-level model-space entities in a
grid (when there are 2000 or more). The walk and the sheet framing then only
look at entities near each viewport's window. On one test drawing (228 000 top-level
entities, about 2 % visible per viewport) this removed most of the walk time.
The pictures are the same pixel for pixel.

## Tests

- `crates/cadrender-raster/src/lib.rs`: snapping, 2 px lines, fills with
  holes, region paste and clip, colour promotion.
- `crates/cadrender-raster/tests/golden.rs`: a golden picture of all width
  classes, angles, dashes, arcs, fills and a region, with a tolerance made for
  anti-aliased edges. Update on purpose with `CADRENDER_UPDATE_GOLDEN=1`.
- `crates/cadrender/tests/strips.rs`: strips give the same pixels as one
  piece; width limits; a 10 000 px picture.
- `crates/cadrender-core/src/render/index.rs`: grid query against brute force.
