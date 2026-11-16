# Writes attributes.dxf for the metadata tests (tests/attributes.rs and the
# Python tests). Run once: python3 gen_attributes.py <this folder>. Needs ezdxf.
# The output is committed. All names and values are made up.
#
# What is in it:
# - two sheets "A01" and "A02", each with a TITLE_BLOCK insert with attributes;
# - a ROOM insert with an attribute in model space;
# - a DETAIL block that holds a TAG insert with an attribute (nested block);
# - an accented value written as \U+00E0, a value with %%d;
# - a long value split into group 3 lines (as old writers do; ezdxf writes one
#   group 1, so the file is edited after saving);
# - a multi-line attribute (R2018, embedded MTEXT);
# - an attribute on its own layer, a hidden attribute;
# - TEXT, MTEXT and MULTILEADER texts.
import sys, os
import ezdxf

out = sys.argv[1]
LONG = "Long note: " + " ".join(f"part {i:02d} of the long value" for i in range(1, 21))

doc = ezdxf.new("R2018")
doc.layers.add("TITLE_TEXT")
doc.layers.add("NOTES")

tb = doc.blocks.new("TITLE_BLOCK")
tb.add_lwpolyline([(0, 0), (180, 0), (180, 60), (0, 60)], close=True)
for i, tag in enumerate(["PROJECT", "CLIENT", "SITE", "DRAWING", "SCALE", "NOTES", "DESCRIPTION", "SHEET_NO", "REVISION"]):
    tb.add_attdef(tag, (5, 55 - i * 6), dxfattribs={"height": 2.5})

tag = doc.blocks.new("TAG")
tag.add_circle((0, 0), 3)
tag.add_attdef("ID", (-1, -1), dxfattribs={"height": 2})

room = doc.blocks.new("ROOM")
room.add_lwpolyline([(0, 0), (40, 0), (40, 30), (0, 30)], close=True)
room.add_attdef("NAME", (2, 2), dxfattribs={"height": 2.5})

detail = doc.blocks.new("DETAIL")
detail.add_line((0, 0), (20, 0))
ref = detail.add_blockref("TAG", (25, 0))
ref.add_attrib("ID", "D-1", (24, -1), dxfattribs={"height": 2})


def title_block(layout, values, hidden=(), on_layer=None):
    ref = layout.add_blockref("TITLE_BLOCK", (200, 10))
    for i, (tag, value) in enumerate(values.items()):
        attribs = {"height": 2.5}
        if tag == on_layer:
            attribs["layer"] = "TITLE_TEXT"
        a = ref.add_attrib(tag, value, (205, 65 - i * 6), dxfattribs=attribs)
        if tag in hidden:
            a.is_invisible = True
        if tag == "DESCRIPTION":
            # multi-line attribute: the text lives in an embedded MTEXT
            m = layout.add_mtext(value.replace("\n", "\\P"), dxfattribs={"char_height": 2.5})
            a.embed_mtext(m)
            layout.delete_entity(m)
    return ref


a01 = doc.layouts.get("Layout1")
a01.rename("A01")
a02 = doc.layouts.new("A02")

title_block(a01, {
    "PROJECT": "P-001",
    "CLIENT": "Example Client Ltd",
    "SITE": "Citt\\U+00E0 di prova",
    "DRAWING": "Ground floor plan",
    "SCALE": "1:100",
    "NOTES": "@LONG@",
    "DESCRIPTION": "First line\nSecond line",
    "SHEET_NO": "A01",
}, on_layer="CLIENT")
title_block(a02, {
    "PROJECT": "P-001",
    "CLIENT": "Example Client Ltd",
    "DRAWING": "Section A-A",
    "SCALE": "1:50",
    "SHEET_NO": "A02",
    "REVISION": "B - angle 45%%d",
}, hidden=("REVISION",))
a01.add_text("Sheet note", dxfattribs={"insert": (20, 20), "height": 3, "layer": "NOTES"})
a02.add_mtext("{\\fArial|b1;Bold} heading\\PSecond line", dxfattribs={"insert": (20, 40), "char_height": 3})

msp = doc.modelspace()
msp.add_line((0, 0), (100, 0))
r = msp.add_blockref("ROOM", (10, 10))
r.add_attrib("NAME", "Kitchen", (12, 12), dxfattribs={"height": 2.5})
msp.add_blockref("DETAIL", (60, 10))
msp.add_text("Model text", dxfattribs={"insert": (0, -10), "height": 2.5})
ml = msp.add_multileader_mtext("Standard")
ml.set_content("Leader note", char_height=2.5)
ml.add_leader_line(ezdxf.render.mleader.ConnectionSide.left, [(0, 50)])
ml.build(insert=ezdxf.math.Vec2(10, 60))

path = os.path.join(out, "attributes.dxf")
doc.saveas(path)

# Old writers split a long value into group 3 lines with the last part in
# group 1; ezdxf writes one group 1. Split it here.
text = open(path, encoding="utf-8").read()
parts = [LONG[i:i + 100] for i in range(0, len(LONG), 100)]
split = "".join(f"  3\n{p}\n" for p in parts[:-1]) + f"  1\n{parts[-1]}\n"
assert text.count("  1\n@LONG@\n") == 1
text = text.replace("  1\n@LONG@\n", split)
with open(path, "w", encoding="utf-8", newline="\n") as f:
    f.write(text)
print("wrote", path, "long value:", len(LONG), "chars in", len(parts), "parts")
