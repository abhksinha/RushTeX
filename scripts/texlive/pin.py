#!/usr/bin/env python3
"""The pinned TeX Live snapshot (progress.md A1): what RushTeX's zero shift is proven against.

    scripts/texlive/pin.py files     # run the corpus with RT_FILES, list every TeX file read (files.tsv)
    scripts/texlive/pin.py record    # manifest.tsv: TeX Live, pdfTeX, LaTeX kernel, package revisions
    scripts/texlive/pin.py check     # does this installation still match? (exit 1 on drift)
    scripts/texlive/pin.py pack OUT.tar.xz     # the files, their search paths and the references
    scripts/texlive/pin.py unpack TAR DIR      # where no TeX Live is installed (CI); prints the env

The snapshot holds only the files the corpus reads, under one directory per search root, so it
is relocatable: `unpack` rewrites the search-path file (`RUSHTEX_KPSE_PATHS`) to where it lands.
"""

import concurrent.futures
import glob
import hashlib
import io
import os
import re
import subprocess
import sys
import tarfile

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
HERE = os.path.join(ROOT, "scripts", "texlive")
CORPUS = os.path.join(ROOT, "scripts", "parity", "corpus")
PARITY = os.path.join(ROOT, "target", "parity")
FILES = os.path.join(HERE, "files.tsv")
MANIFEST = os.path.join(HERE, "manifest.tsv")
KPSE = os.path.join(PARITY, "kpse-paths")


def sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for b in iter(lambda: f.read(1 << 20), b""):
            h.update(b)
    return h.hexdigest()


def smoke(env):
    env = dict(os.environ, RT_FILES="1", SOURCE_DATE_EPOCH="1792454400", FORCE_SOURCE_DATE="1", **env)
    cmd = ["cargo", "test", "-q", "--release", "--lib", "format_smoke", "--", "--ignored", "--nocapture"]
    r = subprocess.run(cmd, env=env, capture_output=True, text=True, cwd=ROOT)
    return {l[5:] for l in (r.stdout + r.stderr).split("\n") if l.startswith("FILE ")}


