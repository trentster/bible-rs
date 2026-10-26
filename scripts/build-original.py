#!/usr/bin/env python3
"""Build the bundled original-language data from STEPBible's tagged texts.

Source: STEPBible.org "Translators Amalgamated" Hebrew OT and Greek NT, plus the
Translators Brief lexicons, all CC BY 4.0 (https://creativecommons.org/licenses/by/4.0/).
The tags say, for every word of the original text, which Strong's number it carries,
so the reader can see the Hebrew or Greek behind an English verse.

Attribution is shown in the app wherever this data appears: "STEPBible.org · CC BY 4.0".

Usage:  python3 scripts/build-original.py          # rebuild data/original/*.tsv.zst
Inputs are cached in .cache/original/ so repeat runs do not re-download 97 MB.
"""

import collections
import hashlib
import io
import os
import re
import subprocess
import sys
import urllib.parse
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CACHE = os.path.join(ROOT, ".cache", "original")
OUT = os.path.join(ROOT, "data", "original")
BASE = "https://raw.githubusercontent.com/STEPBible/STEPBible-Data/master"
OT = "Translators Amalgamated OT+NT/TAHOT {} - Translators Amalgamated Hebrew OT - STEPBible.org CC BY.txt"
NT = "Translators Amalgamated OT+NT/TAGNT {} - Translators Amalgamated Greek NT - STEPBible.org CC-BY.txt"
LEX = "Lexicons/{} - Translators Brief lexicon of Extended Strongs for {} - STEPBible.org CC BY.txt"

SOURCES = [
    ("ot-gen-deu", OT.format("Gen-Deu")),
    ("ot-jos-est", OT.format("Jos-Est")),
    ("ot-job-sng", OT.format("Job-Sng")),
    ("ot-isa-mal", OT.format("Isa-Mal")),
    ("nt-mat-jhn", NT.format("Mat-Jhn")),
    ("nt-act-rev", NT.format("Act-Rev")),
    ("lex-h", LEX.format("TBESH", "Hebrew")),
    ("lex-g", LEX.format("TBESG", "Greek")),
]

# STEPBible's book abbreviations → the codes bible-core uses.
BOOKS = {
    "Gen": "GEN", "Exo": "EXO", "Lev": "LEV", "Num": "NUM", "Deu": "DEU", "Jos": "JOS",
    "Jdg": "JDG", "Rut": "RUT", "1Sa": "1SA", "2Sa": "2SA", "1Ki": "1KI", "2Ki": "2KI",
    "1Ch": "1CH", "2Ch": "2CH", "Ezr": "EZR", "Neh": "NEH", "Est": "EST", "Job": "JOB",
    "Psa": "PSA", "Pro": "PRO", "Ecc": "ECC", "Sng": "SNG", "Isa": "ISA", "Jer": "JER",
    "Lam": "LAM", "Ezk": "EZK", "Dan": "DAN", "Hos": "HOS", "Jol": "JOL", "Amo": "AMO",
    "Oba": "OBA", "Jon": "JON", "Mic": "MIC", "Nam": "NAM", "Hab": "HAB", "Zep": "ZEP",
    "Hag": "HAG", "Zec": "ZEC", "Mal": "MAL", "Mat": "MAT", "Mrk": "MRK", "Luk": "LUK",
    "Jhn": "JHN", "Act": "ACT", "Rom": "ROM", "1Co": "1CO", "2Co": "2CO", "Gal": "GAL",
    "Eph": "EPH", "Php": "PHP", "Col": "COL", "1Th": "1TH", "2Th": "2TH", "1Ti": "1TI",
    "2Ti": "2TI", "Tit": "TIT", "Phm": "PHM", "Heb": "HEB", "Jas": "JAS", "1Pe": "1PE",
    "2Pe": "2PE", "1Jn": "1JN", "2Jn": "2JN", "3Jn": "3JN", "Jud": "JUD", "Rev": "REV",
}

# "Gen.1.1#01=L", and "Psa.3.1(3.2)#01=L" where the first number is the English verse
# and the bracketed one the Hebrew, and "Psa.3.0(3.1)" is a title before verse one.
REF = re.compile(r"^([1-3]?[A-Za-z]{2,3})\.(\d+)\.(\d+)(?:\([0-9.]*\))?#(\d+)")


def fetch(name, path, attempts=3):
    os.makedirs(CACHE, exist_ok=True)
    dst = os.path.join(CACHE, name)
    if os.path.exists(dst) and os.path.getsize(dst) > 1000:
        return dst
    url = f"{BASE}/{urllib.parse.quote(path)}"
    for i in range(attempts):
        try:
            print(f"  fetching {name} …", flush=True)
            urllib.request.urlretrieve(url, dst)
            return dst
        except Exception as e:  # noqa: BLE001 - a network stutter should not kill a rebuild
            if i == attempts - 1:
                sys.exit(f"download failed for {name}: {e}")
    return dst


# STEPBible escapes the marks it keeps for its own use with a backslash, so "\u05c3" style
# sequences like the sof pasuq arrive as "\\\u05c3" and must come off the displayed word.
ESCAPE = re.compile(r"\\.", re.S)


def clean_hebrew(surface, translit):
    """Prefix morphemes are joined with "/", and STEP's escaped marks with a backslash."""
    surface = ESCAPE.sub("", surface.replace("/", "")).strip()
    translit = ESCAPE.sub("", translit.replace(".", "").replace("/", ""))
    return surface, translit


