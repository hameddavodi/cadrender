"""Writes crates/cadrender/tests/data/image.dxf: a paper tab with two IMAGEs,
one whose file exists (pics/red_blue.png, 8 x 4 px) and one whose file is missing."""
import sys

out = sys.argv[1]


def image(handle, x, imagedef):
    return [(0, "IMAGE"), (5, handle), (330, "20"), (100, "AcDbEntity"), (67, 1), (8, "0"), (100, "AcDbRasterImage"),
            (90, 0), (10, x), (20, 0), (30, 0), (11, 10), (21, 0), (31, 0), (12, 0), (22, 10), (32, 0),
            (13, 8), (23, 4), (340, imagedef), (70, 7), (280, 0), (281, 50), (282, 50), (283, 0),
            (71, 1), (91, 2), (14, -0.5), (24, -0.5), (14, 7.5), (24, 3.5)]


g = [(0, "SECTION"), (2, "HEADER"), (9, "$ACADVER"), (1, "AC1032"), (0, "ENDSEC"),
     (0, "SECTION"), (2, "TABLES"), (0, "TABLE"), (2, "BLOCK_RECORD"),
     (0, "BLOCK_RECORD"), (5, "1F"), (100, "AcDbSymbolTableRecord"), (100, "AcDbBlockTableRecord"), (2, "*Model_Space"),
     (0, "BLOCK_RECORD"), (5, "20"), (100, "AcDbSymbolTableRecord"), (100, "AcDbBlockTableRecord"), (2, "*Paper_Space"),
     (0, "ENDTAB"), (0, "ENDSEC"),
     (0, "SECTION"), (2, "ENTITIES")]
g += image("100", 0, "300") + image("101", 100, "301")
g += [(0, "ENDSEC"), (0, "SECTION"), (2, "OBJECTS"),
      (0, "DICTIONARY"), (5, "C"), (330, "0"), (100, "AcDbDictionary"), (3, "ACAD_LAYOUT"), (350, "1A"),
      (0, "LAYOUT"), (5, "1C"), (330, "1A"), (100, "AcDbPlotSettings"), (1, ""), (100, "AcDbLayout"), (1, "Sheet"), (70, 1), (71, 1),
      (10, 0), (20, 0), (11, 200), (21, 50), (330, "20"),
      (0, "IMAGEDEF"), (5, "300"), (100, "AcDbRasterImageDef"), (90, 0), (1, ".\\PICS\\red_blue.png"), (10, 8), (20, 4),
      (0, "IMAGEDEF"), (5, "301"), (100, "AcDbRasterImageDef"), (90, 0), (1, "C:\\nowhere\\missing.jpg"), (10, 8), (20, 4),
      (0, "ENDSEC"), (0, "EOF")]
with open(out, "w") as f:
    for c, v in g:
        f.write(f"{c:>3}\n{v}\n")
