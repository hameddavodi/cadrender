"""Document.sheet_info / attributes / texts on attributes.dxf (and its DWG twin)."""

from __future__ import annotations

import pytest

import cadrender
from conftest import REPO

DXF = REPO / "crates" / "cadrender-dxf" / "tests" / "data" / "attributes.dxf"
DWG = REPO / "crates" / "cadrender-dwg-scene" / "tests" / "data" / "attributes.dwg"
LONG = "Long note: " + " ".join(f"part {i:02d} of the long value" for i in range(1, 21))


@pytest.fixture(scope="module")
def doc() -> cadrender.Document:
    return cadrender.open(DXF)


def test_sheet_info(doc: cadrender.Document) -> None:
    info = doc.sheet_info()
    assert [s["sheet"] for s in info] == doc.sheets == ["A01", "A02"]
    orders = [s["order"] for s in info]
    assert orders == sorted(orders) and orders[0] == 1  # as the file says (ezdxf writes 1, 3)
    paper = info[0]["paper"]
    assert set(paper) == {"units_per_mm", "size_mm", "margins_mm", "limits", "psltscale"}
    assert len(paper["margins_mm"]) == 4


def test_attributes_all(doc: cadrender.Document) -> None:
    recs = doc.attributes()
    assert [(r["sheet"], r["block"]) for r in recs] == [
        ("Model", "ROOM"), ("A01", "TITLE_BLOCK"), ("A02", "TITLE_BLOCK"), ("DETAIL", "TAG")]
    a01 = recs[1]
    assert a01["values"] == {
        "PROJECT": "P-001",
        "CLIENT": "Example Client Ltd",
        "SITE": "Città di prova",
        "DRAWING": "Ground floor plan",
        "SCALE": "1:100",
        "NOTES": LONG,
        "DESCRIPTION": "First line\nSecond line",
        "SHEET_NO": "A01",
    }
    assert list(a01["values"])[0] == "PROJECT"  # file order
    assert a01["layer"] == "0"
    assert a01["position"] == (200.0, 10.0)
    assert int(a01["handle"], 16) > 0 and a01["handle"] == a01["handle"].upper()


def test_attributes_one_sheet(doc: cadrender.Document) -> None:
    a02 = doc.attributes("A02")
    assert len(a02) == 1
    assert a02[0]["values"]["REVISION"] == "B - angle 45°"  # hidden attribute, %%d decoded
    assert doc.attributes("Model")[0]["values"] == {"NAME": "Kitchen"}
    assert doc.attributes("DETAIL")[0]["values"] == {"ID": "D-1"}


def test_unknown_sheet(doc: cadrender.Document) -> None:
    with pytest.raises(cadrender.SheetNotFoundError):
        doc.attributes("NOPE")
    with pytest.raises(cadrender.SheetNotFoundError):
        doc.texts("NOPE")


def test_texts(doc: cadrender.Document) -> None:
    got = [(t["sheet"], t["type"], t["layer"], t["text"]) for t in doc.texts()]
    assert got == [
        ("Model", "TEXT", "0", "Model text"),
        ("Model", "MULTILEADER", "0", "Leader note"),
        ("A01", "TEXT", "NOTES", "Sheet note"),
        ("A02", "MTEXT", "0", "Bold heading\nSecond line"),
    ]
    assert [t["text"] for t in doc.texts(layers=["notes"])] == ["Sheet note"]
    assert [t["text"] for t in doc.texts("A02")] == ["Bold heading\nSecond line"]
    assert doc.texts("A01")[0]["position"] == (20.0, 20.0)


def test_dwg_gives_the_same_records(doc: cadrender.Document) -> None:
    dwg = cadrender.open(DWG)
    assert dwg.texts() == doc.texts()
    a, b = dwg.attributes(), doc.attributes()
    # ODA ignores the group 3 lines of the split NOTES value (see the Rust test)
    assert LONG.endswith(a[1]["values"].pop("NOTES"))
    assert b[1]["values"].pop("NOTES") == LONG
    assert a == b
    assert [s["sheet"] for s in dwg.sheet_info()] == ["A01", "A02"]
