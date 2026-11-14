#!/usr/bin/env python3
"""Validate cadrender-dwg against ODA (DXF) and LibreDWG (DXF and JSON).

For every DWG it compares:
  1. object counts per type: ours vs ODA DXF vs LibreDWG DXF (vs dwgread
     JSON when given);
  2. per handle, for the entity types we decode: type, layer name, colour,
     key numbers (coordinates, radius, angles, counts) and key strings
     (text, block name, pattern name) against ODA's DXF.

ODA is the main reference. Run with the paths of the inputs:

  validate.py --dwgdump PATH/dwgdump --dwg FILE.dwg --oda FILE.dxf \
      [--libre-dxf FILE.dxf] [--libre-json FILE.json] [--out report.md]

Only the Python standard library is used. Your files are read from the
given paths and nothing is copied into the repository.
"""
import argparse
import collections
import json
import math
import re
import subprocess
import sys

# ---------------------------------------------------------------- DXF reading

ENTITY_SECTIONS = {"ENTITIES", "BLOCKS"}
GROUP_TYPES = {"DIMSTYLE", "MLEADERSTYLE"}  # raw group checks
UNICODE_ESC = re.compile(r"\\U\+([0-9A-Fa-f]{4})")


CARET = re.compile(r"\^(.)")


def dxf_unescape(s):
    """Undo DXF text escapes: \\U+XXXX and caret control codes (^I = tab)."""
    s = UNICODE_ESC.sub(lambda m: chr(int(m.group(1), 16)), s)
    return CARET.sub(lambda m: "^" if m.group(1) == " " else chr(ord(m.group(1)) & 0x1F), s)


def read_dxf(path):
    """Yield (section, type, list of (code, value)) for every DXF record."""
    with open(path, "rb") as f:
        data = f.read()
    text = data.decode("utf-8", errors="replace") if b"\r\n" not in data[:200] else data.decode(
        "utf-8", errors="replace")
    lines = text.splitlines()
    section = None
    cur_type = None
    cur = []
    i = 0
    n = len(lines) - 1
    while i < n:
        code = lines[i].strip()
        val = lines[i + 1]
        i += 2
        if code == "0":
            if cur_type is not None:
                yield section, cur_type, cur
            v = val.strip()
            if v == "SECTION":
                cur_type = None
                cur = []
                # next pair is 2/name
                section = lines[i + 1].strip()
                i += 2
                continue
            if v in ("ENDSEC", "EOF"):
                cur_type = None
                cur = []
                continue
            cur_type = v
            cur = []
        else:
            try:
                c = int(code)
            except ValueError:
                continue
            cur.append((c, val.rstrip("\r")))
    if cur_type is not None:
        yield section, cur_type, cur


def first(rec, code, default=None):
    for c, v in rec:
        if c == code:
            return v
    return default


def all_of(rec, code):
    return [v for c, v in rec if c == code]


def f(rec, code, default=0.0):
    v = first(rec, code)
    return float(v) if v is not None else default


