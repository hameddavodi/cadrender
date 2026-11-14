#!/usr/bin/env python3
"""Write the tiny hand-made SHX test files in ../tests/data/.

Every byte is written here by hand, from the public SHX format description.
No real AutoCAD font is copied. Run from anywhere:

    python3 -I tools/make_test_shx.py
"""
import os
import struct

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "tests", "data")


def shapes_file(records, eof=True):
    """records: dict number -> bytes (name + 0 + code). Shapes 1.0 layout."""
    nums = sorted(records)
    b = bytearray(b"AutoCAD-86 shapes 1.0\r\n\x1a")
    b += struct.pack("<HHH", nums[0], nums[-1], len(nums))
    for n in nums:
        b += struct.pack("<HH", n, len(records[n]))
    for n in nums:
        b += records[n]
    if eof:
        b += b"EOF"
    return bytes(b)


def unifont_file(info, records):
    b = bytearray(b"AutoCAD-86 unifont 1.0\r\n\x1a")
    b += struct.pack("<IH", len(records) + 1, len(info))
    b += info
    for n in sorted(records):
        b += struct.pack("<HH", n, len(records[n]))
        b += records[n]
    return bytes(b)


def bigfont_file(records, ranges):
    nums = sorted(records)
    head = bytearray(b"AutoCAD-86 bigfont 1.0\r\n\x1a")
    index_size = 8 * len(nums)
    head += struct.pack("<HHH", index_size, len(nums), len(ranges))
    for a, z in ranges:
        head += struct.pack("<HH", a, z)
    offset = len(head) + index_size
    index = bytearray()
    blob = bytearray()
    for n in nums:
        index += struct.pack("<HHI", n, len(records[n]), offset + len(blob))
        blob += records[n]
    return bytes(head + index + blob)


def rec(code, name=b""):
    return name + b"\x00" + bytes(code)


def main():
    os.makedirs(OUT, exist_ok=True)

    # 1. A small normal text font. above 10, below 3, modes 2 (dual orientation).
    font = {
        0: b"cadrender test font\x00" + bytes([10, 3, 2, 0]),
        # 'A' (65): vector bytes. Up-right twice (len 5, dir 2), down-right twice
        # (dir E), then a crossbar (len 10, dir 0) and a move to advance 20.
        65: rec([0x52, 0x52, 0x5E, 0x5E, 2, 8, 0xF1, 5, 1, 0xA0, 2, 8, 5, 0xFB, 0]),
        # 'H' (72): 8 / 9 displacements, ends at advance 10.
        72: rec([1, 8, 0, 10, 2, 8, 6, 0, 1, 8, 0, 246, 2, 8, 250, 5, 1, 9, 6, 0, 0, 0, 2, 8, 4, 251, 0]),
        # '?' (63): a square with 9, then move to advance 8.
        63: rec([2, 8, 1, 1, 1, 9, 5, 0, 0, 5, 251, 0, 0, 251, 0, 0, 2, 8, 7, 255, 0]),
        # space (32): pen up, advance 6
        32: rec([2, 8, 6, 0, 0]),
        # 'B' (66): vertical-only code: 14 skips the next command in horizontal text.
        66: rec([1, 8, 0, 10, 14, 8, 0, 5, 2, 8, 4, 246, 0]),
        # 'C' (67): subshape 'H' twice (code 7)
        67: rec([2, 7, 72, 7, 72, 0]),
        # 'D' (68): scale (3 / 4) around a vector, then push/pop
        68: rec([4, 2, 0x10, 3, 2, 5, 0x14, 6, 0x10, 0]),
        # '0' (48): full circle, octant arc radius 5, start octant 0
        48: rec([2, 8, 10, 5, 1, 10, 5, 0x00, 2, 8, 2, 251, 0]),
        # '1' (49): clockwise quarter arc: radius 4, start octant 2 (90 deg), 2 octants, CW
        49: rec([1, 10, 4, 0xA2, 0]),
        # '2' (50): fractional arc 55..95 deg radius 3 (spec example 11,(56,28,0,3,012))
        50: rec([1, 11, 56, 28, 0, 3, 0x12, 0]),
        # '3' (51): bulge arc: semicircle to (10,0), bulge 127
        51: rec([1, 12, 10, 0, 127, 0]),
        # '4' (52): polyarc: (0,5,127),(0,5,-127),(0,0)  (spec example)
        52: rec([1, 13, 0, 5, 127, 0, 5, 129, 0, 0, 0]),
        # 233 'é' in Latin-1: subshape 'H' + accent
        233: rec([2, 8, 3, 12, 1, 8, 2, 2, 2, 8, 251, 242, 7, 72, 0]),
        # 127: old AutoCAD degree number
        127: rec([2, 8, 2, 8, 1, 10, 2, 0x00, 2, 8, 2, 248, 0]),
    }
    open(os.path.join(OUT, "test_font.shx"), "wb").write(shapes_file(font))

    # 2. A shape file (no shape 0) with named shapes, like a linetype shape file.
    shapes = {
        132: rec([1, 9, 2, 2, 2, 254, 0, 0, 0], b"ZIG"),
        133: rec([1, 10, 1, 0x00, 0], b"CIRC1"),
    }
    open(os.path.join(OUT, "test_shapes.shx"), "wb").write(shapes_file(shapes))

    # 3. A unifont: above 8, below 2, Unicode numbers, 2-byte subshape.
    uinfo = b"cadrender test unifont\x00" + bytes([8, 2, 0, 0, 0, 0])
    uni = {
        0x0041: rec([1, 8, 4, 8, 8, 4, 248, 2, 8, 2, 0, 0]),       # 'A'
        0x4E00: rec([2, 8, 0, 4, 1, 8, 10, 0, 2, 8, 2, 252, 0]),   # one stroke
        0x00C5: rec([2, 8, 4, 10, 1, 10, 1, 0x00, 2, 8, 252, 246, 7, 0x00, 0x41, 0]),  # A + ring, subshape 0x0041
        0x2205: rec([1, 9, 8, 8, 0, 0, 2, 8, 0, 248, 0]),
    }
    open(os.path.join(OUT, "test_unifont.shx"), "wb").write(unifont_file(uinfo, uni))

    # 4. An extended big font: escape range 0x81..0x9F, height 16, width 16.
    big = {
        0: b"cadrender test bigfont\x00" + bytes([16, 0, 0, 16, 0]),
        # primitive: a horizontal line across the box (16 units)
        0x8001: rec([1, 8, 16, 0, 0]),
        # primitive: a vertical line
        0x8002: rec([1, 8, 0, 16, 0]),
        # character 0x8140: two primitives placed in half boxes
        0x8140: rec([7, 0, 0x80, 0x01, 0, 8, 16, 8, 7, 0, 0x80, 0x02, 8, 0, 8, 16, 0]),
    }
    open(os.path.join(OUT, "test_bigfont.shx"), "wb").write(bigfont_file(big, [(0x81, 0x9F)]))

    # 5. A broken file: the test font cut in the middle of its data.
    data = shapes_file(font)
    open(os.path.join(OUT, "broken_truncated.shx"), "wb").write(data[: len(data) * 2 // 3])


if __name__ == "__main__":
    main()
