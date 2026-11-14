#!/usr/bin/env python3
"""Convert a Hershey font (.jhf) into a compiled AutoCAD-style SHX shape font.

This is our own code (no third-party code). It writes the built-in fallback
fonts in ../fonts/. The input data is the public Hershey font data; see
../fonts/HERSHEY-NOTICE.txt for the acknowledgement that must ship with it.

Usage:
    python3 -I hershey_to_shx.py rowmans.jhf "Hershey Roman Simplex" out.shx

What it writes:
- "AutoCAD-86 shapes 1.0" file, shape numbers = Unicode code points (Latin-1),
  above = 21, below = 7 (Hershey cap height is 21 units).
- ASCII 32..126 from the .jhf file.
- Latin-1 letters with accents, built from the base letter (code 7 subshape)
  plus a hand-drawn accent.
- Symbols for the AutoCAD special codes: degree (0xB0), plus-minus (0xB1),
  diameter (shape 129, also 0x2205 and 0x2300), with legacy numbers 127 and 128.
- Superscripts 1, 2, 3 (scale codes 3 and 4 around a subshape).
"""
import struct
import sys

ABOVE = 21
BELOW = 7
XHEIGHT = 14


def read_jhf(path):
    """Return list of (left, right, strokes) in file order. y is up, baseline 0."""
    text = open(path, "rb").read().decode("ascii")
    text = text.replace("\r", "")
    glyphs = []
    i = 0
    n = len(text)
    while i < n:
        if text[i] == "\n":
            i += 1
            continue
        count = int(text[i + 5 : i + 8])
        i += 8
        chars = []
        while len(chars) < 2 * count and i < n:
            if text[i] != "\n":
                chars.append(text[i])
            i += 1
        left = ord(chars[0]) - ord("R")
        right = ord(chars[1]) - ord("R")
        strokes = []
        cur = []
        for k in range(2, len(chars), 2):
            if chars[k] == " " and chars[k + 1] == "R":
                if cur:
                    strokes.append(cur)
                cur = []
                continue
            x = ord(chars[k]) - ord("R") - left
            y = 9 - (ord(chars[k + 1]) - ord("R"))
            cur.append((x, y))
        if cur:
            strokes.append(cur)
        glyphs.append((0, right - left, strokes))
    return glyphs


class Code:
    """Builds the byte code of one shape. Pen starts down (SHX rule)."""

    def __init__(self):
        self.b = bytearray()
        self.pos = (0, 0)

    def move(self, x, y, draw):
        dx, dy = x - self.pos[0], y - self.pos[1]
        while dx or dy:
            sx = max(-127, min(127, dx))
            sy = max(-127, min(127, dy))
            self.b += bytes([8, sx & 0xFF, sy & 0xFF])
            dx -= sx
            dy -= sy
        self.pos = (x, y)

    def stroke(self, pts):
        pts = [p for k, p in enumerate(pts) if k == 0 or p != pts[k - 1]]
        if len(pts) < 2:
            return
        self.b.append(2)
        self.move(pts[0][0], pts[0][1], False)
        self.b.append(1)
        self.b.append(9)
        for x, y in pts[1:]:
            dx, dy = x - self.pos[0], y - self.pos[1]
            assert -127 <= dx <= 127 and -127 <= dy <= 127
            self.b += bytes([dx & 0xFF, dy & 0xFF])
            self.pos = (x, y)
        self.b += b"\x00\x00"

    def circle(self, cx, cy, r):
        """Full circle with an octant arc (code 10), start at 0 degrees."""
        self.b.append(2)
        self.move(cx + r, cy, False)
        self.b.append(1)
        self.b += bytes([10, r, 0x00])  # start octant 0, 0 = 8 octants
        self.pos = (cx + r, cy)

    def sub(self, number):
        assert number < 256
        self.b += bytes([7, number])

    def finish(self, advance):
        self.b.append(2)
        self.move(advance, 0, False)
        self.b.append(0)
        return bytes(self.b)


