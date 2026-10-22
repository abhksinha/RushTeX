#!/usr/bin/env python3
r"""Benchmark the `rushtex` command against pdfLaTeX over the parity corpus.

  bench_corpus.py [DOC..] [--repeat N] [--csv FILE]

Each document is compiled as a user compiles it, from no auxiliary files, until the files it
reads back stop changing:
  - rushtex: one `rushtex doc.tex` (the TeX engine on the LaTeX format; it runs the document
    again internally while its `.aux`/`.toc` change, at most 5 runs);
  - pdflatex: `pdflatex` again while `.aux`, `.toc`, `.lof`, `.lot`, `.out` change, at most 5
    runs (latexmk's rule, as scripts/parity/parity.py makes the references).

Measured from each process's own resource usage (`wait4`): wall time, user and system CPU
(summed over pdflatex's runs) and peak resident memory (the largest run). With `--repeat N`
each tool is timed N times and the fastest wall time kept (CPU and memory from that run).
The fixed SOURCE_DATE_EPOCH of the parity harness is used, so both tools typeset the same
date. The format (`target/parity/pdflatex.rfmt`) and the file-search cache must exist (run
`parity.py zero --engine` once).
"""
import argparse, os, shutil, subprocess, sys, tempfile, time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CORPUS = ROOT / "scripts" / "parity" / "corpus"
OUT = ROOT / "target" / "parity"
FMT = OUT / "pdflatex.rfmt"
RUSHTEX = ROOT / "target" / "release" / "rushtex"
ENV = {**os.environ, "SOURCE_DATE_EPOCH": "1792454400", "FORCE_SOURCE_DATE": "1", "RUSHTEX_CACHE": str(OUT)}
AUX = (".aux", ".toc", ".lof", ".lot", ".out")
# ru_maxrss is bytes on macOS, kilobytes on Linux.
RSS_UNIT = 1 if sys.platform == "darwin" else 1024


