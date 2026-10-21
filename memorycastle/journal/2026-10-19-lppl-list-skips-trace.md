# lppl parity: list skips, enumerate refs, math text masters, page trace
Author: abhi+claude | Date: 2026-10-19
Files: `src/layout/layout_engine.rs`, `src/layout/layout_pagebuilder.rs`, `src/expand/expand_mod.rs`, `src/math/math_fonts.rs`, `src/backend/backend_pdf.rs`, `src/frontend/frontend_parser.rs`, `src/frontend/frontend_ast.rs`

## Context
`lppl` was the first sample under 100%. Each drift was located by comparing pdfTeX's `\tracingpages=1` log against rushtex's new `RUSHTEX_TRACE_PAGES` stderr trace (`trace_pages` in `src/layout/layout_pagebuilder.rs`), then implementing the latex.ltx macro behind the first differing breakpoint.

## Decision
Rules implemented from latex.ltx:
- `\@trivlist`: `\@topsep` includes the current `\parskip` (`trivlist_topsep`). Inside an item that is the outer `\parsep` (`item_par_skip`). Lists and quotes get no paragraph `\parskip` glue before them (`par_skip_glue`).
- `\addvspace{-\parskip}` does nothing when `\parsep` has zero natural width, so the first item keeps its `\parsep` glue. `\@nbitem` nets the outer `\parskip` (`list_begin_skip`).
- `\@itempenalty` and `\@endparpenalty` are `\addpenalty` (`page_addpenalty`). `\addvspace\itemsep` merges with a nested list's end skip. After an `\addvspace` replacement, `\lastskip` is the whole skip (`set_lastskip_width`).
- `\item\begin{list}`: `\@inlabel`/`\@noparlist`, so the list gets no `\@topsep` and no `\@endparenv` (`in_label`).
- `\paragraph`: `Block::RunInHeadSkip`, which is `\addpenalty\@secpenalty\addvspace`. Neither is added after a heading (`\if@nobreak`).
- Breakpoints between paragraphs of an item (`par_glue_breakpoint`).
- Enumerate `\item` does `\refstepcounter`, giving `\ref` values such as `2`, `2a`, `2(a)iii` (`enum_ref`).
- CM text faces in math (`\mathrm`, logos, footnote marks) use design-size masters for both width and font (`math_text_master`). The logo kerns use the font at the current size and `\sf@size`.

## Rejected
- An extra parskip in `trivlist_topsep` while `par_skip_glue` still ran before lists: it counted `\parskip` twice. Measured as a +1pt stretch per list.

## Measured
- `lppl`: 61.5% → 100.0% of glyphs within 0.1pt.
- `sample2e` and `small2e`: 100%.
- `docs/manual.tex`: 60.2% → 61.0%.
- Manual compile: 10.4ms median (was 11.7ms).
- Tests: 234 pass, plus the 4 known NewCM failures. Clippy is clean.

## Open
- `\item\label{..}` in vertical mode leaves a whatsit, so the next `\parskip` is a breakpoint (p=0) that rushtex lacks. In `lppl` this breakpoint is never chosen, but it could change a break elsewhere.
- One page-shrink mismatch of 1pt after nested list ends. Not yet explained.
