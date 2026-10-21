#!/usr/bin/env python3
r"""Where TeX and LaTeX define something: one lookup over the reference sources.

  refindex.py NAME [NAME..]     every definition of NAME (\selectfont, hskip, line_break, ...)
  refindex.py §1057             a tex.web section (also pdftex:§N)
  refindex.py -g REGEX          names matching REGEX
  refindex.py -s NAME           also print the first hit's text (at most 80 lines)
  refindex.py -a NAME           all hits (default: the 10 most relevant)
  refindex.py --rebuild         rebuild the index (otherwise rebuilt when a source changes)

Sources, all read-only:
  texlive-source/texk/web2c/tex.web and pdftexdir/pdftex.web
      sections (§), procedures and functions, @d constants, @<module@> definitions,
      primitives (`primitive("hskip",hskip,..)`) and the main_control/expand cases that
      handle their command code (`hmode+hskip: append_glue`)
  latex-source/**/*.dtx       \begin{macro}{\name} / \begin{environment}{name} blocks
  every .ltx/.cls/.sty/.clo/.fd/.cfg/.def pdfLaTeX loaded for the parity corpus (read from
  target/parity/ref/*.log) plus latex.ltx and expl3-code.tex: \def, \let, \newcommand,
  \DeclareRobustCommand, \cs_new:Npn, \newif, \new<register>, \setlength assignments, ...

Each lookup also lists the comments in src/ that cite the name (`rushtex` hits): where
rushtex already implements it, or nothing when it does not yet.
Hits print as `kind  path:start-end  detail`; read them with `sed -n 'start,endp' path`.
The index is cached in target/refindex.json.
"""
import json, os, re, subprocess, sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CACHE = ROOT / "target/refindex.json"
WEB = {
    "tex": ROOT / "texlive-source/texk/web2c/tex.web",
    "pdftex": ROOT / "texlive-source/texk/web2c/pdftexdir/pdftex.web",
}
DTX = ROOT / "latex-source"
LOGS = ROOT / "target/parity/ref"
ALWAYS = ["latex.ltx", "expl3-code.tex"]
MAX_DEF_LINES = 40


def rel(p):
    p = Path(p)
    try:
        return str(p.relative_to(ROOT))
    except ValueError:
        return str(p)


# ----- WEB ------------------------------------------------------------------------------

SEC_RE = re.compile(r"^@(?: |\*|$|\t)")
PROC_RE = re.compile(r"^\s*(?:@p\s*)?(procedure|function)\s+([a-z_]+)")
DEF_RE = re.compile(r"^@d\s+([a-z_]+)\s*==?")
MODULE_RE = re.compile(r"^@<(.+?)@>=")
PRIM_RE = re.compile(r'primitive\("([^"]+)",\s*([a-z_]+)\s*,\s*([a-z_0-9]+)')
CASE_RE = re.compile(r"\b(?:vmode|hmode|mmode)\+([a-z_]+)|\b(?:any_mode|non_math)\(([a-z_]+)\)")


def index_web(tag, path, add):
    lines = path.read_text(errors="replace").split("\n")
    starts = [i for i, l in enumerate(lines) if SEC_RE.match(l)]
    sec_of = [0] * len(lines)
    for n, s in enumerate(starts, 1):
        end = starts[n] if n < len(starts) else len(lines)
        for i in range(s, end):
            sec_of[i] = n
        title = lines[s][1:].strip()[:60]
        add(f"§{n}" if tag == "tex" else f"{tag}:§{n}", "section", path, s + 1, end, title)

    def sec_range(i):
        n = sec_of[i]
        s = starts[n - 1] if n else 0
        e = starts[n] if n < len(starts) else len(lines)
        return s + 1, e, n

    cases = {}  # command code -> [(line, text)]
    for i, l in enumerate(lines):
        if m := PROC_RE.match(l):
            s, e, n = sec_range(i)
            add(m.group(2), m.group(1), path, s, e, f"{tag} §{n}")
        elif m := DEF_RE.match(l):
            s, e, n = sec_range(i)
            add(m.group(1), "@d", path, i + 1, i + 1, f"{tag} §{n}: {l[2:].strip()[:70]}")
        elif m := MODULE_RE.match(l):
            s, e, n = sec_range(i)
            add(m.group(1), "module", path, s, e, f"{tag} §{n}")
        for m in PRIM_RE.finditer(l):
            s, e, n = sec_range(i)
            add(m.group(1), "primitive", path, i + 1, i + 1, f"{tag} §{n}: cmd {m.group(2)}, chr {m.group(3)}")
        if ":" in l:
            for m in CASE_RE.finditer(l):
                cases.setdefault(m.group(1) or m.group(2), []).append(i)
    # Where each command code is acted on: main_control's mode+cmd cases.
    for cmd, at in cases.items():
        for i in sorted(set(at))[:6]:
            s, e, n = sec_range(i)
            add(cmd, "handler", path, i + 1, i + 1, f"{tag} §{n}: {lines[i].strip()[:80]}")


# ----- LaTeX ----------------------------------------------------------------------------

