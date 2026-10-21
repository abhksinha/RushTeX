# Footnotes as TeX insertions in the page builder
Author: abhi+claude | Date: 2026-10-19
Files: `src/layout/layout_float.rs`, `src/layout/layout_pagebuilder.rs`, `src/layout/layout_engine.rs`, `src/frontend/frontend_ast.rs`, `src/frontend/frontend_parser.rs`, `src/math/math_ast.rs`, `src/math/math_layout.rs`

## Context
`sample2e` stopped at `\footnote`. It is one of the five CTAN samples the user wants to compile exactly as pdfLaTeX does, and faster. Companion notes: `memorycastle/journal/2026-10-19-tex-input-spacing-rules.md`, `memorycastle/journal/2026-10-19-quote-verse-list-environments.md`.

## Decision
`\footnote` is `Inline::Footnote` (mark plus text). Before a paragraph is broken into lines, `set_footnotes` sets each footnote's text into a box (`capture_float`, which now swaps in a fresh `PageBuilder`). After the line holding the mark is added to the page, `insert_footnote_of` inserts the box, as in tex.web §1008. The page goal loses `\skip\footins` and the box height (`ins_total`, `goal_bottom`). At a break, `split_footnotes` keeps the boxes inserted before the break. `place_footnotes` draws `\footnoterule` and the boxes `\skip\footins` below the text, or at the bottom when `\newpage`/`\clearpage` ends the page.

## Rejected
- Footnote boxes set while the outer paragraph is being broken: setting them reuses `words_soa` and the line descriptors.
- Leaving the outer paragraph's `block_entry_advance` and `has_preceding_text` in force inside the box: the first footnote line moved up 2.5pt.

## Measured
- 10pt and 11pt test pages: marks and footnote lines within 0.05pt of pdfLaTeX. On `sample2e` page 2 the footnote line is 0.08pt low and the mark 0.25pt right.
- `\scriptspace` is a fixed 0.5pt (latex.ltx). Script sizes follow `\DeclareMathSizes` (`latex_math_sizes`), so 8pt text gets 6pt scripts.
- Harness 38/38 A. 100k stress document: 267 pages (pdfLaTeX 267), 0.062 s.

## Open
- The residual mark offsets come from math using scaled 10pt fonts. LaTeX uses cmr6 and the cmsy8 parameters at footnote size. This needs math design-size masters: cmmi6/8/9/12, cmsy6/8/9, and cmr6/8/9/12 math metrics.
- A footnote inside a float or minipage is dropped.
