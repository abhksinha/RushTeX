#!/usr/bin/env python3
"""Run every engine probe suite in scripts/engine_probes/ against pdfTeX -ini.

Each suite is NAME.tex (run as the format file itself, all lines) with NAME.probes (one
expression per line, shown with \\immediate\\write16 after the file). A suite whose first
line is `%% typeset` runs with RT_TYPESET=1, so the engine owns the main vertical list
(page builder, output routine, shipped pages).

    scripts/engine_suites.py              # every suite, one summary line each
    scripts/engine_suites.py page hyphen  # some suites, with the differing probes
"""

import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DIR = os.path.join(ROOT, "scripts", "engine_probes")


def run(name, verbose):
    tex = os.path.join(DIR, f"{name}.tex")
    lines = open(tex, encoding="latin-1").read().split("\n")
    env = dict(os.environ)
    if lines and lines[0].strip() == "%% typeset":
        env["RT_TYPESET"] = "1"
    cmd = [sys.executable, os.path.join(ROOT, "scripts", "engine_probe.py"), tex, str(len(lines)), "--format", f"{name}.tex",
           "--exprs-file", os.path.join(DIR, f"{name}.probes"), "--quiet"]
    r = subprocess.run(cmd, env=env, capture_output=True, text=True, cwd=ROOT)
    out = [l for l in r.stdout.split("\n") if l]
    summary = out[-1] if out else (r.stderr.strip().split("\n") or ["no output"])[-1]
    print(f"{name:10} {summary}")
    if verbose and r.returncode:
        print("\n".join(out[:-1]))
    return r.returncode == 0


def main():
    names = sys.argv[1:] or sorted(f[:-4] for f in os.listdir(DIR) if f.endswith(".tex"))
    ok = [run(n, bool(sys.argv[1:])) for n in names]
    sys.exit(0 if all(ok) else 1)


if __name__ == "__main__":
    main()