def accent_strokes(kind, cx, upper):
    lift = 7 if upper else 0
    a = []
    if kind == "grave":
        a.append([(cx - 2, 19), (cx + 1, 16)])
    elif kind == "acute":
        a.append([(cx + 2, 19), (cx - 1, 16)])
    elif kind == "circ":
        a.append([(cx - 3, 16), (cx, 19), (cx + 3, 16)])
    elif kind == "tilde":
        a.append([(cx - 4, 16), (cx - 2, 18), (cx + 2, 16), (cx + 4, 18)])
    elif kind == "diaer":
        for dx in (-3, 3):
            a.append([(cx + dx, 16), (cx + dx - 1, 17), (cx + dx, 18), (cx + dx + 1, 17), (cx + dx, 16)])
    elif kind == "cedil":
        return [[(cx, 0), (cx, -2), (cx + 2, -3), (cx + 2, -5), (cx - 1, -6)]]
    elif kind == "slash":
        return [[(cx - 8, -1), (cx + 8, 22 if upper else 15)]]
    return [[(x, y + lift) for x, y in s] for s in a]


COMPOSE = {
    # code: (base, accent)
    0xC0: ("A", "grave"), 0xC1: ("A", "acute"), 0xC2: ("A", "circ"), 0xC3: ("A", "tilde"),
    0xC4: ("A", "diaer"), 0xC5: ("A", "ring"), 0xC7: ("C", "cedil"),
    0xC8: ("E", "grave"), 0xC9: ("E", "acute"), 0xCA: ("E", "circ"), 0xCB: ("E", "diaer"),
    0xCC: ("I", "grave"), 0xCD: ("I", "acute"), 0xCE: ("I", "circ"), 0xCF: ("I", "diaer"),
    0xD1: ("N", "tilde"),
    0xD2: ("O", "grave"), 0xD3: ("O", "acute"), 0xD4: ("O", "circ"), 0xD5: ("O", "tilde"),
    0xD6: ("O", "diaer"), 0xD8: ("O", "slash"),
    0xD9: ("U", "grave"), 0xDA: ("U", "acute"), 0xDB: ("U", "circ"), 0xDC: ("U", "diaer"),
    0xDD: ("Y", "acute"),
    0xE0: ("a", "grave"), 0xE1: ("a", "acute"), 0xE2: ("a", "circ"), 0xE3: ("a", "tilde"),
    0xE4: ("a", "diaer"), 0xE5: ("a", "ring"), 0xE7: ("c", "cedil"),
    0xE8: ("e", "grave"), 0xE9: ("e", "acute"), 0xEA: ("e", "circ"), 0xEB: ("e", "diaer"),
    0xEC: ("ı", "grave"), 0xED: ("ı", "acute"), 0xEE: ("ı", "circ"), 0xEF: ("ı", "diaer"),
    0xF1: ("n", "tilde"),
    0xF2: ("o", "grave"), 0xF3: ("o", "acute"), 0xF4: ("o", "circ"), 0xF5: ("o", "tilde"),
    0xF6: ("o", "diaer"), 0xF8: ("o", "slash"),
    0xF9: ("u", "grave"), 0xFA: ("u", "acute"), 0xFB: ("u", "circ"), 0xFC: ("u", "diaer"),
    0xFD: ("y", "acute"), 0xFF: ("y", "diaer"),
}


