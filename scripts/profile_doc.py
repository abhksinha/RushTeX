#!/usr/bin/env python3
"""Where `rushtex doc.tex` spends its time: self time per function, from the OS sampler.

    python3 scripts/profile_doc.py scripts/parity/corpus/listings.tex [-n 30] [--callers FUNC]

macOS uses `sample` (1 ms), Linux `perf record -g`. Self time is the function holding the
program counter, inlined callees included: the release build inlines most small helpers
into their callers, so a hot `get_token` includes the input stack and the lexer it inlines.
In-process samplers (pprof) attribute inlined frames unreliably on macOS and are not used.
With --callers, the call paths that reach FUNC, by samples.
"""
import argparse
import collections
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BIN = os.path.join(ROOT, "target", "release", "rushtex")


def demangle(name):
    """Short readable name of a v0-mangled Rust symbol: its last two identifiers."""
    if not name.startswith("_R"):
        return name
    name = re.sub(r"Cs[0-9A-Za-z]+_|B[0-9A-Za-z]*_", "", name)  # crate hashes, back references
    words, i = [], 0
    while i < len(name):
        m = re.compile(r"(\d+)_?").match(name, i)
        if not m:
            i += 1
            continue
        n, j = int(m.group(1)), m.end()
        if n and j + n <= len(name):
            words.append(name[j:j + n])
            i = j + n
        else:
            i = j
    words = [w for w in words if w not in ("rushtex", "engine", "core", "alloc", "std")]
    return "::".join(words[-2:]) if words else name


def run_sample(doc, out_pdf, env):
    with tempfile.NamedTemporaryFile(suffix=".sample", delete=False) as f:
        report = f.name
    p = subprocess.Popen([BIN, doc, "-o", out_pdf], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    subprocess.run(["sample", str(p.pid), "600", "1", "-mayDie", "-file", report], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    p.wait()
    text = open(report).read()
    os.unlink(report)
    return text


def self_times(text):
    top = collections.Counter()
    m = re.search(r"Sort by top of stack.*?\n(.*?)(\n\n|\Z)", text, re.S)
    if m:
        for line in m.group(1).splitlines():
            parts = line.strip().rsplit(None, 1)
            if len(parts) == 2 and parts[1].isdigit():
                top[demangle(parts[0].split("  (in ")[0].strip())] += int(parts[1])
    return top


def callers(text, func):
    """Call paths to `func` from the call graph section: indentation is depth."""
    paths = collections.Counter()
    stack = []
    for line in text.split("Sort by top of stack")[0].splitlines():
        m = re.match(r"^(\s*[+!:| ]*)(\d+)\s+(\S+)", line)
        if not m:
            continue
        depth, n, name = len(m.group(1)), int(m.group(2)), demangle(m.group(3))
        while stack and stack[-1][0] >= depth:
            stack.pop()
        if func in name:
            paths[" < ".join(s[1] for s in reversed(stack[-4:]))] += n
        stack.append((depth, name))
    return paths


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("doc")
    ap.add_argument("-n", type=int, default=30)
    ap.add_argument("--callers")
    a = ap.parse_args()
    if sys.platform != "darwin":
        sys.exit("only macOS `sample` is wired; on Linux run: perf record -g target/release/rushtex DOC")
    env = dict(os.environ, SOURCE_DATE_EPOCH=os.environ.get("SOURCE_DATE_EPOCH", "1792454400"), FORCE_SOURCE_DATE="1")
    with tempfile.TemporaryDirectory() as d:
        text = run_sample(os.path.abspath(a.doc), os.path.join(d, "out.pdf"), env)
    top = self_times(text)
    total = sum(top.values()) or 1
    print(f"{total} samples (1 ms each)")
    for name, n in top.most_common(a.n):
        print(f"{100 * n / total:5.1f}%  {n:6}  {name}")
    if a.callers:
        print(f"\ncallers of {a.callers}:")
        for path, n in callers(text, a.callers).most_common(15):
            print(f"{n:6}  {path}")


if __name__ == "__main__":
    main()
