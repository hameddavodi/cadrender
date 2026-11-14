# Bundled font

`LiberationSans-Regular.ttf`: Liberation Sans 2.1.5 (Red Hat), SIL Open Font
License 1.1 (`LICENSE-LiberationSans-OFL.txt`). Same metrics as Arial.

cadrender ships no AutoCAD or other non-free fonts. It draws TrueType text
with one sans font: Arial if it is installed, else this font. It is built into the library with
`include_bytes!`, so the program and the Python wheel always have a font.
