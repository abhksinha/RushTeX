#!/usr/bin/env python3
"""Writes the native benchmark documents of 10 pages or more (deterministic, seeded).

  scripts/bench/gen_bench.py        # writes native-math-20p.tex, native-text-61p.tex and
                                    # native-tables-29p.tex here

Only constructs the native job runs are used, so every document stays on the native path;
`scripts/native_probes.sh DOC` checks each one against pdflatex.
"""
import os, random, re

HERE = os.path.dirname(os.path.abspath(__file__))
# Real prose for the text benchmark: the words and punctuation of these guides in order,
# commands, math, comments and arguments of environments removed. A word memo or a
# hyphenation cache meets the vocabulary of real documents, not a synthetic one.
PROSE = ["clsguide.tex", "usrguide.tex", "fntguide.tex", "encguide.tex", "cfgguide.tex", "amsldoc.tex"]

def prose_words():
    out = []
    for name in PROSE:
        t = open(os.path.join(HERE, "..", "parity", "corpus", name), encoding="latin-1").read()
        t = re.sub(r"(?<!\\)%.*", " ", t)
        t = re.sub(r"\\begin\{(verbatim|lstlisting|tabular|array|align\*?|equation\*?)\}.*?\\end\{\1\}", " ", t, flags=re.S)
        t = re.sub(r"\$[^$]*\$", " ", t)
        t = re.sub(r"\\[A-Za-z@]+\*?(\[[^\]]*\])?", " ", t)
        t = re.sub(r"\\.", " ", t)
        t = re.sub(r"[{}~&#^_|<>=\[\]\"`'()/*+@-]", " ", t)
        for w in t.split():
            if re.fullmatch(r"[A-Za-z]+[.,;:]?", w):
                out.append(w)
    return out
WORDS = ("energy manifold operator limit sequence regular field theorem flow proof bounded partition "
         "converges lemma kernel measure integral domain compact module norm group lattice density "
         "boundary solution series space ring function estimate uniformly").split()

def sentence(r, n=None):
    n = n or r.randint(8, 18)
    w = [r.choice(WORDS) for _ in range(n)]
    return w[0].capitalize() + " " + " ".join(w[1:]) + "."

def para(r, k=5, inline=()):
    out = []
    for i in range(k):
        s = sentence(r)
        if inline and r.random() < 0.5:
            s = s[:-1] + " where " + r.choice(inline) + " holds."
        out.append(s)
    return " ".join(out)

INLINE = [r"$x_{n+1} = x_n - \frac{f(x_n)}{f'(x_n)}$", r"$\sqrt{a^2+b^2} \le |a| + |b|$",
          r"$\mathbb{R}^{n} \subset \mathbb{C}^{n}$", r"$\mathcal{O}(n \log n)$",
          r"$\alpha \leq \beta \lesssim \gamma$", r"$\mathbf{u} \cdot \mathbf{v} = 0$",
          r"$\lim_{n\to\infty} a_n = L$", r"$\sum_{k} 2^{-k} = 2$", r"$\widehat{f}(\xi)$",
          r"$\sin^2\theta + \cos^2\theta = 1$", r"$\det A \neq 0$", r"$\gcd(m,n) = 1$"]

DISPLAYS = [
    r"\int_{-\infty}^{\infty} e^{-x^{2}} \, dx = \sqrt{\pi}",
    r"\sum_{n=1}^{\infty} \frac{1}{n^{2}} = \frac{\pi^{2}}{6}",
    r"i\hbar \frac{\partial}{\partial t} \Psi(\mathbf{r},t) = \hat{H} \Psi(\mathbf{r},t)",
    r"x_{1,2} = \frac{-b \pm \sqrt{b^{2} - 4ac}}{2a}",
    r"\left( \sum_{k=1}^{n} a_k b_k \right)^{2} \le \left( \sum_{k=1}^{n} a_k^{2} \right) \left( \sum_{k=1}^{n} b_k^{2} \right)",
    r"\lim_{n\to\infty} \left( 1 + \frac{1}{n} \right)^{n} = e",
    r"\nabla \cdot \mathbf{E} = \frac{\rho}{\varepsilon_0}",
    r"\mathcal{F}(\xi) = \int_{\mathbb{R}} f(x) \, e^{-2\pi i x \xi} \, dx",
    r"\left\| f \right\|_{2} = \left( \int_{0}^{1} \left| f(t) \right|^{2} dt \right)^{1/2}",
    r"\frac{d}{dx} \left( \frac{u}{v} \right) = \frac{u'v - uv'}{v^{2}}",
    r"\binom{n}{k} = \frac{n!}{k!\,(n-k)!}" if False else r"\frac{n!}{k!\,(n-k)!} = \frac{n}{k} \cdot \frac{(n-1)!}{(k-1)!\,(n-k)!}",
    r"e^{i\pi} + 1 = 0",
]

