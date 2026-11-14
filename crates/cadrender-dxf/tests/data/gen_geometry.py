# Writes the small hand-made DXF files for the geometry tests (tests/geometry.rs).
# Run once: python3 gen_geometry.py <this folder>. The output is committed.
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
    return g + [(0, "ENDSEC")]

def table(name, *entries):
    g = [(0, "TABLE"), (2, name)]
    for e in entries: g += e
    return g + [(0, "ENDTAB")]

def write(name, groups):
    with open(os.path.join(out, name), "w", newline="\n") as f:
        f.write(dxf(*groups, (0, "EOF")))

def hdr(*extra):
    return sec("HEADER", [(9, "$ACADVER"), (1, "AC1032"), (9, "$LTSCALE"), (40, 1.0), (9, "$PSLTSCALE"), (70, 1)] + list(extra))

def ent(kind, handle, layer, body, extra=(), owner=None):
    g = [(0, kind), (5, handle)]
    if owner: g.append((330, owner))
    return g + [(100, "AcDbEntity")] + list(extra) + [(8, layer)] + body

def layer(name, handle, color=7, ltype="CONTINUOUS", extra=()):
    return [(0, "LAYER"), (5, handle), (100, "AcDbSymbolTableRecord"), (100, "AcDbLayerTableRecord"), (2, name), (70, 0), (62, color), (6, ltype), (370, -3)] + list(extra)

def ltype(name, handle, elements, total):
    g = [(0, "LTYPE"), (5, handle), (100, "AcDbSymbolTableRecord"), (100, "AcDbLinetypeTableRecord"), (2, name), (70, 0), (3, ""), (72, 65), (73, len(elements)), (40, total)]
    for e in elements: g += e
    return g

def brec(name, handle):
    return [(0, "BLOCK_RECORD"), (5, handle), (100, "AcDbSymbolTableRecord"), (100, "AcDbBlockTableRecord"), (2, name)]

def block(name, ents, owner=None):
    g = [(0, "BLOCK"), (8, "0"), (2, name), (70, 0), (10, 0), (20, 0), (30, 0)]
    if owner: g.insert(1, (330, owner))
    for e in ents: g += e
    return g + [(0, "ENDBLK"), (8, "0")]

def line(h, x0, y0, x1, y1, layer="0", extra=(), owner=None):
    return ent("LINE", h, layer, [(100, "AcDbLine"), (10, x0), (20, y0), (30, 0), (11, x1), (21, y1), (31, 0)], extra, owner)

def lwpoly(h, pts, flags=0, layer="0", extra=(), owner=None):
    body = [(100, "AcDbPolyline"), (90, len(pts)), (70, flags)]
    for x, y in pts: body += [(10, x), (20, y)]
    return ent("LWPOLYLINE", h, layer, body, extra, owner)

DASHED = ltype("DASHED", "14", [[(49, 10), (74, 0)], [(49, -5), (74, 0)]], 15)
# a text linetype like acad.lin "GAS_LINE": dash, gap with "GAS" in it, gap
GAS = ltype("GAS", "15", [[(49, 0.5), (74, 0)], [(49, -0.2), (74, 2), (75, 0), (340, "11"), (46, 0.1), (50, 0), (44, -0.1), (45, -0.05), (9, "GAS")], [(49, -0.25), (74, 0)]], 0.95)
# the insulation linetype (acad.lin "BATTING") with shape 134 of ltypeshp.shx
BATT = ltype("ISOLAMENTO", "16", [[(49, 0.0001), (74, 0)], [(49, -0.1), (74, 4), (75, 134), (340, "12"), (46, 0.1), (50, 0), (44, -0.1), (45, 0)],
                                   [(49, -0.2), (74, 4), (75, 134), (340, "12"), (46, 0.1), (50, 180), (44, 0.1), (45, 0)], [(49, -0.1), (74, 0)]], 0.4001)
TRANSP = [(1001, "AcCmTransparency"), (1071, 33554508)]  # 0x0200004C: alpha 76
tables = sec("TABLES",
    table("LTYPE", DASHED, GAS, BATT),
    table("LAYER", layer("0", "10"), layer("Defpoints", "17"), layer("GLASS", "18", 5, extra=TRANSP),
          layer("DASHL", "19", 1, "DASHED", extra=[(102, "{ACAD_XDICTIONARY"), (360, "500"), (102, "}")])),
    table("BLOCK_RECORD", brec("*Model_Space", "1F"), brec("*Paper_Space", "20"), brec("ATT", "21")))

