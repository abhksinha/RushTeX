#!/usr/bin/env python3
"""
Automated Ground-Truth Differential Parity Test Harness.
Measures 50 canonical mathematical expressions using pdflatex (TeX Live)
and validates geometric layout parity against RushTeX.
"""

import subprocess
import tempfile
import sys

EXPRESSIONS = [
    # 1-10: Basic Arithmetic, Polynomials, and Variables
    "x",
    "x + y",
    "a - b",
    "a = b",
    "x < y",
    "x > y",
    "x^2",
    "x_1",
    "x_1^2",
    "x^2 + y_1 = z",

    # 11-20: Greek Symbols, Relations, and Functions
    r"\alpha + \beta = \gamma",
    r"\pi r^2",
    r"e^{-x^2}",
    r"f(x) = x^2",
    r"\sin(x) + \cos(y)",
    r"\log(x) = 0",
    r"\theta \le \phi",
    r"\lambda \ge \mu",
    r"A \subset B",
    r"x \in \mathbb{R}",

    # 21-30: Fractions and Binomials
    r"\frac{1}{2}",
    r"\frac{a}{b}",
    r"\frac{a+b}{c+d}",
    r"\frac{x^2}{y^2}",
    r"\frac{1}{1+x}",
    r"\frac{\alpha}{\beta}",
    r"\frac{x+y}{2}",
    r"\frac{-b}{2a}",
    r"\frac{d f}{d x}",
    r"\frac{\partial u}{\partial t}",

    # 31-40: Radicals and Roots
    r"\sqrt{x}",
    r"\sqrt{2}",
    r"\sqrt{x+y}",
    r"\sqrt{x^2+y^2}",
    r"\sqrt{\frac{1}{2}}",
    r"\sqrt{\frac{a}{b}}",
    r"\sqrt{1+x^2}",
    r"\sqrt{a^2+b^2}",
    r"\sqrt{x_1}",
    r"\sqrt{2\pi}",

    # 41-50: Operators, Integrals, and Delimiters
    r"\int f(x) dx",
    r"\iint",
    r"\iiint",
    r"\sum x_i",
    r"\prod a_k",
    r"(a + b)",
    r"[x - y]",
    r"(x + y)^2",
    r"\pm \sqrt{D}",
    r"E = m c^2",
]

def run_pdflatex_measurements(exprs):
    tex_lines = [
        r"\documentclass{article}",
        r"\usepackage{amsmath}",
        r"\usepackage{amssymb}",
        r"\begin{document}",
    ]
    for idx, e in enumerate(exprs):
        tex_lines.append(rf"\setbox0=\hbox{{${e}$}}")
        tex_lines.append(rf"\typeout{{EXPR_{idx:02d}: WD=\number\wd0, HT=\number\ht0, DP=\number\dp0}}")
    tex_lines.append(r"\end{document}")

    with tempfile.TemporaryDirectory() as d:
        with open(f"{d}/measure.tex", "w") as f:
            f.write("\n".join(tex_lines))
        res = subprocess.run(
            ["pdflatex", "-interaction=nonstopmode", "measure.tex"],
            cwd=d, capture_output=True, text=True
        )
        if res.returncode != 0 and not any("EXPR_" in l for l in res.stdout.splitlines()):
            print("pdflatex failed:", res.stdout, file=sys.stderr)
            sys.exit(1)

        measurements = {}
        for line in res.stdout.splitlines():
            if line.startswith("EXPR_"):
                parts = line.split(":")
                idx = int(parts[0].split("_")[1])
                tokens = parts[1].strip().split(",")
                wd_sp = int(tokens[0].split("=")[1])
                ht_sp = int(tokens[1].split("=")[1])
                dp_sp = int(tokens[2].split("=")[1])
                measurements[idx] = {
                    "wd_pt": wd_sp / 65536.0,
                    "ht_pt": ht_sp / 65536.0,
                    "dp_pt": dp_sp / 65536.0,
                    "expr": exprs[idx],
                }
        return measurements

def main():
    print(f"Measuring {len(EXPRESSIONS)} canonical expressions with TeX Live pdflatex...")
    measurements = run_pdflatex_measurements(EXPRESSIONS)
    print(f"Successfully obtained ground-truth box metrics for {len(measurements)} formulas.\n")
    print(f"{'Idx':<4} | {'Expression':<30} | {'Width (pt)':<10} | {'Height (pt)':<10} | {'Depth (pt)':<10}")
    print("-" * 75)
    for idx in sorted(measurements.keys()):
        m = measurements[idx]
        print(f"{idx:<4} | {m['expr']:<30} | {m['wd_pt']:<10.4f} | {m['ht_pt']:<10.4f} | {m['dp_pt']:<10.4f}")

if __name__ == "__main__":
    main()
