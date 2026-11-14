# Writes the small hand-made DXF test files (run once; the output is committed).
import sys, os
out = sys.argv[1]

def dxf(*groups):
    lines = []
    for code, val in groups:
        lines.append(f"{code:>3}")
        lines.append(str(val))
    return "\n".join(lines) + "\n"

def sec(name, *items):
    g = [(0, "SECTION"), (2, name)]
    for it in items: g += it
    g += [(0, "ENDSEC")]
    return g

def eof(): return [(0, "EOF")]

def line(x0, y0, x1, y1, layer="0", handle=None, extra=()):
    g = [(0, "LINE")]
    if handle: g.append((5, handle))
    g += [(100, "AcDbEntity"), (8, layer)] + list(extra) + [(100, "AcDbLine"), (10, x0), (20, y0), (30, 0), (11, x1), (21, y1), (31, 0)]
    return g

def table(name, *entries):
    g = [(0, "TABLE"), (2, name)]
    for e in entries: g += e
    return g + [(0, "ENDTAB")]

def layer(name, handle, color=7, flags=0, ltype="CONTINUOUS"):
    return [(0, "LAYER"), (5, handle), (100, "AcDbSymbolTableRecord"), (100, "AcDbLayerTableRecord"), (2, name), (70, flags), (62, color), (6, ltype), (370, -3)]

def brec(name, handle):
    return [(0, "BLOCK_RECORD"), (5, handle), (100, "AcDbSymbolTableRecord"), (100, "AcDbBlockTableRecord"), (2, name)]

def block(name, base, ents, owner=None):
    g = [(0, "BLOCK"), (8, "0"), (2, name), (70, 0), (10, base[0]), (20, base[1]), (30, 0)]
    if owner: g.insert(1, (330, owner))
    for e in ents: g += e
    return g + [(0, "ENDBLK"), (8, "0")]

def insert(name, x, y, sx=1, sy=1, rot=0, layer="0"):
    return [(0, "INSERT"), (100, "AcDbEntity"), (8, layer), (100, "AcDbBlockReference"), (2, name), (10, x), (20, y), (30, 0), (41, sx), (42, sy), (50, rot)]

def write(name, text):
    with open(os.path.join(out, name), "w", newline="\r\n" if name.startswith("crlf") else "\n") as f:
        f.write(text)

hdr = sec("HEADER", [(9, "$ACADVER"), (1, "AC1032"), (9, "$LTSCALE"), (40, 1.0), (9, "$PSLTSCALE"), (70, 1)])

# 1. one line in model space
write("line.dxf", dxf(*hdr, *sec("ENTITIES", line(0, 0, 100, 50)), *eof()))

# 2. block with base point, inserted with scale
write("block_insert.dxf", dxf(*hdr,
    *sec("BLOCKS", block("B", (1, 1), [line(1, 1, 2, 1)])),
    *sec("ENTITIES", insert("B", 10, 10, 2, 2)), *eof()))

# 3. nested inserts with rotation; MINSERT grid
write("nested_insert.dxf", dxf(*hdr,
    *sec("BLOCKS",
        block("INNER", (0, 0), [line(0, 0, 1, 0)]),
        block("OUTER", (0, 0), [insert("INNER", 5, 0), insert("INNER", 10, 0)])),
    *sec("ENTITIES",
        insert("OUTER", 0, 0, 1, 1, 90),
        [(0, "INSERT"), (8, "0"), (2, "INNER"), (10, 100), (20, 0), (30, 0), (70, 3), (71, 2), (44, 10), (45, 20)]),
    *eof()))

# 4. a paper-space layout with a viewport; one model layer frozen in the
#    viewport; a title-block line on the Defpoints layer (the preview preset drops it)
tables = sec("TABLES",
    table("LTYPE", [(0, "LTYPE"), (5, "14"), (2, "DASHED"), (70, 0), (73, 2), (40, 15), (49, 10), (74, 0), (49, -5), (74, 0)]),
    table("LAYER", layer("0", "10"), layer("WALLS", "30", 1), layer("HIDDEN", "31", 3), layer("Defpoints", "33")),
    table("BLOCK_RECORD", brec("*Model_Space", "1F"), brec("*Paper_Space", "20")))
vp_common = lambda handle, status, cx, cy, w, h, vcx, vcy, vh, frozen=(): [(0, "VIEWPORT"), (5, handle), (330, "20"), (100, "AcDbEntity"), (67, 1), (8, "Defpoints"), (100, "AcDbViewport"),
    (10, cx), (20, cy), (30, 0), (40, w), (41, h), (68, status), (69, status), (12, vcx), (22, vcy), (16, 0), (26, 0), (36, 1), (17, 0), (27, 0), (37, 0), (45, vh), (51, 0)] + [(331, f) for f in frozen]
def paper_line(*a, layer="0"):
    g = line(*a, layer=layer)
    return g[:1] + [(330, "20")] + g[1:3] + [(67, 1)] + g[3:]
