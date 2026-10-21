# testmath parity from 97.4% to 99.6% strict
Author: abhi+claude | Date: 2026-10-20
Files: `src/math/math_layout.rs`, `src/math/math_parser.rs`, `src/math/math_ast.rs`, `src/math/math_fonts.rs`, `src/expand/expand_mod.rs`, `src/frontend/frontend_parser.rs`, `src/layout/layout_engine.rs`, `src/backend/backend_cm_fonts.rs`

## Context
The goal was at least 99% strict glyph parity on testmath (0.1pt tolerance, all 41 pages), with identical equation numbers and references. sample2e, small2e and lppl had to stay at 100%.

## Decision
Every fix follows a rule from tex.web, amsmath or latex.ltx:
- amsmath `\measure@`/`\x@calc@shift@rc` (`align_shift_and_sep`): eqnshift and alignsep shrink for rows whose number would collide. A nested alignment uses its natural width (`ctx.inner_align`).
- amsmath numbers each row. `number_rows` in expand splits rows at top-level `\\` and steps `equation` per row unless the row is `\tag`ged or `\notag`ged. A nested `align` in `gather` numbers its own rows, and `\label`s bind to their row. In layout, `display_rows` expands a numbered inner align into rows. `DisplayRows.outer_first`/`outer_last` keep the prevdepth/baselineskip on the outer rows.
- `\multline@` sets each line as `$\displaystyle{}#$`: a leading empty Ord on every line, which puts a thin space before an Op.
- Group-scoped `\setlength{\multlinegap}` is carried as `MathParams` (it replaced `delim_params`) into `MatrixExtras`.
- §755: an operator name's last character keeps its italic correction (`inf` = `f\/`).
- Rules 8 and 9: an overlined or underlined nucleus is a box, so a subscript sits at w+ic. The §738 accent script swap applies only to real accents.
- §1039: a discretionary follows every explicit `-`, including a leading one after math (`$x$-negligible`).
- `\not` is emitted before its relation, matching the pdfTeX stream order.
- An unbraced `\begin{alignat*}3` argument is now read correctly.
- `\colon` ends with `\mskip6mu plus1mu`. `MathSkip::MuGlue` keeps finite mu stretch for paragraph lines.
- Wide accents take cmex7 below text size (amsmath OMX), and the variant is chosen as in §738.
- CMEX wide accents map to the spacing modifiers U+02C6/U+02DC in ToUnicode, so text extraction matches pdfTeX.

## Rejected
- Adding ic to the overline/underline box width: our nucleus box plus the row's trailing ic already gives w+ic. Only the subscript position was wrong.
- Returning ic 0 for overline in `node_italic_correction_styled`: this broke the row's trailing kern.

## Measured
testmath strict went from 97.4% to 99.6% (42221/42400). Every equation number and `\ref` matches pdfLaTeX. The other three documents stay at 100%. Tests: 234 pass, plus the 4 known NewCM failures. Clippy is clean.

## Open
- Math design sizes. Footnote math needs cmmi8/cmsy8/cmr8 (with 6 for script). `\Large`/`\large` math needs cmmi12/cmsy10 at 14.4pt. These cover pages 11 and 28, about 170 glyphs.
- A script-style `\widetilde` skew is off by about 0.1pt (pages 31–33).
