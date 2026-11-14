# cadrender

[![PyPI](https://img.shields.io/pypi/v/cadrender.svg)](https://pypi.org/project/cadrender/)
[![PyPI downloads](https://static.pepy.tech/badge/cadrender)](https://pepy.tech/project/cadrender)
[![crates.io](https://img.shields.io/crates/v/cadrender.svg)](https://crates.io/crates/cadrender)
[![crates.io downloads](https://img.shields.io/crates/d/cadrender.svg)](https://crates.io/crates/cadrender)
[![CI](https://github.com/hameddavodi/cadrender/actions/workflows/ci.yml/badge.svg)](https://github.com/hameddavodi/cadrender/actions/workflows/ci.yml)

cadrender turns CAD drawings (DXF and DWG files) into pictures (PNG and SVG),
one picture per sheet. A **sheet** is a paper-space layout tab, the page you
would print. cadrender is written in Rust, and you can use it from the command
line, from Python or from Rust.

## Why

It is fast. A typical sheet takes about **10 ms**. In one test, 8 real-world
architectural drawings with 320 sheets in total rendered in about **3 seconds
on one CPU core**. The same job with a Python pipeline (ezdxf + matplotlib)
took hours.

That makes it useful for previews, thumbnails, search indexes, document
pipelines and anything else that must look at many drawings quickly.

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
