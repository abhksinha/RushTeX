#!/usr/bin/env bash
# ==============================================================================
# RushTeX vs pdfLaTeX benchmark: CPU instructions retired and peak RAM.
#
#   ./benchmark.sh [file.tex] [iterations]     (default docs/manual.tex, 3)
#   TIME_RUNS=21 ./benchmark.sh ...            (timed runs, default 11)
#
# Instructions retired are the primary measure: they count the work done and do not
# change with machine load. CPU time (user + sys, all threads) and wall time are
# reported as medians after one warm-up run, raw and net of start-up: minus the same
# tool on a one-word document, which is process spawn, loading and the first page.
# RushTeX writes the PDF on a second thread, so its wall time can be below its CPU time.
#
# pdfLaTeX is counted twice: one run (the architecture floor), and the runs a user
# needs for the final PDF: rerun until the PDF stops changing, counting the runs up to
# the first that made it (1 for a document without references or contents, usually
# 3 with them).
#
# macOS: /usr/bin/time -l. Linux: perf stat (instructions:u).
# ==============================================================================
set -euo pipefail
export SOURCE_DATE_EPOCH=1 FORCE_SOURCE_DATE=1

TEX_FILE="${1:-docs/manual.tex}"
RUNS="${2:-3}"
if [ ! -f "$TEX_FILE" ]; then
    echo "Usage: $0 <file.tex> [iterations (default 3)]"
    exit 1
fi
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUSH_BIN="${RUSHTEX_BIN:-$SCRIPT_DIR/target/release/rushtex}"
[ -x "$RUSH_BIN" ] || (cd "$SCRIPT_DIR" && cargo build --release)
command -v pdflatex >/dev/null || { echo "pdflatex not found on PATH"; exit 1; }

python3 - "$RUSH_BIN" "$TEX_FILE" "$RUNS" <<'PY'
import hashlib, os, platform, re, shutil, statistics, subprocess, sys, tempfile, time

rush, tex, runs = sys.argv[1], os.path.abspath(sys.argv[2]), max(1, int(sys.argv[3]))
mac = sys.platform == "darwin"

def measure(cmd, cwd):
    """(instructions retired, peak RSS in bytes) of one run of cmd."""
    if mac:
        p = subprocess.run(["/usr/bin/time", "-l"] + cmd, cwd=cwd, capture_output=True, text=True)
        ins = re.search(r"(\d+)\s+instructions retired", p.stderr)
        rss = re.search(r"(\d+)\s+maximum resident set size", p.stderr)
        return int(ins.group(1)) if ins else 0, int(rss.group(1)) if rss else 0
    p = subprocess.run(["perf", "stat", "-x,", "-e", "instructions:u", "/usr/bin/time", "-f", "RSS %M"] + cmd, cwd=cwd, capture_output=True, text=True)
    ins = re.search(r"^(\d+),[^,]*,instructions", p.stderr, re.M)
    rss = re.search(r"RSS (\d+)", p.stderr)
    return int(ins.group(1)) if ins else 0, int(rss.group(1)) * 1024 if rss else 0

def median_of(cmd, cwd, before=None):
    ins, rss = [], []
    for _ in range(runs):
        if before:
            before()
        i, r = measure(cmd, cwd)
        ins.append(i)
        rss.append(r)
    return statistics.median(ins), max(rss)

