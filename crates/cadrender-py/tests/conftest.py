"""Shared test helpers. Tests run against the installed wheel (see README)."""

from __future__ import annotations

import struct
import zlib
from pathlib import Path

import pytest

import cadrender

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
#: Every hand-made DXF in the repo (crates/*/tests/data/*.dxf).
HAND_MADE = sorted(REPO.glob("crates/*/tests/data/*.dxf"))
THREE = HERE / "data" / "three_sheets.dxf"
VIEWPORT = REPO / "crates" / "cadrender-dxf" / "tests" / "data" / "viewport.dxf"


@pytest.fixture(scope="session")
def three() -> cadrender.Document:
    """three_sheets.dxf: tabs "01", "02/A", "03"."""
    return cadrender.open(THREE)


def png_info(data: bytes) -> tuple[int, int, int, bytes]:
    """(width, height, colour type, first pixel bytes) of a PNG.

    The first pixel of the first row is stored as is with every PNG filter
    (its left and upper neighbours count as zero), so no full decoder is needed.
    """
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    w, h, depth, ctype = struct.unpack(">IIBB", data[16:26])
    assert depth == 8
    idat, pos = b"", 8
    while pos < len(data):
        n, kind = struct.unpack(">I4s", data[pos:pos + 8])
        if kind == b"IDAT":
            idat += data[pos + 8:pos + 8 + n]
        pos += 12 + n
    raw = zlib.decompress(idat)
    channels = {0: 1, 2: 3, 4: 2, 6: 4}[ctype]
    return w, h, ctype, raw[1:1 + channels]
