#!/usr/bin/env python3
"""The native-port gate (native-port-plan.md section 2, step 5): every corpus document run
twice by the rushtex command, once on LaTeX's macros and once with RT_NATIVE, and the two
compared: the PDF bytes, and the RT_STATE dumps (every meaning and register at each
shipout and at the end). Prints the documents that differ and how many calls each native
command settled itself.

    scripts/parity/kernel_diff.py [NATIVE] [DOC..]     NATIVE as RT_NATIVE takes it (default all)
"""
import glob, os, subprocess, sys
root = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
S = os.path.join(root, "target", "kernel_diff")
os.makedirs(os.path.join(S, "k"), exist_ok=True)
native = sys.argv[1] if len(sys.argv) > 1 else "all"
only = set(sys.argv[2:])
exe = os.path.join(root, "target/release/rushtex")
corpus = os.path.join(root, "scripts/parity/corpus")
bad = tot_n = tot_m = 0
docs = [p for p in sorted(glob.glob(os.path.join(corpus, "*.tex"))) if not only or os.path.splitext(os.path.basename(p))[0] in only]
for tex in docs:
    d = os.path.splitext(os.path.basename(tex))[0]
    out = {}
    for mode in ("mac", "nat"):
        env = dict(os.environ, RT_STATE=f"{S}/k/{d}.{mode}.st", SOURCE_DATE_EPOCH="1792454400", FORCE_SOURCE_DATE="1")
        if mode == "nat":
            env["RT_NATIVE"] = native
        r = subprocess.run([exe, tex, "-o", f"{S}/k/{d}.{mode}.pdf"], cwd=corpus, env=env, capture_output=True, text=True)
        out[mode] = r.stderr + r.stdout
    pdf = open(f"{S}/k/{d}.mac.pdf", "rb").read() == open(f"{S}/k/{d}.nat.pdf", "rb").read()
    st = open(f"{S}/k/{d}.mac.st", "rb").read() == open(f"{S}/k/{d}.nat.st", "rb").read()
    line = [l for l in out["nat"].splitlines() if l.startswith("RT_NATIVE")]
    for l in line:
        w = l.split()
        tot_n += int(w[3].rstrip(",")); tot_m += int(w[5])
    if not (pdf and st):
        bad += 1
        print(f"{d}: pdf {'same' if pdf else 'DIFFERS'}, state {'same' if st else 'DIFFERS'}")
    if pdf and st:
        for mode in ("mac", "nat"):
            os.remove(f"{S}/k/{d}.{mode}.st")
print(f"{len(docs) - bad}/{len(docs)} documents identical; native {tot_n}, macro {tot_m}")