def dxf_values(etype, rec):
    """Key numbers and string, in the same order dwgdump --json writes them."""
    if etype == "LINE":
        return [f(rec, 10), f(rec, 20), f(rec, 30), f(rec, 11), f(rec, 21), f(rec, 31)], ""
    if etype == "CIRCLE":
        return [f(rec, 10), f(rec, 20), f(rec, 30), f(rec, 40)], ""
    if etype == "ARC":
        return [f(rec, 10), f(rec, 20), f(rec, 30), f(rec, 40), f(rec, 50), f(rec, 51)], ""
    if etype == "POINT":
        return [f(rec, 10), f(rec, 20), f(rec, 30)], ""
    if etype == "ELLIPSE":
        return [f(rec, 10), f(rec, 20), f(rec, 30), f(rec, 11), f(rec, 21), f(rec, 31), f(rec, 40),
                f(rec, 41), f(rec, 42, 2 * math.pi)], ""
    if etype in ("SOLID", "TRACE"):
        return [f(rec, 10), f(rec, 20), f(rec, 11), f(rec, 21), f(rec, 12), f(rec, 22), f(rec, 13),
                f(rec, 23)], ""
    if etype == "TEXT":
        return [f(rec, 10), f(rec, 20), f(rec, 40), f(rec, 50)], dxf_unescape(first(rec, 1, ""))
    if etype in ("ATTRIB", "ATTDEF"):
        return [f(rec, 10), f(rec, 20), f(rec, 40), f(rec, 50)], dxf_unescape(first(rec, 2, ""))
    if etype == "MTEXT":
        txt = "".join(all_of(rec, 3)) + first(rec, 1, "")
        return [f(rec, 10), f(rec, 20), f(rec, 30), f(rec, 40), f(rec, 41)], dxf_unescape(txt)
    if etype == "INSERT":
        return [f(rec, 10), f(rec, 20), f(rec, 30), f(rec, 41, 1.0), f(rec, 42, 1.0), f(rec, 43, 1.0),
                f(rec, 50)], dxf_unescape(first(rec, 2, ""))
    if etype == "LWPOLYLINE":
        xs = [float(v) for v in all_of(rec, 10)]
        ys = [float(v) for v in all_of(rec, 20)]
        p = [float(first(rec, 90, len(xs)))]
        if xs:
            p += [xs[0], ys[0], xs[-1], ys[-1]]
        return p, ""
    if etype == "POLYLINE":
        return [f(rec, 70)], ""
    if etype == "VERTEX":
        return [f(rec, 10), f(rec, 20), f(rec, 42)], ""
    if etype == "HATCH":
        solid = first(rec, 70, "0").strip() == "1"
        return [f(rec, 91), 0.0 if solid else f(rec, 78)], dxf_unescape(first(rec, 2, ""))
    if etype == "SPLINE":
        return [f(rec, 71), f(rec, 73), f(rec, 74), f(rec, 72)], ""
    if etype in ("WIPEOUT", "IMAGE"):
        return [f(rec, 10), f(rec, 20), f(rec, 30)], ""
    if etype == "DIMENSION":
        return [], dxf_unescape(first(rec, 2, ""))
    if etype == "VIEWPORT":
        return [f(rec, 10), f(rec, 20), f(rec, 40), f(rec, 41), f(rec, 90)], ""
    if etype == "BLOCK":
        return [], dxf_unescape(first(rec, 2, ""))
    if etype == "LEADER":
        xs = [float(v) for v in all_of(rec, 10)]
        ys = [float(v) for v in all_of(rec, 20)]
        p = [f(rec, 76, len(xs))]
        if xs:
            p += [xs[0], ys[0], xs[-1], ys[-1]]
        return p, ""
    if etype in ("RAY", "XLINE"):
        return [f(rec, 10), f(rec, 20), f(rec, 30)], ""
    if etype == "TOLERANCE":
        return [f(rec, 10), f(rec, 20), f(rec, 30)], dxf_unescape(first(rec, 1, ""))
    if etype == "3DFACE":
        return [f(rec, c + d) for c in (10, 11, 12, 13) for d in (0, 10, 20)], ""
    if etype == "MLINE":
        return [f(rec, 72), f(rec, 10), f(rec, 20)], ""
    if etype == "ACAD_TABLE":
        return [f(rec, 10), f(rec, 20), f(rec, 30)], dxf_unescape(first(rec, 2, ""))
    if etype == "MULTILEADER":
        # context data: 40 scale, 10/20 content base, 41 text height; 304 is
        # both the "LEADER_LINE{" marker and the text label
        labels = [v for v in all_of(rec, 304) if v.strip() != "LEADER_LINE{"]
        roots = sum(1 for v in all_of(rec, 302) if v.strip() == "LEADER{")
        lines = sum(1 for v in all_of(rec, 304) if v.strip() == "LEADER_LINE{")
        p = [float(roots), float(lines), f(rec, 40), f(rec, 10), f(rec, 20), f(rec, 41)]
        return p, dxf_unescape(labels[0]) if labels else ""
    return None, None


def dxf_color(rec):
    tc = first(rec, 420)
    if tc is not None:
        return "#%06x" % (int(tc) & 0xFFFFFF)
    c = first(rec, 62)
    if c is None:
        return "bylayer"
    c = int(c)
    if c == 256:
        return "bylayer"
    if c == 0:
        return "byblock"
    return abs(c)


