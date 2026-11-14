"""Writes the small hand-made DXF files for the Python tests (run once; the
output is committed). Usage: python gen.py <out_dir>

three_sheets.dxf: model space with two lines, and three paper tabs
"01", "02/A" (a slash, to test file names) and "03". Each tab has a border
and one viewport onto model space; tab "03" also has a line on the Defpoints
layer, which the preview preset drops.
"""

import os
import sys


def dxf(groups):
    return "".join(f"{code:>3}\n{val}\n" for code, val in groups)


def sec(name, *items):
    g = [(0, "SECTION"), (2, name)]
    for it in items:
        g += it
    return g + [(0, "ENDSEC")]


def line(x0, y0, x1, y1, layer="0", owner=None, paper=False):
    g = [(0, "LINE")]
    if owner:
        g.append((330, owner))
    g += [(100, "AcDbEntity")] + ([(67, 1)] if paper else []) + [(8, layer)]
    return g + [(100, "AcDbLine"), (10, x0), (20, y0), (30, 0), (11, x1), (21, y1), (31, 0)]


def viewport(handle, owner, status, cx, cy, w, h, vcx, vcy, vh):
    return [(0, "VIEWPORT"), (5, handle), (330, owner), (100, "AcDbEntity"), (67, 1), (8, "Defpoints"), (100, "AcDbViewport"),
            (10, cx), (20, cy), (30, 0), (40, w), (41, h), (68, status), (69, status), (12, vcx), (22, vcy),
            (16, 0), (26, 0), (36, 1), (17, 0), (27, 0), (37, 0), (45, vh), (51, 0)]


def layer(name, handle, color=7):
    return [(0, "LAYER"), (5, handle), (100, "AcDbSymbolTableRecord"), (100, "AcDbLayerTableRecord"),
            (2, name), (70, 0), (62, color), (6, "CONTINUOUS"), (370, -3)]


def brec(name, handle):
    return [(0, "BLOCK_RECORD"), (5, handle), (100, "AcDbSymbolTableRecord"), (100, "AcDbBlockTableRecord"), (2, name)]


def block(name, owner, ents):
    g = [(0, "BLOCK"), (330, owner), (8, "0"), (2, name), (70, 0), (10, 0), (20, 0), (30, 0)]
    for e in ents:
        g += e
    return g + [(0, "ENDBLK"), (8, "0")]


def border(owner):
    pts = [(0, 0), (297, 0), (297, 210), (0, 210), (0, 0)]
    return [line(*a, *b, owner=owner, paper=True) for a, b in zip(pts, pts[1:])]


def layout(handle, name, order, owner):
    return [(0, "LAYOUT"), (5, handle), (330, "1A"), (100, "AcDbPlotSettings"), (1, ""), (100, "AcDbLayout"),
            (1, name), (70, 1), (71, order), (10, 0), (20, 0), (11, 297), (21, 210), (330, owner)]


def three_sheets():
    hdr = sec("HEADER", [(9, "$ACADVER"), (1, "AC1032"), (9, "$LTSCALE"), (40, 1.0), (9, "$PSLTSCALE"), (70, 1)])
    tables = sec("TABLES",
                 [(0, "TABLE"), (2, "LAYER")] + layer("0", "10") + layer("WALLS", "30", 1) + layer("Defpoints", "32")
                 + layer("Defpoints", "33") + [(0, "ENDTAB")],
                 [(0, "TABLE"), (2, "BLOCK_RECORD")] + brec("*Model_Space", "1F") + brec("*Paper_Space", "20")
                 + brec("*Paper_Space0", "21") + brec("*Paper_Space1", "22") + [(0, "ENDTAB")])
    # paper tab 1 lives in the ENTITIES section; tabs 2 and 3 in their blocks
    tab2 = border("21") + [viewport("210", "21", 1, 148.5, 105, 297, 210, 148.5, 105, 210),
                           viewport("211", "21", 2, 150, 100, 200, 100, 50, 25, 100)]
    tab3 = border("22") + [viewport("220", "22", 1, 148.5, 105, 297, 210, 148.5, 105, 210),
                           viewport("221", "22", 2, 150, 100, 200, 100, 0, 0, 50),
                           line(10, 10, 60, 10, layer="Defpoints", owner="22", paper=True)]
    blocks = sec("BLOCKS", block("*Model_Space", "1F", []), block("*Paper_Space", "20", []),
                 block("*Paper_Space0", "21", tab2), block("*Paper_Space1", "22", tab3))
    ents = sec("ENTITIES",
               line(0, 0, 100, 0, layer="WALLS"), line(0, 0, 100, 50, layer="WALLS"),
               *border("20"), viewport("200", "20", 1, 148.5, 105, 297, 210, 148.5, 105, 210),
               viewport("201", "20", 2, 150, 100, 200, 100, 50, 0, 100))
    objs = sec("OBJECTS",
               [(0, "DICTIONARY"), (5, "C"), (330, "0"), (100, "AcDbDictionary"), (3, "ACAD_LAYOUT"), (350, "1A")],
               layout("1B", "Model", 0, "1F"), layout("1C", "01", 1, "20"),
               layout("1D", "02/A", 2, "21"), layout("1E", "03", 3, "22"))
    return dxf(hdr + tables + blocks + ents + objs + [(0, "EOF")])


if __name__ == "__main__":
    with open(os.path.join(sys.argv[1], "three_sheets.dxf"), "w", newline="\n") as f:
        f.write(three_sheets())
