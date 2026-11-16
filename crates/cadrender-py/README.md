# cadrender for Python

`cadrender` turns DXF and DWG drawings into PNG and SVG pictures, one
picture per sheet (layout tab). It is written in Rust and is very fast:
about 10 ms per sheet, so a drawing with 16 sheets takes well under half a
second.

This package is a thin Python layer over the Rust library. It uses
**PyO3** (a tool to call Rust from Python) and **maturin** (a tool to build
a Python wheel from Rust). A **wheel** is the file `pip` installs.

```sh
pip install cadrender
```

## Quick start

```python
import cadrender

doc = cadrender.open("drawing.dxf")        # read the file once (.dxf or .dwg)
print(doc.sheets)                          # ['01', '02', ...] in tab order
print(doc.info["version"], doc.info["parse_ms"])

png = doc.render("01", width=2000)         # bytes of a PNG file
doc.render_to("01", "out/01.png")          # or straight into a file
doc.render_to("01", "out/01.svg")          # SVG: the format comes from the extension

summary = doc.render_all("out/", threads=4)    # all sheets, in parallel
print(summary["done"], summary["failed"])
for s in summary["sheets"]:
    print(s["sheet"], s["ms"], s["files"])
```

## The API

| Call | What it does |
|---|---|
| `cadrender.open(path, font=None)` | Read a drawing. `font`: `"auto"` (Arial if installed, else Liberation Sans), `"liberation"`, or a `.ttf` path. |
| `doc.sheets` | Sheet names in tab order. Model space is not a sheet. |
| `doc.info` | A dict: `version`, `size_bytes`, `parse_ms`, `font`, counts, `skipped_types`, `reader_warnings`. |
| `doc.sheet_info()` | Sheets in tab order: `sheet`, `order`, `paper` (`size_mm`, `margins_mm`, `limits`, `units_per_mm`, `psltscale`). |
| `doc.attributes(sheet=None)` | Block attributes: one dict per INSERT with attributes (`sheet`, `block`, `layer`, `handle`, `position`, `values` = {tag: value}). `"Model"` = model space. |
| `doc.texts(sheet=None, layers=None)` | TEXT, MTEXT and MULTILEADER texts as plain text (`sheet`, `layer`, `type`, `handle`, `text`, `position`). |
| `doc.render(sheet, width=2000, format="png", **options)` | One sheet as `bytes`. |
| `doc.render_to(sheet, path, width=2000, format=None, **options)` | One sheet into a file. The format comes from the file extension. Returns the sheet's result dict. |
| `doc.render_all(out_dir, width=2000, formats=("png",), threads=None, sheets=None, skip_existing=False, on_sheet=None, **options)` | Many sheets in parallel. Returns a summary dict. |

`render_all` details:

- File names are `<stem>__<sheet>.png`. `stem` is the drawing's file name
  without extension (you can pass `stem=...`). A `/` or `\` in a sheet
  name becomes `_`.
- **One bad sheet never stops the others.** The summary has `done` and
  `skipped` (lists of names), `failed` (name -> error text) and `sheets`
  (one dict per sheet: `ok`, `error`, `files`, `ms`, `render_ms`,
  `encode_ms`, `write_ms`, `width`, `height`, `warnings`, `stats`).
- `threads=None` uses all cores. `threads=1` is one core.
- `sheets=[...]` renders only these sheets. A name that is not in the file
  is listed in `failed`.
- `skip_existing=True` leaves a sheet alone when its files already exist.
- `on_sheet(name, result)` is called after each sheet. It runs in a worker
  thread. If it raises, sheets that have not started are cancelled, and the
  exception is raised at the end.

Files are written to a temporary name first and then renamed. So a file
never exists half written, even if the process is killed.

## Options

Give options as keyword arguments, or as a `RenderOptions` object, or both
(keyword arguments win):

```python
from cadrender import RenderOptions