# 1. entities: POINT (and one on Defpoints), RAY, XLINE, 3DFACE, MESH, polyface and
#    polygon mesh POLYLINEs, IMAGE, TRACE, ATTDEF in a block and in model space
def vertex(x, y, flags, idx=()):
    return [(0, "VERTEX"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbVertex"), (10, x), (20, y), (30, 0), (70, flags)] + [(71 + i, v) for i, v in enumerate(idx)]
polyface = [(0, "POLYLINE"), (5, "40"), (100, "AcDbEntity"), (8, "0"), (66, 1), (10, 0), (20, 0), (30, 0), (70, 64), (71, 4), (72, 2)]
for x, y in [(0, 0), (10, 0), (10, 10), (0, 10)]: polyface += vertex(x + 100, y, 192)
polyface += vertex(0, 0, 128, (1, 2, -3)) + vertex(0, 0, 128, (-1, 3, 4)) + [(0, "SEQEND"), (8, "0")]
pmesh = [(0, "POLYLINE"), (5, "41"), (100, "AcDbEntity"), (8, "0"), (66, 1), (10, 0), (20, 0), (30, 0), (70, 16), (71, 2), (72, 3)]
for i in range(2):
    for j in range(3): pmesh += vertex(200 + 10 * j, 10 * i, 64)
pmesh += [(0, "SEQEND"), (8, "0")]
mesh = ent("MESH", "42", "0", [(100, "AcDbSubDMesh"), (71, 2), (72, 0), (91, 0), (92, 4),
    (10, 300), (20, 0), (30, 0), (10, 310), (20, 0), (30, 0), (10, 310), (20, 10), (30, 5), (10, 300), (20, 10), (30, 5),
    (93, 8), (90, 3), (90, 0), (90, 1), (90, 2), (90, 3), (90, 0), (90, 2), (90, 3), (94, 0), (95, 0), (90, 0)])
attdef = lambda h, flags, tag, value, x: ent("ATTDEF", h, "0", [(100, "AcDbText"), (10, x), (20, 0), (30, 0), (40, 2.5), (1, value),
    (100, "AcDbAttributeDefinition"), (3, "prompt"), (2, tag), (70, flags), (73, 0)])
write("entities.dxf", hdr((9, "$PDMODE"), (70, 34), (9, "$PDSIZE"), (40, 2.0)) + tables + [
    *sec("BLOCKS", block("ATT", [line("30", 0, 0, 10, 0), attdef("31", 0, "NAME", "default", 0), attdef("32", 2, "CONST", "fixed", 20), attdef("33", 3, "HIDDEN", "x", 40)], "21")),
    *sec("ENTITIES",
        ent("POINT", "50", "0", [(100, "AcDbPoint"), (10, 5), (20, 5), (30, 0)]),
        ent("POINT", "51", "Defpoints", [(100, "AcDbPoint"), (10, 6), (20, 6), (30, 0)]),
        ent("RAY", "52", "0", [(100, "AcDbRay"), (10, 0), (20, 0), (30, 0), (11, 1), (21, 0), (31, 0)]),
        ent("XLINE", "53", "0", [(100, "AcDbXline"), (10, 0), (20, 0), (30, 0), (11, 0), (21, 1), (31, 0)]),
        ent("3DFACE", "54", "0", [(100, "AcDbFace"), (10, 0), (20, 50), (30, 0), (11, 10), (21, 50), (31, 0), (12, 10), (22, 60), (32, 3), (13, 10), (23, 60), (33, 3), (70, 2)]),
        polyface, pmesh, mesh,
        ent("IMAGE", "55", "0", [(100, "AcDbRasterImage"), (90, 0), (10, 400), (20, 0), (30, 0), (11, 0.1), (21, 0), (31, 0), (12, 0), (22, 0.1), (32, 0),
            (13, 200), (23, 100), (340, "600"), (70, 7), (280, 0), (281, 50), (282, 50), (283, 0), (360, "0"), (71, 1), (91, 2), (14, -0.5), (24, -0.5), (14, 199.5), (24, 99.5)]),
        ent("TRACE", "56", "0", [(100, "AcDbTrace"), (10, 500), (20, 0), (30, 0), (11, 510), (21, 0), (31, 0), (12, 500), (22, 5), (32, 0), (13, 510), (23, 5), (33, 0)]),
        attdef("57", 0, "TOPTAG", "value", 600),
        [(0, "INSERT"), (5, "58"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbBlockReference"), (2, "ATT"), (10, 700), (20, 0), (30, 0)]),
])

# 2. linetypes and transparency: a text linetype, the insulation linetype, a
#    dashed polyline with and without "linetype generation", a dashed line in a
#    block inserted at scale 0.001, a line on a transparent layer, a line with
#    its own transparency
write("linetypes.dxf", hdr() + tables + [
    *sec("BLOCKS", block("ATT", [line("30", 0, 0, 100, 0, extra=[(6, "DASHED")])], "21")),
    *sec("ENTITIES",
        line("70", 0, 0, 100, 0, extra=[(6, "GAS")]),
        line("71", 0, 10, 100, 10, extra=[(6, "ISOLAMENTO"), (48, 10)]),
        lwpoly("72", [(0, 20), (50, 20), (50, 21), (100, 21)], 0, "DASHL"),
        lwpoly("73", [(0, 30), (50, 30), (50, 31), (100, 31)], 128, "DASHL"),
        [(0, "INSERT"), (5, "74"), (100, "AcDbEntity"), (8, "0"), (100, "AcDbBlockReference"), (2, "ATT"), (10, 0), (20, 40), (30, 0), (41, 0.001), (42, 0.001), (43, 0.001)],
        line("75", 0, 50, 100, 50, "GLASS"),
        line("76", 0, 60, 100, 60, extra=[(440, 33554559)])),
])

# 3. viewports: a clipped viewport (clip = closed LWPOLYLINE in paper space), a
#    view from below, a twisted view, an edge-on view (not drawn), and layer
#    overrides for one viewport (colour 1 -> 3, lineweight 50)
P = "20"
plan = lambda tw=0: [(16, 0), (26, 0), (36, 1), (17, 0), (27, 0), (37, 0), (45, 100), (51, tw)]
write("viewports.dxf", hdr() + tables + [
    *sec("BLOCKS", block("*Model_Space", [], "1F"), block("*Paper_Space", [], P)),
    *sec("ENTITIES",
        line("80", -50, 0, 50, 0, "DASHL"),
        ent("VIEWPORT", "90", "0", [(100, "AcDbViewport"), (10, 148.5), (20, 105), (30, 0), (40, 297), (41, 210), (68, 1), (69, 1), (12, 148.5), (22, 105), (45, 210)], [(67, 1)], P),
        ent("VIEWPORT", "91", "0", [(100, "AcDbViewport"), (10, 50), (20, 50), (30, 0), (40, 100), (41, 100), (68, 2), (69, 2), (12, 0), (22, 0)] + plan() + [(90, 65536), (340, "A0")], [(67, 1)], P),
        lwpoly("A0", [(0, 0), (100, 0), (100, 50), (50, 50), (50, 100), (0, 100)], 1, owner=P, extra=[(67, 1)]),
        ent("VIEWPORT", "92", "0", [(100, "AcDbViewport"), (10, 200), (20, 50), (30, 0), (40, 100), (41, 100), (68, 3), (69, 3), (12, 0), (22, 0),
            (16, 0), (26, 0), (36, -1), (17, 0), (27, 0), (37, 0), (45, 100), (51, 0)], [(67, 1)], P),
        ent("VIEWPORT", "93", "0", [(100, "AcDbViewport"), (10, 50), (20, 160), (30, 0), (40, 100), (41, 100), (68, 4), (69, 4), (12, 0), (22, 0)] + plan(90), [(67, 1)], P),
        ent("VIEWPORT", "94", "0", [(100, "AcDbViewport"), (10, 200), (20, 160), (30, 0), (40, 100), (41, 100), (68, 5), (69, 5), (12, 0), (22, 0),
            (16, 0), (26, -1), (36, 0), (17, 0), (27, 0), (37, 0), (45, 100), (51, 0)], [(67, 1)], P)),
    *sec("OBJECTS",
        [(0, "DICTIONARY"), (5, "C"), (330, "0"), (100, "AcDbDictionary"), (3, "ACAD_LAYOUT"), (350, "1A")],
        [(0, "LAYOUT"), (5, "1C"), (330, "1A"), (100, "AcDbPlotSettings"), (1, ""), (100, "AcDbLayout"), (1, "Sheet A"), (70, 1), (71, 1), (330, P)],
        [(0, "DICTIONARY"), (5, "500"), (330, "19"), (100, "AcDbDictionary"), (3, "ADSK_XREC_LAYER_COLOR_OVR"), (360, "501"), (3, "ADSK_XREC_LAYER_LINEWT_OVR"), (360, "502")],
        [(0, "XRECORD"), (5, "501"), (330, "500"), (100, "AcDbXrecord"), (280, 1), (102, "{ADSK_LYR_COLOR_OVERRIDE"), (335, "93"), (420, -1023410173), (102, "}")],
        [(0, "XRECORD"), (5, "502"), (330, "500"), (100, "AcDbXrecord"), (280, 1), (102, "{ADSK_LYR_LINEWT_OVERRIDE"), (335, "93"), (91, 50), (102, "}")]),
])
print("ok")