def load_dxf(path):
    """Return (counts per type, {handle: record info}) for a DXF file."""
    counts = collections.Counter()
    entity_counts = collections.Counter()
    recs = {}
    block_record = {}  # block name -> BLOCK_RECORD handle
    for section, etype, rec in read_dxf(path):
        if section not in ("ENTITIES", "BLOCKS", "OBJECTS", "TABLES"):
            continue
        if section == "TABLES" and etype in ("TABLE", "ENDTAB"):
            continue
        counts[etype] += 1
        h = first(rec, 105 if etype == "DIMSTYLE" else 5)
        if h is None:
            continue
        h = h.strip().upper()
        if section == "TABLES" and etype == "BLOCK_RECORD":
            block_record[dxf_unescape(first(rec, 2, ""))] = h
        if section in ENTITY_SECTIONS:
            if etype != "ENDBLK":
                entity_counts[etype] += 1
            p, s = dxf_values(etype, rec)
            bh = ""
            if etype in ("INSERT", "DIMENSION", "ACAD_TABLE"):
                bh = block_record.get(s, "?")
            elif etype == "BLOCK":
                bh = first(rec, 330, "?").strip().upper()
            recs[h] = {"t": etype, "layer": dxf_unescape(first(rec, 8, "")), "color": dxf_color(rec),
                       "p": p, "s": s, "bh": bh}
        else:
            recs[h] = {"t": etype, "rec": rec if etype in GROUP_TYPES else None}
    return counts, recs, entity_counts


# ------------------------------------------------------------ our side

# our raw type name -> DXF record name
RAW_TO_DXF = {
    "BLOCK_HEADER": "BLOCK_RECORD",
    "POLYLINE_2D": "POLYLINE",
    "POLYLINE_3D": "POLYLINE",
    "POLYLINE_PFACE": "POLYLINE",
    "POLYLINE_MESH": "POLYLINE",
    "VERTEX_2D": "VERTEX",
    "VERTEX_3D": "VERTEX",
    "VERTEX_MESH": "VERTEX",
    "VERTEX_PFACE": "VERTEX",
    "VERTEX_PFACE_FACE": "VERTEX",
    "MINSERT": "INSERT",
    "VX_TABLE_RECORD": None,
    "ACDBDICTIONARYWDFLT": "ACDBDICTIONARYWDFLT",
    "ACDBPLACEHOLDER": "ACDBPLACEHOLDER",
    "PLACEHOLDER": "ACDBPLACEHOLDER",
    "ACAD_PROXY_ENTITY": "ACAD_PROXY_ENTITY",
}


def dxf_name(raw):
    if raw.startswith("DIMENSION_"):
        return "DIMENSION"
    if raw.endswith("_CONTROL"):
        return None
    return RAW_TO_DXF.get(raw, raw)


def load_ours(dwgdump, dwg):
    out = subprocess.run([dwgdump, dwg, "--json"], check=True, capture_output=True).stdout
    recs = {}
    counts = collections.Counter()
    entity_counts = collections.Counter()
    for line in out.decode("utf-8").splitlines():
        o = json.loads(line)
        n = dxf_name(o["raw"])
        if n:
            counts[n] += 1
            if "layer" in o and n != "ENDBLK":
                entity_counts[n] += 1
        recs[o["h"]] = o
    return counts, recs, entity_counts


def load_libre_json(path):
    try:
        with open(path, "rb") as fh:
            d = json.load(fh)
    except ValueError as e:
        # LibreDWG sometimes writes invalid JSON (seen on some files).
        sys.stderr.write("dwgread JSON not readable: %s\n" % e)
        return None, None
    counts = collections.Counter()
    handles = {}
    for o in d["OBJECTS"]:
        raw = o.get("entity") or o.get("object")
        n = dxf_name(raw)
        if n:
            counts[n] += 1
        h = o.get("handle")
        if h:
            handles["%X" % h[-1]] = o
    return counts, handles


# ------------------------------------------------------------ comparing

