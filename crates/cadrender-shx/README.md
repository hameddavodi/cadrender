# cadrender-shx

Fonts for CAD text, in pure Rust, with **no dependencies**.

- **SHX reader** (`ShxFont`): reads compiled AutoCAD shape files (`.shx`):
  normal text fonts, shape files for linetypes, Unicode fonts ("unifont")
  and Asian big fonts. All shape codes work. A glyph (one letter) becomes
  polylines (lists of points) in font units plus an advance width.
- **Font resolution** (`FontResolver`): turns the font name of a text style
  (`romans.shx`, `arial.ttf`, "Swis721 Lt BT", ...) into a file on disk, a
  similar free font, or a built-in stroke font.
- **Built-in fonts** (`BuiltinFont`): two stroke fonts made from the free
  Hershey font data. They look like AutoCAD's `romans.shx` and `romand.shx`
  and are always there, also on a Linux server with no fonts.
- **Text layout** (`layout_text`): one line of text with height, width
  factor, oblique angle, all TEXT alignments, and the special codes
  `%%c %%d %%p %%u %%o %%k %%% %%nnn \U+XXXX \M+nXXXX`.
  It works through a small trait (`FontFace`), so the core can use the same
  layout for TrueType fonts.

Build and test alone (the crate joins the workspace later through
`members = ["crates/*"]`):

```
cargo test  --manifest-path crates/cadrender-shx/Cargo.toml
cargo bench --manifest-path crates/cadrender-shx/Cargo.toml
cargo run   --manifest-path crates/cadrender-shx/Cargo.toml --example resolve
cargo run   --manifest-path crates/cadrender-shx/Cargo.toml --example specimen -- out.svg [font.shx]
```

## 1. Fonts used by real drawings

Source: 8 real-world architectural drawings (as DXF). I read the
STYLE table (code 3 = font file, code 4 = big font, ACAD xdata = TrueType
family name), counted every TEXT, MTEXT, ATTRIB, ATTDEF and MULTILEADER by its
style (code 7, MULTILEADER code 343, no code 7 = "Standard"), and collected
the MTEXT inline font codes (`\F...;` and `\f...;`).

**91,270 text entities in total. 82 % use an SHX font, almost all `romans.shx`.**

| Font file (code 3) | Kind | Family (xdata) | Styles | Text uses | Files (of 8) | On a test Mac? | Free to ship? |
|---|---|---|---|---|---|---|---|
| romans.shx | SHX | – | 96 | **74,081** | 8 | no | no (Autodesk) |
| arial.ttf | TTF | Arial | 43 | 12,007 | 8 | yes (`Arial.ttf`) | no (Monotype) |
| swissl.ttf | TTF | Swis721 Lt BT | 15 | 1,748 | 8 | no | no (Bitstream/Monotype) |
| arialn.ttf | TTF | Arial Narrow | 10 | 1,285 | 3 | yes (`Arial Narrow.ttf`) | no |
| gothic.ttf | TTF | Century Gothic | 7 | 1,028 | 5 | no | no (Monotype) |
| simplex.shx | SHX | – | 19 | 424 | 8 | no | no |
| stylu.ttf | TTF | Stylus BT | 8 | 329 | 8 | no | no (Bitstream) |
| verdana.ttf | TTF | Verdana | 1 | 113 | 1 | yes | no (Microsoft) |
| romand.shx | SHX | – | 7 | 109 | 3 | no | no |
| isocpeur.ttf | TTF | ISOCPEUR | 6 | 61 | 3 | no | no (ships with AutoCAD) |
| txt.shx | SHX | – | 2 | 55 | 1 | no | no |
| isocp (= isocp.shx) | SHX | – | 3 | 14 | 3 | no | no |
| swiss.ttf | TTF | Swis721 BT | 1 | 14 | 1 | no | no |
| gothicb.ttf | TTF | Century Gothic | 1 | 2 | 1 | no | no |
| arialbd.ttf, arialni.ttf | TTF | Arial, Arial Narrow | 3 | 0 | 2 | yes | no |
| ltypeshp.shx | SHX shape file | – | 5 | (linetypes) | 5 | no | no |

