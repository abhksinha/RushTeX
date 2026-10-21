# Common LaTeX text commands and front matter
Author: abhi+claude | Date: 2026-10-19
Files: `src/expand/expand_mod.rs`, `src/frontend/frontend_parser.rs`, `src/frontend/frontend_ast.rs`, `src/layout/layout_engine.rs`, `src/lib.rs`

## Context
Follow-up to `memorycastle/journal/2026-10-19-real-world-samples.md`: the user asked to store `\title`/`\author`/`\date` until `\maketitle` and to add the most common missing LaTeX commands. Geometry work from the same task is in `memorycastle/journal/2026-10-19-lists-listings-geometry.md` and `memorycastle/journal/2026-10-19-vertical-mode-page-builder.md`.

## Decision
- The expander keeps `\title`, `\author`, `\date` in `front_matter` until `\begin{document}` and replays them there, so macros defined after `\title` work.
- Font declarations are `Inline::FontDecl` acting on the rest of the group (`FontDecl::apply`); `\textrm`, `\emph` and friends are groups starting with one; `check_icr` adds `\check@icr`'s italic correction. Declaration environments (`em`, `sloppypar`, sizes) become groups in the expander (`EnvKind::Declaration`).
- `\, \: \; \! \quad \qquad \enskip` and the kerns are `Inline::EmSpace` in the current font's em; `\@` is `Inline::SpaceFactor`; `\paragraph` is a run-in heading spliced into the next paragraph.

## Rejected
- Expanding `\title` where it appears: macros defined later are unknown, as in `testmath` and `amsthdoc`.
- Font switches as wrapper nodes around the following inlines: a declaration ends at the group end, not at a node boundary.

## Measured
- `\emph`, `\paragraph` and declaration environments match pdfLaTeX within 0.02pt.
- Samples: `lppl` compiles 8 pages like pdfLaTeX; `sample2e` stops at `\footnote`, `amsthdoc` at `\setlength\leftmargini{2em}`, `testmath` at line 112.

## Open
- Unsupported: `\footnote`, `\hfill`, `\underline`, `\fbox`, `\parbox`, `\raisebox`, `\phantom`, `\rule`, `\begingroup`, `\fontfamily`, `\setlength` without braces. `\_` prints the OT1 dot accent; `\slshape` uses italic; no cmbxti10.
- The five CTAN samples are not in the CI parity corpus: three still stop with errors and would fail CI.
