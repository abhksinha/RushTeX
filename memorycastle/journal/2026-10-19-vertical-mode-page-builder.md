# Vertical-mode material in TeX's page builder: rules, skips, tabulars, figures
Author: abhi+claude | Date: 2026-10-19
Files: `src/layout/layout_engine.rs`, `src/layout/layout_pagebuilder.rs`, `src/layout/layout_float.rs`, `src/tables/tables_layout.rs`, `src/frontend/frontend_parser.rs`, `src/frontend/frontend_ast.rs`, `src/lib.rs`

## Context
After the list and listing fixes the 100k stress document still had 271 to 276 pages against pdfLaTeX's 267. Each remaining gap was material placed or counted outside the page builder.

## Decision
Every block now goes through the page builder as TeX sees it: `\hrule` is a box with ignore-depth `\prevdepth` (`Layouter::ignore_depth`); skips carry stretch and shrink (`Block::VSpace`); an inline `tabular` is an axis-centred box (`layout_inline_tabular`); `figure` is a float (`capture_float`, graphicx draft box from `graphic_box_size`); `\parskip` only starts paragraphs and follows a heading's `\addvspace`; `\textfloatsep` is left out of break costs (`float_sep_shrink`); the footer centres in `text_span`.

## Rejected
- `ensure_room` for rules and figures: it breaks pages outside the builder, so pages ran into the footer or broke early.
- Counting `\textfloatsep` shrink in break costs: TeX takes the float area off `\pagegoal`, and the extra 4pt let rushtex fit lines pdfLaTeX moves on.
- `\parskip` before every block: positions matched, but the heading's `\addvspace` could not cancel the list's end glue, so shrink totals and page breaks differed.

## Measured
- 100k stress document: 277 pages before, 267 after; pdfLaTeX 267. Pages 1 to 55 start on the same line as pdfLaTeX's (compared with `\tracingpages` logs).
- `stress_40k` 78 pages, as pdfLaTeX. Harness 38/38 A; tests 234 pass with the 4 known NewCM failures; clippy clean; 100k latency 0.06 s, unchanged.
- `\hrule`, tabular after a paragraph and after a heading, top and `h` figures at 10 and 11pt: within 0.02pt.

## Open
- From page 56, page starts drift: pdfLaTeX shrinks page 55 to fit a heading after a verbatim block; rushtex breaks before it.
