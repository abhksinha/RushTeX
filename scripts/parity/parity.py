#!/usr/bin/env -S uv run -q --script
# /// script
# dependencies = ["pymupdf", "numpy"]
# ///
r"""Glyph-level parity against pdfLaTeX for the documents in scripts/parity/corpus.

  parity.py score [DOC..|--all] [--build] [--dx PAGE:DX] [--save-baseline]
                                                     strict per-page score, delta vs last run;
                                                     REGRESSED vs baseline.json (committed)
  parity.py gaps [DOC..]                             unsupported commands/environments/errors of
                                                     every corpus document, ranked by docs blocked
  parity.py rank DOC [-n N]                          pages by glyphs lost, first drifting line each
  parity.py lines DOC PAGE [--all]                   baselines ours vs ref (drifting only by default)
  parity.py miss DOC PAGE                            unmatched ref glyphs grouped by baseline
  parity.py boxes DOC PAGE [--all]                   pdfTeX's shipped page box (\showbox): for each
                                                     drifting baseline, the vertical glue, kerns and
                                                     box sizes TeX put above it (the recipe to match)
  parity.py repro DOC A B | --body TEX [--name N] [--trace pages|lists] [--build]
                                                     the document's preamble + lines A..B (or TEX) as
                                                     a fresh test, compiled by both, then `lines` per page

  parity.py tj DOC PAGE                              text operators (TJ, Td) of the page that differ
  parity.py hlist DOC PATTERN                        TeX's hlist_out of the line holding PATTERN
                                                     replayed in sp; ours minus TeX per character
  parity.py zero [DOC..|--all] [--engine] [--pixels] [--dpi N] [--cached] [--why PAGE]
                                                     zero tolerance: every glyph and path exactly
                                                     (rendering matrix as fractions), pixels, links,
                                                     outlines, metadata (scripts/parity/pdfexact.py)
  parity.py exact [DOC..|--all] [-v]                every ref glyph paired with ours (same char,
                                                     nearest origin): residual buckets, font/size
                                                     mismatches; the zero-shift target

A glyph matches when one of ours with the same origin lies within 0.1pt in x and y
(`--tol` changes it; `exact` reports the residuals themselves).
Reference PDFs are cached in target/parity/ref and rebuilt when the source is newer.
Every run deletes stale outputs first, so a failed compile is reported, never scored.
"""
import argparse, itertools, json, os, re, shutil, subprocess, sys
from collections import Counter
from pathlib import Path

import pymupdf

ROOT = Path(__file__).resolve().parents[2]
CORPUS = ROOT / "scripts/parity/corpus"
OUT = ROOT / "target/parity"
RUSHTEX = ROOT / "target/release/rushtex"
DEFAULT_DOCS = ["sample2e", "small2e", "lppl", "testmath"]
BASELINE = ROOT / "scripts/parity/baseline.json"


def corpus_docs():
    return sorted(p.stem for p in CORPUS.glob("*.tex"))
TOL = 0.1


def die(msg):
    sys.exit(f"parity: {msg}")


def build():
    r = subprocess.run(["cargo", "build", "--release"], cwd=ROOT, capture_output=True, text=True)
    if r.returncode:
        die("cargo build failed\n" + r.stderr[-3000:])


def source(doc):
    p = Path(doc)
    if p.suffix == ".tex" and p.exists():
        return p.resolve()
    p = CORPUS / f"{doc}.tex"
    if not p.exists():
        die(f"no document {doc} (looked in {CORPUS})")
    return p


# Both sides typeset the same date (`\\today`, `\\time`): web2c takes `SOURCE_DATE_EPOCH`
# for them when `FORCE_SOURCE_DATE=1`. 2026-10-20 00:00 UTC.
FIXED_DATE = {"SOURCE_DATE_EPOCH": "1792454400", "FORCE_SOURCE_DATE": "1"}


def run_pdflatex(tex, outdir, passes=5, halt=True):
    """pdfLaTeX run until the files it reads back (`.aux`, `.toc`, `.lof`, `.lot`, `.out`) stop
    changing, at most `passes` times, as latexmk runs it: a fixed two passes can leave a
    document whose contents list moves its own pages (encguide) one step short."""
    outdir.mkdir(parents=True, exist_ok=True)
    pdf = outdir / (tex.stem + ".pdf")
    pdf.unlink(missing_ok=True)
    aux = [outdir / (tex.stem + ext) for ext in (".aux", ".toc", ".lof", ".lot", ".out")]
    for f in aux:
        f.unlink(missing_ok=True)
    state = lambda: [f.read_bytes() if f.exists() else None for f in aux]
    for _ in range(passes):
        before = state()
        subprocess.run(
            ["pdflatex", "-interaction=nonstopmode", *(["-halt-on-error"] if halt else []), f"-output-directory={outdir}", str(tex)],
            cwd=tex.parent, capture_output=True, env={**os.environ, **FIXED_DATE},
        )
        if state() == before:
            break
    if not pdf.exists() and halt:
        # Some documents recover from errors by design (encguide omits the table of a font
        # that is not installed); TeX's own nonstop mode is the reference then.
        print(f"parity: {tex.name}: pdflatex stopped on an error, retrying without -halt-on-error", file=sys.stderr)
        return run_pdflatex(tex, outdir, passes, halt=False)
    if not pdf.exists():
        log = outdir / (tex.stem + ".log")
        err = [l for l in log.read_text(errors="replace").splitlines() if l.startswith("!")] if log.exists() else []
        die(f"pdflatex failed on {tex}: {' | '.join(err[:3])}")
    return pdf


def tex_install_mtime():
    """When the TeX installation last changed: `tlmgr` rewrites a tree's package database
    (`tlpkg/texlive.tlpdb`, in the tree for user mode, beside it for the system) and
    `mktexlsr` its `ls-R` on every install, so a reference older than that may lack a font
    or package the engine now finds."""
    stamps = []
    for var in ("TEXMFHOME", "TEXMFDIST", "TEXMFLOCAL"):
        r = subprocess.run(["kpsewhich", f"-var-value={var}"], capture_output=True, text=True)
        for tree in r.stdout.split(":") if r.returncode == 0 else []:
            t = Path(tree.strip()).expanduser()
            for f in (t / "ls-R", t / "tlpkg" / "texlive.tlpdb", t.parent / "tlpkg" / "texlive.tlpdb"):
                if f.exists():
                    stamps.append(f.stat().st_mtime)
    return max(stamps, default=0)


