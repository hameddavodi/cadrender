"""cadrender: very fast DXF (and soon DWG) to PNG/SVG rendering.

Open a drawing once, then render its sheets (layout tabs) as often as you
like::

    import cadrender
    doc = cadrender.open("drawing.dxf")
    print(doc.sheets)
    png = doc.render(doc.sheets[0], width=2000)          # bytes
    summary = doc.render_all("out/", threads=4)          # all sheets, in parallel

Reading, drawing and PNG encoding run in Rust with the GIL (Python's global
lock) released, so other Python threads keep running.
"""

from __future__ import annotations

import os
from pathlib import Path
from typing import Any, Callable, Iterable, Literal, Mapping, Sequence

from . import _cadrender
from ._errors import (
    DrawingNotFoundError,
    Error,
    OpenError,
    OptionsError,
    RenderError,
    SheetNotFoundError,
    UnsupportedFormatError,
)
from ._options import RenderOptions, merge

__version__: str = _cadrender.__version__

#: ``True`` once this build can write SVG. Until then ``format="svg"`` raises
#: ``NotImplementedError``.
SVG_AVAILABLE: bool = _cadrender.SVG_AVAILABLE

Format = Literal["png", "svg"]
PathLike = str | os.PathLike[str]
Options = RenderOptions | Mapping[str, Any] | None

#: Names accepted by the ``preset`` option.
PRESETS: tuple[str, ...] = tuple(_cadrender.presets())


def open(path: PathLike, *, font: str | PathLike | None = None) -> Document:  # noqa: A001 - mirrors builtins.open on purpose
    """Read a drawing file and return a :class:`Document`.

    path: a ``.dxf`` file (ASCII DXF). ``.dwg`` goes through the same call
        once the library reads DWG; until then it raises
        :class:`UnsupportedFormatError`.
    font: the one sans font used for all text (the font policy):
        ``"auto"`` (default: Arial if installed, else the built-in
        Liberation Sans), ``"liberation"``, or the path of a ``.ttf`` file.
        Fonts are part of the opened drawing, so they are chosen here and not
        per render.

    Raises :class:`DrawingNotFoundError` if the file does not exist and
    :class:`OpenError` if it is not a drawing we can read.
    """
    return Document(_cadrender.open(os.fspath(path), None if font is None else os.fspath(font)))


