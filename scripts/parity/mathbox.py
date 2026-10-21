#!/usr/bin/env python3
"""mathbox.py FILE|-: math box oracle. Each input line is `D|T|S|SS<TAB>formula`. Sets every
formula in an hbox with pdfLaTeX (article, 10pt, amsmath) and with rushtex
(`examples/math_box.rs`), and prints width, height and depth in scaled points with the
differences, so a math rule can be checked against TeX exactly, formula by formula.
"""
import os, re, subprocess, sys, tempfile

STYLE = {"D": r"\displaystyle", "T": r"\textstyle", "S": r"\scriptstyle", "SS": r"\scriptscriptstyle"}
root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
rows = [l.rstrip("\n").split("\t", 1) for l in (sys.stdin if sys.argv[1:] in ([], ["-"]) else open(sys.argv[1])) if "\t" in l]

body = "".join(
    rf"\setbox0\hbox{{${STYLE[s]} {f}$}}\typeout{{MBOX {i}:\number\wd0,\number\ht0,\number\dp0}}" "\n"
    for i, (s, f) in enumerate(rows))
doc = "\\documentclass{article}\\usepackage{amsmath,amssymb}\\begin{document}\n" + body + "\\end{document}\n"
with tempfile.TemporaryDirectory() as d:
    open(os.path.join(d, "m.tex"), "w").write(doc)
    log = subprocess.run(["pdflatex", "-interaction=nonstopmode", "m.tex"], cwd=d, capture_output=True, text=True, encoding="latin1").stdout
# Formulas TeX cannot set (undefined macros) are skipped, matched by index.
tex = {int(m.group(1)): tuple(map(int, m.groups()[1:])) for m in re.finditer(r"MBOX (\d+):(-?\d+),(-?\d+),(-?\d+)", log)}
ours = subprocess.run(["cargo", "run", "-q", "--release", "--example", "math_box"], cwd=root,
                      input="".join(f"{s}\t{f}\n" for s, f in rows), capture_output=True, text=True).stdout.split("\n")
bad = 0
rows_all, ours_all = rows, ours
rows = [r for i, r in enumerate(rows_all) if i in tex]
ours = [o for i, o in enumerate(ours_all) if i in tex]
for (s, f), t, o in zip(rows, [tex[i] for i in sorted(tex)], ours):
    o = tuple(map(int, o.split())) if o and o != "error" else (0, 0, 0)
    diff = tuple(a - b for a, b in zip(o, t))
    bad += diff != (0, 0, 0)
    print(f"{'ok ' if diff == (0, 0, 0) else 'BAD'} {s:2} {f:40} tex w{t[0]} h{t[1]} d{t[2]}  ours-tex {diff[0]:+} {diff[1]:+} {diff[2]:+}")
if len(rows) < len(rows_all):
    print(f"skipped {len(rows_all) - len(rows)} formulas pdflatex could not set")
print(f"{len(rows) - bad}/{len(rows)} exact")
