"""Errors raised by cadrender. All of them are subclasses of :class:`Error`.

The compiled module raises these classes by name, so do not rename them.
"""

from __future__ import annotations


class Error(Exception):
    """Base class of every cadrender error."""


class OpenError(Error, OSError):
    """The file could not be opened or read as a drawing."""


class DrawingNotFoundError(OpenError, FileNotFoundError):
    """The file does not exist."""


class UnsupportedFormatError(OpenError):
    """The file type is not read by this build (yet), for example ``.dwg``
    before DWG support lands."""


class SheetNotFoundError(Error, KeyError):
    """There is no sheet (layout tab) with this name."""

    def __str__(self) -> str:  # KeyError would show the message in quotes
        return str(self.args[0]) if self.args else ""


class OptionsError(Error, ValueError):
    """An option name or value is wrong."""


class RenderError(Error):
    """Drawing one sheet failed (or its file could not be written)."""


__all__ = [
    "Error",
    "OpenError",
    "DrawingNotFoundError",
    "UnsupportedFormatError",
    "SheetNotFoundError",
    "OptionsError",
    "RenderError",
]
