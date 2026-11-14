"""Render options as a small Python class.

You can give options as keyword arguments (``doc.render(s, color="true")``)
or as a :class:`RenderOptions` object (``doc.render(s, options=opts)``), or
both: keyword arguments win. A field left as ``None`` keeps the value of the
preset. The compiled module checks every name and value, so a typo raises
:class:`cadrender.OptionsError`.
"""

from __future__ import annotations

from dataclasses import asdict, dataclass, field, replace
from typing import Any, Literal, Mapping, Sequence, Union

Color = Union[str, Sequence[int]]
Clean = Union[bool, Mapping[str, Sequence[str]]]


@dataclass(frozen=True)
class RenderOptions:
    """Everything that changes how a sheet is drawn (not the picture width).

    preset: ``"preview"`` (default: grey look, paper lineweights, the
        ``Defpoints`` layer left out) or ``"none"`` (true colour, no clean
        rules). Applied first.
    color: ``"mono"`` (grey levels) or ``"true"`` (real colours).
    background: page colour: ``"#RRGGBB"``, ``"#RRGGBBAA"``, ``"white"``,
        ``"black"``, ``"transparent"`` or ``(r, g, b[, a])``.
    lineweight: ``"paper"`` (true plot width), ``"preview"`` (ezdxf's mm as
        points at ``dpi``) or ``"hairline"`` (every line ``min_lw_px``).
    default_lw: lineweight for "default" layers, in 1/100 mm (25 = 0.25 mm).
    min_lw_px: thinnest pen in pixels.
    dpi: dots per inch for the ``"preview"`` lineweight rule (150).
    frame: ``"extents"`` (what is drawn, plus ``margin``) or ``"paper"``.
    margin: margin around the extents, part of the size (0.05 = 5 %).
    max_height: highest picture height in pixels.
    full_detail: ``True`` turns level of detail off (no dots, no grey bars
        for tiny text, no tint for dense hatches).
    lod: change single level-of-detail limits, for example
        ``{"dot_px": 0.5}``. Keys: enabled, dot_px, text_hide_px,
        text_greek_px, hatch_tint_px, dash_min_period_px.
    clean: ``False`` (no clean rules), ``True`` (the preset's rules) or your
        own rules: ``{"layers": [...], "block_keywords": [...]}``.
    draw_order: respect the draw order table (SORTENTS).
    text_masks: draw MTEXT background masks.
    wipeout_frames: ``False`` never draws WIPEOUT frames; ``True`` (default)
        draws them as the file says (WIPEOUTFRAME).
    parallel_viewports: draw the viewports of one sheet in parallel.
    max_depth, max_visits: safety limits for deep or huge block trees.
    """

    preset: Literal["preview", "none"] | None = None
    color: Literal["mono", "true"] | None = None
    background: Color | None = None
    lineweight: Literal["paper", "preview", "hairline"] | None = None
    default_lw: int | None = None
    min_lw_px: float | None = None
    dpi: float | None = None
    frame: Literal["extents", "paper"] | None = None
    margin: float | None = None
    max_height: int | None = None
    full_detail: bool | None = None
    lod: Mapping[str, float | bool] | None = field(default=None)
    clean: Clean | None = None
    draw_order: bool | None = None
    text_masks: bool | None = None
    wipeout_frames: bool | None = None
    parallel_viewports: bool | None = None
    max_depth: int | None = None
    max_visits: int | None = None

    def to_dict(self) -> dict[str, Any]:
        """Only the fields that are set (not ``None``)."""
        return {k: v for k, v in asdict(self).items() if v is not None}

    def replace(self, **changes: Any) -> RenderOptions:
        """A copy with some fields changed."""
        return replace(self, **changes)


def merge(options: RenderOptions | Mapping[str, Any] | None, width: int | None, extra: Mapping[str, Any]) -> dict[str, Any]:
    """One plain dict for the compiled module: options, then keywords, then width."""
    if options is None:
        d: dict[str, Any] = {}
    elif isinstance(options, RenderOptions):
        d = options.to_dict()
    else:
        d = {k: v for k, v in dict(options).items() if v is not None}
    d.update({k: v for k, v in extra.items() if v is not None})
    if width is not None:
        d["width"] = width
    return d
