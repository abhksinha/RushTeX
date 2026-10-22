#!/usr/bin/env python3
"""First difference between pdfTeX's and the engine's box tree of one shipped page.

    engine_boxdiff.py PDFTEX.log ENGINE.boxes PAGE

PDFTEX.log: a pdflatex log with `\\tracingoutput1 \\showboxdepth100 \\showboxbreadth100000`
in force (set them `\\global`ly in the document). ENGINE.boxes: the file `format_smoke`
writes with `RT_BOXES=<file>`. PAGE: pdfTeX's `[n]` label (`\\count0`); the engine's pages
are numbered in shipping order, the same while no page is unnumbered or roman.

Whatsits, `\\setlanguage`, `\\write` and `\\mark` lines, and token lists cut by `\\ETC.`, are
left out (the engine prints them differently); pdfTeX's 79-column wraps are joined.
"""
import re
import sys


def unwrap(text):
    """The page's lines, up to the blank line that ends the box. A line of exactly 79
    characters was wrapped: the next line continues it, and when it ended there anyway
    TeX printed an empty line, which is not the end."""
    out = []
    for line in text.splitlines():
        if out and len(out[-1]) == 79 and not out[-1].endswith("\0"):
            if line == "":
                out[-1] += "\0"
            else:
                out[-1] += line
            continue
        if line == "":
            break
        out.append(line)
    return [l.removesuffix("\0") for l in out]


def skip(line):
    return any(k in line for k in ("whatsit", "\\pdf", "\\write", "\\mark", "\\special", "ETC", "setlanguage"))


def main(log_path, boxes_path, page):
    log = open(log_path, encoding="latin-1").read()
    parts = re.split(r"Completed box being shipped out \[([^\]]*)\]\n", log)
    ref = {parts[i]: unwrap(parts[i + 1]) for i in range(1, len(parts), 2)}
    ours = {}
    # pdfTeX's log (TeX Live's cp227 translation) prints codes 128-255 as they are.
    high = re.compile(r"\^\^([89a-f][0-9a-f])")
    for chunk in open(boxes_path).read().split("PAGE ")[1:]:
        n, rest = chunk.split("\n", 1)
        ours[n] = [high.sub(lambda m: chr(int(m.group(1), 16)), l) for l in rest.splitlines()]
    r = [l for l in ref[page] if l.strip() and not skip(l)]
    o = [l for l in ours[page] if l.strip() and not skip(l)]
    for k, (a, b) in enumerate(zip(r, o)):
        if a != b:
            print(f"first difference at line {k}")
            for j in range(max(0, k - 12), min(k + 3, len(r))):
                print("pdfTeX ", r[j][:160])
            print()
            for j in range(max(0, k - 12), min(k + 3, len(o))):
                print("engine ", o[j][:160])
            return 1
    print(f"same: {len(r)} lines" if len(r) == len(o) else f"same prefix; {len(r)} against {len(o)} lines")
    return 0


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:4]))