DTX_RE = re.compile(r"^%\s*\\begin\{(macro|environment)\}\{([^}]*)\}")
DTX_END_RE = re.compile(r"^%\s*\\end\{(macro|environment)\}")
CS = r"\\[A-Za-z@_:]+|\\."
DEF_PATTERNS = [
    ("def", re.compile(r"^\s*(?:\\(?:long|protected|global|outer)\s*)*\\[gex]?def\s*(" + CS + ")")),
    ("let", re.compile(r"^\s*(?:\\global\s*)?\\let\s*(" + CS + ")")),
    ("command", re.compile(r"^\s*\\(?:new|renew|provide)command\*?\s*\{?\s*(" + CS + ")")),
    ("command", re.compile(r"^\s*\\(?:DeclareRobustCommand|NewDocumentCommand|RenewDocumentCommand|DeclareDocumentCommand|ProvideDocumentCommand)\*?\s*\{?\s*(" + CS + ")")),
    ("environment", re.compile(r"^\s*\\(?:new|renew)environment\*?\s*\{([^}]+)\}")),
    ("environment", re.compile(r"^\s*\\(?:New|Renew|Declare)DocumentEnvironment\s*\{([^}]+)\}")),
    ("option", re.compile(r"^\s*\\DeclareOption\*?\s*\{([^}]*)\}")),
    ("expl3", re.compile(r"^\s*\\cs_(?:new|set|gset)(?:_protected)?(?:_nopar)?(?::[Nc][npwTFx]*|_eq:NN)\s*(" + CS + ")")),
    ("if", re.compile(r"^\s*\\newif\s*(" + CS + ")")),
    ("register", re.compile(r"^\s*\\new(?:dimen|skip|count|toks|box|length|muskip|read|write|insert)\s*\{?\s*(" + CS + ")")),
    ("set", re.compile(r"^\s*\\setlength\s*\{?\s*(" + CS + ")")),
    ("set", re.compile(r"^\s*(?:\\global\s*)?(\\[A-Za-z@]+(?:skip|width|height|depth|sep|indent|margin|size|penalty|parameter|fraction|number))\s*=?\s*[-\d.\\]")),
    ("textcommand", re.compile(r"^\s*\\DeclareText(?:Command|Symbol|Accent|Composite)(?:Default)?\s*\{?\s*(" + CS + ")")),
    ("mathsymbol", re.compile(r"^\s*\\DeclareMath(?:Symbol|Delimiter|Accent|Radical|Alphabet|Operator)\s*\{?\s*(" + CS + ")")),
    ("fontshape", re.compile(r"^\s*\\DeclareFontShape\s*\{([^}]*)\}\s*\{([^}]*)\}\s*\{([^}]*)\}\s*\{([^}]*)\}")),
    ("fontfamily", re.compile(r"^\s*\\DeclareFontFamily\s*\{([^}]*)\}\s*\{([^}]*)\}")),
    ("symbolfont", re.compile(r"^\s*\\(?:DeclareSymbolFont|SetSymbolFont)\s*\{([^}]*)\}")),
]


def index_dtx(path, add):
    lines = path.read_text(errors="replace").split("\n")
    stack = []
    for i, l in enumerate(lines):
        if m := DTX_RE.match(l):
            stack.append((i, m.group(1), m.group(2)))
        elif DTX_END_RE.match(l) and stack:
            s, kind, names = stack.pop()
            for name in re.findall(CS, names) if kind == "macro" else [names.strip()]:
                add(name, "dtx-" + kind, path, s + 1, i + 1, "")
        elif not l.startswith("%"):
            # Code ranges end with their macrocode block.
            end = next((k for k in range(i + 1, min(i + 200, len(lines))) if lines[k].startswith("%") and "\\end{macrocode}" in lines[k]), i)
            def_line(l, i, path, add, end)


def def_line(l, i, path, add, n_lines):
    if "\\" not in l:
        return
    for kind, rx in DEF_PATTERNS:
        if m := rx.match(l):
            if kind == "fontshape":
                name, detail = "/".join(m.groups()), ""
            elif kind == "fontfamily":
                name, detail = "/".join(m.groups()), ""
            else:
                name, detail = m.group(1), ""
            add(name, kind, path, i + 1, max(i + 1, min(i + MAX_DEF_LINES, n_lines)), detail)
            return


def index_plain(path, add):
    lines = path.read_text(errors="replace").split("\n")
    hits = []
    for i, l in enumerate(lines):
        if l.lstrip().startswith("%"):
            continue
        def_line(l, i, path, lambda *a: hits.append(a), len(lines))
    # A definition runs to the next one (at most MAX_DEF_LINES lines).
    for k, (name, kind, p, s, e, d) in enumerate(hits):
        nxt = hits[k + 1][3] - 1 if k + 1 < len(hits) else len(lines)
        add(name, kind, p, s, max(s, min(e, nxt)), d)