def close(a, b):
    if a is None or b is None:
        return False
    return abs(a - b) <= 1e-6 + 1e-9 * max(abs(a), abs(b))


def angle_close(a, b):
    d = (a - b) % 360.0
    return min(d, 360.0 - d) <= 1e-6


def compare_values(t, ours, theirs):
    """Return the list of field names that differ."""
    bad = []
    if ours.get("layer") != theirs["layer"]:
        bad.append("layer")
    if ours.get("color") != theirs["color"]:
        bad.append("color")
    op, tp = ours.get("p") or [], theirs["p"] or []
    if len(op) != len(tp):
        bad.append("p-len")
    else:
        for i, (a, b) in enumerate(zip(op, tp)):
            is_angle = (t == "ARC" and i >= 4) or (t in ("TEXT", "ATTRIB", "ATTDEF") and i == 3) or (
                t == "INSERT" and i == 6)
            ok = angle_close(a, b) if is_angle else close(a, b)
            if not ok:
                bad.append("p%d" % i)
                break
    if t in ("INSERT", "DIMENSION", "ACAD_TABLE", "BLOCK"):
        # Anonymous block names (*U12, *D3) are renumbered by DXF writers;
        # compare the block record handle instead of the name.
        if ours.get("bh") != theirs.get("bh"):
            bad.append("block")
    else:
        # \U+XXXX inside text is an AutoCAD escape stored in the DWG itself;
        # the DXF side was unescaped too, so compare both unescaped.
        s = UNICODE_ESC.sub(lambda m: chr(int(m.group(1), 16)), ours.get("s", ""))
        if t == "HATCH" and s.endswith(",_O"):
            s = s[:-3]  # ODA drops this suffix when it writes DXF
        if s != theirs["s"]:
            bad.append("s")
    return bad