- **No big fonts** (code 4 is empty everywhere). All files use code page
  `ANSI_1252`.
- Styles use width factors 0.5 to 1.6 and oblique angles 1°, 10°, 14.94°, 20°.
- `ltypeshp.shx` is a **shape file** for complex linetypes (shape 134 is used
  in 5 files; in the standard file this is most likely `BAT`, the insulation
  "batting" pattern). It is not a text font.
- **MTEXT inline font changes** (`\f`): `@Arial Unicode MS` 191 times in 5
  files, `Arial` 103, `Arial Narrow` 71, `Century Gothic` 14, `Symbol` 4,
  `Verdana` 2. No inline `\F` SHX switches.
- So the most important font by far is `romans.shx`. It is not free and not
  on the machine, so the built-in Hershey Simplex font draws it.

## 2. Fallback table and licences

Resolution order: (1) the exact file in the search folders
(case-insensitive), (2) the same font under another file name, (3) a similar
free font, with a width hint, (4) a built-in stroke font. Only fonts that
are installed are used; nothing here downloads fonts.

| Requested | Same font, other name | Similar free font (width hint) | Last resort |
|---|---|---|---|
| romans, simplex, txt, isocp, isoct, monotxt, italic, scripts (.shx) | – | – | **builtin Simplex** |
| romand, romanc, romant, complex, italicc, italict, scriptc (.shx) | – | – | **builtin Duplex** |
| any other .shx text font | – | – | builtin Simplex |
| arial.ttf / Arial | Arial.ttf | Liberation Sans, Arimo (1.0), Helvetica.ttc, DejaVu Sans (0.9) | default sans |
| arialuni.ttf / (@)Arial Unicode MS | Arial Unicode.ttf | Arial.ttf, Liberation Sans, Arimo, DejaVu Sans | default sans |
| arialbd / ariali / arialbi.ttf | Arial Bold/Italic.ttf | Liberation Sans Bold/Italic, Arimo | default sans |
| arialn.ttf / Arial Narrow (+ b, i) | Arial Narrow.ttf | Liberation Sans Narrow (1.0) if installed, else Liberation Sans / Arimo / Arial (0.82), DejaVu Sans (0.74) | default sans |
| swissl.ttf, swiss.ttf / Swis721 (Lt) BT | – | Liberation Sans, Arimo, Arial, Helvetica (1.0), DejaVu Sans (0.9) | default sans |
| swissb, swissc, swisscl.ttf | – | Liberation Sans Bold / Narrow | default sans |
| gothic.ttf, gothicb.ttf / Century Gothic | Century Gothic.ttf | TeX Gyre Adventor, URW Gothic (1.0), Liberation Sans (1.1) | default sans |
| stylu.ttf / Stylus BT | – | – | **builtin Simplex** (same technical-lettering look) |
| isocpeur, isocteur.ttf / ISOCPEUR | – | osifont (1.0) | **builtin Simplex** |
| verdana.ttf / Verdana | Verdana.ttf | DejaVu Sans (1.0), Liberation Sans (1.1) | default sans |
| times.ttf / Times New Roman | Times New Roman.ttf | Liberation Serif, Tinos, DejaVu Serif | default sans |
| cour.ttf / Courier New | Courier New.ttf | Liberation Mono, Cousine, DejaVu Sans Mono | default sans |
| calibri.ttf | – | Carlito, Liberation Sans | default sans |
| symbol.ttf / Symbol | Symbol.ttf | – | default sans |
| big font (code 4) | exact file only | – | none: show '?' |
| shape file (style flag 1) | exact file only | – | none: draw the linetype without shapes |

