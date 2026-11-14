"""Opt-in test on your own real drawings (never committed).

Run with: CADRENDER_SAMPLES=/path/to/dxf_dir pytest tests/test_real_samples.py
Every .dxf and .dwg file in that folder is rendered. Outputs go to pytest's
tmp dir.
"""

from __future__ import annotations

import os
import threading
import time
from pathlib import Path

import pytest

import cadrender
from conftest import png_info

_ENV = os.environ.get("CADRENDER_SAMPLES", "")
SAMPLES = Path(_ENV) if _ENV else None
FILES = sorted(p.name for p in SAMPLES.iterdir() if p.suffix.lower() in (".dxf", ".dwg")) if SAMPLES and SAMPLES.is_dir() else []

pytestmark = pytest.mark.skipif(not FILES, reason="set CADRENDER_SAMPLES=/path/to/folder to run on real drawings")


@pytest.mark.parametrize("name", FILES)
def test_real_file_all_sheets(name: str, tmp_path: Path) -> None:
    assert SAMPLES is not None
    doc = cadrender.open(SAMPLES / name)
    r = doc.render_all(tmp_path, width=2000)
    assert r["failed"] == {}
    assert r["done"] == doc.sheets
    for s in r["sheets"]:
        assert png_info(Path(s["files"][0]).read_bytes())[0] == 2000


def test_gil_is_released(tmp_path: Path) -> None:
    assert SAMPLES is not None
    # the biggest file gives the other thread the most time to run
    doc = cadrender.open(max((SAMPLES / n for n in FILES), key=lambda p: p.stat().st_size))
    ticks = 0
    running = True

    def count() -> None:
        nonlocal ticks
        while running:
            ticks += 1
            time.sleep(0.001)

    t = threading.Thread(target=count)
    t.start()
    t0 = time.perf_counter()
    doc.render_all(tmp_path, width=2000, threads=2)
    took = time.perf_counter() - t0
    running = False
    t.join()
    if took < 0.05:
        pytest.skip("the biggest file renders too fast to measure the GIL")
    # the other Python thread kept running during the whole render
    assert ticks > took * 100
