# Writes svg_rich.dxf: formatted MTEXT and gradient hatches for the SVG tests
# (run once; the output is committed): python3 gen_rich.py <this folder>
import os
import sys

out = sys.argv[1]


def dxf(groups):
    return "".join(f"{c:>3}\n{v}\n" for c, v in groups)


def brec(name, handle):
    return [(0, "BLOCK_RECORD"), (5, handle), (100, "AcDbSymbolTableRecord"), (100, "AcDbBlockTableRecord"), (2, name)]


def block(name, owner):
    return [(0, "BLOCK"), (330, owner), (8, "0"), (2, name), (70, 0), (10, 0), (20, 0), (30, 0), (0, "ENDBLK"), (8, "0")]


def doc(*ents):
    """One paper tab, "Sheet R", with all entities in paper space."""
    g = [(0, "SECTION"), (2, "HEADER"), (9, "$ACADVER"), (1, "AC1032"), (0, "ENDSEC")]
    g += [(0, "SECTION"), (2, "TABLES"), (0, "TABLE"), (2, "BLOCK_RECORD")] + brec("*Model_Space", "1F") + brec("*Paper_Space", "20")
    g += [(0, "ENDTAB"), (0, "ENDSEC"), (0, "SECTION"), (2, "BLOCKS")] + block("*Model_Space", "1F") + block("*Paper_Space", "20")
    g += [(0, "ENDSEC"), (0, "SECTION"), (2, "ENTITIES")]
    for e in ents:
        g += e
    g += [(0, "ENDSEC"), (0, "SECTION"), (2, "OBJECTS"),
          (0, "DICTIONARY"), (5, "C"), (330, "0"), (100, "AcDbDictionary"), (3, "ACAD_LAYOUT"), (350, "1A"),
          (0, "LAYOUT"), (5, "1B"), (330, "1A"), (100, "AcDbPlotSettings"), (1, ""), (100, "AcDbLayout"), (1, "Model"), (70, 1), (71, 0), (330, "1F"),
          (0, "LAYOUT"), (5, "1C"), (330, "1A"), (100, "AcDbPlotSettings"), (1, ""), (100, "AcDbLayout"), (1, "Sheet R"), (70, 1), (71, 1),
          (10, 0), (20, 0), (11, 297), (21, 210), (330, "20")]
    return g + [(0, "ENDSEC"), (0, "EOF")]


def mtext(x, y, h, s, width=0, attach=1, color=None):
    g = [(0, "MTEXT"), (330, "20"), (100, "AcDbEntity"), (67, 1), (8, "0")]
    if color is not None:
        g += [(62, color)]
    return g + [(100, "AcDbMText"), (10, x), (20, y), (30, 0), (40, h), (41, width), (71, attach), (1, s)]


def gradient_hatch(x, y, s, name, angle=0, shift=0, c1=0x0000FF, c2=0xFFFF00):
    g = [(0, "HATCH"), (330, "20"), (100, "AcDbEntity"), (67, 1), (8, "0"), (100, "AcDbHatch"), (10, 0), (20, 0), (30, 0),
         (210, 0), (220, 0), (230, 1), (2, "SOLID"), (70, 1), (71, 0), (91, 1), (92, 3), (72, 0), (73, 1), (93, 4)]
    for px, py in [(x, y), (x + s, y), (x + s, y + s * 0.6), (x, y + s * 0.6)]:
        g += [(10, px), (20, py)]
    g += [(97, 0), (75, 0), (76, 1), (98, 1), (10, 0), (20, 0)]
    g += [(450, 1), (451, 0), (452, 0), (453, 2), (460, angle), (461, shift), (462, 0)]
    g += [(463, 0), (63, 5), (421, c1), (463, 1), (63, 2), (421, c2), (470, name)]
    return g


ents = [
    # colour, bold, italic, underline, height, width factor, oblique, stacked fractions
    mtext(0, 100, 5, "Plain {\\C1;red} {\\fArial|b1;bold} {\\fArial|i1;ital}\\P\\Lunder\\l \\Oover\\o \\H2x;big\\H1x; \\W0.5;narrow\\W1; \\Q20;slant"),
    mtext(0, 70, 5, "Size \\S1/2; and \\S3#4; and \\S+0.1^-0.2; mm", color=3),
    mtext(0, 50, 4, "A & B <tag> {\\c16711680;blue}"),
    # gradients: the AutoCAD names, angle (radians in DXF) and shift
    gradient_hatch(150, 100, 40, "LINEAR"),
    gradient_hatch(200, 100, 40, "CYLINDER", angle=0.785398),
    gradient_hatch(250, 100, 40, "INVCYLINDER"),
    gradient_hatch(150, 60, 40, "SPHERICAL", shift=1),
    gradient_hatch(200, 60, 40, "HEMISPHERICAL"),
    gradient_hatch(250, 60, 40, "CURVED"),
    gradient_hatch(150, 20, 40, "INVSPHERICAL"),
    gradient_hatch(200, 20, 40, "INVHEMISPHERICAL"),
    gradient_hatch(250, 20, 40, "INVCURVED", angle=1.570796),
]

with open(os.path.join(out, "svg_rich.dxf"), "w", newline="\n") as f:
    f.write(dxf(doc(*ents)))
print("ok")
