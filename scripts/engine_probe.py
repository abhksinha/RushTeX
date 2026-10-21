#!/usr/bin/env python3
"""Differential probe of the rushtex engine against pdfTeX -ini.

Truncates a TeX Live file after a given line, appends `\\immediate\\write16{[...]}` probes
(one per expression), runs the format build (latex.ltx by default) through both pdfTeX
(INITEX, e-TeX mode) and rushtex's `format_smoke` test, and prints each probe's result side
by side, marking differences.

    scripts/engine_probe.py expl3-code.tex 1892 '\\meaning\\cs_if_exist:NTF' '\\the\\catcode`\\:'
    scripts/engine_probe.py expl3-code.tex 1892 --errors      # also print rushtex's errors

The truncated copy is written to a scratch tree that both engines read before TeX Live.
"""
import argparse
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def kpse(name):
    return subprocess.run(["kpsewhich", name], capture_output=True, text=True).stdout.strip()


def oracle(tree, fmt, work):
    env = dict(os.environ, TEXINPUTS=f"{tree}//:")
    subprocess.run(["pdftex", "-ini", "-etex", "-interaction=batchmode", "-translate-file=cp227.tcx", fmt], cwd=work, env=env,
                   capture_output=True, stdin=subprocess.DEVNULL, timeout=600)
    log = open(os.path.join(work, os.path.splitext(fmt)[0] + ".log"), encoding="latin-1").read()
    # pdfTeX wraps the log at 79 columns.
    return log.replace("\n", "")


def ours(tree, fmt, errors):
    env = dict(os.environ, RT_OUT="1", RT_DIR=tree, RT_FORMAT=fmt)
    r = subprocess.run(["cargo", "test", "--release", "--lib", "format_smoke", "--", "--ignored",
                        "--nocapture"], cwd=ROOT, env=env, capture_output=True, text=True)
    text = r.stderr + r.stdout
    if errors:
        for line in text.splitlines():
            if line.startswith("  l.") or ": " in line and "out tokens" in line:
                print(line[:200])
    log = text.split("LOG: ", 1)[1] if "LOG: " in text else ""
    return log.replace("\n", "")


def probes(log, n):
    found = re.findall(r"\[!!(\d+)!!(.*?)!!\]", log)
    out = [None] * n
    for i, v in found:
        out[int(i)] = v
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("file", help="TeX Live file to truncate, e.g. expl3-code.tex, or a path to a file of your own")
    ap.add_argument("line", type=int, help="keep lines 1..LINE")
    ap.add_argument("expr", nargs="*", help="TeX material to show, inside \\immediate\\write16")
    ap.add_argument("--format", default="latex.ltx")
    ap.add_argument("--errors", action="store_true", help="print rushtex's errors")
    ap.add_argument("--code", action="append", default=[], help="TeX lines to run before the probes")
    ap.add_argument("--exprs-file", help="more probes, one per line")
    ap.add_argument("--quiet", action="store_true", help="print only the probes that differ")
    a = ap.parse_args()

    if a.exprs_file:
        a.expr += [l for l in open(a.exprs_file, encoding="latin-1").read().split("\n") if l]
    src = open(a.file if os.path.isfile(a.file) else kpse(a.file), encoding="latin-1").read().split("\n")[: a.line]
    lines = [f"\\immediate\\write16{{[!!{i}!!{e}!!]}}" for i, e in enumerate(a.expr)]
    with tempfile.TemporaryDirectory() as tree, tempfile.TemporaryDirectory() as work:
        with open(os.path.join(tree, os.path.basename(a.file)), "w", encoding="latin-1") as f:
            f.write("\n".join(src + a.code + lines + ["\\endinput", ""]))
        theirs = probes(oracle(tree, a.format, work), len(a.expr))
        mine = probes(ours(tree, a.format, a.errors), len(a.expr))
    bad = 0
    for e, t, m in zip(a.expr, theirs, mine):
        same = t == m
        bad += not same
        if not (a.quiet and same):
            print(f"{'  ' if same else '!!'} {e}\n   pdftex:  {t}\n   rushtex: {m}")
    print(f"{len(a.expr) - bad}/{len(a.expr)} probes agree")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
