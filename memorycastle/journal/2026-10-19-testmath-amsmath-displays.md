# testmath: amsthm heads, amsmath displays (hdotsfor, multline, split), make_ord kerns
Author: abhi+claude | Date: 2026-10-19
Files: `src/expand/expand_mod.rs`, `src/frontend/frontend_ast.rs`, `src/frontend/frontend_parser.rs`, `src/config/config_mod.rs`, `src/layout/layout_engine.rs`, `src/layout/layout_pagebuilder.rs`, `src/math/math_ast.rs`, `src/math/math_parser.rs`, `src/math/math_layout.rs`, `src/math/math_display.rs`, `src/math/math_mod.rs`, `src/backend/backend_scaled.rs`, `src/backend/backend_cm_fonts.rs`

## Context
This note covers the testmath page-by-page work after `memorycastle/journal/2026-10-19-testmath-plan.md`. Each fix is a general rule taken from amsmath, amsthm, latex.ltx or tex.web.

## Decision
- amsthm head (`begin_theorem`):
  - The head number is `\textup`.
  - The note is `\normalfont`.
  - `\thm@headsep` is `HGlue` 5pt plus1pt minus1pt (`parse_glue3_pt`).
  - The body goes through `ignore_spaces`.
  - The italic body comes from the layout's `body_face`, not from rewriting the source. `italicize_paragraphs` was removed.
- `\eqref` emits `\textup{(N)}`, so latex.ltx `\maybe@ic` adds the italic correction.
- amsmath `\hdotsfor[s]{n}` is `MathNode::HDotsFor`. It spans n columns as `\xleaders` of `\mkern 1.5s mu . \mkern 1.5s mu`, spaced by tex.web §627 (`paint_hdotsfor`). A `&` after dots is the end of the formula (`\extra@`), so a thin space follows.
- make_ord (tex.web §752): an Ord char followed by a char of the same font takes the TFM kern from the CMMI10/7/5 LigKernTables (`cm_math_kern_table`). The hand table `cmmi_pair_kern` was removed.
- `multline`:
  - Each row is `\hbox to\displaywidth`.
  - The first row starts `\multlinegap` from the left.
  - The last row ends `\multlinegap` from the right, or `\multlinetaggap` before a right tag; the tag sits on that row.
  - Middle rows are centred.
- `\text` (`MathNode::TextFont`) uses the surrounding text face (`MathLayoutContext.text_face`), and the interword space is that font's fontdimen2 (`space_pt`).
- `\displaywidowpenalty` (50) replaces `\widowpenalty` for lines before a display (`Inline::DisplayFollows`, tex.web §890). A display ends vertical mode, so a following trivlist gets no `\partopsep` (`ends_display`).
- `fire_page` keeps the paragraph state (predisplaysize, prevgraf, prevdepth) when boxes move to the next page.
- When the equation number goes on its own line, the displacement uses tag width 0 (tex.web §1201–1202).
- Undelimited macro arguments and scripts take one token (`parse_arg_atom`), so `\frac12` and `x^12` take one digit.
- `split` (`MatrixData.split`, `is_split_display`):
  - A display that is one `split` is an alignment, so it gets the regular skips (tex.web §1206).
  - Its box loses `\jot` at the top. This is the `\vskip-\lineskiplimit` from `\@display@init`'s `\everycr` at the inner preamble `\cr`.
  - The number is raised by half a `\jot`.
- The first-row `\jot` from `\openup` applies above the display only for `multline` (measuring pass) and `split` (inner alignment). `align` and `gather` take it back at their first `\cr`.
- The math parser skips `%` comments.
- CMSY ToUnicode: 0x34 is U+25B3, 0x35 is U+25BD, 0x36 is U+0338.

## Rejected
- A first-row `\jot` for every display alignment. `align`, `gather` and `gather*` came out 3pt low against pdfLaTeX.
- Treating page 5's tightness as a page-builder bug. The cause was the `\frac12` parse and the missing `split` alignment skips.

## Measured
- testmath went from 9.0% to 13.1% (theorem, hdotsfor, kern, multline and widow fixes). It then went to 21.1% (8956/42400) with the `split` and argument fixes. It is still 41 pages against our 46.
- Repros: hdotsfor matrix, multline, `\eval` smallmatrix and display widow at 100%; gather/align/gather* at 100%; `split` pair at 96% (the misses are CMEX glyph names only).
- sample2e, small2e and lppl stay at 100%. Tests: 234 pass, plus the 4 known NewCM failures. Clippy is clean.

## Open
- The page breaks after page 5 still differ.
- Remaining plan items: `\iiiint`, `\varliminf`/`\varlimsup`, `\numberwithin`, `\pmb`, `\leftroot`/`\uproot`, capital accents, `\multlinegap` settings, `\delimiterfactor`, `\qed` `\hfill`, `\setcounter{equation}`, and the cmbxsl10/cmbxti10 fonts.
