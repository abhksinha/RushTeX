# RushTeX LaTeX-Superset Roadmap

Governing documents: `Resource_Conscious_Software_Engineering.md` (RCSE) and
`god_instruction.md`. Section numbers in brackets cite RCSE.

## Architectural decision: an expansion stage before the parser

The frontend AST borrows `&'a str` slices directly from the source text, and the
parser rejects unknown control sequences. User macros, cross-references, and
generated text (numbers, dates, labels) therefore cannot be spliced into the
token stream after parsing. They are resolved by a dedicated **expansion stage**
(`src/expand/`) that runs once, before the parser:

```text
source ──(no trigger commands?)──────────────► parser   (borrowed, zero copy)
   └────(triggers present)──► expand ──► String ──► parser
```

- **Common case stays free** [16, 44]: a single byte scan looks for trigger
  control words. Documents without them borrow the original source (no
  allocation, no copy).
- **One pass + patch** [19, 28]: expansion records label values and reference
  sites while it scans; forward references are patched in a single splice at
  the end, not by re-running the whole expansion.
- **Bounded** [23, 38]: an explicit expansion-count limit, input-stack depth
  limit, and output-size limit turn runaway recursion (`\def\a{\a}`) into a
  diagnostic instead of unbounded memory growth.
- **Contiguous input stack** [7, 11]: pending macro bodies live in a `Vec` of
  frames with byte cursors; plain-text runs are copied in bulk between `\`
  and `%` bytes, not character by character.
- Verbatim regions (`verbatim`, `lstlisting`, `\verb`) and comments are copied
  without expansion.

Layout-level features (alignment, sizes, colour, footnotes, links) need engine
support and are implemented in `frontend`/`layout`/`backend` directly.

## Phases

| Phase | Scope | Stage | Status |
|---|---|---|---|
| 1 | `\newcommand`, `\renewcommand`, `\providecommand` (optional default arg), `\def` (undelimited and delimited parameters), `\let`, `\newenvironment`/`\renewenvironment`, `\newif` + `\iftrue`/`\iffalse`/`\ifx`/user conditionals with `\else`/`\fi`, user counters (`\newcounter`, `\setcounter`, `\addtocounter`, `\stepcounter`, `\arabic`, `\roman`, `\Roman`, `\alph`, `\Alph`, `\the<counter>`), `\today` | expand | done |
| 2 | `\label`, `\ref`, `\eqref`, `\cite`, `thebibliography`/`\bibitem`, `\newtheorem` (shared and parent counters, starred), `proof` | expand | done |
| 3 | `center`, `flushleft`, `flushright`, `\centering`, `\raggedright`, `\raggedleft`; font sizes `\tiny`…`\Huge`; `\textsc`; colour (`xcolor`: `\textcolor`, `\color`, `\definecolor`) | frontend + layout + backend | planned |
| 4 | `\footnote`, `\pageref` (needs layout positions), `hyperref` (`\href`, `\url`, internal links, PDF outline bookmarks) | layout + backend | planned |
| 5 | `report`/`book` classes, `\chapter`, `\appendix`, `fancyhdr` headers/footers | config + layout | planned |
| 6 | `minipage`, `\parbox`, `\mbox`, `\makebox`, text-mode `\hbox`/`\vbox`/`\kern`/`\raise`; `\twocolumn`, `multicol` | layout | planned |
| 7 | Sans-serif text face (`\textsf`, `\sffamily`) alongside the main family | backend fonts | planned |
| 8 | BibTeX `.bib` reading (`\bibliography`, `\bibliographystyle`) | expand (I/O, lazy) | planned |
| 9 | Unicode/OpenType text fonts (`fontspec`), glyph subsetting via the existing CID path | backend | planned |
| 10 | Multilingual hyphenation and `babel`/`polyglossia` | layout | planned |
| 11 | Graphics: `picture`, then a TikZ/PGF subset | new subsystem | planned |
| 12 | Full TeX semantics: catcodes, `\expandafter`, `\csname`, registers, output routines | expand → token engine | planned |

Phase 12 is the step from "LaTeX-compatible documents" to "TeX superset": the
text-level expander becomes a token-level engine with category codes. The
expansion stage boundary established in Phase 1 is where that engine plugs in,
so no later phase depends on the current text-level implementation details.

## Resource budgets

- Documents with no trigger commands: **zero** extra allocations, one linear
  scan.
- Documents with macros: one output `String` (reserved at input size + 1/8),
  one frame stack, one label table; forward-reference patching adds at most one
  extra copy and only when references exist.
- Hard limits: 1,000,000 expansions, input stack depth 1,024, output size
  `max(64 MiB, 64 × input)`.
