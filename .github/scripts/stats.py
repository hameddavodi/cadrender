#!/usr/bin/env python3
"""Append one row per download source to stats/downloads.csv.

Sources: PyPI (pypistats.org), crates.io and GitHub release assets (CLI
binaries). A source that fails or has
no data is skipped with a warning; the other rows are still written.

Environment:
  GITHUB_REPOSITORY  owner/name (set by GitHub Actions)
  GITHUB_TOKEN       for release asset counts (read-only is enough)

Note: pypistats.org keeps only the last 180 days, so the PyPI "total" is a
180-day number; the weekly rows (recent, 7d) add up to the real history.
Standard library only.
"""

from __future__ import annotations

import datetime as dt
import json
import os
import sys
import urllib.request
from pathlib import Path

PACKAGE = "cadrender"
UA = "cadrender-stats"
CSV = Path(__file__).resolve().parents[2] / "stats" / "downloads.csv"
HEADER = ["date", "source", "total", "recent", "recent_window"]


def get(url: str, token: str | None = None) -> object:
    req = urllib.request.Request(url, headers={"User-Agent": UA, "Accept": "application/json"})
    if token:
        req.add_header("Authorization", f"Bearer {token}")
        req.add_header("X-GitHub-Api-Version", "2022-11-28")
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.load(r)


def pypi() -> list[list[object]]:
    recent = get(f"https://pypistats.org/api/packages/{PACKAGE}/recent")["data"]
    overall = get(f"https://pypistats.org/api/packages/{PACKAGE}/overall?mirrors=false")["data"]
    total = sum(d["downloads"] for d in overall if d["category"] == "without_mirrors")
    return [["pypi", total, recent["last_week"], "7d"],
            ["pypi", total, recent["last_month"], "30d"]]


def crates() -> list[list[object]]:
    c = get(f"https://crates.io/api/v1/crates/{PACKAGE}")["crate"]
    return [["crates-io", c["downloads"], c.get("recent_downloads") or 0, "90d"]]


def releases(repo: str) -> list[list[object]]:
    token = os.environ.get("GITHUB_TOKEN")
    total, page = 0, 1
    while True:
        batch = get(f"https://api.github.com/repos/{repo}/releases?per_page=100&page={page}", token)
        total += sum(a["download_count"] for rel in batch for a in rel["assets"])
        if len(batch) < 100:
            break
        page += 1
    return [["github-releases", total, "", ""]]


def main() -> int:
    repo = os.environ.get("GITHUB_REPOSITORY", "")
    today = dt.date.today().isoformat()
    sources = [("pypi", pypi), ("crates.io", crates)]
    if repo:
        sources.append(("releases", lambda: releases(repo)))
    rows: list[list[object]] = []
    for name, fn in sources:
        try:
            rows += [[today, *r] for r in fn()]
        except Exception as e:  # one broken source must not stop the others
            print(f"warning: {name}: {e}", file=sys.stderr)
    CSV.parent.mkdir(parents=True, exist_ok=True)
    new = not CSV.exists() or CSV.stat().st_size == 0
    # Plain lines with \n endings (the values never contain commas or quotes).
    with CSV.open("a", newline="") as f:
        for r in ([HEADER] if new else []) + rows:
            f.write(",".join(map(str, r)) + "\n")
    for r in rows:
        print(",".join(map(str, r)))
    return 0


if __name__ == "__main__":
    sys.exit(main())
