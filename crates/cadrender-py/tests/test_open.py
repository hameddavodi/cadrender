"""cadrender.open, Document.sheets / info, and open errors."""

from __future__ import annotations

from pathlib import Path

import pytest

import cadrender
from conftest import HAND_MADE, THREE, png_info


def test_hand_made_files_found() -> None:
    assert len(HAND_MADE) >= 8


@pytest.mark.parametrize("path", HAND_MADE, ids=lambda p: p.name)
def test_every_hand_made_dxf_opens_and_renders(path: Path) -> None:
    doc = cadrender.open(path)
    info = doc.info
    assert info["entities"] >= 1
    assert info["version"] == "AC1032"
    for sheet in doc.sheets:
        w, h, _, _ = png_info(doc.render(sheet, width=300))
        assert w == 300 and h > 0


def test_sheets_in_tab_order(three: cadrender.Document) -> None:
    assert three.sheets == ["01", "02/A", "03"]
    assert three.path == THREE
    assert "3 sheets" in repr(three)


def test_info_keys(three: cadrender.Document) -> None:
    info = three.info
    for key in ("path", "size_bytes", "parse_ms", "version", "font", "sheets", "entities", "blocks", "layers",
                "entity_counts", "skipped_types", "reader_warnings"):
        assert key in info, key
    assert info["sheets"] == 3
    assert info["size_bytes"] == THREE.stat().st_size
    assert info["entity_counts"]["LINE"] >= 2


def test_open_accepts_str_and_pathlike() -> None:
    assert cadrender.open(str(THREE)).sheets == cadrender.open(THREE).sheets


def test_missing_file(tmp_path: Path) -> None:
    with pytest.raises(cadrender.DrawingNotFoundError) as e:
        cadrender.open(tmp_path / "nope.dxf")
    # also catchable as the standard Python errors
    assert isinstance(e.value, FileNotFoundError)
    assert isinstance(e.value, cadrender.OpenError)
    assert isinstance(e.value, cadrender.Error)
    assert "nope.dxf" in str(e.value)


def test_not_a_drawing(tmp_path: Path) -> None:
    bad = tmp_path / "bad.dxf"
    bad.write_bytes(b"hello, this is not a DXF file\n")
    with pytest.raises(cadrender.OpenError):
        cadrender.open(bad)


def test_dwg_goes_through_open(tmp_path: Path) -> None:
    # A fake .dwg: today "unsupported", once DWG lands "cannot read". Both are OpenError.
    fake = tmp_path / "fake.dwg"
    fake.write_bytes(b"AC1032" + b"\0" * 200)
    with pytest.raises(cadrender.OpenError):
        cadrender.open(fake)


def test_font_choice() -> None:
    doc = cadrender.open(THREE, font="liberation")
    assert "liberation" in doc.info["font"].lower()


def test_bad_font_file() -> None:
    with pytest.raises(cadrender.OptionsError):
        cadrender.open(THREE, font="/no/such/font.ttf")


def test_module_facts() -> None:
    assert cadrender.__version__
    assert set(cadrender.PRESETS) >= {"preview", "none"}
    assert isinstance(cadrender.SVG_AVAILABLE, bool)