def layout_obj(status_paper, status_vp):
    return dxf(*hdr, *tables,
        *sec("BLOCKS", block("*Model_Space", (0, 0), [], "1F"), block("*Paper_Space", (0, 0), [], "20")),
        *sec("ENTITIES",
            line(0, 0, 100, 0, layer="WALLS", handle="100"),
            line(0, 10, 100, 10, layer="HIDDEN", handle="101"),
            vp_common("200", status_paper, 148.5, 105, 297, 210, 148.5, 105, 210),
            vp_common("201", status_vp, 150, 100, 200, 100, 50, 0, 100, frozen=("31",)),
            paper_line(0, 0, 297, 0), paper_line(297, 0, 297, 210), paper_line(297, 210, 0, 210), paper_line(0, 210, 0, 0),
            paper_line(10, 10, 60, 10, layer="Defpoints")),
        *sec("OBJECTS",
            [(0, "DICTIONARY"), (5, "C"), (330, "0"), (100, "AcDbDictionary"), (3, "ACAD_LAYOUT"), (350, "1A")],
            [(0, "LAYOUT"), (5, "1B"), (330, "1A"), (100, "AcDbPlotSettings"), (1, ""), (100, "AcDbLayout"), (1, "Model"), (70, 1), (71, 0), (330, "1F")],
            [(0, "LAYOUT"), (5, "1C"), (330, "1A"), (100, "AcDbPlotSettings"), (1, ""), (100, "AcDbLayout"), (1, "Sheet A"), (70, 1), (71, 1), (10, 0), (20, 0), (11, 297), (21, 210), (330, "20")]),
        *eof())
write("viewport.dxf", layout_obj(1, 2))
# LibreDWG writes status 0 for every viewport
write("viewport_libredwg.dxf", layout_obj(0, 0))

# 5. hatches: one solid square, one line pattern (45 degrees, 2 units apart), one with a hole
def hatch(pts_loops, solid, pattern=()):
    g = [(0, "HATCH"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbHatch"), (10, 0), (20, 0), (30, 0), (210, 0), (220, 0), (230, 1),
         (2, "SOLID" if solid else "ANSI31"), (70, 1 if solid else 0), (71, 0), (91, len(pts_loops))]
    for pts in pts_loops:
        g += [(92, 2), (72, 0), (73, 1), (93, len(pts))]
        for x, y in pts: g += [(10, x), (20, y)]
        g += [(97, 0)]
    g += [(75, 0), (76, 1)]
    if not solid:
        g += [(52, 0), (41, 1), (77, 0), (78, len(pattern))]
        for ang, bx, by, ox, oy, dashes in pattern:
            g += [(53, ang), (43, bx), (44, by), (45, ox), (46, oy), (79, len(dashes))] + [(49, d) for d in dashes]
    g += [(98, 0)]
    return g
sq = lambda x, y, s: [(x, y), (x + s, y), (x + s, y + s), (x, y + s)]
write("hatch.dxf", dxf(*hdr, *sec("ENTITIES",
    hatch([sq(0, 0, 10)], True),
    hatch([sq(20, 0, 10)], False, [(45, 0, 0, -1.414214, 1.414214, ())]),
    hatch([sq(40, 0, 10), sq(43, 3, 4)], False, [(0, 0, 0, 0, 1, (0.5, -0.25))])), *eof()))

# 6. LibreDWG quirks: ATTRIB with a junk XRECORD part ("70 1" after it),
#    MTEXT with a garbage box_fill_scale and a page-colour background
write("libredwg_quirks.dxf", dxf(*hdr,
    *sec("BLOCKS", block("TAG", (0, 0), [line(0, 0, 10, 0)])),
    *sec("ENTITIES",
        [(0, "INSERT"), (5, "50"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbBlockReference"), (66, 1), (2, "TAG"), (10, 0), (20, 0), (30, 0)],
        [(0, "ATTRIB"), (5, "51"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbText"), (10, 1), (20, 1), (30, 0), (40, 2.5), (1, "ABC"),
         (100, "AcDbAttribute"), (2, "TAG1"), (70, 0), (280, 0), (100, "AcDbXrecord"), (280, 0), (70, 1)],
        [(0, "ATTRIB"), (5, "52"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbText"), (10, 1), (20, 5), (30, 0), (40, 2.5), (1, "HIDDEN"),
         (100, "AcDbAttribute"), (2, "TAG2"), (70, 1)],
        [(0, "SEQEND"), (5, "53"), (100, "AcDbEntity"), (8, "0")],
        [(0, "MTEXT"), (5, "60"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbMText"), (10, 50), (20, 50), (30, 0), (40, 2.0), (41, 0), (71, 1), (1, "Mask"),
         (90, 3), (63, 256), (45, "1.2e+308"), (441, 0)]),
    *eof()))
# 7. ODA MTEXT: the embedded object (101) repeats 10/11; 11 there is not the direction
write("oda_mtext.dxf", dxf(*hdr, *sec("ENTITIES",
    [(0, "MTEXT"), (5, "70"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbMText"), (10, 0), (20, 0), (30, 0), (40, 2.0), (41, 0), (71, 1), (1, "Legend"), (73, 1), (44, 1.0),
     (101, "Embedded Object"), (70, 1), (10, 1.0), (20, 0.0), (30, 0.0), (11, 30.0), (21, 18.0), (31, 0.0), (40, 0.0), (41, 0.0)]), *eof()))
print("ok")
