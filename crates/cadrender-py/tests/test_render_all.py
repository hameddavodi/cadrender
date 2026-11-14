"""Document.render_all: parallel, per-sheet results, errors, callbacks."""

from __future__ import annotations

import threading
from pathlib import Path

import pytest

import cadrender
from conftest import png_info

NAMES = {"01": "three_sheets__01.png", "02/A": "three_sheets__02_A.png", "03": "three_sheets__03.png"}


def test_all_sheets(three: cadrender.Document, tmp_path: Path) -> None:
    r = three.render_all(tmp_path, width=400)
    assert r["done"] == ["01", "02/A", "03"]
    assert r["failed"] == {} and r["skipped"] == []
    assert sorted(p.name for p in tmp_path.iterdir()) == sorted(NAMES.values())
    assert [s["sheet"] for s in r["sheets"]] == ["01", "02/A", "03"]  # tab order
    for s in r["sheets"]:
        assert s["ok"] and s["error"] is None
        assert isinstance(s["ms"], float) and s["ms"] > 0
        assert s["files"] == [str(tmp_path / NAMES[s["sheet"]])]
        assert png_info(Path(s["files"][0]).read_bytes())[0] == 400
    for key in ("parse_ms", "prepare_ms", "render_wall_ms", "total_ms", "threads", "formats"):
        assert key in r


def test_threads_and_subset(three: cadrender.Document, tmp_path: Path) -> None:
    r = three.render_all(tmp_path, width=100, threads=1, sheets=["03", "01", "nope"])
    assert r["threads"] == 1
    assert r["done"] == ["01", "03"]
    assert list(r["failed"]) == ["nope"]
    assert [s["sheet"] for s in r["sheets"]] == ["01", "03", "nope"]


def test_threads_zero(three: cadrender.Document, tmp_path: Path) -> None:
    with pytest.raises(cadrender.OptionsError):
        three.render_all(tmp_path, threads=0)


def test_stem_and_options(three: cadrender.Document, tmp_path: Path) -> None:
    r = three.render_all(tmp_path, width=100, stem="x", sheets=["01"], preset="none", background="#000000")
    f = tmp_path / "x__01.png"
    assert r["sheets"][0]["files"] == [str(f)]
    assert png_info(f.read_bytes())[3] == b"\0\0\0"


def test_skip_existing(three: cadrender.Document, tmp_path: Path) -> None:
    (tmp_path / NAMES["02/A"]).write_bytes(b"old")
    r = three.render_all(tmp_path, width=100, skip_existing=True)
    assert r["skipped"] == ["02/A"] and r["done"] == ["01", "03"]
    assert (tmp_path / NAMES["02/A"]).read_bytes() == b"old"
    r = three.render_all(tmp_path, width=100)  # default: overwrite
    assert r["skipped"] == [] and (tmp_path / NAMES["02/A"]).read_bytes() != b"old"


def test_one_bad_sheet_does_not_stop_the_others(three: cadrender.Document, tmp_path: Path) -> None:
    # a directory where sheet 02/A's picture should go: writing it fails
    (tmp_path / NAMES["02/A"]).mkdir()
    r = three.render_all(tmp_path, width=100)
    assert r["done"] == ["01", "03"]
    assert list(r["failed"]) == ["02/A"] and r["failed"]["02/A"]
    bad = r["sheets"][1]
    assert not bad["ok"] and bad["error"] == r["failed"]["02/A"] and bad["files"] == []


def test_on_sheet_called_for_every_sheet(three: cadrender.Document, tmp_path: Path) -> None:
    calls: list[tuple[str, bool, str]] = []
    r = three.render_all(tmp_path, width=100, sheets=["01", "02/A", "03", "nope"],
                         on_sheet=lambda name, res: calls.append((name, res["ok"], threading.current_thread().name)))
    # "nope" is not a sheet of the file, so it is never rendered and never reported to on_sheet
    assert sorted(c[0] for c in calls) == ["01", "02/A", "03"]
    assert all(c[1] for c in calls)
    assert r["done"] == ["01", "02/A", "03"]


def test_on_sheet_error_is_raised(three: cadrender.Document, tmp_path: Path) -> None:
    class Stop(Exception):
        pass

    def boom(name: str, res: dict) -> None:
        raise Stop(name)

    with pytest.raises(Stop):
        three.render_all(tmp_path, width=100, threads=1, on_sheet=boom)
    # with one thread the first sheet is written, the others are cancelled
    assert [p.name for p in tmp_path.iterdir()] == [NAMES["01"]]


def test_formats(three: cadrender.Document, tmp_path: Path) -> None:
    if cadrender.SVG_AVAILABLE:
        r = three.render_all(tmp_path, width=100, formats=("png", "svg"))
        assert len(r["sheets"][0]["files"]) == 2
    else:
        with pytest.raises(NotImplementedError):
            three.render_all(tmp_path, formats=("png", "svg"))
        assert list(tmp_path.iterdir()) == []  # checked before any work
    with pytest.raises(cadrender.OptionsError):
        three.render_all(tmp_path, formats=())
    r = three.render_all(tmp_path, width=100, formats="png", sheets=["01"])
    assert r["formats"] == ["png"]


def test_many_python_threads_share_one_document(three: cadrender.Document) -> None:
    out: list[bytes] = []
    lock = threading.Lock()

    def work() -> None:
        b = three.render("02/A", width=256)
        with lock:
            out.append(b)

    ts = [threading.Thread(target=work) for _ in range(8)]
    for t in ts:
        t.start()
    for t in ts:
        t.join()
    assert len(out) == 8 and len(set(out)) == 1  # same picture every time