class Document:
    """One opened drawing. Read once; render many times, also from many threads."""

    __slots__ = ("_doc",)

    def __init__(self, native: _cadrender.Document) -> None:
        self._doc = native

    @property
    def path(self) -> Path:
        """The file this document was read from."""
        return Path(self._doc.path)

    @property
    def sheets(self) -> list[str]:
        """Sheet (layout tab) names in tab order. Model space is not a sheet."""
        return self._doc.sheets

    @property
    def info(self) -> dict[str, Any]:
        """Facts about the file: ``version``, ``size_bytes``, ``parse_ms``,
        ``font``, counts (``entities``, ``blocks``, ``layers``),
        ``entity_counts``, ``skipped_types`` and ``reader_warnings``."""
        return self._doc.info

    def sheet_info(self) -> list[dict[str, Any]]:
        """Sheets in tab order, one dict each: ``sheet`` (name), ``order``
        (tab order) and ``paper``: ``units_per_mm``, ``size_mm``
        ``(width, height)``, ``margins_mm`` ``(left, bottom, right, top)``,
        ``limits`` ``(x0, y0, x1, y1)`` and ``psltscale``; a value the file
        does not give is ``None``."""
        return self._doc.sheet_info()

    def attributes(self, sheet: str | None = None) -> list[dict[str, Any]]:
        """Block attributes: one dict per INSERT that has attributes (a title
        block, for example)::

            {"sheet": "A01", "block": "TITLE_BLOCK", "layer": "0", "handle": "2F1A",
             "position": (x, y), "values": {"PROJECT": "P-001", ...}}

        sheet: only this sheet; ``"Model"`` for model space. ``None`` (the
            default) = all. An INSERT inside another block reports that
            block's name as ``sheet``.

        ``values`` maps tag -> value, in file order (for a tag used twice, the
        last value). Hidden attributes are included. Values are plain text:
        special characters (``\\U+00E8``, ``%%d``) are decoded, MTEXT
        formatting is removed, the lines of a multi-line attribute are joined
        with ``"\\n"``, and spaces at both ends are removed. ``position`` is the
        insertion point (paper units on a sheet). Order: model space, the
        sheets in tab order, then other blocks by name.

        Raises :class:`SheetNotFoundError` for an unknown sheet.
        """
        return self._doc.attributes(sheet)

    def texts(self, sheet: str | None = None, layers: Iterable[str] | None = None) -> list[dict[str, Any]]:
        """TEXT, MTEXT and MULTILEADER texts as plain text, one dict each::

            {"sheet": "A01", "layer": "NOTES", "type": "MTEXT", "handle": "2F1B",
             "text": "First line\\nSecond line", "position": (x, y)}

        sheet: as in :meth:`attributes`.
        layers: only texts on these layers (case does not matter).

        Invisible texts are left out; texts on layers that are off or frozen
        are included. Same order as :meth:`attributes`.
        """
        return self._doc.texts(sheet, None if layers is None else [layers] if isinstance(layers, str) else list(layers))

    def __repr__(self) -> str:
        return repr(self._doc)

    def render(self, sheet: str, width: int = 2000, format: Format = "png", *, options: Options = None, **kwargs: Any) -> bytes:  # noqa: A002
        """Render one sheet and return the file bytes (PNG or SVG).

        sheet: a name from :attr:`sheets`.
        width: picture width in pixels; the height follows the drawing.
        format: ``"png"`` or ``"svg"``.
        options / kwargs: see :class:`RenderOptions` (keywords win).

        Raises :class:`SheetNotFoundError`, :class:`OptionsError` or
        :class:`RenderError`.
        """
        return self._doc.render(sheet, format, merge(options, width, kwargs))

    def render_to(self, sheet: str, path: PathLike, width: int = 2000, format: Format | None = None, *, options: Options = None, **kwargs: Any) -> dict[str, Any]:  # noqa: A002
        """Render one sheet into a file and return its result dict
        (``ms``, ``width``, ``height``, ``warnings``, ...).

        format: ``"png"`` or ``"svg"``; by default taken from the file
        extension (``.svg`` means SVG, anything else PNG). The file is written
        to a temporary name first and then renamed, so it never exists half
        written.
        """
        path = os.fspath(path)
        if format is None:
            format = "svg" if path.lower().endswith(".svg") else "png"
        return self._doc.render_to(sheet, path, format, merge(options, width, kwargs))

    def render_all(
        self,
        out_dir: PathLike,
        width: int = 2000,
        formats: Iterable[Format] = ("png",),
        threads: int | None = None,
        sheets: Sequence[str] | None = None,
        skip_existing: bool = False,
        on_sheet: Callable[[str, dict[str, Any]], object] | None = None,
        *,
        stem: str | None = None,
        options: Options = None,
        **kwargs: Any,
    ) -> dict[str, Any]:
        """Render many sheets in parallel into ``out_dir``.

        Files are named ``<stem>__<sheet>.<ext>`` (``/`` and ``\\`` in a sheet
        name become ``_``); ``stem`` defaults to the drawing's file name
        without extension.

        threads: worker threads; ``None`` = all cores.
        sheets: only these sheets (tab order is kept). A name that is not in
            the file is reported in ``failed``.
        skip_existing: leave a sheet alone when all its files exist already.
        on_sheet: ``on_sheet(name, result)`` after each sheet (also failed
            ones), called from a worker thread while holding the GIL. If it
            raises, sheets not started yet are cancelled and the exception is
            raised when the running ones are done.

        One bad sheet never stops the others. Returns a dict with ``done``,
        ``skipped`` (lists of names), ``failed`` (name -> error), ``sheets``
        (one result dict per sheet with ``ok``, ``files``, ``ms``,
        ``render_ms``, ``encode_ms``, ``write_ms``, ``error``, ...) and the
        times ``parse_ms``, ``prepare_ms``, ``render_wall_ms``, ``total_ms``.
        """
        if stem is None:
            stem = self.path.stem
        fmts: list[str] = [formats] if isinstance(formats, str) else list(formats)
        return self._doc.render_all(
            os.fspath(out_dir), stem, fmts, merge(options, width, kwargs),
            threads, None if sheets is None else list(sheets), skip_existing, on_sheet,
        )


__all__ = [
    "open",
    "Document",
    "RenderOptions",
    "Error",
    "OpenError",
    "DrawingNotFoundError",
    "UnsupportedFormatError",
    "SheetNotFoundError",
    "OptionsError",
    "RenderError",
    "SVG_AVAILABLE",
    "PRESETS",
    "__version__",
]
