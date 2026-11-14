# cadrender-dwg

A DWG reader in pure Rust, part of the cadrender renderer.
It reads AutoCAD DWG files from R2004 to R2018 and gives a clean, typed
object model (`Drawing`, `Object`, `Entity`, ...). The crate
`cadrender-dwg-scene` turns that model into a cadrender `Scene` for drawing.

- No GPL code. The reader is written from the public ODA "Open Design
  Specification for .dwg files", with ACadSharp (MIT) as a second
  reference. See "Licences of the sources used" below.
- Safe: the file is memory-mapped, every bit read is bounds-checked, bad
  input gives a clear `DwgError`, never a panic (tested with damaged files).
- Fast: tens of milliseconds for a typical architectural drawing (see
  "Speed").

```rust
let d = cadrender_dwg::read_file("drawing.dwg", &Default::default())?;
for o in &d.objects {
    if let Some(e) = o.entity() { /* e.common.layer, e.kind, ... */ }
}
```

Tools:

- `cargo run --release --bin dwgdump -- FILE.dwg` prints header values,
  timings and a count per object type (with self-checks, see below).
  `--json` writes one JSON line per object, `--serial` decodes on one core.
- `tools/validate.py` compares `dwgdump --json` with a DXF of the same file
  (for example from ODA File Converter) and, if you have it, with the
  output of another DWG reader. You give the paths; it uses the Python
  standard library only.

Build and test: `cargo test -p cadrender-dwg`.
Extra tests on your own files (opt-in, ignored by default):
`CADRENDER_DWG_SAMPLES=/path/to/folder cargo test -p cadrender-dwg -- --ignored`,
where the folder holds your own `.dwg` files.

## Status per version

| Version | AutoCAD | File structure | Objects | Tested on |
|---|---|---|---|---|
| AC1032 | 2018+ | done | done | 6 real files, all decode |
| AC1027 | 2013-2017 | done (same as 2018) | done (same code path) | no test file yet |
| AC1024 | 2010-2012 | done | done | 1 real file decodes |
| AC1021 | 2007-2009 | done (own code path, see below) | done (version checks) | 1 real file, all objects decode |
| AC1018 | 2004-2006 | done (same as 2010) | done (version checks) | ACadSharp's `sample_AC1018.dwg` (MIT test file) decodes, renders like its DXF |
| older | R13-R2000 | no | no | - |

## How it works (short)

1. `container.rs`: R2004+ file structure. Decrypt the 0x6C header bytes,
   check their CRC-32, read the section page map and the section map (system
   pages, LZ77), then put the data pages of a section together. Pages are
   independent, so they are decompressed **in parallel**. Page checksums and
   the header CRC are checked; a mismatch is a warning, not a stop.
   R2007 files go to `container_r21.rs` instead (next section).
2. `lz77.rs`: the R2004 LZ77 variant, bounds-checked.
3. `header.rs`: AcDb:Header. All variables up to the model/paper space block
   records are walked in order; we keep units, LTSCALE, PSLTSCALE, CELTSCALE,
   extents, limits, INSUNITS and all table/dictionary handles.
4. `classes.rs`: AcDb:Classes (type numbers 500+ -> DXF name).
5. `objmap.rs`: AcDb:Handles (handle -> offset), with the per-block CRC.
6. `objects.rs`: one object. Frame (size, handle stream size, type, handle,
   EED skip), the three R2007+ streams (data, strings, handles, in
   `streams.rs`), common entity / non-entity data, then the type data. All
   objects are decoded **in parallel** with rayon (the object map gives every
   offset, objects do not depend on each other).
7. `drawing.rs`: runs it all and fills in two things a user expects: the
   real colour of colour-book entities (from their DBCOLOR) and the owner of
   model/paper space entities (they do not store it).

### R2007 (AC1021) file structure

R2007 has its own file structure (spec chapter 5). The objects inside are
almost the same as in R2010, so steps 3-7 above are shared.

- `container_r21.rs`: the *file header* at 0x80 is 0x400 bytes of
  Reed-Solomon code words, then LZ77-R21 compressed data (0x110 bytes). It
  gives the place, sizes and CRCs of the *page map* and *section map*
  (both "system pages"). Data pages are Reed-Solomon coded too, then
  optionally compressed. Pages are read **in parallel**.
