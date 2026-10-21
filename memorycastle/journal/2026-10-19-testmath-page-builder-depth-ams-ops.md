# testmath pages 14–19: alignment row glue, `\text` space factor, `\pmb`, multiple integrals
Author: abhi+claude | Date: 2026-10-19
Files: `src/layout/layout_engine.rs`, `src/math/math_parser.rs`, `src/math/math_layout.rs`, `src/math/math_ast.rs`, `src/math/math_bigop.rs`, `src/tables/tables_layout.rs`

## Context
Continued from `memorycastle/journal/2026-10-19-testmath-float-pages.md`. Pages 14, 15, 18 and 19 were off.

## Decision
- amsmath alignments (`multline`, `gather`, `align`) end every row with `\penalty10000` and `\vskip0pt`. So at `\postdisplaypenalty` the last row's full depth is on the page, not just the part beyond `\maxdepth` (`note_glue(0.0)` when `rows.is_some()`).
- `\text` words follow TeX's space factor (`update_space_factor`). A space after a sentence end is `MathSkip::TextSpaceExtra`, which adds fontdimen 7.
- `\pmb` in math (`MathNode::Pmb`) is amsbsy's three copies at −.8mu, −.4mu (raised .5mu) and 0. Its class follows `\binrel@`.
- `\iiiint` and `\idotsint` are integral BigOps. With limits, multiple integrals centre their limits on amsmath's `\ints@b` box: 7mu (9mu in display) of leading kern, and no italic correction.

## Rejected
- Comparing natural heights with a taller `\textheight` in the preamble. rushtex ignores it there, so the page breaks differently. Compare with `RUSHTEX_TRACE_PAGES` against TeX's `\tracingpages` instead.

## Measured
- testmath 56.4% → 63.1%. Page 14: 87%. Page 15: 72% → 92%. Page 18: 19% → 98%. Page 19: 9% → 96%.
- `sample2e`, `small2e` and `lppl` are at 100%. Tests: 234 pass, plus the 4 known NewCM failures. Clippy is clean.

## Open
- `\setlength` inside a group (`\multlinegap` 0pt on page 14) needs TeX grouping for lengths.
- Footnote math needs the cmmi8/cmsy8 masters (page 11). See `memorycastle/journal/2026-10-19-footnote-insertions.md`.
- `\widetilde` glyphs report as unmatched because of how they are extracted (a measurement artefact).