_install = []


def INSTALL_MTIME():
    if not _install:
        _install.append(tex_install_mtime())
    return _install[0]


def ref_pdf(tex):
    pdf = OUT / "ref" / (tex.stem + ".pdf")
    if pdf.exists() and pdf.stat().st_mtime >= max(tex.stat().st_mtime, INSTALL_MTIME()):
        return pdf
    return run_pdflatex(tex, OUT / "ref")


def run_rushtex(tex, pdf, env=None):
    if not RUSHTEX.exists():
        build()
    pdf.parent.mkdir(parents=True, exist_ok=True)
    pdf.unlink(missing_ok=True)
    r = subprocess.run([str(RUSHTEX), "--native", str(tex), "-o", str(pdf)], capture_output=True, text=True,
                       env={**os.environ, **(env or {})})
    out = r.stdout + r.stderr
    if r.returncode or "[ERROR]" in out or not pdf.exists():
        err = [re.sub(r"\x1b\[[0-9;]*m", "", l) for l in out.splitlines() if "ERROR" in l]
        die(f"rushtex failed on {tex}: {' | '.join(err[:3]) or 'exit ' + str(r.returncode)}")
    return out


ENGINE = False
ENGINE_FMT = OUT / "pdflatex.rfmt"


ENGINE_BUILT = False


def run_engine(tex, pdf):
    """The document end to end through the `rushtex` command on the TeX engine (latex.ltx's
    macros on our TeX), from the format `ENGINE_FMT` (made with `RT_FMT=...
    RT_FORMAT=pdflatex.ini` and the `format_smoke` test, or by `rushtex` itself)."""
    global ENGINE_BUILT
    if not ENGINE_BUILT:
        subprocess.run(["cargo", "build", "--release", "--quiet"], cwd=ROOT, check=True)
        ENGINE_BUILT = True
    pdf.parent.mkdir(parents=True, exist_ok=True)
    pdf.unlink(missing_ok=True)
    env = {**os.environ, **FIXED_DATE, "RUSHTEX_CACHE": str(OUT)}
    r = subprocess.run([str(ROOT / "target" / "release" / "rushtex"), "--fmt", str(ENGINE_FMT), str(tex), "-o", str(pdf)],
                       cwd=ROOT, capture_output=True, text=True, env=env)
    errs = [l[2:].strip() for l in r.stderr.splitlines() if re.match(r"! l\.\d+ \[", l)]
    if not pdf.exists() or (r.returncode and not errs):
        die(f"engine failed on {tex}: exit {r.returncode} {r.stderr[-300:]}")
    # A document may recover from errors by design (encguide skips the table of a font that
    # is not installed). Errors pass only when they are exactly pdfTeX's: same message, same
    # input line, same number of times.
    ours = Counter(re.sub(r"^l\.(\d+) \[\d+\] ", r"\1 ", e) for e in errs)
    if ours and ours != reference_errors(tex):
        die(f"engine errors on {tex}: {' | '.join(errs[:3])}")


def reference_errors(tex):
    """pdfTeX's errors in the reference log, as `line message` (TeX's final period dropped)."""
    ref_pdf(tex)
    log = (OUT / "ref" / (tex.stem + ".log")).read_text(encoding="latin-1").splitlines()
    out = Counter()
    for i, l in enumerate(log):
        if l.startswith("! "):
            line = next((m.group(1) for m in (re.match(r"l\.(\d+) ", x) for x in log[i + 1:i + 20]) if m), "?")
            out[f"{line} {l[2:].removesuffix('.')}"] += 1
    return out


def ours_pdf(tex):
    pdf = OUT / ("engine" if ENGINE else "ours") / (tex.stem + ".pdf")
    if ENGINE:
        run_engine(tex, pdf)
    else:
        run_rushtex(tex, pdf)
    return pdf


def glyphs(page):
    """(x, y, char, size) of every visible glyph."""
    o = []
    for b in page.get_text("rawdict")["blocks"]:
        for l in b.get("lines", []):
            for s in l["spans"]:
                for c in s["chars"]:
                    if c["c"].strip():
                        o.append((c["origin"][0], c["origin"][1], c["c"], s["size"], s["font"].split("+")[-1]))
    return o


def match(a, b, dx=0.0):
    """Indices of glyphs of `a` with no partner in `b` (each partner used once)."""
    grid = {}
    for k, g in enumerate(b):
        grid.setdefault((int(g[0] + dx), int(g[1])), []).append(k)
    used, missing = set(), []
    for i, g in enumerate(a):
        hit = False
        for gx in (int(g[0]) - 1, int(g[0]), int(g[0]) + 1):
            for gy in (int(g[1]) - 1, int(g[1]), int(g[1]) + 1):
                for k in grid.get((gx, gy), ()):
                    if k not in used and abs(b[k][0] + dx - g[0]) <= TOL and abs(b[k][1] - g[1]) <= TOL:
                        used.add(k)
                        hit = True
                        break
                if hit:
                    break
            if hit:
                break
        if not hit:
            missing.append(i)
    return missing


# Residual buckets of `exact`, in pt: pdfTeX writes positions to 0.001bp (\pdfdecimaldigits 3).
EXACT = 0.002
BUCKETS = [(EXACT, "exact"), (0.01, "<0.01"), (0.1, "<0.1"), (1.0, "<1"), (float("inf"), ">=1")]