def compare_groups(ours, rec):
    """Compare our (code, value) groups with the DXF record, k-th occurrence
    of a code against the k-th occurrence. Codes the DXF record does not have
    are not counted (ODA leaves out default values). Returns (compared,
    equal, Counter of differing codes)."""
    theirs = collections.defaultdict(list)
    for c, v in rec:
        theirs[int(c)].append(v.strip())
    seen = collections.Counter()
    n = ok = 0
    bad = collections.Counter()
    for c, v in ours:
        k = seen[c]
        seen[c] += 1
        if k >= len(theirs.get(c, [])):
            continue
        t = theirs[c][k]
        n += 1
        if 320 <= c <= 369:
            same = v.upper().lstrip("0") == t.upper().lstrip("0")
        else:
            try:
                same = close(float(v), float(t))
            except ValueError:
                same = dxf_unescape(v) == dxf_unescape(t)
        if same:
            ok += 1
        else:
            bad[c] += 1
    return n, ok, bad


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dwgdump", required=True)
    ap.add_argument("--dwg", required=True)
    ap.add_argument("--oda", required=True)
    ap.add_argument("--libre-dxf")
    ap.add_argument("--libre-json")
    ap.add_argument("--out")
    ap.add_argument("--examples", type=int, default=5)
    a = ap.parse_args()

    oc, orecs, oe = load_ours(a.dwgdump, a.dwg)
    dc, drecs, de = load_dxf(a.oda)
    lc, _, le = load_dxf(a.libre_dxf) if a.libre_dxf else (None, None, None)
    jc, jh = load_libre_json(a.libre_json) if a.libre_json else (None, None)

    lines = []
    w = lines.append
    w("# Validation: %s\n" % a.dwg.rsplit("/", 1)[-1])
    w("## Counts per type\n")
    head = "| type | ours | ODA DXF |" + (" LibreDWG DXF |" if lc else "") + (" dwgread JSON |" if jc else "")
    w(head)
    w("|" + "---|" * (head.count("|") - 1))
    types = sorted(set(oc) | set(dc))
    n_same = 0
    for t in types:
        row = "| %s | %d | %d |" % (t, oc.get(t, 0), dc.get(t, 0))
        if lc is not None:
            row += " %d |" % lc.get(t, 0)
        if jc is not None:
            row += " %d |" % jc.get(t, 0)
        if oc.get(t, 0) == dc.get(t, 0):
            n_same += 1
        w(row)
    w("\n%d of %d types have the same count as ODA.\n" % (n_same, len(types)))
    w("Entities (ENTITIES + BLOCKS sections, without ENDBLK): ours %d, ODA %d%s.\n" % (
        sum(oe.values()), sum(de.values()), (", LibreDWG DXF %d" % sum(le.values())) if le else ""))
    ent_types = sorted(set(oe) | set(de))
    diff = [t for t in ent_types if oe.get(t, 0) != de.get(t, 0)]
    w("Entity types with a different count than ODA: %s.\n" % (", ".join(
        "%s (ours %d, ODA %d)" % (t, oe.get(t, 0), de.get(t, 0)) for t in diff) or "none"))

    # Per-handle comparison for decoded entity types.
    checked = collections.Counter()
    good = collections.Counter()
    missing = collections.Counter()
    fields = collections.defaultdict(collections.Counter)
    examples = collections.defaultdict(list)
    for h, o in orecs.items():
        t = o["t"]
        if "layer" not in o:
            continue
        theirs = drecs.get(h)
        if theirs is None or theirs.get("p") is None:
            if theirs is None and dxf_name(o["raw"]) in dc:
                missing[t] += 1
            continue
        if theirs["t"] != t:
            fields[t]["type:" + theirs["t"]] += 1
            continue
        checked[t] += 1
        bad = compare_values(t, o, theirs)
        if not bad:
            good[t] += 1
        else:
            for b in bad:
                fields[t][b] += 1
            if len(examples[t]) < a.examples:
                examples[t].append((h, bad, o, theirs))
    w("## Per-handle values vs ODA (decoded entity types)\n")
    w("| type | compared | all equal | differences by field | in ours, not in ODA |")
    w("|---|---|---|---|---|")
    for t in sorted(set(checked) | set(missing) | set(fields)):
        fd = ", ".join("%s %d" % kv for kv in fields[t].most_common(6))
        w("| %s | %d | %d | %s | %d |" % (t, checked[t], good[t], fd, missing[t]))
    tot_c, tot_g = sum(checked.values()), sum(good.values())
    w("\nTotal: %d entities compared, %d fully equal (%.3f %%).\n" % (
        tot_c, tot_g, 100.0 * tot_g / max(tot_c, 1)))
    w("## Examples of differences\n")
    for t, exs in sorted(examples.items()):
        for h, bad, o, theirs in exs:
            w("- %s %s %s: ours layer=%r color=%r p=%s s=%r | ODA layer=%r color=%r p=%s s=%r" % (
                t, h, bad, o.get("layer"), o.get("color"), o.get("p"), o.get("s", "")[:60],
                theirs["layer"], theirs["color"], theirs["p"], theirs["s"][:60]))
    w("\n## Table objects: raw groups vs ODA (%s)\n" % ", ".join(sorted(GROUP_TYPES)))
    w("| type | objects | groups compared | equal | differing codes |")
    w("|---|---|---|---|---|")
    gstat = collections.defaultdict(lambda: [0, 0, 0, collections.Counter()])
    for h, o in orecs.items():
        theirs = drecs.get(h)
        if o["t"] not in GROUP_TYPES or not theirs or not theirs.get("rec"):
            continue
        st = gstat[o["t"]]
        st[0] += 1
        n, ok, bad = compare_groups(o.get("g", []), theirs["rec"])
        st[1] += n
        st[2] += ok
        st[3].update(bad)
    for t, (n_obj, n, ok, bad) in sorted(gstat.items()):
        w("| %s | %d | %d | %d | %s |" % (t, n_obj, n, ok, ", ".join("%d (%d)" % kv for kv in bad.most_common(8))))
    if jh is not None:
        same_t = sum(1 for h, o in orecs.items() if h in jh)
        w("\n## dwgread JSON\n\n%d of our %d handles also exist in dwgread's output (it has %d).\n" % (
            same_t, len(orecs), len(jh)))
    text = "\n".join(lines) + "\n"
    if a.out:
        with open(a.out, "w") as fh:
            fh.write(text)
    sys.stdout.write(text)


if __name__ == "__main__":
    main()
