# Writes the small hand-made HATCH test files (run once; the output is committed):
#   python3 gen_hatch.py <this folder>
import os
import sys

out = sys.argv[1]


def dxf(groups):
    return "".join(f"{c:>3}\n{v}\n" for c, v in groups)


def doc(*ents):
    g = [(0, "SECTION"), (2, "HEADER"), (9, "$ACADVER"), (1, "AC1032"), (0, "ENDSEC"), (0, "SECTION"), (2, "ENTITIES")]
    for e in ents:
        g += e
    return g + [(0, "ENDSEC"), (0, "EOF")]


def head(name="SOLID", solid=True, extrusion_z=1.0, layer="0"):
    return [(0, "HATCH"), (100, "AcDbEntity"), (8, layer), (100, "AcDbHatch"), (10, 0), (20, 0), (30, 0),
            (210, 0), (220, 0), (230, extrusion_z), (2, name), (70, 1 if solid else 0), (71, 0)]


def poly_loop(pts, flags=1, bulges=None):
    g = [(92, flags | 2), (72, 1 if bulges else 0), (73, 1), (93, len(pts))]
    for k, (x, y) in enumerate(pts):
        g += [(10, x), (20, y)]
        if bulges:
            g += [(42, bulges[k])]
    return g + [(97, 0)]


def edge_loop(edges, flags=1, sources=0):
    g = [(92, flags), (93, len(edges))]
    for e in edges:
        g += e
    g += [(97, sources)] + [(330, "1F")] * sources
    return g


def line(x0, y0, x1, y1):
    return [(72, 1), (10, x0), (20, y0), (11, x1), (21, y1)]


def arc(cx, cy, r, a0, a1, ccw=1):
    return [(72, 2), (10, cx), (20, cy), (40, r), (50, a0), (51, a1), (73, ccw)]


def ellipse(cx, cy, mx, my, ratio, a0, a1, ccw=1):
    return [(72, 3), (10, cx), (20, cy), (11, mx), (21, my), (40, ratio), (50, a0), (51, a1), (73, ccw)]


def spline(deg, knots, ctrl, weights=None, fit=()):
    g = [(72, 4), (94, deg), (73, 1 if weights else 0), (74, 0), (95, len(knots)), (96, len(ctrl))]
    g += [(40, k) for k in knots]
    for k, (x, y) in enumerate(ctrl):
        g += [(10, x), (20, y)]
        if weights:
            g += [(42, weights[k])]
    g += [(97, len(fit))]
    for x, y in fit:
        g += [(11, x), (21, y)]
    return g


def tail(style=0, pattern=(), double=0, gradient=None):
    g = [(75, style), (76, 1)]
    if pattern:
        g += [(52, 0), (41, 1), (77, double), (78, len(pattern))]
        for ang, bx, by, ox, oy, dashes in pattern:
            g += [(53, ang), (43, bx), (44, by), (45, ox), (46, oy), (79, len(dashes))] + [(49, d) for d in dashes]
    g += [(98, 1), (10, 0), (20, 0)]
    if gradient:
        name, c1, c2, angle, shift = gradient
        g += [(450, 1), (451, 0), (452, 0), (453, 2), (460, angle), (461, shift), (462, 0)]
        g += [(463, 0), (63, 5), (421, c1), (463, 1), (63, 2), (421, c2), (470, name)]
    return g


def sq(x, y, s):
    return [(x, y), (x + s, y), (x + s, y + s), (x, y + s)]


def write(name, ents):
    with open(os.path.join(out, name), "w", newline="\n") as f:
        f.write(dxf(doc(*ents)))


# 1. islands: the same 4 nested squares (listed inside-out) with each hatch style
nest = [sq(30, 30, 40), sq(20, 20, 60), sq(10, 10, 80), sq(0, 0, 100)]
write("hatch_islands.dxf", [
    head() + [(91, 4)] + sum((poly_loop(p) for p in nest), []) + tail(style)
    for style in (0, 1, 2)
])

# 2. one loop of every edge type: line, clockwise arc, ellipse, rational spline
#    (a quarter circle with fit-point count 0), and a line stored backwards.
#    Then a polyline loop with a bulge (half circle).
w = 0.7071067811865476
edges = [
    line(0, 0, 10, 0),
    ellipse(10, 2, 2, 0, 0.5, 270, 360),         # (10,1) .. (12,2): real angles; gap (10,0)-(10,1)
    line(12, 2, 12, 8),
    spline(2, [0, 0, 0, 1, 1, 1], [(12, 8), (12, 10), (10, 10)], [1, w, 1]),  # quarter circle around (10,8)
    arc(5, 10, 5, 0, 180, 0),                     # clockwise (10,10) -> (5,5) -> (0,10)
    line(0, 0, 0, 10),                            # stored backwards
]
write("hatch_edges.dxf", [
    head() + [(91, 1)] + edge_loop(edges, sources=1) + tail(),
    head() + [(91, 1)] + poly_loop([(20, 0), (30, 0)], bulges=[1, 0]) + tail(),
])

# 3. a spherical gradient, red to yellow
write("hatch_gradient.dxf", [head() + [(91, 1)] + poly_loop(sq(0, 0, 100)) + tail(gradient=("SPHERICAL", 0xFF0000, 0xFFFF00, 0, 0))])

# 4. a dense pattern (lines 0.1 apart) and a dot pattern; one hatch mirrored (extrusion -Z)
write("hatch_dense.dxf", [
    head("DENSE", False) + [(91, 1)] + poly_loop(sq(0, 0, 100)) + tail(pattern=[(0, 0, 0, 0, 0.1, ())]),
    head("DOTS", False) + [(91, 1)] + poly_loop(sq(200, 0, 100)) + tail(pattern=[(0, 0, 0, 0, 10, (0, -10))]),
    head("ANSI31", False, extrusion_z=-1) + [(91, 1)] + poly_loop(sq(-500, 0, 100)) + tail(pattern=[(45, 0, 0, -7.0710678, 7.0710678, ())]),
    head("_USER", False) + [(91, 1)] + poly_loop(sq(400, 0, 100)) + tail(pattern=[(0, 0, 0, 0, 10, ())], double=1),
])
