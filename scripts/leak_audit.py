#!/usr/bin/env python3
"""Model M's ownership gate: every corpus document under the ownership-check build.

The checker build (`--features ownership-check`) poisons freed node blocks and box
records and panics on a double free, a read after free or a freed span that is not the
one stored. `RT_LEAKS` then reports, per node table, the entries neither free nor
reachable at the end of the run, and the source line that made them. Zero everywhere
means every free point of tex.web is in place (`memorycastle/journal/2026-10-22-zero-gc-core-model.md`).

    scripts/leak_audit.py                 # all of scripts/parity/corpus/, one line per failure
    scripts/leak_audit.py listings cases  # some documents, with their leak sites
    scripts/leak_audit.py -j 4            # documents in parallel (default: half the cores)

Needs the format at target/parity/pdflatex.rfmt (`parity.py zero --engine` makes it).
"""

import argparse
import concurrent.futures
import glob
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CORPUS = os.path.join(ROOT, "scripts", "parity", "corpus")
FMT = os.path.join(ROOT, "target", "parity", "pdflatex.rfmt")
CHECK = ["--release", "--features", "ownership-check", "--target-dir", "target/check"]
LEAKS = re.compile(r"^LEAKS (.*)$", re.M)


def audit(doc):
    env = dict(os.environ, RT_LEAKS="1", RT_FMT_IN=FMT, RT_FORMAT=os.path.join(CORPUS, f"{doc}.tex"),
               SOURCE_DATE_EPOCH="1792454400", FORCE_SOURCE_DATE="1")
    cmd = ["cargo", "test", "-q", *CHECK, "--lib", "format_smoke", "--", "--ignored", "--nocapture"]
    r = subprocess.run(cmd, env=env, capture_output=True, text=True, cwd=ROOT)
    out = r.stdout + r.stderr
    if "panicked" in out:
        line = next(l for l in out.split("\n") if "panicked" in l or "freed" in l)
        return doc, False, f"PANIC {line.strip()}"
    m = LEAKS.search(out)
    if not m:
        return doc, False, "no LEAKS line (did the run finish?)"
    tables = m.group(1).split("  ")[0]
    counts = re.findall(r"(\w+) (-?\d+) of \d+", tables)
    bad = [(t, int(n)) for t, n in counts if int(n) != 0]
    sites = [l.strip() for l in out[m.end():].split("\n") if re.match(r"^\s+\d+ ", l)]
    return doc, not bad, ", ".join(f"{t} {n}" for t, n in bad) + ("".join(f"\n    {s}" for s in sites[:12]) if bad else "")


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("docs", nargs="*")
    ap.add_argument("-j", type=int, default=max(1, (os.cpu_count() or 2) // 2))
    a = ap.parse_args()
    docs = a.docs or sorted(os.path.splitext(os.path.basename(p))[0] for p in glob.glob(os.path.join(CORPUS, "*.tex")))
    b = subprocess.run(["cargo", "test", "-q", *CHECK, "--lib", "--no-run"], cwd=ROOT, capture_output=True, text=True)
    if b.returncode != 0:
        sys.exit(b.stderr)
    failed = 0
    with concurrent.futures.ThreadPoolExecutor(a.j) as ex:
        for doc, ok, msg in ex.map(audit, docs):
            if not ok:
                failed += 1
                print(f"{doc}: {msg}")
            elif a.docs:
                print(f"{doc}: no leaks")
    print(f"leak audit: {len(docs) - failed}/{len(docs)} documents clean")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
