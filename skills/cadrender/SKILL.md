---
name: cadrender
description: Render DXF or DWG CAD drawings (AutoCAD files) to PNG or SVG pictures, one per layout tab/sheet, fast. Use when the user wants previews, thumbnails, images or SVGs of .dxf/.dwg files, wants to see what a drawing looks like, or needs to batch-convert many CAD sheets to images. Not for editing CAD data.
---

# cadrender: DXF/DWG to PNG/SVG

cadrender reads DXF and DWG files directly (no ODA converter, no AutoCAD) and draws each
paper-space layout tab ("sheet") as a PNG or SVG, in about 10 ms per sheet.

## Install

```sh
pip install cadrender          # Python package, includes everything
```

The CLI binary is on https://github.com/hameddavodi/cadrender/releases, or
`cargo install cadrender-cli`.

## Python

```python
import cadrender

doc = cadrender.open("drawing.dwg")          # .dxf or .dwg
print(doc.sheets)                            # layout tab names in tab order
doc.render_to(doc.sheets[0], "out/first.png", width=2000)
summary = doc.render_all("out/", formats=("png",), threads=4)   # every sheet
print(summary["done"], summary["failed"])
```

- Only some tabs: `render_all("out/", sheets=["A01", "A03"])`.
- Leave out title blocks or layers: `clean={"layers": ["TITLE"], "block_keywords": ["logo"]}`.
- True colours instead of grey: `preset="none"` or `color="true"`.
- Whole paper sheet instead of drawing extents: `frame="paper"`.
- SVG: `format="svg"` in `render`, or a `.svg` path in `render_to`.

## Command line

```sh
cadrender sheets drawing.dwg
cadrender render drawing.dwg --sheet A01 -o a01.png
cadrender render drawing.dwg --all --out-dir out/ --threads 8 [--svg] [--sheets A01,A02]
```

## Tips

- Open a file once and render many sheets from the same `doc`; reading is the slow part.
- A broken sheet is reported in `summary["failed"]`; the other sheets still render.
- Pictures (IMAGE entities) are loaded from files next to the drawing; missing ones show
  only their frame.
- To look at a result, open the PNG with your image viewer tool.