def residuals(a, b):
    """Per ref glyph: (max(|dx|,|dy|) to the nearest unused glyph of ours with the same char, or
    None, and whether font name and size agree)."""
    grid = {}
    for k, g in enumerate(b):
        grid.setdefault((g[2], int(g[0]), int(g[1])), []).append(k)
    used, out = set(), []
    for g in a:
        best = None
        for gx in (int(g[0]) - 1, int(g[0]), int(g[0]) + 1):
            for gy in (int(g[1]) - 1, int(g[1]), int(g[1]) + 1):
                for k in grid.get((g[2], gx, gy), ()):
                    if k in used:
                        continue
                    d = max(abs(b[k][0] - g[0]), abs(b[k][1] - g[1]))
                    if best is None or d < best[0]:
                        best = (d, k)
        if best is None:
            out.append((None, False, g))
            continue
        used.add(best[1])
        o = b[best[1]]
        out.append((best[0], o[4] == g[4] and abs(o[3] - g[3]) < 1e-3, g))
    # Second pass: same font, other extracted text. pdfTeX's CMEX/CMSY carry no ToUnicode
    # (a viewer extracts raw codes); ours map to Unicode. Same glyph, other text.
    left = [k for k in range(len(b)) if k not in used]
    for j, (r, same, g) in enumerate(out):
        if r is not None:
            continue
        best = None
        for k in left:
            o = b[k]
            if k in used or o[4] != g[4]:
                continue
            d = max(abs(o[0] - g[0]), abs(o[1] - g[1]))
            if d < 1.0 and (best is None or d < best[0]):
                best = (d, k)
        if best:
            used.add(best[1])
            out[j] = (best[0], "text", g)
    return out


def cmd_exact(args):
    if args.build:
        build()
    docs = corpus_docs() if args.all else (args.docs or DEFAULT_DOCS)
    for d in docs:
        tex = source(d)
        try:
            A, B = pymupdf.open(ref_pdf(tex)), pymupdf.open(ours_pdf(tex))
        except SystemExit as e:
            print(f"{tex.stem}: FAILS {str(e)[:160]}")
            continue
        counts, font_bad, worst, tot, text_only = [0] * (len(BUCKETS) + 1), 0, (0.0, 0, None), 0, 0
        for i in range(len(A)):
            for r, same, g in residuals(glyphs(A[i]), glyphs(B[i]) if i < len(B) else []):
                tot += 1
                if r is None:
                    counts[-1] += 1
                    continue
                counts[next(j for j, (t, _) in enumerate(BUCKETS) if r < t)] += 1
                if same == "text":
                    text_only += 1
                    continue
                font_bad += not same
                if r < 1.0 and r > worst[0]:
                    worst = (r, i + 1, g)
        ex = counts[0]
        line = " ".join(f"{n}:{c}" for (_, n), c in zip(BUCKETS, counts)) + f" none:{counts[-1]} other-text:{text_only}"
        print(f"{tex.stem}: exact {100 * ex / max(tot, 1):.2f}% ({ex}/{tot})  {line}  font/size-mismatch:{font_bad}")
        if worst[2] and args.v:
            print(f"  worst <1pt: {worst[0]:.4f}pt page {worst[1]} {worst[2][2]!r} at ({worst[2][0]:.3f},{worst[2][1]:.3f})")


def cmd_zero(args):
    """Zero-tolerance parity: exact marks, optionally pixels, and structure."""
    global ENGINE
    ENGINE = args.engine
    sys.path.insert(0, str(Path(__file__).parent))
    import pdfexact as X

    if args.build:
        build()
    docs = corpus_docs() if args.all else (args.docs or DEFAULT_DOCS)
    summary = []
    for d in docs:
        tex = source(d)
        try:
            ours = OUT / ("engine" if ENGINE else "ours") / (tex.stem + ".pdf")
            if not (args.cached and ours.exists()):
                ours = ours_pdf(tex)
            A, B = pymupdf.open(ref_pdf(tex)), pymupdf.open(ours)
        except SystemExit as e:
            print(f"{tex.stem}: FAILS {str(e)[:160]}")
            summary.append((tex.stem, False))
            continue
        if args.why:
            m, near = X.first_difference(A, B, args.why)
            print(f"{tex.stem} page {args.why}: first ref mark without an exact twin:\n  ref  {m}\n  ours {near}")
            continue
        rows = X.compare_marks(A, B)
        g, gm, p, pm, cb, extra = (sum(r[k] for r in rows) for k in range(6))
        bad = [i + 1 for i, r in enumerate(rows) if r[1] < r[0] or r[3] < r[2] or r[4] or r[5]]
        marks_ok = len(A) == len(B) and not bad
        line = f"{tex.stem}: pages {len(B)}/{len(A)}  glyphs {gm}/{g}  paths {pm}/{p}  colour-diff {cb}  extra {extra}"
        if bad:
            line += f"  pages-off {bad[:12]}{'...' if len(bad) > 12 else ''}"
        st, _, _ = X.compare_structure(A, B)
        st_ok = all(v[2] for v in st.values())
        line += "  | " + " ".join(f"{k} {v[1]}/{v[0]}" for k, v in st.items())
        px_ok = True
        if args.pixels:
            px = X.pixel_diffs(A, B, args.dpi)
            px_ok = not any(px)
            line += f"  | pixels@{args.dpi} {sum(px)}" + (f" pages {[i + 1 for i, v in enumerate(px) if v][:12]}" if not px_ok else "")
        ok = marks_ok and st_ok and px_ok
        print(("ZERO  " if ok else "      ") + line)
        summary.append((tex.stem, ok))
    if len(summary) > 1:
        print(f"zero-shift: {sum(ok for _, ok in summary)}/{len(summary)} documents")


def page_scores(ref, ours, dx_from=None):
    A, B = pymupdf.open(ref), pymupdf.open(ours)
    rows = []
    for i in range(len(A)):
        a = glyphs(A[i])
        b = glyphs(B[i]) if i < len(B) else []
        dx = dx_from[1] if dx_from and i + 1 >= dx_from[0] else 0.0
        rows.append((len(a), len(a) - len(match(a, b, dx))))
    return rows, len(A), len(B)