def build(jhf, name):
    glyphs = read_jhf(jhf)
    shapes = {}
    adv = {}
    strokes_of = {}
    for k, (_, right, strokes) in enumerate(glyphs[:95]):
        code = 32 + k
        c = Code()
        for s in strokes:
            c.stroke(s)
        shapes[code] = c.finish(right)
        adv[code] = right
        strokes_of[code] = strokes
    # The Hershey set draws '^' as an up arrow and '`' as a curly quote.
    # Draw a plain caret and a grave stroke instead.
    for code, strokes, w in ((ord("^"), [[(3, 15), (8, 20), (13, 15)]], 16),
                             (ord("`"), [[(3, 21), (6, 17)]], 10)):
        c = Code()
        for s in strokes:
            c.stroke(s)
        shapes[code] = c.finish(w)
        adv[code] = w
        strokes_of[code] = strokes
    # dotless i (U+0131): the 'i' without strokes above the x-height
    i_strokes = [s for s in strokes_of[ord("i")] if min(y for _, y in s) <= XHEIGHT]
    c = Code()
    for s in i_strokes:
        c.stroke(s)
    shapes[0x131] = c.finish(adv[ord("i")])
    adv[0x131] = adv[ord("i")]

    # accented letters: accent first, then the base letter as a subshape
    for code, (base, kind) in COMPOSE.items():
        b = ord(base)
        upper = base.isupper()
        w = adv[b]
        cx = w // 2
        c = Code()
        if kind == "ring":
            c.circle(cx, 25 if upper else 18, 2)
        else:
            for s in accent_strokes(kind, cx, upper):
                c.stroke(s)
        c.b.append(2)
        c.move(0, 0, False)
        if b < 256:
            c.sub(b)
            c.pos = (w, 0)
        else:  # dotless i is above 255: copy its strokes instead of a subshape
            for s in i_strokes:
                c.stroke(s)
        shapes[code] = c.finish(w)

    # degree sign
    c = Code()
    c.circle(5, 18, 3)
    shapes[0xB0] = c.finish(10)
    # plus-minus: the '+' lifted by 3 units, a bar under it
    plus = strokes_of[ord("+")]
    c = Code()
    pmin = min(x for s in plus for x, _ in s)
    pmax = max(x for s in plus for x, _ in s)
    for s in plus:
        c.stroke([(x, y + 3) for x, y in s])
    c.stroke([(pmin, 0), (pmax, 0)])
    shapes[0xB1] = c.finish(adv[ord("+")])
    # diameter: circle with a slash
    c = Code()
    c.circle(10, 8, 7)
    c.stroke([(2, -1), (18, 17)])
    shapes[129] = c.finish(20)
    # legacy AutoCAD numbers and Unicode aliases
    for code, target in ((127, 0xB0), (128, 0xB1), (0x2205, 129), (0x2300, 129)):
        c = Code()
        c.b.append(2)
        c.sub(target)
        shapes[code] = bytes(c.b) + b"\x00"
    # superscripts: half size, lifted
    for code, digit in ((0xB9, "1"), (0xB2, "2"), (0xB3, "3")):
        d = ord(digit)
        c = Code()
        c.b += bytes([2, 8, 0, 12, 3, 2, 7, d, 4, 2])
        c.pos = (adv[d] // 2, 12)
        shapes[code] = c.finish(adv[d] // 2 + 1)
    # middle dot
    c = Code()
    c.stroke([(4, 7), (3, 8), (4, 9), (5, 8), (4, 7)])
    shapes[0xB7] = c.finish(8)

    info = name.encode("ascii") + b"\x00" + bytes([ABOVE, BELOW, 0, 0])
    records = {0: info}
    for code, data in shapes.items():
        records[code] = b"\x00" + data  # empty shape name
    return records


def write_shapes_file(records, out):
    nums = sorted(records)
    blob = bytearray(b"AutoCAD-86 shapes 1.0\r\n\x1a")
    blob += struct.pack("<HHH", nums[0], nums[-1], len(nums))
    for n in nums:
        blob += struct.pack("<HH", n, len(records[n]))
    for n in nums:
        blob += records[n]
    blob += b"EOF"
    open(out, "wb").write(blob)


def main():
    jhf, name, out = sys.argv[1:4]
    write_shapes_file(build(jhf, name), out)


if __name__ == "__main__":
    main()
