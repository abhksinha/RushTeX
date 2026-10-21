# Display spacing, floats, title/TOC, accents and TeX's page builder
Author: abhi+claude | Date: 2026-10-18
Files: src/layout/layout_engine.rs, src/layout/layout_float.rs, src/layout/layout_pagebuilder.rs, src/index/index_toc.rs, src/math/math_layout.rs, src/math/math_matrix.rs, src/math/math_parser.rs, src/frontend/frontend_parser.rs, src/backend/backend_shape.rs, src/backend/backend_pdf.rs, src/tables/tables_model.rs

## Context
A 10-page mathematics survey compiled with rushtex had a number of problems: captions overlapping text, a table split from its caption and no float placement, text cramped after displays, a NewCMMath fallback font being embedded, missing `\TeX`, `\ `, `\"o`, and page breaks that differed from pdfLaTeX.

## Decision
- Interline glue now follows TeX §679 using a real `\prevdepth`; line heights and depths come from the TFM metrics. Display skips follow §1199–1206. A number that does not fit goes on its own line below the display with no `\belowdisplayskip` (§1205). A display alignment's rows are lines of the vertical list, with amsmath `\openup\jot` and a number per row unless `\notag`.
- Floats follow LaTeX's output routine: `h`, then `t` if it fits (`\topnumber`, `\topfraction`), otherwise the deferred list, and a float page when the float is too tall. Each float is captured into its own op list and shifted into place.
- Page breaking is TeX's page builder (§980–1028). Material is set immediately and may run past the bottom. Every legal breakpoint is costed: page badness from the finite stretch/shrink of the vertical glue, plus the club, widow, broken, `\@secpenalty`, `\postdisplaypenalty` and `\maxdepth` contributions. When the page is overfull, the ops after the best breakpoint move to the next page and the remaining glue shrinks. TOC page numbers are corrected when a heading moves.
- Matrix and cases delimiters use Rule 19 (`max(.901z, z-5pt)`), as amsmath's `\left…\right` does. They had been one extensible repeat too tall.
- Section headings use interline glue with `\lineskip` against the previous depth, and the measured height of the first line instead of 0.7em.
- Text accents use TeX's `\accent` (§1123–1125) with OT1 accent glyphs. Ties and control spaces are glue at space factor 1000. The `\TeX` and `\LaTeX` logos follow latex.ltx.
- For Computer Modern, vector-glyph emission is skipped, so `\mathbb`/`\mathbf` no longer embed NewCMMath. Roots are cmsy/cmex surds, as in TeX.
- `\usepackage{a,b}` loads each package.

## Rejected
- Moving a whole display or paragraph to the next page when its last line would not fit (the old `ensure_room`). It cannot reproduce TeX's choices driven by shrink and penalties.
- `Vec<bool>` for the per-row `\notag` flags. It pushed `MatrixData` over 64 bytes, so a `u64` mask is used instead.

## Measured
- Math survey: all 10 page breaks are identical to pdfLaTeX, as are the TOC page numbers. Pages 2, 3, 9 and 10 have every baseline within 0.3pt. Elsewhere the offsets are at most a few points, caused by differences in the shrink inventory on overfull pages (float separators, section skips). Displays measured in isolation match within 0.1pt.
- Cases/pmatrix/bmatrix/vmatrix box height and depth now equal TeX's to 0.01pt (cases was 1.5pt too tall).
- Tests: 223 pass, with the 4 known NewCM font failures. Clippy is clean. Parity harness: 38/38 A, `table_float_caption` 98.8% (was 97.9%).

## Open
- The shrink inventory is incomplete: `\floatsep`/`\textfloatsep` shrink, and display glue removed by `\addvspace` before a heading still counts as shrinkable.
- `\raggedbottom` is assumed; `\flushbottom` (twoside) would need page stretch.

## Follow-up: line breaks and page shrink (same day)

### Decision
- Inline math glue between top-level atoms carries TeX's stretch/shrink (`\medmuskip` 4mu plus 2mu minus 4mu, `\thickmuskip` 5mu plus 5mu). This fixed every line-break difference in the survey.
- TeX breaks after `--`/`---`: `wrapup` tests the hyphen character before the ligature is packed (tex.web §1035). The old rule that forbade it was wrong.
- `\addvspace` cancels the shrink of the skip it replaces. `\textfloatsep` (plus 2pt minus 4pt) joins the page glue, because `\@makecol` sets the floats and the text to `\@colht` together.
- A display too wide for its number shrinks its top-level `\medmuskip` glue to fit before the number is dropped to the next line (§1201). The number's margin is the math quad, not max(quad, 12pt).
- amsmath's `\frac`, `\dfrac`, `\binom` and `\genfrac` are Ord atoms (braced); only the primitive `\over` family is Inner.
- The math spacing table had `\nonscript` spaces active in script style (after punctuation, around Inner); it now follows §764. The em LUT is derived from the mu table at compile time.
- With `lmodern`, `lmex10` is `sfixed`: script-style operators are the 10pt glyphs, not amsmath's cmex7.
- Caption tags apply the cmr10 `Ta` kern.

### Measured
- Survey vs pdfLaTeX: 203/209 text spans identical, x and y within 0.3pt for 98% (y median 0.03pt, p95 0.13pt). Pixel coverage within 1px is 96.9–100% per page (was 72.7–99.9%).
- Performance against e4f7250 (7 runs, median): the 100k stress doc goes from 30.2 to 31.7 ms (+5%), RSS 7.0 to 7.1 MB. The 176-page math doc stays at 24.4 ms and 7.1 MB.
- `table_float_caption` 98.8% to 99.8%; harness 38/38 A.

### Open
- rushtex uses CM lig/kern programs for `lmodern`. Latin Modern `rm-lm*` TFMs have about 2450 lig/kern entries against CM's 192 (for example `Av`, `Fc`, no `f)` kern), with widths equal to 1e-5 em. Generate the LM tables for the CM family.
- The +5% on the stress doc is not profiled. Suspects are per-character `line_extent` heights and per-line page breakpoints.
