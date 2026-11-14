"""Document.render / render_to and the options."""

from __future__ import annotations

from pathlib import Path

import pytest

import cadrender
from cadrender import RenderOptions
from conftest import png_info


def test_render_png(three: cadrender.Document) -> None:
    w, h, ctype, px = png_info(three.render("01"))
    assert w == 2000 and h > 0
    assert ctype == 0  # grey PNG: "preview" preset is monochrome on white
    assert px == b"\xff"  # the corner is page colour


@pytest.mark.parametrize("width", [64, 333, 1200])
def test_width(three: cadrender.Document, width: int) -> None:
    assert png_info(three.render("02/A", width=width))[0] == width


def test_unknown_sheet(three: cadrender.Document) -> None:
    with pytest.raises(cadrender.SheetNotFoundError) as e:
        three.render("99")
    assert isinstance(e.value, KeyError)
    assert "99" in str(e.value) and "01" in str(e.value)


def test_unknown_option(three: cadrender.Document) -> None:
    with pytest.raises(cadrender.OptionsError) as e:
        three.render("01", colour="true")
    assert isinstance(e.value, ValueError)
    assert "colour" in str(e.value)


@pytest.mark.parametrize("kw", [
    {"preset": "acad"}, {"color": "pink"}, {"lineweight": "thick"}, {"frame": "zoom"},
    {"background": "#12"}, {"background": (1, 2)}, {"lod": {"dots": 1}}, {"clean": {"layer": ["X"]}},
    {"clean": "yes"}, {"width": 0}, {"format": "bmp"},
])
def test_bad_option_values(three: cadrender.Document, kw: dict) -> None:
    with pytest.raises(cadrender.OptionsError):
        three.render("01", **kw)


def test_background_and_colour(three: cadrender.Document) -> None:
    _, _, ctype, px = png_info(three.render("01", width=100, preset="none", background="#102030"))
    assert ctype == 2 and px == bytes([0x10, 0x20, 0x30])
    _, _, ctype, px = png_info(three.render("01", width=100, background=(0, 0, 0, 0)))
    assert ctype == 6 and px == b"\0\0\0\0"


def test_options_object_and_keywords(three: cadrender.Document) -> None:
    opts = RenderOptions(preset="none", background="black")
    assert opts.to_dict() == {"preset": "none", "background": "black"}
    assert png_info(three.render("01", width=50, options=opts))[3] == b"\0\0\0"
    # keywords win over the options object
    assert png_info(three.render("01", width=50, options=opts, background="white"))[3] == b"\xff\xff\xff"
    # a plain dict works too
    assert png_info(three.render("01", width=50, options={"preset": "none"}))[2] == 2
    assert opts.replace(background=None).to_dict() == {"preset": "none"}


def test_clean_rules_change_the_picture(three: cadrender.Document) -> None:
    # tab "03" has one line on Defpoints, which the preview preset drops
    kept = three.render("03", width=600, clean=False)
    dropped = three.render("03", width=600)
    own = three.render("03", width=600, clean={"layers": ["DEF points"], "block_keywords": []})
    assert kept != dropped
    assert own == dropped


@pytest.mark.parametrize("kw", [
    {"full_detail": True}, {"lod": {"dot_px": 2.0, "enabled": True}}, {"lineweight": "hairline", "min_lw_px": 2.0},
    {"lineweight": "preview", "dpi": 300}, {"frame": "paper"}, {"margin": 0.2}, {"draw_order": False},
    {"text_masks": False}, {"wipeout_frames": False}, {"parallel_viewports": False}, {"max_depth": 4, "max_visits": 1000},
    {"color": "true"}, {"default_lw": 50}, {"max_height": 100},
])
def test_all_options_accepted(three: cadrender.Document, kw: dict) -> None:
    assert png_info(three.render("01", width=200, **kw))[0] == 200


def test_render_to(three: cadrender.Document, tmp_path: Path) -> None:
    out = tmp_path / "a.png"
    r = three.render_to("02/A", out, width=300)
    assert r["ok"] and r["files"] == [str(out)] and r["width"] == 300
    assert r["ms"] >= r["render_ms"] > 0
    assert png_info(out.read_bytes())[0] == 300
    assert [p.name for p in tmp_path.iterdir()] == ["a.png"]  # no .tmp left


def test_render_to_missing_dir_is_render_error(three: cadrender.Document, tmp_path: Path) -> None:
    with pytest.raises(cadrender.RenderError):
        three.render_to("01", tmp_path / "no" / "such" / "dir.png")


def test_svg_switch(three: cadrender.Document, tmp_path: Path) -> None:
    if cadrender.SVG_AVAILABLE:
        assert b"<svg" in three.render("01", format="svg")[:400]
        assert (tmp_path / "a.svg").exists() is False
        three.render_to("01", tmp_path / "a.svg")
        assert b"<svg" in (tmp_path / "a.svg").read_bytes()[:400]
    else:
        with pytest.raises(NotImplementedError):
            three.render("01", format="svg")
        with pytest.raises(NotImplementedError):
            three.render_to("01", tmp_path / "a.svg")
        assert not (tmp_path / "a.svg").exists()