- `rs.rs`: Reed-Solomon (RS) codes. A code word is 255 bytes: data bytes,
  then parity bytes. System pages use RS(255,239), data pages RS(255,251).
  The bytes of several code words are *interleaved* (mixed). A healthy page
  needs no math: we only pick the data bytes. When a CRC says a page is
  damaged, a real RS decoder (Berlekamp-Massey, Chien, Forney) repairs up
  to 8 (system) or 2 (data) bad bytes per code word, and we read again.
- `lz77_r21.rs`: the R2007 LZ77 variant. Literal bytes are stored in a
  shuffled order (spec table 5.10.1).
- `crc64.rs`: the two 64-bit CRCs (normal and mirrored) and the R2007 page
  checksum. All CRCs and checksums are checked; on the R2007 test file all
  of them match.
- Objects: the frame differs (type is a BS, then an RL "data size in
  bits" instead of the R2010 MC handle-stream size), and a few fields
  exist only from R2010 on (marked "version check for R2007" in
  `objects.rs`).

Self-checks for every object: the object CRC; and after decoding, the data
stream must be used to the exact bit and fewer than 8 bits may stay in the
handle stream. `dwgdump` counts objects that break a rule. On all 8 real
test files the counts are 0 for every type we decode. This check found
most of the bugs during development.

## Status per object type (R2010+)

"Decoded" means all fields are read and the bit self-check passes on every
sample object. Values were compared with ODA for the types marked "ODA".

| Object | Status | Main fields in the model |
|---|---|---|
| LINE, CIRCLE, ARC, POINT, ELLIPSE | decoded, ODA | points, radius, angles, thickness, extrusion |
| SOLID, TRACE | decoded, ODA (SOLID) | 4 corners, elevation |
| LWPOLYLINE | decoded, ODA | flags, widths, vertices, bulges |
| POLYLINE (2D) + VERTEX (2D) + SEQEND | decoded, ODA | flags, widths, vertex handles; point, bulge |
| INSERT, MINSERT | decoded, ODA | block record, point, scale, rotation, ATTRIB handles, grid |
| ATTRIB, ATTDEF | decoded, ODA | text data, tag, flags, prompt; R2018 multi-line MTEXT |
| TEXT | decoded, ODA | all TEXT fields, style |
| MTEXT | decoded, ODA | text, box, attachment, line spacing, **background fill flags, scale, colour** |
| HATCH | decoded, ODA | all boundary paths (polyline, line, arc, ellipse, spline edges), pattern definition lines, seeds, gradient name |
| SPLINE | decoded, ODA | knots, control points, weights, fit points, tangents |
| WIPEOUT, IMAGE | decoded, ODA | frame vectors, clip boundary |
| DIMENSION (all 7 kinds, ARC_DIMENSION, LARGE_RADIAL) | decoded, ODA | dimension style, **anonymous block**, text point |
| VIEWPORT | decoded, ODA | centre, size, view centre/height/target/direction, twist, clip, **status flags**, frozen layers |
| BLOCK, ENDBLK | decoded, ODA | block name |
| BLOCK_HEADER | decoded | name, flags, base point, entity handles, BLOCK/ENDBLK, layout |
| LAYER | decoded | name, on/frozen/locked/plot, colour, linetype, lineweight |
| LTYPE | decoded | pattern length, dashes, shapes, text in dashes |
| STYLE | decoded | font and big font file, height, width, oblique |
| LAYOUT | decoded | plot settings part, name, tab order, limits, extents, block record, viewports |
| SORTENTSTABLE | decoded | (sort handle, entity handle) pairs |
| DICTIONARY, DICTIONARYWDFLT | decoded | entries |
| *_CONTROL tables | decoded | entry handles |
| DBCOLOR | decoded | colour, name, book |
| POLYLINE 3D / mesh / pface + their VERTEX | decoded, ODA (3D) | flags, vertices, face records |
| LEADER | decoded, ODA | points, arrow flag, dimstyle |
| MULTILEADER | decoded, ODA (R2007, R2010+) | full annotation context: leader roots and lines, text or block content, entity values |
| ACAD_TABLE | partly (insert part) | block record of its anonymous block, point, scale, rotation; cell content not read |
| TOLERANCE, 3DFACE, MLINE, RAY, XLINE | decoded (no sample objects) | points, text, vertices |
| MESH | decoded | vertices, faces (edges and creases read, not kept) |
| HELIX | decoded, ODA | AutoCAD's spline + axis, start point, radius, turns, turn height, handedness |
| DIMSTYLE, MLEADERSTYLE | decoded, ODA (raw groups) | every value as a (DXF group code, text) pair |
| VPORT, APPID, XRECORD, WIPEOUTVARIABLES | decoded | view, name, typed items, frame flag |
| LAYER EED | kept | raw EED per APPID (transparency is in "AcCmTransparency") |
| everything else (proxy entities, REGION, ...) | skipped by size, counted | entities still get their common data (layer, colour, ...) |

"Unknown" objects are never a problem: each object has its size, so the
reader jumps over it. `Drawing::stats` counts them per type name.


## Validation

Main reference: the DXF that ODA File Converter writes for the same DWG.
The results were also cross-checked against the output of other DWG
readers. The test set is 8 real-world architectural drawings (they are not
part of this repository).

"Entities" = all entities in the ENTITIES and BLOCKS sections (without
ENDBLK). "Compared" = entities of the decoded types found in both, by
handle. A compared entity is "equal" when type, layer name, colour, the key
numbers (all coordinates, radius, angles, scales, counts) and the key text
(text value, tag, pattern name; block record for INSERT/DIMENSION/BLOCK)
match ODA within 1e-9 relative.

| File | Version | Objects | Failed | Entities ours / ODA DXF | Compared | Equal to ODA |
|---|---|---|---|---|---|---|
| A | AC1032 | 26,311 | 0 | 21,167 / 21,167 | 20,946 | 100 % |
| B | AC1032 | 68,122 | 0 | 55,843 / 55,817 | 55,384 | 100 % |
| C | AC1032 | 105,385 | 0 | 83,331 / 83,331 | 81,583 | 99.993 % (6 splines, see below) |
| D | AC1032 | 187,236 | 0 | 144,689 / 144,689 | 144,348 | 100 % |
| E | AC1032 | 302,845 | 0 | 236,392 / 236,392 | 234,710 | 100 % |
| F | AC1032 | 663,790 | 0 | 546,601 / 546,601 | 529,491 | 100 % |
| G | AC1024 | 142,949 | 0 | 113,554 / 113,554 | 102,619 | 99.865 % (see below) |
| H | AC1021 | 364,681 | 0 | 282,311 / 267,753 | 264,211 | 99.613 % (all explained, see below) |

Header variables LTSCALE, PSLTSCALE, CELTSCALE, INSUNITS, LUNITS, EXTMIN
and EXTMAX are equal to ODA's `$` variables in all 8 files.

### Where we differ from ODA, and who is right

All remaining differences come from ODA changing data when it writes DXF.
In every case other DWG readers read the same raw value as we do.

- **File B, 13 BLOCK + 13 INSERT more than ODA**: 13 anonymous blocks
  (`*U620`...) that nothing references, each with one INSERT. ODA drops
  unreferenced anonymous blocks when it saves. They are in the DWG.
- **Anonymous block names** (`*U12`, `*D7`, `*Paper_Space3`): ODA renumbers
  them in DXF. We compare by block record handle, which always matches.
- **6 + 1 SPLINEs with fit points only**: the DWG stores fit points and no
  control points. ODA computes control points and knots for the DXF. A
  renderer must build such a spline from the fit points.
- **File G, 17 LINEs**: the DWG really holds NaN coordinates. ODA writes
  0.0. The renderer should skip non-finite geometry.
- **File G, 121 MTEXTs**: ODA adds a `\pxql;` paragraph code taken from the
  MTEXT's extension dictionary. The MTEXT string in the DWG has no such
  code.
- **HATCH pattern name `SOLID,_O`**: stored like this in the DWG; ODA strips
  the `,_O` suffix.
- **Object counts**: ODA writes many objects without a DXF form (dynamic
  block parameters, ASSOC objects, AEC objects) as `ACAD_PROXY_OBJECT`, and
  leaves out a few unused DICTIONARY/XRECORD objects.

### File H (R2007): annotative objects saved for old AutoCAD

File H was saved to the 2007 format by a newer AutoCAD with SAVEFIDELITY
on. AutoCAD then writes each *annotative* object (one that has several
annotation scales) in an old-style form: an anonymous block (`*U9985`)
holds one copy per scale, each copy on a layer named `<layer> @ <scale>`
(26 such layers), and the drawing has an INSERT of that block. Each paper
space viewport freezes the `@` layers of the other scales (537 of 654
viewports freeze 21-26 of them), so AutoCAD 2007 shows one copy per
viewport. ODA undoes this when it loads the file: it rebuilds the
annotative object and writes "object context data" for each scale. That
explains every difference:

- **2,651 BLOCK + 2,651 INSERT more, 8,579 DIMENSION + 3,328 MULTILEADER
  "not in ODA"**: the anonymous blocks and their copies. ODA has exactly
  8,579 ACDB_ALDIMOBJECTCONTEXTDATA and 3,328 ACDB_MLEADEROBJECTCONTEXTDATA
  objects instead, with new handles.
- **1,956 + 695 handles that are INSERT for us, DIMENSION / MULTILEADER
  for ODA**: ODA turns the INSERT back into the annotative object.
- **117 MTEXT layers**: ours is `<layer> @ <scale>`, ODA's is `<layer>`
  (ODA has 117 ACDB_MTEXTOBJECTCONTEXTDATA objects).
- **867 MTEXT texts**: only the leading `\p...;` paragraph code differs;
  ODA rewrites it (as in file G).
- **38 SPLINEs**: fit points only, ODA computes control points (as above).

For the renderer this means: draw the INSERT like any other, and respect
the viewport's frozen layers. Then each viewport shows the right scale.

## Speed

Apple M4 (10 cores), wall time of the whole `dwgdump` process (full decode
of all objects + CRC checks + summary), best of 3:

| File | DWG MB | parallel | 1 thread |
|---|---|---|---|
| A | 1.7 | 0.028 s | 0.035 s |
| B | 3.3 | 0.038 s | 0.054 s |
| C | 5.5 | 0.048 s | 0.074 s |
| D | 6.0 | 0.066 s | 0.095 s |
| G | 7.2 | 0.058 s | 0.089 s |
| E | 13.1 | 0.093 s | 0.156 s |
| F | 28.2 | 0.177 s | 0.315 s |
| H (R2007) | 18.7 | 0.102 s | 0.275 s |

Inside the process for file H (56 MB of decompressed objects, one thread):
the sections take about 76 ms (RS de-interleave 6 ms, CRC-64 9 ms with
slicing-by-8, LZ77-R21 42 ms, page checksum 15 ms); on 10 threads about
13 ms.

Inside the process for file F (60 MB of decompressed objects): container
0.2 ms, sections (parallel LZ77) 11 ms, header + classes + object map 9 ms,
object decode 60 ms on 10 threads (235 ms on one), collecting 50 ms.

## Where real files differ from the ODA spec text

Found with the bit self-checks on real files; each fix is commented in code.

| Topic | Spec text | Real files (and ACadSharp) |
|---|---|---|
| BLL (bit long long) length | "3B" (1-3 bits, stop at 0) | a fixed 3-bit count |
| Object map block size | at most 2032 bytes | blocks of 2033 bytes exist; use the size field |
| Entity colour flag 0x4000 (colour book) | RGB value follows | no RGB value; colour is in the DBCOLOR object |
| LZ77 opcodes 0x12-0x1F | length = low 4 bits | low 3 bits; bit 3 adds 0x4000 to the offset |
| LZ77 first byte > 0x11 | (not described) | literal run of `byte - 17` bytes |
| Common entity data order (R2007+) | ltype, plotstyle, material, shadow flags | ltype, material, shadow, plotstyle |
| INSERT "owned object count" | always (R2004+) | only when "has ATTRIBs" is set |
| Header MENUNAME | R13-R2004 only | still present in R2010-R2018 headers |
| MTEXT background scale | BL | BD (default 1.5) |
| LTYPE_CONTROL | entries only | entries + BYLAYER + BYBLOCK handles |
| R2007: CRC-64 byte order | odd order per 8 bytes (6,7,4,5,2,3,0,1) | true: the CRCs only match with this order |
| R2007: page map / section map CRC seed | "PagesMapCrcSeed" | stored *encoded* (spec 5.11); decoded it is 0 |
| R2007: "encoding 1" pages | RS coded, not interleaved | not RS coded at all: raw data, padded |
| R2007: section map page "size" | page size | not the size in the file; take that from the page map |
| R2007: section name length | characters (x 2, + 2) | bytes, including the 2 zero bytes |
| R2007: data page size | at most "max size" | pages are larger (0xFC00, 0x10B00 for max 0xF800); use the offsets |
| R2007: entity graphic size | RL "R13-R007" | RL (only R2010+ uses BLL) |
| R2007: ATTRIB/ATTDEF/DIMENSION version, VERTEX id, IMAGE clip mode | R2010+ | absent in R2007 (confirmed) |

## Licences of the sources used

| Source | Licence | How it was used |
|---|---|---|
| Open Design Specification for .dwg files, v5.4.1 (ODA, public PDF) | free public document | main source for every structure |
| ACadSharp (github.com/DomCR/ACadSharp, commit 0acbe97, 2026-11-13) | **MIT** ("Copyright (c) 2021 Albert Domenech") | second reference: read the C# readers to settle points where the spec is unclear or wrong (table above). For R2007: the LZ77-R21 main loop (the spec's pseudo code mixes up two index names), the RS code word layout and how system pages are read. For R2004: field order. No code was copied; the Rust code is original. If parts are ever ported 1:1, keep the MIT notice. |
| Other DWG readers and converters | various | only run as outside programs, to compare their output with ours. Their source code was not copied or linked. |
| Rust crates: memmap2, rayon | MIT / Apache-2.0 | dependencies |

## Plan for the other versions

- **AC1018 (R2004)**: done. Same file structure as R2010. Objects: the
  R2007 frame (BS type, RL data size in bits) without a string stream;
  texts are 8-bit `T` in the data stream (read as Latin-1: the file's code
  page is not applied yet). The header has its handles inline in the data
  stream. R2007-only fields are behind version checks (LAYER material,
  BLOCK_HEADER units, MTEXT box height, VIEWPORT/VPORT lights and visual
  style, DIMSTYLE DIMFXL/DIMTFILL/DIMARCSYM/DIMLTYPE..., ATTRIB lock,
  DIMENSION flip arrows, plot settings visual style); R2004 has a 256-byte
  LTYPE strings area, XRECORD strings with a code page byte and LEADER box
  height/width. Field order from ACadSharp (MIT). Tested on ACadSharp's
  `sample_AC1018.dwg` (all 1014 objects pass the bit self-check; the render
  equals the render of its DXF on all 3 tabs). More real R2004 files are
  welcome.
- **AC1021 (R2007)**: done, see "R2007 (AC1021) file structure" and file H
  above. Not done: encrypted R2007 files (`DwgError::Encrypted`) and the
  second copy of the file header at the end of the file (only needed if RS
  cannot repair the first one).
- **AC1027 (R2013)**: same code path as R2018 (only the header/classes
  "unknown RL" rule and the R2018-only ATTRIB/MTEXT fields differ, and both
  are handled by version checks). Needs a sample file to confirm.
- **Objects not decoded yet** that a renderer may want: ACAD_TABLE cell
  content (the drawing uses its anonymous block, which is complete), proxy
  graphics, REGION / 3DSOLID (ACIS data). All are skipped safely today.
  `tools/validate.py` compares the newer types (LEADER, MULTILEADER,
  ACAD_TABLE, RAY/XLINE, 3DFACE, MLINE, TOLERANCE) and the DIMSTYLE /
  MLEADERSTYLE raw groups with a DXF of the same file.

## Notes for the Scene adapter

The adapter is the crate `cadrender-dwg-scene` (Drawing -> Scene).

- `Drawing::objects` is sorted by handle; `Drawing::get(handle)` finds one.
- Model space / paper space: `header.handles.model_space` and
  `paper_space`; each `BlockHeader` lists its entities in order, its
  `layout`, and each `Layout` its viewports and block record.
- Colours: `Color::ByLayer/ByBlock/Index/Rgb`. Colour-book colours are
  already resolved to their RGB.
- Lineweight: `EntityCommon::lineweight_mm100()` (-1 by layer, -2 by block,
  -3 default). Layers: `lineweight_from_raw(layer.lineweight_raw)`.
- Angles are radians (DXF uses degrees for ARC/TEXT/INSERT).
- Text strings are raw AutoCAD strings: MTEXT format codes and `\U+XXXX`
  escapes are still inside.
- Some coordinates in real files are NaN: skip non-finite geometry.
- mmap: if another process truncates the file while we read it, the process
  can crash (normal memory-map trade-off). `read_bytes` avoids this.
