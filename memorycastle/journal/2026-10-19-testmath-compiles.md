# testmath compiles: parser and expander gaps closed; resume guide
Author: abhi+claude | Date: 2026-10-19
Files: `src/frontend/frontend_parser.rs`, `src/frontend/frontend_ast.rs`, `src/expand/expand_mod.rs`, `src/layout/layout_engine.rs`, `src/layout/layout_pagebuilder.rs`, `src/math/math_ast.rs`, `src/math/math_layout.rs`, `src/backend/backend_pdf.rs`, `src/index/index_mod.rs`, `src/index/index_model.rs`, `src/config/config_mod.rs`

## Context
The user's goal: the five CTAN samples (`sample2e`, `small2e`, `lppl`, `testmath`, `amsthdoc`) should compile identically to pdfLaTeX and be much faster. Speed is the priority, and there is no TeX interpreter. Only differences of 0.1pt or more get fixed, each with a general rule taken from latex.ltx or tex.web. This task got `testmath` to compile, following `memorycastle/journal/2026-10-19-lppl-list-skips-trace.md`.

## Decision
Added, each following its LaTeX source:
- `\AmS`: `LogoKind::AmS` in `tex_logo`.
- `\begin{figure}`: `\vspace` becomes `FigureData.vskip`, and `\caption[..]` is accepted. Any command `parse_figure` can't make progress on is now skipped; this was an infinite loop before.
- Brace groups holding a display or list environment are read through (`block_group_end`, `open_block_groups`).
- `\chardef` (`define_chardef`) and `\char<n>`.
- The `math` environment (`EnvKind::InlineMath`).
- `\qed`.
- `\newdimen`, `\newskip`, `\newcount` and `\newlength` only consume their name.
- `\appendix`: `ConfigChange::Appendix`, the `APPENDIX_SECTION` flag, `write_section_number`, and the expander's `appendix` flag.
- Text `\rule` becomes `MathNode::Rule`, with `em`/`ex` taken from the document size (`parse_dimension_pt_in`).
- `\\` inside groups and titles is now `Inline::Break`.
- `\markboth`/`\markright` in vertical mode become `Block::VMark` (`note_vmark`): it stops `\addvspace` merging, as TeX's mark node does.
- The following are ignored rather than modelled: `\setlength` inside paragraphs, `\delimiterfactor` and similar math parameters, and `@`-names under `\makeatletter`.

## Rejected
- A TeX interpreter for testmath's `\@makecol` redefinition (margin rules): ruled out by the speed priority. The margin rules are not drawn.

## Measured
- Glyphs within 0.1pt:
  - `testmath`: 0% (did not compile) → 5.2%. pdfLaTeX has 41 pages, rushtex 45. Compile time is 12ms.
  - `lppl`, `sample2e` and `small2e`: 100%.
  - `docs/manual.tex`: 61.0%.
- Tests: 234 pass, plus the 4 known NewCM failures. Clippy is clean.

## Open
Next drifts in `testmath`, in order:
1. Page 1, `\AmS-\LaTeX`: the hyphen after the S is 0.75pt late. rushtex's math box adds cmsy S's italic correction, but `\AmS` is text and gets none.
2. Page 2 onwards: `\pagestyle{headings}` running heads (cmsl10 `\markboth` text and the page number at the top) are missing. That moves every page.
3. Then compare pages one by one. Missing amsmath commands may surface: `\hdotsfor`, `\iiiint`, `\varliminf`, `\numberwithin`, `\pmb`, `\leftroot`/`\uproot`, capital accents.

After that comes `amsthdoc`: get it to compile with the minimum work.

How to resume (the scratchpad is session-local, so recreate the tools if they are gone):
- Reference PDFs: run `pdflatex` twice on each sample, from CTAN `macros/latex/base` and `amsmath`.
- Parity: for each page, compare every glyph `(char, x, y)` from PyMuPDF `rawdict` and count matches within 0.1pt.
- Page breaks: run `pdflatex '\tracingpages=1 \input X'` and `RUSHTEX_TRACE_PAGES=1 rushtex X.tex`. rushtex prints `% pg= over= plus minus p=`, where `over` is `t - g`. Compare the breakpoints of each page in order.