def cmd_score(args):
    global ENGINE
    ENGINE = args.engine
    if ENGINE:
        args.save_baseline = False
    if args.build:
        build()
    if args.tol is not None:
        global TOL
        TOL = args.tol
        args.save_baseline = False
    dx = tuple(map(float, args.dx.split(":"))) if args.dx else None
    dx = (int(dx[0]), dx[1]) if dx else None
    docs = corpus_docs() if args.all else (args.docs or DEFAULT_DOCS)
    last_path = OUT / ("last_score_engine.json" if ENGINE else "last_score.json")
    last = json.loads(last_path.read_text()) if last_path.exists() and not dx else {}
    base = json.loads(BASELINE.read_text()) if BASELINE.exists() else {}
    now = {}
    for d in docs:
        tex = source(d)
        try:
            rows, na, nb = page_scores(ref_pdf(tex), ours_pdf(tex), dx)
        except SystemExit as e:
            # One failing document must not hide the others' scores.
            now[tex.stem] = {"pct": 0.0, "pages": [], "fail": str(e)[:200]}
            print(f"{tex.stem}: FAILS {str(e)[:160]}")
            if base.get(tex.stem, {}).get("pct", 0) > 0:
                print("  REGRESSED vs baseline (compiled before)")
            continue
        tot, ok = sum(r[0] for r in rows), sum(r[1] for r in rows)
        pct = 100 * ok / max(tot, 1)
        pages = [100 * o // max(t, 1) for t, o in rows]
        now[tex.stem] = {"pct": pct, "pages": pages}
        prev = last.get(tex.stem)
        delta = f" ({pct - prev['pct']:+.1f})" if prev else ""
        print(f"{tex.stem}: pages {nb}/{na}, {ok}/{tot} = {pct:.1f}%{delta}")
        if any(p < 100 for p in pages):
            print("  " + " ".join(f"{i + 1}:{p}" for i, p in enumerate(pages)))
        ref = base.get(tex.stem) or prev
        if ref:
            worse = [f"{i + 1}:{q}->{p}" for i, (p, q) in enumerate(zip(pages, ref["pages"])) if p < q]
            if worse:
                print(("  REGRESSED vs baseline " if tex.stem in base else "  REGRESSED ") + " ".join(worse))
    if not dx:
        OUT.mkdir(parents=True, exist_ok=True)
        last_path.write_text(json.dumps({**last, **now}))
        if args.save_baseline:
            BASELINE.write_text(json.dumps({**base, **now}, indent=0, sort_keys=True) + "\n")
            print(f"baseline saved: {BASELINE.relative_to(ROOT)}")


def cmd_gaps(args):
    """Runs `rushtex --gaps` on each document and ranks what blocks the most documents."""
    if not RUSHTEX.exists():
        build()
    docs = args.docs or corpus_docs()
    items = {}  # (kind, name) -> {doc: count}
    for d in docs:
        tex = source(d)
        r = subprocess.run([str(RUSHTEX), "--gaps", str(tex)], cwd=tex.parent, capture_output=True, text=True)
        for line in r.stdout.splitlines():
            f = line.split("\t")
            if len(f) >= 3 and f[0] not in ("class", "package"):
                items.setdefault((f[0], f[1]), {})[tex.stem] = int(f[2])
            elif len(f) >= 2:
                items.setdefault((f[0], f[1]), {})[tex.stem] = 1
    for kind in ("class", "package", "environment", "command", "error"):
        rows = sorted(((k, v) for k, v in items.items() if k[0] == kind), key=lambda kv: (-len(kv[1]), -sum(kv[1].values()), kv[0][1]))
        if not rows:
            continue
        print(f"== {kind} ({len(rows)})")
        for (_, name), per in rows[: args.n]:
            print(f"  {len(per):2d} docs {sum(per.values()):6d}x  {name[:70]:70s}  {' '.join(sorted(per))}")


def baselines(page, min_size=6):
    L = {}
    for x, y, c, s, _ in glyphs(page):
        if s >= min_size and c not in "|\x0c":
            L.setdefault(round(y, 2), []).append((x, c))
    return [(y, min(v)[0], "".join(c for _, c in sorted(v))) for y, v in sorted(L.items())]


def pair_lines(ra, rb):
    """Ref and our baselines paired by their text (difflib), so an extra or missing line
    on one side does not shift every pair after it."""
    import difflib
    sm = difflib.SequenceMatcher(None, [r[2] for r in ra], [r[2] for r in rb], autojunk=False)
    out = []
    for tag, i1, i2, j1, j2 in sm.get_opcodes():
        if tag == "equal" or (tag == "replace" and i2 - i1 == j2 - j1):
            out += list(zip(ra[i1:i2], rb[j1:j2]))
        else:
            out += [(a, (0, 0, "")) for a in ra[i1:i2]] + [((0, 0, ""), b) for b in rb[j1:j2]]
    return out


def print_lines(ref, ours, page, show_all=False):
    A, B = pymupdf.open(ref)[page - 1], pymupdf.open(ours)[page - 1]
    ra, rb = baselines(A), baselines(B)
    n = 0
    for a, b in pair_lines(ra, rb):
        dy, dx = b[0] - a[0], b[1] - a[1]
        if show_all or abs(dy) > TOL or abs(dx) > TOL or a[2] != b[2]:
            print(f"{a[0]:8.2f} {dy:+7.2f} dx{dx:+6.2f}  {a[2][:40]} | {b[2][:40]}")
            n += 1
    if not n:
        print(f"page {page}: all baselines within {TOL}pt")


def cmd_lines(args):
    tex = source(args.doc)
    print_lines(ref_pdf(tex), OUT / "ours" / (tex.stem + ".pdf") if args.cached else ours_pdf(tex), args.page, args.all)


def cmd_miss(args):
    tex = source(args.doc)
    A = pymupdf.open(ref_pdf(tex))[args.page - 1]
    B = pymupdf.open(ours_pdf(tex))[args.page - 1]
    a, b = glyphs(A), glyphs(B)
    miss = sorted(a[i][:3] for i in match(a, b, args.dx))
    miss.sort(key=lambda g: (round(g[1], 1), g[0]))
    for y, g in itertools.groupby(miss, key=lambda g: round(g[1], 1)):
        g = list(g)
        print(f"{y:8.1f} x{g[0][0]:7.2f} n{len(g):3}  {''.join(t[2] for t in g)[:60]}")


# ----- box oracle: pdfTeX's shipped pages from \showbox --------------------------------

BOX_RE = re.compile(r"\\([hv])box\(([-\d.]+)\+([-\d.]+)\)x([-\d.]+)(?:, glue set (- )?([\d.]+)(fil*)?)?(?:, shifted ([-\d.]+))?")
GLUE_RE = re.compile(r"\\glue(?:\((\\[A-Za-z@]+)\))? ([-\d.]+)(?: plus ([-\d.]+)(fil*)?)?(?: minus ([-\d.]+)(fil*)?)?")
KERN_RE = re.compile(r"\\kern ?([-\d.]+)")
RULE_RE = re.compile(r"\\rule\(([-\d.*]+)\+([-\d.*]+)\)")
CHAR_RE = re.compile(r"\\[A-Z0-9]+/[^ ]+ (.)", re.S)
GLYPH_NL_RE = re.compile(r"^\.*\\[A-Z0-9]+/\S+ $")
PT_TO_BP = 72 / 72.27


def box_log(tex):
    """pdfTeX's log with every shipped page box shown in full (cached like ref_pdf)."""
    d = OUT / "boxes"
    log = d / (tex.stem + ".log")
    if log.exists() and log.stat().st_mtime >= tex.stat().st_mtime:
        return log.read_text(encoding="latin-1")
    d.mkdir(parents=True, exist_ok=True)
    hook = r"\showboxdepth=100000 \showboxbreadth=10000000 \AddToHook{shipout/before}{\showbox\ShipoutBox}"
    for _ in range(2):
        subprocess.run(["pdflatex", "-interaction=nonstopmode", f"-output-directory={d}", f"-jobname={tex.stem}",
                        hook + r"\input{" + tex.name + "}"], cwd=tex.parent, capture_output=True,
                       env={**os.environ, "max_print_line": "10000000"})
    return log.read_text(encoding="latin-1")


def box_pages(log):
    """Each shipped page as a tree: nodes are dicts with kind, line text and children."""
    # Split on newlines only: pdfTeX prints glyph codes such as the fi ligature (12) raw.
    pages, lines = [], log.split("\n")
    i = 0
    while i < len(lines):
        if lines[i].startswith("> \\box") and lines[i].endswith("="):
            j = i + 1
            body = []
            while j < len(lines) and lines[j] and not lines[j].startswith("! OK"):
                if GLYPH_NL_RE.match(lines[j]) and j + 1 < len(lines):
                    body.append(lines[j] + "\n" + lines[j + 1])  # glyph 10 printed raw
                    j += 2
                    continue
                body.append(lines[j])
                j += 1
            pages.append(build_tree(body))
            i = j
        i += 1
    return pages


def build_tree(body):
    root = {"kind": "root", "children": []}
    stack = [(-1, root)]
    for line in body:
        depth = len(line) - len(line.lstrip("."))
        node = {"kind": "item", "text": line[depth:], "children": []}
        while stack[-1][0] >= depth:
            stack.pop()
        stack[-1][1]["children"].append(node)
        stack.append((depth, node))
    return root["children"][0] if root["children"] else root


def glue_amount(m, box):
    nat = float(m.group(2))
    sign, ratio, order = box
    if ratio:
        if sign > 0 and m.group(3) and (m.group(4) or "") == order:
            nat += ratio * float(m.group(3))
        elif sign < 0 and m.group(5) and (m.group(6) or "") == order:
            nat -= ratio * float(m.group(5))
    return nat


def box_lines(node, top, out, recipe):
    """Walks a vlist from `top` (pt below the page origin): appends (baseline, text, recipe)
    for every hbox with characters, `recipe` being the vertical items since the last one."""
    m = BOX_RE.match(node["text"])
    box = (-1 if m.group(5) else 1, float(m.group(6) or 0), m.group(7) or "")
    y = top
    for c in node["children"]:
        t = c["text"]
        b = BOX_RE.match(t)
        if b:
            h, dp = float(b.group(2)), float(b.group(3))
            if b.group(1) == "v":
                box_lines(c, y, out, recipe)
            else:
                text = hbox_text(c)
                if text:
                    out.append((y + h, text, recipe[:] + [f"box h{h:g} d{dp:g}"]))
                    recipe.clear()
                inner_vboxes(c, y + h, out, recipe)
            y += h + dp
            continue
        g = GLUE_RE.match(t)
        if g:
            a = glue_amount(g, box)
            y += a
            recipe.append(f"{g.group(1) or 'glue'} {a:g}")
            continue
        k = KERN_RE.match(t)
        if k:
            y += float(k.group(1))
            recipe.append(f"kern {float(k.group(1)):g}")
            continue
        r = RULE_RE.match(t)
        if r:
            y += sum(float(v) for v in r.groups() if v != "*")
            recipe.append("rule")
            continue
        if t.startswith("\\penalty"):
            recipe.append(t[1:])


def inner_vboxes(hbox, baseline, out, recipe):
    """Lines of the vboxes inside an hbox at any depth (tabulars, minipages, columns)."""
    for v in hbox["children"]:
        b = BOX_RE.match(v["text"])
        if not b:
            continue
        shift = float(b.group(8) or 0)
        if b.group(1) == "v":
            box_lines(v, baseline + shift - float(b.group(2)), out, recipe)
        else:
            inner_vboxes(v, baseline + shift, out, recipe)


def hbox_text(node):
    """Glyphs of an hbox, nested hboxes included (display formulas, labels)."""
    out = []
    for c in node["children"]:
        if m := CHAR_RE.match(c["text"]):
            out.append(m.group(1))
        elif c["text"].startswith("\\hbox"):
            out.append(hbox_text(c))
    return "".join(out)


def cmd_boxes(args):
    tex = source(args.doc)
    pages = box_pages(box_log(tex))
    if args.page > len(pages):
        die(f"{tex.stem} has {len(pages)} shipped pages")
    tex_lines = []
    box_lines(pages[args.page - 1], 0.0, tex_lines, [])
    ref_pdf_lines = baselines(pymupdf.open(ref_pdf(tex))[args.page - 1], min_size=0)
    ours = baselines(pymupdf.open(ours_pdf(tex))[args.page - 1], min_size=0)
    ours_y = [b[0] for b in ours]
    shown = 0
    for y_pt, text, recipe in tex_lines:
        y = (72.27 + y_pt) * PT_TO_BP
        ref_y = min((r[0] for r in ref_pdf_lines), key=lambda v: abs(v - y), default=y)
        mine = min(ours_y, key=lambda v: abs(v - ref_y), default=None)
        dy = (mine - ref_y) if mine is not None else float("nan")
        if args.all or not abs(dy) <= TOL:
            print(f"{ref_y:8.2f} {dy:+7.2f}  {text[:40]:40s}  <- {', '.join(recipe)}")
            shown += 1
    if not shown:
        print(f"page {args.page}: every TeX line within {TOL}pt")


def cmd_rank(args):
    tex = source(args.doc)
    ref, ours = ref_pdf(tex), ours_pdf(tex)
    rows, _, _ = page_scores(ref, ours)
    A, B = pymupdf.open(ref), pymupdf.open(ours)
    order = sorted(range(len(rows)), key=lambda i: rows[i][1] - rows[i][0])
    for i in order[: args.n]:
        t, o = rows[i]
        if t == o:
            break
        first = ""
        if i < len(B):
            for a, b in zip(baselines(A[i]), baselines(B[i])):
                if abs(b[0] - a[0]) > TOL or abs(b[1] - a[1]) > TOL:
                    first = f"first drift y{a[0]:.1f} dy{b[0] - a[0]:+.2f} dx{b[1] - a[1]:+.2f} {a[2][:30]}"
                    break
        print(f"page {i + 1:3}: lost {t - o:5} of {t:5}  {first}")


def preamble(tex):
    lines = tex.read_text(errors="replace").splitlines()
    for i, l in enumerate(lines):
        if l.lstrip().startswith(r"\begin{document}"):
            return lines[:i]
    die(f"no \\begin{{document}} in {tex}")


def cmd_repro(args):
    if args.build:
        build()
    tex = source(args.doc)
    if args.body is not None:
        body = args.body.splitlines()
    elif args.a and args.b:
        body = tex.read_text(errors="replace").splitlines()[args.a - 1 : args.b]
    else:
        die("repro needs a line range A B or --body")
    d = OUT / "repro"
    d.mkdir(parents=True, exist_ok=True)
    name = args.name or f"{tex.stem}_{args.a or 'body'}"
    src = d / f"{name}.tex"
    src.write_text("\n".join(preamble(tex) + [r"\begin{document}"] + body + [r"\end{document}", ""]))
    env = {}
    tex_src = src
    if args.trace:
        # TeX-only tracing goes in a copy, so rushtex never sees the primitives.
        hook = {"pages": r"\tracingpages=1 \tracingonline=1",
                "lists": r"\showboxdepth=3 \showboxbreadth=100 \tracingonline=1"}[args.trace]
        body_t = body + ([r"\showlists"] if args.trace == "lists" else [])
        tex_src = d / f"{name}_trace.tex"
        tex_src.write_text("\n".join(preamble(tex) + [r"\begin{document}" + hook] + body_t + [r"\end{document}", ""]))
        if args.trace == "pages":
            env["RUSHTEX_TRACE_PAGES"] = "1"
    # `\showlists` stops as an error: let TeX go on past it.
    ref = run_pdflatex(tex_src, d, passes=1, halt=args.trace != "lists")
    ours = d / f"{name}_ours.pdf"
    out = run_rushtex(src, ours, env)
    n = min(len(pymupdf.open(ref)), len(pymupdf.open(ours)))
    print(f"{src}  pages ref {len(pymupdf.open(ref))} ours {len(pymupdf.open(ours))}")
    for p in range(1, n + 1):
        print(f"-- page {p}")
        print_lines(ref, ours, p, args.all)
    if args.trace:
        log = (d / f"{tex_src.stem}.log").read_text(errors="replace").splitlines()
        if args.trace == "pages":
            print("-- TeX\n" + "\n".join(l for l in log if l.startswith("%")))
            print("-- rushtex\n" + "\n".join(l for l in out.splitlines() if l.startswith("%")))
        else:
            s = next((i for i, l in enumerate(log) if l.startswith("### vertical mode entered at line 0")), None)
            if s is not None:
                e = next((i for i in range(s, len(log)) if log[i].startswith("prevdepth")), len(log) - 1)
                print("\n".join(log[s : e + 1]))



def cmd_tj(args):
    """Text operators (TJ arrays, Td moves) of one page that differ, in stream order: where
    pdfTeX's 1/1000 em displacements or 0.001bp moves and ours part."""
    import difflib
    tex = source(args.doc)
    def ops(pdf):
        d = pymupdf.open(pdf); p = d[args.page - 1]
        data = b"".join(d.xref_stream(x) for x in p.get_contents()).decode("latin1")
        data = re.sub(r"/F\w+ ", "/F ", data)
        return [t.strip() for t in re.findall(r"\[[^\]]*\]TJ|-?[\d.]+ -?[\d.]+ Td", data)]
    a, b = ops(ref_pdf(tex)), ops(ours_pdf(tex))
    for tag, i1, i2, j1, j2 in difflib.SequenceMatcher(None, a, b, autojunk=False).get_opcodes():
        if tag != "equal":
            print("ref ", " | ".join(a[i1:i2])[:args.width]); print("ours", " | ".join(b[j1:j2])[:args.width]); print()


# Box-log font identifiers to TFM names: (family, series, shape) and the fd design sizes.
NFSS_TFM = {("cmr", "m", "n"): "cmr", ("cmr", "bx", "n"): "cmbx", ("cmr", "m", "it"): "cmti", ("cmr", "m", "sl"): "cmsl",
            ("cmr", "m", "sc"): "cmcsc", ("cmr", "bx", "it"): "cmbxti", ("cmtt", "m", "n"): "cmtt", ("cmss", "m", "n"): "cmss",
            ("cmss", "bx", "n"): "cmssbx", ("cmm", "m", "it"): "cmmi", ("cmsy", "m", "n"): "cmsy",
            ("cmex", "m", "n"): "cmex", ("cmm", "b", "it"): "cmmib", ("cmsy", "b", "n"): "cmbsy", ("msa", "m", "n"): "msam",
            ("msb", "m", "n"): "msbm"}
DESIGN = {"cmr": [5, 6, 7, 8, 9, 10, 12, 17], "cmbx": [5, 6, 7, 8, 9, 10, 12], "cmti": [7, 8, 9, 10, 12], "cmsl": [8, 9, 10, 12],
          "cmtt": [8, 9, 10, 12], "cmss": [8, 9, 10, 12, 17], "cmmi": [5, 6, 7, 8, 9, 10, 12], "cmsy": [5, 6, 7, 8, 9, 10],
          "cmex": [7, 8, 9, 10], "cmmib": [5, 6, 7, 8, 9, 10], "cmbsy": [5, 6, 7, 8, 9, 10], "msam": [5, 6, 7, 8, 9, 10],
          "msbm": [5, 6, 7, 8, 9, 10]}


def tfm_widths(name, size_sp):
    """Character widths of `name`.tfm at `size_sp`, scaled as tex.web §571 `store_scaled`."""
    import struct
    path = subprocess.run(["kpsewhich", name + ".tfm"], capture_output=True, text=True).stdout.strip()
    d = open(path, "rb").read()
    lh, bc, ec = struct.unpack(">3H", d[2:8])
    ci = 24 + lh * 4; wb = ci + (ec - bc + 1) * 4
    z, alpha = size_sp, 16
    while z >= 0o40000000:
        z //= 2; alpha += alpha
    beta = 256 // alpha; alpha *= z
    def sc(f):
        u = f & 0xFFFFFFFF; a, b, c, e = u >> 24, (u >> 16) & 255, (u >> 8) & 255, u & 255
        sw = (((e * z) // 256 + c * z) // 256 + b * z) // beta
        return sw if a == 0 else sw - alpha
    return {c: sc(struct.unpack(">i", d[wb + d[ci + (c - bc) * 4] * 4:][:4])[0]) for c in range(bc, ec + 1) if d[ci + (c - bc) * 4]}


def font_widths(ident, cache={}):
    if ident not in cache:
        enc, fam, ser, shp, size = ident.split("/")
        base = NFSS_TFM.get((fam, ser, shp))
        if base is None:
            return None
        sizes = DESIGN.get(base, [10]); pt = float(size)
        design = max([s for s in sizes if s <= pt + 0.01] or sizes[:1])
        whole, _, frac = size.partition(".")
        a = 0
        for ch in reversed(frac[:17]):
            a = (a + int(ch) * 0x20000) // 10
        cache[ident] = tfm_widths(f"{base}{design}", int(whole) * 65536 + (a + 1) // 2)
    return cache[ident]


def hlist_items(log, n, depth):
    """The items of the box-log hbox at line `n` (`depth` dots): characters, kerns, glue,
    and nested hboxes with their own items, so `hlist_replay` reaches every character."""
    def sp(x): return round(float(x) * 65536)
    items = []; k = n + 1
    while k < len(log) and log[k].startswith("." * (depth + 1)):
        t = log[k][depth + 1:]; k += 1
        if t.startswith("."):
            continue
        if c := re.match(r"\\(\S+/\S+/\S+/\S+/\S+) (\S+)", t):
            ch = c.group(2)
            code = int(ch[2:], 16) if ch.startswith("^^") and len(ch) == 4 else \
                (ord(ch[2]) - 64 if ord(ch[2]) >= 64 else ord(ch[2]) + 64) if ch.startswith("^^") else ord(ch[0])
            items.append(("c", c.group(1), code))
        elif t.startswith("\\kern"):
            items.append(("k", sp(re.findall(r"-?[\d.]+", t)[0])))
        elif t.startswith("\\glue"):
            body = t[t.index(")") + 1:] if t.startswith("\\glue(") else t[5:]
            g = re.findall(r"(-?[\d.]+)(fil+)?", body)
            if not g:  # `\glue(\nonscript)`
                items.append(("g", 0, 0, "", 0)); continue
            st = sp(g[1][0]) if "plus" in body else 0
            items.append(("g", sp(g[0][0]), st, g[1][1] if "plus" in body else "", sp(g[-1][0]) if "minus" in body else 0))
        elif t.startswith("\\hbox"):
            m = re.match(r"\\hbox.*?x(-?[\d.]+)(?:, glue set (- ?)?([\d.]+)(fil+)?)?", t)
            items.append(("b", sp(m.group(1)), m.group(4) or "", hlist_items(log, k - 1, depth + 1)))
        elif t.startswith(("\\vbox", "\\rule")):
            w = re.search(r"x(-?[\d.*]+)", t).group(1); items.append(("k", 0 if w == "*" else sp(w)))
        elif t.startswith(("\\mathon", "\\mathoff")):
            s = re.search(r", surrounded (-?[\d.]+)", t); items.append(("k", sp(s.group(1)) if s else 0))
    return items


def hlist_width(items):
    return sum(font_widths(i[1])[i[2]] if i[0] == "c" else i[1] for i in items if i[0] != "g") + \
        sum(i[1] for i in items if i[0] == "g")


def hlist_replay(items, width, order, h, out):
    """tex.web §619 `hlist_out` over `items` of a box `width` wide starting at `h`: appends
    (char, h) for every character, nested boxes included, and returns the glue_set."""
    x = width - hlist_width(items)
    idx = 2 if x > 0 else 4
    tot = sum(i[idx] for i in items if i[0] == "g" and (x < 0 or i[3] == order))
    ratio = max(x / tot, -1.0) if tot else 0.0
    cg = cur_g = 0
    for i in items:
        if i[0] == "c":
            out.append((chr(i[2]), h)); h += font_widths(i[1])[i[2]]
        elif i[0] == "k":
            h += i[1]
        elif i[0] == "b":
            hlist_replay(i[3], i[1], i[2], h, out); h += i[1]
        else:
            if x < 0 or i[3] == order:
                cg += i[idx]
            g = zround(ratio * cg); h += i[1] + g - cur_g; cur_g = g
    return ratio


def zround(r):
    """web2c `zround` (texmfmp.c): halves away from zero, unlike Python's `round`."""
    import math
    return math.floor(r + 0.5) if r >= 0 else math.ceil(r - 0.5)


def cmd_hlist(args):
    """Replays tex.web §619 `hlist_out` on the box-log hbox whose text contains PATTERN and
    prints, per character, ours minus TeX in sp (ours from RUSHTEX_TRACE_H)."""
    tex = source(args.doc)
    log = box_log(tex).splitlines()
    for n, l in enumerate(log):
        m = re.match(r"(\.+)\\hbox.*?x(-?[\d.]+)(?:, glue set (- ?)?([\d.]+)(fil+)?)?", l)
        if not m:
            continue
        items = hlist_items(log, n, len(m.group(1)))
        out = []
        try:
            ratio = hlist_replay(items, round(float(m.group(2)) * 65536), m.group(5) or "", 0, out)
        except (TypeError, KeyError):
            continue  # a font without a TFM mapping in NFSS_TFM, or another encoding (TS1)
        text = "".join(c for c, _ in out)
        if args.pattern in text:
            break
    else:
        die(f"no hbox with {args.pattern!r} in the box log")
    trace = run_rushtex(tex, OUT / "ours" / (tex.stem + ".pdf"), env={"RUSHTEX_TRACE_H": "1"})
    seq = [(chr(int(r[1])), int(r[2])) for r in (l.split() for l in trace.splitlines() if l.startswith("H "))]
    s = "".join(c for c, _ in seq); t = "".join(c for c, _ in out)
    j, k = t.find(args.pattern), s.find(args.pattern)
    if k < 0:
        die(f"ours has no {args.pattern!r}")
    while k >= 0 and s[k:k + len(t) - j] != t[j:]:
        if s.find(args.pattern, k + 1) < 0:
            break
        k = s.find(args.pattern, k + 1)
    d0 = seq[k][1] - out[j][1]
    print(f"glue_set {ratio:.10f}  width {round(float(m.group(2)) * 65536)}sp  box left at {d0}sp in ours (offset at {args.pattern!r})")
    # Pair each TeX character with ours by character within a short window: math is drawn in
    # its own order, so a plain zip loses step after the first formula.
    res = []
    for c, h in out[j:]:
        hit = next((q for q in range(k, min(k + 12, len(seq))) if seq[q][0] == c), None)
        if hit is None:
            res.append(f"{c}?"); continue
        res.append(f"{c}{(seq[hit][1] - h - d0):+d}"); k = hit + 1
    print(" ".join(res))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sp = ap.add_subparsers(dest="cmd", required=True)
    s = sp.add_parser("score")
    s.add_argument("docs", nargs="*")
    s.add_argument("--build", action="store_true")
    s.add_argument("--dx", help="PAGE:DX shift ours by DX from PAGE on (not saved as baseline)")
    s.add_argument("--all", action="store_true", help="every document in the corpus")
    s.add_argument("--save-baseline", action="store_true", help="write scripts/parity/baseline.json")
    s.add_argument("--engine", action="store_true", help="typeset with the engine (format_smoke, RT_PDF) instead of the native pipeline")
    s.add_argument("--tol", type=float, help="match tolerance in pt (default 0.1; not saved as baseline)")
    s.set_defaults(f=cmd_score)
    s = sp.add_parser("zero")
    s.add_argument("docs", nargs="*")
    s.add_argument("--all", action="store_true")
    s.add_argument("--build", action="store_true")
    s.add_argument("--engine", action="store_true")
    s.add_argument("--cached", action="store_true", help="reuse our last PDF")
    s.add_argument("--pixels", action="store_true")
    s.add_argument("--dpi", type=int, default=300)
    s.add_argument("--why", type=int, metavar="PAGE", help="the first mark of PAGE without an exact twin")
    s.set_defaults(f=cmd_zero)
    s = sp.add_parser("exact")
    s.add_argument("docs", nargs="*")
    s.add_argument("--all", action="store_true")
    s.add_argument("--build", action="store_true")
    s.add_argument("-v", action="store_true", help="the worst residual under 1pt per document")
    s.set_defaults(f=cmd_exact)
    s = sp.add_parser("gaps")
    s.add_argument("docs", nargs="*")
    s.add_argument("-n", type=int, default=40)
    s.set_defaults(f=cmd_gaps)
    s = sp.add_parser("boxes")
    s.add_argument("doc")
    s.add_argument("page", type=int)
    s.add_argument("--all", action="store_true")
    s.set_defaults(f=cmd_boxes)
    s = sp.add_parser("rank")
    s.add_argument("doc")
    s.add_argument("-n", type=int, default=15)
    s.set_defaults(f=cmd_rank)
    s = sp.add_parser("lines")
    s.add_argument("doc")
    s.add_argument("page", type=int)
    s.add_argument("--all", action="store_true")
    s.add_argument("--cached", action="store_true", help="reuse our last PDF instead of recompiling")
    s.set_defaults(f=cmd_lines)
    s = sp.add_parser("miss")
    s.add_argument("doc")
    s.add_argument("page", type=int)
    s.add_argument("--dx", type=float, default=0.0)
    s.set_defaults(f=cmd_miss)
    s = sp.add_parser("tj")
    s.add_argument("doc")
    s.add_argument("page", type=int)
    s.add_argument("--width", type=int, default=700)
    s.set_defaults(f=cmd_tj)
    s = sp.add_parser("hlist")
    s.add_argument("doc")
    s.add_argument("pattern")
    s.set_defaults(f=cmd_hlist)
    s = sp.add_parser("repro")
    s.add_argument("doc")
    s.add_argument("a", type=int, nargs="?")
    s.add_argument("b", type=int, nargs="?")
    s.add_argument("--body")
    s.add_argument("--name")
    s.add_argument("--trace", choices=["pages", "lists"])
    s.add_argument("--all", action="store_true")
    s.add_argument("--build", action="store_true")
    s.set_defaults(f=cmd_repro)
    a = ap.parse_args()
    if shutil.which("pdflatex") is None:
        die("pdflatex not on PATH")
    a.f(a)


if __name__ == "__main__":
    main()