def timed(cmd, cwd, k):
    """Median wall and CPU (user + sys) milliseconds of k runs, after one warm-up."""
    subprocess.run(cmd, cwd=cwd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    wall, cpu = [], []
    for _ in range(k):
        t0 = time.perf_counter_ns()
        p = subprocess.Popen(cmd, cwd=cwd, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        _, _, ru = os.wait4(p.pid, 0)
        wall.append((time.perf_counter_ns() - t0) / 1e6)
        cpu.append((ru.ru_utime + ru.ru_stime) * 1e3)
    return statistics.median(wall), statistics.median(cpu)

def pages(pdf):
    try:
        out = subprocess.run(["pdfinfo", pdf], capture_output=True, text=True).stdout
        return int(re.search(r"Pages:\s+(\d+)", out).group(1))
    except Exception:
        return 0

work = tempfile.mkdtemp(prefix="rushtex_bench_")
try:
    name = os.path.splitext(os.path.basename(tex))[0]
    src = os.path.join(work, os.path.basename(tex))
    shutil.copy(tex, src)
    # RushTeX: one run, no .aux.
    rush_pdf = os.path.join(work, "rush.pdf")
    native = "natively" in subprocess.run([rush, src, "-o", rush_pdf], cwd=work, capture_output=True, text=True).stderr
    r_ins, r_rss = median_of([rush, src, "-o", rush_pdf], work)
    # pdfLaTeX: the real cycle from clean, then one run with the .aux complete.
    def clean():
        for e in ("aux", "toc", "lof", "lot", "out", "log"):
            try:
                os.remove(os.path.join(work, f"{name}.{e}"))
            except FileNotFoundError:
                pass
    def pdf_hash():
        f = os.path.join(work, f"{name}.pdf")
        return hashlib.md5(open(f, "rb").read()).hexdigest() if os.path.exists(f) else ""
    pdf_cmd = ["pdflatex", "-interaction=batchmode", src]
    cycle = []
    for _ in range(runs):
        # Runs until the PDF stops changing (dates fixed so equal output hashes equal);
        # the runs needed are those up to the first that made the final PDF, the
        # confirming run not counted.
        clean()
        counts, hashes = [], []
        while len(counts) < 6:
            i, _ = measure(pdf_cmd, work)
            counts.append(i)
            hashes.append(pdf_hash())
            if len(hashes) > 1 and hashes[-1] == hashes[-2]:
                break
        n = hashes.index(hashes[-1]) + 1
        cycle.append((sum(counts[:n]), n))
    p_real = statistics.median(c[0] for c in cycle)
    p_runs = cycle[0][1]
    p_ins, p_rss = median_of(pdf_cmd, work)
    p_pdf = os.path.join(work, f"{name}.pdf")
    k = max(1, int(os.environ.get("TIME_RUNS", "11")))
    r_wall, r_cpu = timed([rush, src, "-o", rush_pdf], work, k)
    p_wall, p_cpu = timed(pdf_cmd, work, k)
    rp, pp = pages(rush_pdf), pages(p_pdf)
    rk, pk = os.path.getsize(rush_pdf) / 1024, os.path.getsize(p_pdf) / 1024
    text = open(tex, encoding="latin-1").read()

    # Start-up: the same preamble with a one-word body (process, class and packages, one
    # page), and the OS's own process floor.
    pre, body = text.split("\\begin{document}", 1)
    body = body.rsplit("\\end{document}", 1)[0]
    word = os.path.join(work, "word.tex")
    open(word, "w", encoding="latin-1").write(pre + "\\begin{document}\nx\n\\end{document}\n")
    rb_ins, _ = median_of([rush, word, "-o", os.path.join(work, "word-rush.pdf")], work)
    pb_ins, _ = median_of(["pdflatex", "-interaction=batchmode", word], work)
    rb_wall, rb_cpu = timed([rush, word, "-o", os.path.join(work, "word-rush.pdf")], work, k)
    pb_wall, pb_cpu = timed(["pdflatex", "-interaction=batchmode", word], work, k)
    os_ins, _ = median_of(["/usr/bin/true"], work)

    # Scaling: the body after the title and contents repeated 1, 4 and 10 times (keys
    # renamed per copy), instructions against pages; the slope is the cost of a page,
    # the intercept the fixed cost. pdfLaTeX is measured on a run with its .aux complete.
    cut = max((body.find(m) + len(m) for m in ("\\tableofcontents", "\\maketitle", "\\end{abstract}") if m in body), default=0)
    head, rest = body[:cut], body[cut:]
    keys = re.compile(r"(\\(?:label|ref|pageref|eqref|cite|bibitem)\*?(?:\[[^\]]*\])?\{)([^}]*)\}")
    def copy(i):
        return rest if i == 0 else keys.sub(lambda m: m.group(1) + ",".join(f"{k.strip()}-{i}" for k in m.group(2).split(",")) + "}", rest)
    fit, all_native = [], True
    for n in (1, 4, 10):
        f = os.path.join(work, f"scale{n}.tex")
        open(f, "w", encoding="latin-1").write(pre + "\\begin{document}" + head + "".join(copy(i) for i in range(n)) + "\\end{document}\n")
        out = os.path.join(work, f"scale{n}-rush.pdf")
        all_native &= "natively" in subprocess.run([rush, f, "-o", out], cwd=work, capture_output=True, text=True).stderr
        ri, _ = median_of([rush, f, "-o", out], work)
        for _ in range(2):
            subprocess.run(["pdflatex", "-interaction=batchmode", f], cwd=work, capture_output=True)
        pi, _ = median_of(["pdflatex", "-interaction=batchmode", f], work)
        fit.append((pages(out), ri, pages(os.path.join(work, f"scale{n}.pdf")), pi))
    def line(xs, ys):
        mx, my = statistics.mean(xs), statistics.mean(ys)
        slope = sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / max(sum((x - mx) ** 2 for x in xs), 1)
        return slope, my - slope * mx
    r_slope, r_icpt = line([f[0] for f in fit], [f[1] for f in fit])
    p_slope, p_icpt = line([f[2] for f in fit], [f[3] for f in fit])

    M = lambda x: f"{x / 1e6:,.1f}M"
    # RushTeX's value against pdfLaTeX's, one decimal, said the way round that is above 1.
    def ratio(r, p, better="less", worse="more"):
        return f"{p / r:.1f}x {better}" if r <= p else f"{r / p:.1f}x {worse}"
    ver = subprocess.run(["pdflatex", "--version"], capture_output=True, text=True).stdout.splitlines()[0]
    try:
        rev = subprocess.run(["git", "-C", os.path.dirname(rush), "rev-parse", "--short", "HEAD"], capture_output=True, text=True).stdout.strip()
    except Exception:
        rev = "?"
    cpu_name = subprocess.run(["sysctl", "-n", "machdep.cpu.brand_string"], capture_output=True, text=True).stdout.strip() if mac else platform.processor()
    counter = "/usr/bin/time -l (counts the kernel's process set-up and dyld too)" if mac else "perf stat -e instructions:u (user space only)"
    print("=" * 84)
    print(f" Machine: {cpu_name or platform.machine()}, {platform.system()} {platform.release()}; instructions: {counter}")
    print(f" pdfLaTeX: {ver}; `pdflatex -interaction=batchmode`, no latexmk")
    print(f"   (the script reruns it itself until the PDF stops changing); RushTeX {rev}, one run")
    print(f" {os.path.relpath(tex)}: {rp} pages (pdfLaTeX {pp}); RushTeX ran {'natively' if native else 'on the ENGINE (not native)'}")
    print(f" Instructions: median of {runs}; times: median of {k} after a warm-up; pdfLaTeX needed {p_runs} run{'s' if p_runs > 1 else ''} for its final PDF")
    print("=" * 84)
    rows = [("Instructions, 1 run", M(r_ins), M(p_ins), ratio(r_ins, p_ins)),
            (f"Instructions, real ({p_runs} run{'s' if p_runs > 1 else ''})", M(r_ins), M(p_real), ratio(r_ins, p_real)),
            ("CPU time (user+sys), 1 run", f"{r_cpu:.1f} ms", f"{p_cpu:.1f} ms", ratio(r_cpu, p_cpu)),
            ("Wall time, 1 run", f"{r_wall:.1f} ms", f"{p_wall:.1f} ms", ratio(r_wall, p_wall)),
            ("Peak RAM (RSS)", f"{r_rss / 2**20:.1f} MB", f"{p_rss / 2**20:.1f} MB", ratio(r_rss, p_rss)),
            ("PDF size", f"{rk:.1f} KB", f"{pk:.1f} KB", ratio(rk, pk, "smaller", "larger")),
            ("Fixed cost (fit intercept)", M(r_icpt), M(p_icpt), ratio(r_icpt, p_icpt)),
            ("Cost per page (fit slope)", M(r_slope), M(p_slope), ratio(r_slope, p_slope)),
            ("Same preamble, one-word body", M(rb_ins), M(pb_ins), ratio(rb_ins, pb_ins)),
            ("  its CPU / wall time", f"{rb_cpu:.1f}/{rb_wall:.1f} ms", f"{pb_cpu:.0f}/{pb_wall:.0f} ms", ""),
            ("OS process floor (/usr/bin/true)", M(os_ins), M(os_ins), "")]
    print(f" {'Metric':34s} {'RushTeX':>13s} {'pdfLaTeX':>13s}   Factor")
    for m, a, b, f in rows:
        print(f" {m:34s} {a:>13s} {b:>13s}   {f}")
    print("-" * 84)
    pts = "; ".join(f"x{n}: {f[0]} p {M(f[1])} / {f[2]} p {M(f[3])}" for n, f in zip((1, 4, 10), fit))
    print(f" Fit points (RushTeX / pdfLaTeX): {pts}" + ("" if all_native else "  [a RushTeX variant ran on the ENGINE]"))
    print("=" * 84)
finally:
    shutil.rmtree(work, ignore_errors=True)
PY