Default sans (any other TrueType font): Liberation Sans, Arimo, Arial,
Helvetica, DejaVu Sans; if none is installed: builtin Simplex.

The width hint (`Resolved::width_scale`) is meant to be multiplied into the
style's width factor. 1.0 = metric-compatible (same advance widths). The
other values (0.82, 0.9, 1.1, 0.74) are rough estimates, not measured.

Licences of the fonts named above:

| Font | Licence | Free to ship with cadrender? |
|---|---|---|
| **Hershey font data** (our built-in fonts) | Free for any use, commercial too; the acknowledgement in `fonts/HERSHEY-NOTICE.txt` must ship with it | **yes** (already built in) |
| Liberation Sans / Serif / Mono 2.x | SIL OFL 1.1 | yes (as separate font files) |
| Liberation Sans **Narrow** | only in old 1.07 releases: GPL-2 + font exception (dropped from 2.x) | no; use only if installed |
| Arimo, Tinos, Cousine | Apache-2.0 (original release), OFL 1.1 (Google Fonts) | yes |
| Carlito | SIL OFL 1.1 | yes |
| DejaVu fonts | Bitstream Vera licence + public domain changes (permissive) | yes |
| TeX Gyre Adventor | GUST Font License (LPPL-style, free) | yes, with its licence file |
| URW Gothic (URW base35) | AGPL-3 + font exception | no; use only if installed |
| osifont | GPL-3 + font exception | no; use only if installed |
| LibreCAD `.lff` fonts | mostly GPL-2+ (`romans.lff`: Hershey basic Latin + GPL additions) | **no, not used** |
| QCAD `.cxf` fonts | come with GPL QCAD; not checked file by file | **no, not used** |
| AutoCAD `.shx`, Arial, Verdana, Century Gothic, Swiss 721, Stylus BT, ISOCPEUR | commercial | no; use only if the user has them |

The Liberation, DejaVu and Hershey rows were checked against the project
pages; the Arimo/Carlito/TeX Gyre/URW/osifont rows are from my knowledge and
should be re-checked before we ship any of those files.

## 3. API

```rust
use cadrender_shx::*;

// Once per process: scan font folders (about 5 ms for 460 files on a test Mac).
// CADRENDER_FONT_PATH (":" or ";" separated) is searched first.
let resolver = FontResolver::system();          // or FontResolver::with_dirs([...])

// Per text style (code 3 font file, ACAD xdata family, code 70 flags):
let r: Resolved = resolver.resolve("romans.shx", None);
match &r.source {
    FontSource::Shx(path)              => { /* ShxFont::from_file(path)? */ }
    FontSource::Builtin(b)             => { let font: &'static ShxFont = b.font(); }
    FontSource::TrueType { path, index } => { /* core: ttf-parser */ }
}
// r.quality: Exact | SameFont | Similar | Fallback   (log substitutions)
// r.width_scale: multiply into the style width factor

resolver.resolve_bigfont("gbcbig.shx");        // Option<Resolved>
resolver.resolve_shape_file("ltypeshp.shx");   // Option<Resolved>
resolver.resolve_family("Arial", bold, italic); // MTEXT \fArial|b1|i0;
resolver.resolve_inline("romans.shx");          // MTEXT \Fromans.shx;

// Layout one line (TEXT / ATTRIB / one MTEXT run). Reuse both buffers.
let mut layout = TextLayout::new();
let mut lines = Polylines::default();
let p = TextParams {
    height: 2.5, width_factor: 0.8, oblique_deg: 15.0,
    halign: HAlign::from_dxf(72_value), valign: VAlign::from_dxf(73_value),
    fit_width: 0.0,          // distance between the 2 points for Aligned / Fit
    backward: false, upside_down: false,   // TEXT code 71 bits 2 and 4
};
layout_text(font, "%%c120 %%uNOTE%%u", &p, &mut layout);
lines.clear();
layout.stroke_polylines(font, &mut lines);  // text-space polylines (f64)
```

