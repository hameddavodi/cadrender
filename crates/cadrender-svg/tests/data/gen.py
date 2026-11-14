# Writes svg_golden.dxf, the small drawing behind the SVG golden test
# (run once; the output is committed): python3 gen.py <this folder>
import os
import sys

out = sys.argv[1]


def dxf(*groups):
    return "".join(f"{c:>3}\n{v}\n" for c, v in groups)


def sec(name, *items):
    g = [(0, "SECTION"), (2, name)]
    for it in items:
        g += it
    return g + [(0, "ENDSEC")]


def table(name, *entries):
    g = [(0, "TABLE"), (2, name)]
    for e in entries:
        g += e
    return g + [(0, "ENDTAB")]


def layer(name, handle, color=7, ltype="CONTINUOUS"):
    return [(0, "LAYER"), (5, handle), (100, "AcDbSymbolTableRecord"), (100, "AcDbLayerTableRecord"), (2, name), (70, 0), (62, color), (6, ltype), (370, -3)]


def brec(name, handle):
    return [(0, "BLOCK_RECORD"), (5, handle), (100, "AcDbSymbolTableRecord"), (100, "AcDbBlockTableRecord"), (2, name)]


def block(name, owner):
    return [(0, "BLOCK"), (330, owner), (8, "0"), (2, name), (70, 0), (10, 0), (20, 0), (30, 0), (0, "ENDBLK"), (8, "0")]


def paper(g):
    """Put an entity in paper space."""
    return g[:1] + [(330, "20"), (100, "AcDbEntity"), (67, 1)] + g[1:]


def line(x0, y0, x1, y1, layer="0", extra=()):
    return [(0, "LINE"), (8, layer)] + list(extra) + [(100, "AcDbLine"), (10, x0), (20, y0), (30, 0), (11, x1), (21, y1), (31, 0)]


def text(x, y, h, s, rot=0, wf=1, obl=0):
    return [(0, "TEXT"), (8, "0"), (100, "AcDbText"), (10, x), (20, y), (30, 0), (40, h), (1, s), (50, rot), (41, wf), (51, obl)]


def solid_hatch(pts):
    g = [(0, "HATCH"), (8, "0"), (62, 8), (100, "AcDbHatch"), (10, 0), (20, 0), (30, 0), (210, 0), (220, 0), (230, 1),
         (2, "SOLID"), (70, 1), (71, 0), (91, 1), (92, 2), (72, 0), (73, 1), (93, len(pts))]
    for x, y in pts:
        g += [(10, x), (20, y)]
    return g + [(97, 0), (75, 0), (76, 1), (98, 0)]


hdr = sec("HEADER", [(9, "$ACADVER"), (1, "AC1032"), (9, "$LTSCALE"), (40, 1.0), (9, "$PSLTSCALE"), (70, 1)])
tables = sec("TABLES",
    table("LTYPE", [(0, "LTYPE"), (5, "14"), (2, "DASHED"), (70, 0), (73, 2), (40, 15), (49, 10), (74, 0), (49, -5), (74, 0)]),
    table("LAYER", layer("0", "10"), layer("WALLS", "30", 1), layer("DASH", "31", 3, "DASHED"), layer("Defpoints", "33")),
    table("BLOCK_RECORD", brec("*Model_Space", "1F"), brec("*Paper_Space", "20")))


def viewport(handle, status, cx, cy, w, h, vcx, vcy, vh):
    return [(0, "VIEWPORT"), (5, handle), (330, "20"), (100, "AcDbEntity"), (67, 1), (8, "Defpoints"), (100, "AcDbViewport"),
            (10, cx), (20, cy), (30, 0), (40, w), (41, h), (68, status), (69, status), (12, vcx), (22, vcy),
            (16, 0), (26, 0), (36, 1), (17, 0), (27, 0), (37, 0), (45, vh), (51, 0)]


ents = [
    # model space: seen through the viewport
    line(0, 0, 100, 0, layer="WALLS"), line(0, 0, 0, 50, layer="WALLS"),
    text(10, 20, 5, "Model"),
    viewport("200", 1, 148.5, 105, 297, 210, 148.5, 105, 210),
    viewport("201", 2, 200, 120, 160, 100, 50, 25, 100),
    # paper space: border, a dashed line, texts, a solid fill
    paper(line(0, 0, 297, 0)), paper(line(297, 0, 297, 210)), paper(line(297, 210, 0, 210)), paper(line(0, 210, 0, 0)),
    paper(line(10, 30, 110, 30, layer="DASH")),
    paper(text(10, 190, 8, "A&B <1>")),
    paper(text(10, 150, 6, "Narrow turned", rot=30, wf=0.7, obl=15)),
    paper(solid_hatch([(10, 50), (60, 50), (60, 80), (10, 80)])),
]
objects = sec("OBJECTS",
    [(0, "DICTIONARY"), (5, "C"), (330, "0"), (100, "AcDbDictionary"), (3, "ACAD_LAYOUT"), (350, "1A")],
    [(0, "LAYOUT"), (5, "1B"), (330, "1A"), (100, "AcDbPlotSettings"), (1, ""), (100, "AcDbLayout"), (1, "Model"), (70, 1), (71, 0), (330, "1F")],
    [(0, "LAYOUT"), (5, "1C"), (330, "1A"), (100, "AcDbPlotSettings"), (1, ""), (100, "AcDbLayout"), (1, "Sheet A"), (70, 1), (71, 1),
     (10, 0), (20, 0), (11, 297), (21, 210), (330, "20")])
doc = dxf(*hdr, *tables, *sec("BLOCKS", block("*Model_Space", "1F"), block("*Paper_Space", "20")), *sec("ENTITIES", *ents), *objects, (0, "EOF"))
with open(os.path.join(out, "svg_golden.dxf"), "w") as f:
    f.write(doc)
print("ok")
