#!/usr/bin/env python3
"""Builds data/related/xrefs.tsv.zst — the verse-to-verse cross references.

The source is OpenBible.info's cross-reference set (https://www.openbible.info/labs/cross-references/),
which draws mostly on the public-domain Treasury of Scripture Knowledge and ranks each link by the
votes of the site's own readers. Licence: CC BY 4.0, credited in the app's About panel and at the
foot of the related-verses card.

The set is 344,799 links over 29,364 verses — a median of ten per verse, up to ninety-seven. The card
shows a handful rather than a wall, so only the best-voted N per verse are bundled: about 0.9 MB
compressed against 5.2 MB for the lot.

    scripts/build-related.py            # top 8 per verse
    scripts/build-related.py --top 10
"""
from __future__ import annotations

import argparse
import collections
import os
import re
import subprocess
import sys
import urllib.request
import zipfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SOURCE = "https://a.openbible.info/data/cross-references.zip"
BOOKS_RS = os.path.join(ROOT, "crates", "core", "src", "books.rs")
OUT_DIR = os.path.join(ROOT, "data", "related")
OUT = os.path.join(OUT_DIR, "xrefs.tsv.zst")

CREDIT = (
    "# Related verses: OpenBible.info cross references, CC BY 4.0\n"
    "# https://www.openbible.info/labs/cross-references/\n"
    "# Mostly the Treasury of Scripture Knowledge, ranked by the site's readers' votes.\n"
    "# One @CODE chapter verse header per verse, then: votes, book, chapter, verse, end chapter, end verse\n"
    "# (the last two are 0 when the link is a single verse).\n"
)


def codes() -> dict[str, str]:
    """{OSIS name: bible-core code} straight from books.rs, so the two cannot drift."""
    text = open(BOOKS_RS, encoding="utf-8").read()
    pairs = re.findall(r'book!\(\s*"([A-Z0-9]{3})",\s*"([^"]+)"', text)
    if len(pairs) != 66:
        sys.exit(f"expected 66 books in books.rs, found {len(pairs)}")
    return {osis.lower(): code for code, osis in pairs}


def fetch(cache: str) -> str:
    path = os.path.join(cache, "cross-references.zip")
    if not os.path.exists(path):
        print(f"  downloading {SOURCE}")
        urllib.request.urlretrieve(SOURCE, path)
    with zipfile.ZipFile(path) as z:
        name = next(n for n in z.namelist() if n.endswith(".txt"))
        return z.read(name).decode("utf-8")


def ref(text: str, by_osis: dict[str, str]) -> tuple[str, int, int]:
    """'1John.4.10' → ('1JN', 4, 10)."""
    parts = text.strip().split(".")
    if len(parts) != 3:
        raise ValueError(text)
    code = by_osis.get(parts[0].lower())
    if code is None:
        raise KeyError(parts[0])
    return code, int(parts[1]), int(parts[2])


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("--top", type=int, default=8, help="links kept per verse (default 8)")
    ap.add_argument("--out", default=OUT)
    args = ap.parse_args()

    by_osis = codes()
    text = fetch(os.environ.get("XDG_CACHE_HOME", "/tmp"))
    links: dict[tuple[str, int, int], list[tuple[int, str, int, int, int, int]]] = collections.defaultdict(list)
    rows = 0
    for line in text.splitlines():
        if not line or line.startswith("From Verse"):
            continue
        parts = line.split("\t")
        if len(parts) < 3:
            continue
        try:
            votes = int(parts[2])
            # A link the site's readers voted down is not one to offer: 1% of the set, and for 43
            # verses it is every link they have, which is the honest answer for those verses.
            if votes <= 0:
                continue
            book, chapter, verse = ref(parts[0], by_osis)
            ends = parts[1].split("-")
            to = ref(ends[0], by_osis)
            end = ref(ends[-1], by_osis) if len(ends) > 1 else None
        except (ValueError, KeyError):
            continue
        end_chapter, end_verse = (end[1], end[2]) if end and end != to else (0, 0)
        links[(book, chapter, verse)].append((votes, to[0], to[1], to[2], end_chapter, end_verse))
        rows += 1

    kept = 0
    os.makedirs(os.path.dirname(args.out), exist_ok=True)
    tmp = args.out.removesuffix(".zst") + ".tmp"
    with open(tmp, "w", encoding="utf-8") as out:
        out.write(CREDIT)
        for key in sorted(links, key=lambda k: k):
            book, chapter, verse = key
            out.write(f"@{book}\t{chapter}\t{verse}\n")
            for votes, to_book, to_chapter, to_verse, end_chapter, end_verse in sorted(
                links[key], reverse=True
            )[: args.top]:
                out.write(f"{votes}\t{to_book}\t{to_chapter}\t{to_verse}\t{end_chapter}\t{end_verse}\n")
                kept += 1
    subprocess.run(["zstd", "-q", "-f", "-19", tmp, "-o", args.out], check=True)
    raw = os.path.getsize(tmp)
    packed = os.path.getsize(args.out)
    os.remove(tmp)
    print(f"  {rows:,} links read, {len(links):,} verses, {kept:,} kept (top {args.top})")
    print(f"  {args.out}: {raw/1e6:.2f} MB → {packed/1e6:.2f} MB")


if __name__ == "__main__":
    main()