**Text space**: the alignment point is (0, 0), the baseline runs along +x,
height, width factor, oblique, alignment and mirroring are already applied.
The caller adds rotation, the insertion (or alignment) point and the
OCS/block matrix.

Without copying geometry: `layout.glyphs` holds `PlacedGlyph { id, x, scale }`
and `layout.glyph_transform(&g)` gives the `Affine` from the glyph's cached
font-unit points (`font.glyph(id)`, see `Glyph::polylines`) to text space.
A renderer can apply `entity_matrix × glyph_transform` directly to the cached
points and never build its own copy.

Other useful calls:

- `ShxFont::from_bytes / from_file`, `kind()`, `info()`, `above()`,
  `below()`, `slot_for_char(ch)`, `slot_for_code(n)`, `glyph(slot)`,
  `glyph_vertical(slot)`, `slot_by_name("BAT")` (linetype shapes),
  `issues()` (problems found while reading a broken file), `parse_all()`.
- `ShxStyleFont::new(&primary, Some(&bigfont))`: a style with a big font;
  `\M+nXXXX` codes and escape-range codes go to the big font, scaled to the
  primary cap height.
- `tokens(text)`: the special-code decoder alone (no allocation), for the
  core's MTEXT parser.
- `FontFace` trait: implement it for a TrueType face (cap height, descent,
  glyph id, advance) and `layout_text` works for TrueType too.
- `layout.bounds()`: the text box in text space (for culling and masks).

### How the core should call this crate

1. Build one `FontResolver` when the renderer starts; share it.
2. When reading the STYLE table: resolve every style once; load each SHX
   file once into an `Arc<ShxFont>` (key: path). Built-in fonts are
   `&'static`. The glyph cache is inside `ShxFont` and is thread-safe, so
   all render threads share one font. Optionally call `parse_all()` once.
3. Per TEXT/ATTRIB: `layout_text` into a per-thread `TextLayout`, then
   either `stroke_polylines` into a per-thread `Polylines`, or walk
   `layout.glyphs` with `glyph_transform` and the cached glyph points.
   SHX glyphs are **strokes** (draw with the entity lineweight, no fill).
4. For TrueType: implement `FontFace` on the core's ttf-parser face
   (`cap_height` = OS/2 cap height: as far as I know AutoCAD, and ezdxf too,
   make the capitals of TrueType text as tall as the text height; please
   check this with the visual diff) and call the same
   `layout_text`. Fill the outlines.
5. Level of detail: `layout.bounds()` gives the box; when it is smaller than
   about 2 pixels the core can draw a bar instead of glyphs.

## 4. Speed

`cargo bench` on the Apple M4 (release build):

| What | Time |
|---|---|
| parse the font file and all 162 glyphs (cold) | 43 µs |
| `layout_text`, typical drawing text (avg 17 characters) | 64 ns per text |
| `layout_text` + `stroke_polylines` | 251 ns per text |
| character lookup + cached glyph | 2.2 ns per character |

So 25,000 texts (the biggest test drawing) cost about 6 ms of layout on one
core. In the hot path nothing is allocated once the buffers have grown
(a test checks this). Glyphs are parsed once, on first use.

## 5. SHX format notes

Written from the public AutoCAD Customization Guide ("About Shape
Descriptions", "Special Codes Reference", "About Unicode Font Descriptions",
"About Defining a Big Font", "About Defining an Extended Big Font File").
Choices where the public text is not exact (also in `src/interp.rs`):

- The octant byte of codes 10 and 11 is sign + magnitude: bit 7 =
  clockwise, bits 4–6 = start octant, bits 0–2 = octant count (0 = 8).
- Code 11: end offset 0 means "end on the boundary after the last octant";
  radius = high byte × 256 + low byte.