opts = RenderOptions(preset="none", color="true", background="#202020", full_detail=True)
png = doc.render("01", options=opts, lineweight="hairline")
```

| Option | Values |
|---|---|
| `preset` | `"preview"` (default: grey look, paper lineweights, the `Defpoints` layer left out) or `"none"` (true colour, no clean rules) |
| `color` | `"mono"` (grey levels) or `"true"` (real colours) |
| `background` | `"#RRGGBB"`, `"#RRGGBBAA"`, `"white"`, `"black"`, `"transparent"` or `(r, g, b[, a])` |
| `lineweight` | `"paper"` (true plot width), `"preview"` (ezdxf's rule, uses `dpi`), `"hairline"` |
| `default_lw`, `min_lw_px`, `dpi` | default lineweight (1/100 mm), thinnest pen (px), dpi for the `"preview"` rule |
| `frame`, `margin`, `max_height` | `"extents"` or `"paper"`; margin around the extents (0.05 = 5 %); highest picture height |
| `full_detail` | `True` = no level-of-detail shortcuts (no dots, no grey bars for tiny text, no tint for dense hatches) |
| `lod` | single limits, for example `{"dot_px": 0.5}` |
| `clean` | `False` (no clean rules), `True` (the preset's), or `{"layers": [...], "block_keywords": [...]}` |
| `draw_order`, `text_masks`, `wipeout_frames`, `parallel_viewports` | `True` / `False` |
| `max_depth`, `max_visits` | safety limits for very deep or very big block trees |

A wrong name or value raises `cadrender.OptionsError`. Typos are never
ignored silently. The font is chosen in `open()`, because fonts are part of
the opened drawing.

## Errors

All errors are subclasses of `cadrender.Error`:

| Error | When | Also a |
|---|---|---|
| `OpenError` | the file cannot be read as a drawing | `OSError` |
| `DrawingNotFoundError` | the file does not exist | `FileNotFoundError` |
| `UnsupportedFormatError` | a file type cadrender does not read | `OpenError` |
| `SheetNotFoundError` | no sheet with that name | `KeyError` |
| `OptionsError` | a wrong option name or value | `ValueError` |
| `RenderError` | one sheet could not be drawn or written (`render`, `render_to`) | |

SVG: `cadrender.SVG_AVAILABLE` is `True` in every release build.
`format="svg"` (or a `.svg` file name) writes SVG with real `<text>`
elements.

## Threads and the GIL

The **GIL** is Python's global lock: only one thread runs Python code at a
time. cadrender lets go of the GIL while it reads, draws and writes. So
other Python threads keep running, and you can call `render` on the same
`Document` from many threads at once. `on_sheet` takes the GIL again, only
for the call.

## Speed

Measured on an Apple M4 (best of 3, width 2000, PNG):

| Drawing | Sheets | 1 thread | all threads | CLI, all threads |
|---|---|---|---|---|
| real-world drawing, 12 MB DXF | 10 | 0.17 s | 0.06 s | 0.09 s |
| real-world drawing, 52 MB DXF | 16 | 0.43 s | 0.18 s | 0.21 s |

For comparison, an ezdxf + matplotlib pipeline took 162 s for one
hatch-heavy sheet of the second drawing. The PNG files are byte for byte
the same as the CLI's.

## Build and test

You need Rust (stable) and [uv](https://docs.astral.sh/uv/) (or pip).

```sh
cd crates/cadrender-py
uv venv .venv --python 3.12                      # .venv is gitignored
uv pip install --python .venv/bin/python "maturin>=1.9.4,<2" pytest
.venv/bin/maturin build --release --out dist -i .venv/bin/python
uv pip install --python .venv/bin/python --reinstall dist/cadrender-*.whl
.venv/bin/python -m pytest -q                    # tests use the installed wheel
CADRENDER_SAMPLES=/path/to/folder .venv/bin/python -m pytest -q tests/test_real_samples.py   # opt-in
```

The wheel is **abi3**: one wheel works on every CPython from 3.10 up.
The release workflow (`.github/workflows/release.yml`) builds wheels for
Linux x86_64 and aarch64 (manylinux), macOS x86_64 and arm64, and Windows
x86_64.

`CADRENDER_SAMPLES` is optional: point it at a folder with your own `.dxf`
and `.dwg` files, and the test renders every sheet of every file.

The test drawing `tests/data/three_sheets.dxf` is made by
`tests/data/gen.py`. The tests also open every hand-made DXF in
`crates/*/tests/data/`.

## Files

```
Cargo.toml, build.rs      Rust crate (PyO3); never links libpython
pyproject.toml            maturin settings: module cadrender._cadrender
src/lib.rs                open(), module setup
src/doc.rs                Document: sheets, info, metadata, render, render_to, render_all
src/batch.rs              render_all: rayon threads, callbacks, summary
src/render.rs             one sheet -> bytes / files, result dicts
src/options.rs            option dict -> RenderOptions (checks every key)
src/svg.rs                the SVG switch
python/cadrender/         the Python API, errors, RenderOptions, stubs
tests/                    pytest
```