def run(cmd, cwd):
    """(wall s, user s, sys s, peak RSS bytes, stderr) of one process."""
    t = time.perf_counter()
    p = subprocess.Popen(cmd, cwd=cwd, env=ENV, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
    err = p.stderr.read()
    _, _, ru = os.wait4(p.pid, 0)
    wall = time.perf_counter() - t
    return wall, ru.ru_utime, ru.ru_stime, ru.ru_maxrss * RSS_UNIT, err.decode("latin-1")


def bench_rushtex(tex, tmp):
    pdf = Path(tmp) / (tex.stem + ".pdf")
    wall, user, sys_, rss, err = run([str(RUSHTEX), "--fmt", str(FMT), str(tex), "-o", str(pdf)], ROOT)
    runs = next((int(l.split("(")[-1].split()[0]) for l in err.splitlines() if l.startswith("[DONE]")), 0)
    return {"wall": wall, "user": user, "sys": sys_, "rss": rss, "runs": runs, "ok": pdf.exists()}


def bench_pdflatex(tex, tmp):
    out = Path(tmp)
    aux = [out / (tex.stem + e) for e in AUX]
    state = lambda: [f.read_bytes() if f.exists() else None for f in aux]
    tot = {"wall": 0.0, "user": 0.0, "sys": 0.0, "rss": 0, "runs": 0}
    for _ in range(5):
        before = state()
        wall, user, sys_, rss, _ = run(["pdflatex", "-interaction=nonstopmode", f"-output-directory={out}", str(tex)], tex.parent)
        tot["wall"] += wall
        tot["user"] += user
        tot["sys"] += sys_
        tot["rss"] = max(tot["rss"], rss)
        tot["runs"] += 1
        if state() == before:
            break
    tot["ok"] = (out / (tex.stem + ".pdf")).exists()
    return tot


def best(fn, tex, repeat):
    """The fastest of `repeat` timings, each from a fresh directory (no auxiliary files)."""
    res = None
    for _ in range(repeat):
        tmp = tempfile.mkdtemp(prefix="bench-")
        try:
            r = fn(tex, tmp)
        finally:
            shutil.rmtree(tmp, ignore_errors=True)
        if res is None or r["wall"] < res["wall"]:
            res = r
    return res


def pages(tex):
    ref = OUT / "ref" / (tex.stem + ".pdf")
    try:
        out = subprocess.run(["pdfinfo", str(ref)], capture_output=True, text=True).stdout
        return int(next(l.split()[1] for l in out.splitlines() if l.startswith("Pages:")))
    except Exception:
        return 0


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("docs", nargs="*")
    ap.add_argument("--repeat", type=int, default=1)
    ap.add_argument("--csv", type=Path, default=OUT / "bench_corpus.csv")
    a = ap.parse_args()
    if not FMT.exists():
        sys.exit(f"bench: no format at {FMT}; run `uv run scripts/parity/parity.py zero --engine small2e` first")
    subprocess.run(["cargo", "build", "--release", "--quiet"], cwd=ROOT, check=True)
    docs = [CORPUS / f"{d}.tex" for d in a.docs] if a.docs else sorted(CORPUS.glob("*.tex"))
    rows = []
    hdr = f"{'document':<18}{'pages':>6} | {'runs':>4} {'wall s':>7} {'cpu s':>7} {'RSS MB':>7} | {'runs':>4} {'wall s':>7} {'cpu s':>7} {'RSS MB':>7} | {'speed':>6} {'RAM':>6}"
    print(f"{'':<24} | {'rushtex':^29} | {'pdflatex':^29} |")
    print(hdr)
    print("-" * len(hdr))
    for tex in docs:
        r, p = best(bench_rushtex, tex, a.repeat), best(bench_pdflatex, tex, a.repeat)
        n = pages(tex)
        rows.append((tex.stem, n, r, p))
        rc, pc = r["user"] + r["sys"], p["user"] + p["sys"]
        flag = "" if r["ok"] and p["ok"] else "  (no PDF)"
        print(f"{tex.stem[:18]:<18}{n:>6} | {r['runs']:>4} {r['wall']:>7.3f} {rc:>7.3f} {r['rss'] / 2**20:>7.1f} | {p['runs']:>4} {p['wall']:>7.3f} {pc:>7.3f} {p['rss'] / 2**20:>7.1f} | {p['wall'] / r['wall']:>5.1f}x {r['rss'] / p['rss']:>5.2f}x{flag}", flush=True)
    s = lambda k, w: sum(x[w][k] for x in rows)
    rw, pw = s("wall", 2), s("wall", 3)
    rc, pc = s("user", 2) + s("sys", 2), s("user", 3) + s("sys", 3)
    rr = sorted(x[2]["rss"] for x in rows)
    pr = sorted(x[3]["rss"] for x in rows)
    med = lambda v: v[len(v) // 2] / 2**20
    print("-" * len(hdr))
    print(f"{'total':<18}{sum(x[1] for x in rows):>6} | {int(s('runs', 2)):>4} {rw:>7.2f} {rc:>7.2f} {'':>7} | {int(s('runs', 3)):>4} {pw:>7.2f} {pc:>7.2f} {'':>7} | {pw / rw:>5.1f}x")
    print(f"\n{len(rows)} documents, {sum(x[1] for x in rows)} pages")
    print(f"wall time   rushtex {rw:8.2f} s   pdflatex {pw:8.2f} s   rushtex {pw / rw:.1f}x faster")
    print(f"CPU time    rushtex {rc:8.2f} s   pdflatex {pc:8.2f} s   rushtex uses {rc / pc:.0%} of pdflatex's CPU")
    print(f"  user/sys  rushtex {s('user', 2):.2f}/{s('sys', 2):.2f} s   pdflatex {s('user', 3):.2f}/{s('sys', 3):.2f} s")
    print(f"peak RSS    rushtex median {med(rr):.1f} MB, max {rr[-1] / 2**20:.1f} MB   pdflatex median {med(pr):.1f} MB, max {pr[-1] / 2**20:.1f} MB")
    print(f"            rushtex uses less RAM on {sum(x[2]['rss'] < x[3]['rss'] for x in rows)} of {len(rows)} documents")
    print(f"runs        rushtex {int(s('runs', 2))}   pdflatex {int(s('runs', 3))}")
    a.csv.parent.mkdir(parents=True, exist_ok=True)
    with open(a.csv, "w") as f:
        f.write("document,pages,rushtex_runs,rushtex_wall_s,rushtex_user_s,rushtex_sys_s,rushtex_rss_bytes,pdflatex_runs,pdflatex_wall_s,pdflatex_user_s,pdflatex_sys_s,pdflatex_rss_bytes\n")
        for d, n, r, p in rows:
            f.write(f"{d},{n},{r['runs']},{r['wall']:.4f},{r['user']:.4f},{r['sys']:.4f},{r['rss']},{p['runs']},{p['wall']:.4f},{p['user']:.4f},{p['sys']:.4f},{p['rss']}\n")
    print(f"\nper-document numbers: {a.csv}")


if __name__ == "__main__":
    main()