class Prose:
    """Sentences from the real word stream, consecutive, so paragraphs read as the guides do."""
    def __init__(self):
        self.words, self.at = prose_words(), 0

    def para(self, n):
        w = []
        for _ in range(n):
            w.append(self.words[self.at % len(self.words)])
            self.at += 1
        text = " ".join(w)
        text = text[0].upper() + text[1:]
        return text.rstrip(".,;:") + "."

def table(t):
    """A results table as papers write them: a `tabular` in `\\centering` or `center`, a
    `\\multicolumn` heading, `\\hline`, a `\\cline`, bold column heads, numbers."""
    cols = t.randint(3, 6)
    spec = t.choice(["l" + "c" * (cols - 1), "|l|" + "r|" * (cols - 1), "l@{}" + "r" * (cols - 1), "l*{%d}{c}" % (cols - 1)])
    rows = [r"\begin{tabular}{%s}" % spec, r"\hline",
            r"\multicolumn{%d}{c}{\bfseries %s} \\ \hline" % (cols, sentence(t, 4)[:-1]),
            " & ".join([r"\bfseries " + w for w in ["Method"] + [t.choice(WORDS).capitalize() for _ in range(cols - 1)]]) + r" \\ \cline{2-%d}" % cols]
    for _ in range(t.randint(4, 9)):
        rows.append(" & ".join([t.choice(WORDS)] + ["%d.%02d" % (t.randint(0, 99), t.randint(0, 99)) for _ in range(cols - 1)]) + r" \\")
    rows += [r"\hline", r"\end{tabular}"]
    if t.random() < 0.5:
        return [r"\begin{center}"] + rows + [r"\end{center}"]
    return [r"{\centering"] + rows + [r"\par}"]

def doc(title, sections, per, r, math, prose=None, tables=None):
    out = [r"\documentclass{article}", r"\usepackage{amssymb}", rf"\title{{{title}}}",
           r"\author{R. TeX}", r"\date{24 October 2026}", r"\begin{document}", r"\maketitle",
           r"\begin{abstract}", para(r, 4, INLINE if math else ()), r"\end{abstract}", r"\tableofcontents"]
    eq = 0
    for s in range(1, sections + 1):
        out.append(rf"\section{{{sentence(r, 3)[:-1]}}}\label{{sec{s}}}")
        for p in range(per):
            if p and p % 3 == 0:
                out.append(rf"\subsection{{{sentence(r, 3)[:-1]}}}")
            out.append(prose.para(r.randint(60, 120)) if prose else para(r, r.randint(4, 7), INLINE if math else ()))
            if math and r.random() < 0.7:
                eq += 1
                out += [r"\begin{equation}", "  " + r.choice(DISPLAYS) + rf" \label{{eq{eq}}}", r"\end{equation}"]
                out.append(rf"By (\ref{{eq{eq}}}) in Section~\ref{{sec{s}}}, " + para(r, 2)[0].lower() + para(r, 2)[1:])
            elif math and r.random() < 0.3:
                out.append(r"\[ " + r.choice(DISPLAYS) + r" . \]")
            if tables and tables.random() < 0.45:
                out += table(tables)
                out.append(r"\noindent " + prose.para(tables.randint(20, 40)) + r" {\itshape " + sentence(tables) + r"}\hspace{.05\textwidth}" + sentence(tables))
            if r.random() < 0.15:
                out.append(sentence(r) + r"\footnote{" + sentence(r) + "}")
            if r.random() < 0.1:
                out += [r"\begin{itemize}"] + [r"\item " + sentence(r) for _ in range(3)] + [r"\end{itemize}"]
            out.append("")
    out.append(r"\end{document}")
    return "\n".join(out) + "\n"

for name, title, sections, per, seed, math, prose, tables in [
        ("native-math-20p.tex", "Equations Throughout", 6, 9, 12, True, None, None),
        ("native-text-61p.tex", "A Long Text", 20, 14, 60, False, Prose(), None),
        ("native-tables-29p.tex", "Tables and Text", 10, 8, 33, False, Prose(), random.Random(34))]:
    text = doc(title, sections, per, random.Random(seed), math, prose, tables)
    with open(os.path.join(HERE, name), "w") as f:
        f.write(text)
