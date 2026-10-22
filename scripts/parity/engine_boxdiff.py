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
    out = []
    for line in text.splitlines():
        if out and len(out[-1]) == 79:
            out[-1] += line
        else:
            out.append(line)
    return out


def skip(line):
    return any(k in line for k in ("whatsit", "\\pdf", "\\write", "\\mark", "\\special", "ETC", "setlanguage"))


def main(log_path, boxes_path, page):
    log = open(log_path, encoding="latin-1").read()
    parts = re.split(r"Completed box being shipped out \[([^\]]*)\]\n", log)
    ref = {parts[i]: unwrap(parts[i + 1].split("\n\n")[0]) for i in range(1, len(parts), 2)}
    ours = {}
    for chunk in open(boxes_path).read().split("PAGE ")[1:]:
        n, rest = chunk.split("\n", 1)
        ours[n] = rest.splitlines()
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