def loaded_files():
    """Every TeX input file pdfLaTeX opened for the corpus, from its logs."""
    found = set()
    for log in sorted(LOGS.glob("*.log")) if LOGS.exists() else []:
        raw = log.read_text(errors="replace").split("\n")
        # Logs wrap at 79 columns; rejoin before looking for paths.
        text = "".join(l if len(l) == 79 else l + "\n" for l in raw)
        for m in re.finditer(r"\((/[^\s()]+\.(?:cls|sty|clo|fd|ltx|cfg|def|tex))", text):
            p = Path(m.group(1))
            if p.exists() and "/parity/" not in str(p):
                found.add(p)
    r = subprocess.run(["kpsewhich", *ALWAYS], capture_output=True, text=True)
    found.update(Path(p) for p in r.stdout.split())
    return sorted(found)


# ----- build, query ---------------------------------------------------------------------

def sources():
    out = [p for p in WEB.values() if p.exists()]
    out += sorted(DTX.rglob("*.dtx")) if DTX.exists() else []
    return out + loaded_files()


def build(srcs):
    entries = {}

    def add(name, kind, path, s, e, detail):
        entries.setdefault(name, []).append([kind, rel(path), s, e, detail])

    for tag, p in WEB.items():
        if p.exists():
            index_web(tag, p, add)
    for p in srcs:
        if p.suffix == ".dtx":
            index_dtx(p, add)
        elif p.suffix != ".web":
            index_plain(p, add)
    stamp = {rel(p): p.stat().st_mtime for p in srcs}
    CACHE.parent.mkdir(parents=True, exist_ok=True)
    CACHE.write_text(json.dumps({"stamp": stamp, "entries": entries}))
    return entries


def load(rebuild=False):
    srcs = sources()
    if not rebuild and CACHE.exists():
        data = json.loads(CACHE.read_text())
        if data["stamp"] == {rel(p): p.stat().st_mtime for p in srcs}:
            return data["entries"]
    return build(srcs)


KIND_ORDER = ["section", "primitive", "handler", "procedure", "function", "module", "@d",
              "dtx-macro", "dtx-environment"]


def source_rank(path):
    """tex.web before pdftex.web; the executed latex.ltx and the commented .dtx before packages."""
    for k, key in enumerate(("web2c/tex.web", "/latex.ltx", "latex-source/base/", "pdftex.web", "latex-source/")):
        if key in path:
            return k
    return 9


def show(name, hits, text, limit):
    print(name)
    hits = sorted(hits, key=lambda h: (h[0] == "set", source_rank(h[1]),
                                       KIND_ORDER.index(h[0]) if h[0] in KIND_ORDER else 99, h[1], h[2]))
    for kind, path, s, e, detail in hits[:limit]:
        rng = f"{s}-{e}" if e != s else f"{s}"
        print(f"  {kind:15s} {path}:{rng}  {detail}".rstrip())
    if len(hits) > limit:
        print(f"  ... {len(hits) - limit} more (-a shows all)")
    if text and hits:
        _, path, s, e, _ = hits[0]
        p = Path(path) if Path(path).is_absolute() else ROOT / path
        lines = p.read_text(errors="replace").split("\n")[s - 1 : min(e, s + 79)]
        print("\n".join(f"{s + k:6d}  {l}" for k, l in enumerate(lines)))


def rushtex_cites(name, limit):
    """Comment lines in src/ that cite `name` (a § or a macro): what rushtex already models."""
    if name.startswith("pdftex:"):
        return
    pat = re.escape(name).replace("§", "§ ?") + (r"(?![0-9])" if name.startswith("§") else r"(?![A-Za-z@])")
    if not name.startswith(("§", "\\")):
        pat = r"\b" + pat
    rx = re.compile(pat)
    hits = []
    for f in sorted((ROOT / "src").rglob("*.rs")):
        for n, l in enumerate(f.read_text(errors="replace").split("\n"), 1):
            c = l.find("//")
            if c >= 0 and rx.search(l, c):
                hits.append(f"  {'rushtex':15s} {rel(f)}:{n}  {l[c:].strip()[:90]}")
    for h in hits[:limit]:
        print(h)
    if len(hits) > limit:
        print(f"  ... {len(hits) - limit} more rushtex citations (-a)")


def main():
    args = sys.argv[1:]
    if not args or args[0] in ("-h", "--help"):
        print(__doc__)
        return
    rebuild = "--rebuild" in args
    text = "-s" in args
    limit = 10**6 if "-a" in args else 10
    args = [a for a in args if a not in ("--rebuild", "-s", "-a")]
    entries = load(rebuild)
    if rebuild and not args:
        print(f"{len(entries)} names, {sum(len(v) for v in entries.values())} definitions -> {rel(CACHE)}")
        return
    if args[0] == "-g":
        rx = re.compile(args[1])
        for name in sorted(n for n in entries if rx.search(n))[:200]:
            show(name, entries[name], False, limit)
        return
    for q in args:
        cands = [q, "\\" + q.lstrip("\\"), q.lstrip("\\")]
        if q.startswith("§") or q.startswith("pdftex:§"):
            cands = [q]
        name = next((c for c in cands if c in entries), None)
        if name is None:
            print(f"{q}: not found (try -g)")
            continue
        show(name, entries[name], text, limit)
        rushtex_cites(name, limit)


if __name__ == "__main__":
    main()
