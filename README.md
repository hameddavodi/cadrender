# cadrender

[![PyPI](https://img.shields.io/pypi/v/cadrender.svg)](https://pypi.org/project/cadrender/)
[![PyPI downloads](https://static.pepy.tech/badge/cadrender)](https://pepy.tech/project/cadrender)
[![crates.io](https://img.shields.io/crates/v/cadrender.svg)](https://crates.io/crates/cadrender)
[![crates.io downloads](https://img.shields.io/crates/d/cadrender.svg)](https://crates.io/crates/cadrender)
[![CI](https://github.com/hameddavodi/cadrender/actions/workflows/ci.yml/badge.svg)](https://github.com/hameddavodi/cadrender/actions/workflows/ci.yml)

**Fast DXF and DWG to PNG and SVG renderer**, written in Rust, with a Python
package and a command line tool. It reads AutoCAD drawings (DXF, and DWG
directly, no converter needed) and draws one picture per sheet. A **sheet** is a
paper-space layout tab, the page you would print.

About **470 times faster than ezdxf + matplotlib** on the same drawings, with
less memory and AutoCAD-like plots.

```sh
pip install cadrender
cadrender render drawing.dwg --all --out-dir previews/
```

## Speed: cadrender vs ezdxf

Same 30 sheets from 8 real-world architectural drawings, same picture size
(2000 px wide), one CPU core, Apple M4. ezdxf 1.4.5 + matplotlib 3.11,
Python 3.12. Times include reading the file.

| Drawing | DXF size | Sheets in file | Sheets rendered | ezdxf + matplotlib | cadrender | Faster |
|---|---:|---:|---:|---:|---:|---:|
| A | 40 MB | 46 | 5 | 208 s | 0.12 s | 1,680× |
| B | 25 MB | 18 | 2 | 52 s | 0.08 s | 700× |
| C | 158 MB | 117 | 3 | 66 s | 0.33 s | 200× |
| D | 51 MB | 24 | 3 | 27 s | 0.12 s | 220× |
| E | 222 MB | 60 | 5 | 194 s | 0.46 s | 420× |
| F | 52 MB | 16 | 5 | 70 s | 0.14 s | 490× |
| G | 101 MB | 29 | 4 | 83 s | 0.25 s | 340× |
| H | 12 MB | 10 | 3 | 26 s | 0.04 s | 610× |
| **All** | | | **30** | **725 s (12 min)** | **1.5 s** | **470×** |

- The ezdxf numbers are already the **fast** ezdxf setup: we gave its drawing
  frontend a shared bounding-box cache, which makes it about 4× faster than the
  plain `qsave` call. Plain ezdxf is slower still.
- **Every sheet of all 8 drawings (320 sheets):** cadrender takes **2.9 s** on one
  core and **1.3 s** on 10 cores, about **10 ms per sheet**. With ezdxf
  (about 22 s per sheet) the same job would take about 2 hours.
- **In a real production pipeline** (DWG converted to DXF, then ezdxf +
  matplotlib), a 10-sheet drawing took 16 to 27 minutes, and a 39-sheet drawing
  never finished within a 3-hour limit. cadrender reads the DWG directly and
  renders a drawing of that size in well under a second.
- **Memory:** ezdxf used up to 2.7 GB per drawing; cadrender peaks at 0.4 GB
  (DXF) and 0.8 GB (DWG) on the largest file (222 MB DXF).

Why so fast: the file is read once into a read-only scene, blocks are drawn
by reference (never copied), everything outside the view is skipped early,
sheets render in parallel, and thin lines use a custom rasteriser.

## Features

- Reads **DXF** (ASCII) and **DWG** directly, versions **R2004 to R2018**. No
  converter program is needed. The DWG reader is written from scratch in Rust.
- Writes **PNG** and **SVG**.
- **Paper-space viewports**: clipping, frozen layers per viewport, scale and
  twist.
- **Hatches**: solid, pattern and gradient fills, islands.
- **Text**: TEXT, MTEXT (with inline formatting), attributes. Text styles that
  use SHX fonts are drawn with one clean sans font (see "Fonts" below). The
  workspace also has a stand-alone SHX font reader crate, `cadrender-shx`.
- **Dimensions**, LEADER and **MLEADER** (multileader).
- **Images** (IMAGE entities with PNG or JPEG files) and WIPEOUT masks.
- **ACIS outlines** of REGION, 3DSOLID and BODY entities.
- Lineweights, linetypes, draw order (SORTENTS), true colour or grey output.
- Level of detail: things too small to see are simplified, so huge sheets
  stay fast.
- Safe on broken files: a bad sheet gives an error, not a crash.

## Install

Python (wheels for Linux, macOS and Windows; CPython 3.10 or newer):

```sh
pip install cadrender
```

Rust library:

```sh
cargo add cadrender
```

Command line tool: download the archive for your system from the
[Releases](https://github.com/hameddavodi/cadrender/releases) page, or build
it with `cargo install cadrender-cli`.

## Command line

```sh
cadrender sheets drawing.dwg                      # list the sheets
cadrender attribs drawing.dwg --sheet A01 --json  # block attributes (title block values)
cadrender render drawing.dwg --sheet A01 -o a01.png
cadrender render drawing.dxf --sheet A01 -o a01.svg
cadrender render drawing.dxf --all --out-dir out/ --svg   # every sheet, PNG + SVG
```

`render` prints a JSON summary with the time of each sheet. Run `cadrender`
with no arguments to see all options, for example `--width`, `--preset
preview|none`, `--color mono|true` and `--frame extents|paper`.

## Python

```python
import cadrender

doc = cadrender.open("drawing.dwg")      # read the file once (.dxf or .dwg)
print(doc.sheets)                        # ['A01', 'A02', ...] in tab order

png = doc.render("A01", width=2000)      # bytes of a PNG file
doc.render_to("A01", "out/A01.svg")      # the format comes from the extension

summary = doc.render_all("out/", threads=4)   # all sheets, in parallel
print(summary["done"], summary["failed"])
```

See [crates/cadrender-py/README.md](crates/cadrender-py/README.md) for all
options.

## Only some sheets, without title blocks

You do not have to render every tab. Pick the sheets by name, and leave out
layers or blocks you do not want in the picture (for example the title block
or a logo). Nothing is rewritten: the rules are applied while drawing.

```python
doc = cadrender.open("drawing.dwg")
doc.render_all(
    "out/",
    sheets=["A01", "A07", "A12"],                 # only these tabs
    clean={
        "layers": ["TITLE_BLOCK", "Defpoints"],   # layers to leave out
        "block_keywords": ["titleblock", "logo"], # INSERTs whose block name contains these
    },
)
```

Command line: `cadrender render drawing.dwg --all --sheets A01,A07 --out-dir out/`.

## Reading metadata

The same opened drawing also gives its data: block attributes (for example
the values of a title block), texts and the sheet list. From DXF or straight
from DWG, with the same results.

```python
doc = cadrender.open("drawing.dwg")
doc.sheet_info()     # [{"sheet": "A01", "order": 1, "paper": {"size_mm": (1189.0, 841.0), ...}}, ...]

for a in doc.attributes("A01"):              # sheet=None: all; "Model": model space
    print(a["block"], a["values"])           # TITLE_BLOCK {'PROJECT': 'P-001', 'DRAWING': '...', ...}

doc.texts("A01", layers=["NOTES"])          # TEXT, MTEXT and MULTILEADER as plain text
# [{"sheet": "A01", "layer": "NOTES", "type": "MTEXT", "text": "First line\nSecond line", ...}]
```

Values are plain text: `\U+00E8` and `%%d` are decoded, MTEXT formatting is
removed, multi-line attributes are joined with `\n`. Each record also has the
layer, the handle and the position. Hidden attributes are included. An
INSERT inside another block is reported under that block's name; anonymous
blocks (dynamic blocks, dimensions) are named by their handle, `*U#2F1A`.

## Rust

```rust
use cadrender::{load_fonts, Document, FontChoice, RenderOptions};

let fonts = load_fonts(&FontChoice::Auto)?;
let doc = Document::open("drawing.dxf", fonts)?;
let opts = RenderOptions::preset("preview").unwrap();
let prep = doc.prepare(&opts);
for sheet in doc.sheets() {
    let img = doc.render_png(sheet, &opts, &prep)?;
    std::fs::write(format!("{sheet}.png"), &img.png)?;
}
```

## Presets

- `preview` (default): grey levels on white, true paper lineweights, and the
  `Defpoints` layer left out (CAD programs never plot it).
- `none`: true colours, nothing left out.

## Fonts

cadrender does not ship AutoCAD fonts or any other non-free font. All text is
drawn with one sans font: **Arial** if it is installed on your system, else
the bundled **Liberation Sans**, which has the same letter widths as Arial.
Each text style keeps its height, width factor, slant and alignment.

## For AI coding assistants

- [`llms.txt`](llms.txt): a short, plain-text summary of the API for language
  models and AI agents (also at https://hameddavodi.github.io/cadrender/llms.txt).
- [`skills/cadrender/SKILL.md`](skills/cadrender/SKILL.md): a ready-made skill
  for Claude Code and other agents that support skills. Copy the folder to
  `~/.claude/skills/` and the agent will use cadrender when you ask it to turn
  DXF or DWG files into pictures.

## Project layout

| Crate | What it does |
|---|---|
| `cadrender` | The library you use: open a drawing, render sheets |
| `cadrender-cli` | The `cadrender` command line tool |
| `cadrender-core` | Scene model, entities, options, the drawing walk |
| `cadrender-dxf` | DXF reader |
| `cadrender-dwg` | DWG reader |
| `cadrender-dwg-scene` | Turns a DWG drawing into a scene |
| `cadrender-raster` | PNG output |
| `cadrender-svg` | SVG output |
| `cadrender-image` | Loads pictures of IMAGE entities |
| `cadrender-acis` | ACIS outlines |
| `cadrender-shx` | Stand-alone SHX font reader and text layout |
| `cadrender-py` | Python package |

More detail: [docs/architecture.md](docs/architecture.md).

## Build from source

```sh
cargo build --release -p cadrender-cli     # Rust 1.95 (see rust-toolchain.toml)
cargo test --workspace
```

Python package (needs [maturin](https://www.maturin.rs/)):

```sh
cd crates/cadrender-py
maturin develop --release
pytest
```

## Licence

cadrender is dual licensed under either of

- the MIT licence ([LICENSE-MIT](LICENSE-MIT))
- the Apache License 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.

The bundled Liberation Sans font is under the SIL Open Font License 1.1
([crates/cadrender-core/fonts/LICENSE-LiberationSans-OFL.txt](crates/cadrender-core/fonts/LICENSE-LiberationSans-OFL.txt)).
The built-in stroke fonts of `cadrender-shx` are made from the Hershey font
data ([crates/cadrender-shx/fonts/HERSHEY-NOTICE.txt](crates/cadrender-shx/fonts/HERSHEY-NOTICE.txt)).

Unless you say otherwise, any contribution you submit for inclusion in this
project, as defined in the Apache-2.0 licence, is dual licensed as above,
without any extra terms or conditions.