def gloss(text):
    """STEP joins a prefix's meaning to its stem: "in/ beginning" → "in beginning"."""
    return " ".join(text.replace("/", " ").split())


def parse_words():
    """{(code, chapter, verse): [(surface, translit, gloss, strong), …]} in written order."""
    verses = collections.defaultdict(list)
    titles = collections.defaultdict(list)
    for name, path in SOURCES:
        if not (name.startswith("ot-") or name.startswith("nt-")):
            continue
        greek = name.startswith("nt-")
        with open(fetch(name, path), encoding="utf-8") as fh:
            for line in fh:
                m = REF.match(line)
                if not m:
                    continue
                parts = line.rstrip("\n").split("\t")
                if len(parts) < 5:
                    continue
                code = BOOKS[m.group(1)]
                chapter, verse = int(m.group(2)), int(m.group(3))
                if greek:
                    word = parts[1]
                    surface, _, translit = word.partition("(")
                    surface, translit = surface.strip(), translit.rstrip(")").strip()
                    g = gloss(parts[2])
                    strong = parts[3].split("=")[0]
                    aramaic = False
                else:
                    surface, translit = clean_hebrew(parts[1], parts[2])
                    g = gloss(parts[3])
                    numbers = [n for n in re.sub(r"[{}]", "", parts[4].split("\\")[0]).split("/")
                               if n[:1] == "H"]
                    strong = numbers[-1] if numbers else ""
                    aramaic = parts[5].startswith("A")
                if verse == 0:
                    # A superscription: the KJV prints it inside verse one.
                    titles[(code, chapter, 1)].append((surface, translit, g, strong, aramaic))
                else:
                    verses[(code, chapter, verse)].append((surface, translit, g, strong, aramaic))
    for key, words in titles.items():
        verses[key] = words + verses.get(key, [])
    return verses


NT_CODES = {"MAT", "MRK", "LUK", "JHN", "ACT", "ROM", "1CO", "2CO", "GAL", "EPH", "PHP",
            "COL", "1TH", "2TH", "1TI", "2TI", "TIT", "PHM", "HEB", "JAS", "1PE", "2PE",
            "1JN", "2JN", "3JN", "JUD", "REV"}


def write_words(verses):
    out = io.StringIO()
    out.write("# STEPBible.org TAHOT + TAGNT, CC BY 4.0 — original-language words, KJV verse numbering\n")
    for (code, chapter, verse) in sorted(verses, key=lambda k: (BOOK_ORDER[k[0]], k[1], k[2])):
        words = verses[(code, chapter, verse)]
        lang = "G" if code in NT_CODES else ("A" if all(w[4] for w in words) else "H")
        out.write(f"@{code}\t{chapter}\t{verse}\t{len(words)}\t{lang}\n")
        for surface, translit, g, strong, _ in words:
            out.write(f"{surface}\t{translit}\t{g}\t{strong}\n")
    return out.getvalue()


def write_lexicon(name):
    path = dict(SOURCES)[name]
    out = io.StringIO()
    out.write("# STEPBible.org TBES, CC BY 4.0\n")
    count = 0
    with open(fetch(name, path), encoding="utf-8") as fh:
        for line in fh:
            p = line.rstrip("\n").split("\t")
            if len(p) < 8 or p[0][:1] not in "HG" or not p[0][1:2].isdigit():
                continue
            # Column 2 reads "H0430G = a Name of": this entry's id is its first token. Column 3 is a
            # cross-reference to a related entry, not this one.
            ident = (p[1].split()[0] if p[1].strip() else p[0]).strip()
            lemma, translit, pos, short, body = p[3], p[4], p[5], p[6], p[7]
            # Definitions carry <br> markers; they stay so one entry is always one line, and the
            # reader splits on them when it lays the entry out.
            body = body.replace("\t", " ").replace("\r", " ")
            out.write(f"{ident}\t{lemma}\t{translit}\t{pos}\t{short}\t{body}\n")
            count += 1
    return out.getvalue(), count


def compress(text, path):
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as fh:
        fh.write(text)
    subprocess.run(["zstd", "-q", "-f", "-19", tmp, "-o", path], check=True)
    os.remove(tmp)
    data = open(path, "rb").read()
    print(f"  {os.path.basename(path):20s} {len(text.encode())/1e6:6.2f} MB → {len(data)/1e6:5.2f} MB"
          f"  sha256 {hashlib.sha256(data).hexdigest()[:16]}")
    return len(data)


def main():
    global BOOK_ORDER
    BOOK_ORDER = {code: i for i, code in enumerate(BOOKS.values())}
    os.makedirs(OUT, exist_ok=True)
    for name, path in SOURCES:
        fetch(name, path)

    print("packing words …")
    verses = parse_words()
    words = sum(len(v) for v in verses.values())
    total = compress(write_words(verses), os.path.join(OUT, "words.tsv.zst"))
    print(f"  {len(verses):,} verses, {words:,} words")

    for name, out in (("lex-h", "lex-h"), ("lex-g", "lex-g")):
        print(f"packing {out} …")
        text, n = write_lexicon(name)
        total += compress(text, os.path.join(OUT, f"{out}.tsv.zst"))
        print(f"  {n:,} entries")

    print(f"\nadded {total/1e6:.2f} MB compressed")


if __name__ == "__main__":
    main()