def cmd_files():
    subprocess.run(["cargo", "test", "-q", "--release", "--lib", "--no-run"], cwd=ROOT, check=True, capture_output=True)
    fmt = os.path.join(PARITY, "pdflatex.rfmt")
    jobs = [{"RT_FORMAT": "pdflatex.ini", "RT_FMT": os.path.join(PARITY, "pin-check.rfmt")}]
    jobs += [{"RT_FMT_IN": fmt, "RT_FORMAT": p} for p in sorted(glob.glob(os.path.join(CORPUS, "*.tex")))]
    found = set()
    with concurrent.futures.ThreadPoolExecutor(max(1, (os.cpu_count() or 2) // 2)) as ex:
        for s in ex.map(smoke, jobs):
            found |= s
    found = sorted(p for p in found if not p.startswith(CORPUS + os.sep) and os.path.isfile(p))
    with open(FILES, "w") as f:
        for p in found:
            f.write(f"{p}\t{os.path.getsize(p)}\t{sha256(p)}\n")
    print(f"{len(found)} files, {sum(os.path.getsize(p) for p in found) / 1e6:.1f} MB -> {os.path.relpath(FILES, ROOT)}")


def latex_version():
    ltx = subprocess.run(["kpsewhich", "latex.ltx"], capture_output=True, text=True).stdout.strip()
    text = open(ltx, encoding="latin-1").read()
    date = re.search(r"\\edef\\fmtversion\s*\{([^}]*)\}", text).group(1).strip()
    pl = re.search(r"\\def\\patch@level\{(\d+)\}", text).group(1)
    return date, pl


def installation():
    tl = subprocess.run(["tlmgr", "--version"], capture_output=True, text=True).stdout.split("\n")
    pdftex = subprocess.run(["pdftex", "--version"], capture_output=True, text=True).stdout.split("\n")[0]
    date, pl = latex_version()
    return {"texlive": next((l for l in tl if l.startswith("TeX Live")), "?"), "tlmgr": tl[0], "pdftex": pdftex,
            "latex": f"{date} patch level {pl}"}


def cmd_record():
    info = installation()
    pkgs = subprocess.run(["tlmgr", "info", "--only-installed", "--data", "name,localrev"], capture_output=True, text=True).stdout
    with open(MANIFEST, "w") as f:
        for k, v in info.items():
            f.write(f"#{k}\t{v}\n")
        f.write(f"#files.tsv\t{sha256(FILES)}\n")
        for line in sorted(l for l in pkgs.split("\n") if l):
            name, rev = line.split(",")
            f.write(f"{name}\t{rev}\n")
    print(f"{info['texlive']}, {info['pdftex']}, LaTeX {info['latex']} -> {os.path.relpath(MANIFEST, ROOT)}")


def cmd_check():
    want = {l.split("\t")[0][1:]: l.rstrip("\n").split("\t")[1] for l in open(MANIFEST) if l.startswith("#")}
    have = installation()
    drift = [f"{k}: pinned {want[k]!r}, installed {have[k]!r}" for k in ("texlive", "pdftex", "latex") if want.get(k) != have[k]]
    for line in open(FILES):
        p, size, digest = line.rstrip("\n").split("\t")
        if not os.path.isfile(p):
            drift.append(f"missing {p}")
        elif os.path.getsize(p) != int(size) or sha256(p) != digest:
            drift.append(f"changed {p}")
    for d in drift[:40]:
        print(d)
    print(f"pin check: {'DRIFT, ' + str(len(drift)) + ' differences' if drift else 'matches the pinned snapshot'}")
    sys.exit(1 if drift else 0)


def search_paths():
    """kpathsea's search-path file as `Kpse::new` caches it: a key line, then one per format."""
    return open(KPSE).read().rstrip("\n").split("\n")


def cmd_pack(out):
    files = [l.split("\t")[0] for l in open(FILES)]
    text = search_paths()
    # A file's root is the texmf tree holding it: the path up to (not including) its first
    # standard top directory (fonts, tex, dvips, ...).
    tops = ("/fonts/", "/tex/", "/dvips/", "/web2c/", "/doc/", "/bibtex/", "/metapost/", "/scripts/")
    def root_of(p):
        cut = min((p.find(t) for t in tops if p.find(t) > 0), default=-1)
        return p[:cut] if cut > 0 else os.path.dirname(p)
    trees = sorted({root_of(p) for p in files})
    tree_id = {t: f"t{i}" for i, t in enumerate(trees)}
    # The search paths with each tree renamed into the snapshot. An element outside every tree
    # (a host directory the corpus never reads from) is dropped, so nothing outside the
    # snapshot is searched.
    def relocate(e):
        bang = "!!" if e.startswith("!!") else ""
        body = e[len(bang):]
        if not body.startswith("/"):
            return e
        for t in sorted(trees, key=len, reverse=True):
            if body == t or body.startswith(t + "/"):
                return bang + "@ROOT@/" + tree_id[t] + body[len(t):]
        return None
    lines = [text[0]] + [":".join(r for r in map(relocate, l.split(":")) if r is not None) for l in text[1:]]
    paths = "\n".join(lines)
    with tarfile.open(out, "w:xz") as tar:
        def add_bytes(name, data):
            ti = tarfile.TarInfo(name)
            ti.size = len(data)
            ti.mtime = 0
            tar.addfile(ti, io.BytesIO(data))
        for p in files:
            t = root_of(p)
            tar.add(p, arcname=f"tree/{tree_id[t]}/{p[len(t) + 1:]}", recursive=False)
        # One ls-R per tree, listing what the snapshot holds (kpathsea's database format).
        for t in trees:
            dirs = {}
            for p in files:
                if root_of(p) == t:
                    d, b = os.path.split(p[len(t) + 1:])
                    dirs.setdefault(d, []).append(b)
            ls = "% ls-R -- filename database for kpathsea; do not change this line.\n" + "".join(
                f"\n./{d}:\n" + "".join(f"{b}\n" for b in sorted(bs)) for d, bs in sorted(dirs.items()))
            add_bytes(f"tree/{tree_id[t]}/ls-R", ls.encode())
        add_bytes("kpse-paths", paths.encode())
        for f in (FILES, MANIFEST):
            tar.add(f, arcname=os.path.basename(f))
        # The references, and the corpus source each was made from (`parity.py`'s pinned mode
        # refuses a reference whose source has changed since).
        srcs = []
        for tex in sorted(glob.glob(os.path.join(CORPUS, "*.tex"))):
            stem = os.path.splitext(os.path.basename(tex))[0]
            ref = os.path.join(PARITY, "ref", stem + ".pdf")
            if os.path.isfile(ref) and os.path.getmtime(ref) >= os.path.getmtime(tex):
                tar.add(ref, arcname=f"ref/{stem}.pdf")
                log = ref[:-4] + ".log"
                if os.path.isfile(log):
                    tar.add(log, arcname=f"ref/{stem}.log")
                srcs.append(f"{os.path.basename(tex)}\t{sha256(tex)}\n")
        add_bytes("ref/sources.tsv", "".join(srcs).encode())
    print(f"{out}: {os.path.getsize(out) / 1e6:.1f} MB, {len(files)} files in {len(trees)} trees, sha256 {sha256(out)}")


def cmd_unpack(tar_path, dest):
    with tarfile.open(tar_path) as tar:
        # The "data" filter (no absolute paths, no links out) where this Python has it.
        if hasattr(tarfile, "data_filter"):
            tar.extractall(dest, filter="data")
        else:
            tar.extractall(dest)
    p = os.path.join(dest, "kpse-paths")
    text = open(p).read().replace("@ROOT@", os.path.join(os.path.abspath(dest), "tree"))
    with open(p, "w") as f:
        f.write(text)
    os.makedirs(os.path.join(PARITY, "ref"), exist_ok=True)
    for r in glob.glob(os.path.join(dest, "ref", "*")):
        os.replace(r, os.path.join(PARITY, "ref", os.path.basename(r)))
    print(f"export RUSHTEX_KPSE_PATHS={p}")


def main():
    a = sys.argv[1:]
    if a[:1] == ["files"]:
        cmd_files()
    elif a[:1] == ["record"]:
        cmd_record()
    elif a[:1] == ["check"]:
        cmd_check()
    elif a[:1] == ["pack"] and len(a) == 2:
        cmd_pack(a[1])
    elif a[:1] == ["unpack"] and len(a) == 3:
        cmd_unpack(a[1], a[2])
    else:
        sys.exit(__doc__)


if __name__ == "__main__":
    main()