- 2-byte shape numbers inside the byte code (unifont and big font code 7)
  are read high byte first; if that shape does not exist, the swapped
  number is tried.
- Big font `7,0,...` primitives are placed in a box: origin (x, y), scale
  width / font width and height / font height. Extended big font glyphs
  whose pen does not move get the font's character width as advance.
- Code 14 (vertical only): in horizontal text the whole next command is
  skipped (including all its bytes). `glyph_vertical` executes it.
- Pop (code 6) jumps without drawing. The push stack takes 8 entries.
- Tolerance: truncated files keep the complete shapes; bad byte code ends
  the glyph; subshape depth is limited to 10 and the work per glyph is
  limited, so a broken font can not hang the renderer. Every byte of a test
  font is mutated in a test and nothing panics.

Character lookup: Unicode code point as shape number first, then the
Windows-1252 byte (normal shape fonts), then the old AutoCAD numbers
127 = degree, 128 = plus-minus, 129 = diameter. Missing characters become
'?' like in AutoCAD.

## 6. Built-in fonts

`fonts/hershey_simplex.shx` and `fonts/hershey_duplex.shx` are made by
`tools/hershey_to_shx.py` from the Hershey files `rowmans.jhf` and
`rowmand.jhf`. They contain ASCII, the Latin-1 accented letters (made from
the base letter + a hand-drawn accent, with subshapes), ° ± ⌀ (also as the
old numbers 127/128/129), ¹ ² ³ and ·. Cap height 21, descender 7, like
AutoCAD's romans.shx. The `.jhf` input is not in the repo; get it from the
Hershey font distribution (for example the `hershey-fonts` data files)
and run the script to rebuild.

AutoCAD's romans.shx and simplex.shx are themselves Hershey Roman Simplex,
so shapes and widths should be very close. I could not compare with the
real files (they were not available).

## 7. Limits (not done yet)

- Big fonts need the text as double-byte codes. DXF text is Unicode, so the
  core must convert with the drawing code page (cp932/936/949/950) before
  calling `glyph_id_for_code`. No test drawing uses big fonts, so this is not
  built. `\M+nXXXX` codes work.
- Vertical text (style flag 4): `glyph_vertical` gives the right shapes, but
  `layout_text` only does horizontal lines.
- MTEXT (wrapping, inline formatting, stacked fractions) is the core's job;
  this crate gives `tokens()` and single-line layout per run.
- The `Symbol` TrueType font uses its own encoding (Latin letters show as
  Greek). The core must map `\fSymbol` text (4 uses in the test drawings).
- Underline / overline / strike positions (−0.2, 1.2, 0.5 × cap height) are
  close to AutoCAD, not measured.
- There is no free replacement for `ltypeshp.shx` yet (5 test drawings use
  its shape 134 in a linetype).
- Tests use hand-made fonts only; the reader has not met a real AutoCAD
  `.shx` file yet.

## 8. Files

```
src/shx.rs        file reader (shapes, unifont, bigfont), glyph cache
src/interp.rs     shape byte-code interpreter (all codes)
src/glyph.rs      Glyph: flat polylines + advance + box
src/layout.rs     tokens(), layout_text(), TextLayout, Polylines, Affine
src/face.rs       FontFace / StrokeGlyphs traits, ShxStyleFont (font + big font)
src/resolve.rs    FontResolver, fallback table
src/builtin.rs    built-in Hershey fonts
src/codepage.rs   Windows-1252 mapping
fonts/            built-in fonts + HERSHEY-NOTICE.txt
tools/            hershey_to_shx.py, make_test_shx.py (rebuild fonts / test files)
tests/data/       tiny hand-made SHX files (font, shape file, unifont, big font, broken)
tests/            opcodes.rs (one test per shape code), files.rs, layout.rs
benches/layout.rs micro benchmark (no extra crates)
examples/         resolve.rs (font report), specimen.rs (SVG of a font)
```
