# testmath page 9: display paragraphs and the `\endtrivlist` skip
Author: abhi+claude | Date: 2026-10-19
Files: `src/math/math_display.rs`, `src/layout/layout_engine.rs`, `src/layout/layout_pagebuilder.rs`, `src/frontend/frontend_parser.rs`, `src/frontend/frontend_ast.rs`

## Context
Continued from `memorycastle/journal/2026-10-19-testmath-binom-cite-kern-breaks.md`. Page 9 was at 6%. Floats: `memorycastle/journal/2026-10-19-testmath-float-pages.md`.

## Decision
- tex.web §1146: `\predisplaysize` adds `2*quad(cur_font)`, where cur_font is the text font of the paragraph (CMTI10 in a theorem), not the math quad. `compute_display_geometry` takes `text_quad`. §1199 still uses the math quad.
- `Block::Equation.par: DisplayPar` has three values. `Continues`: horizontal mode, so no `\parskip`. `Starts`: `$$` in vertical mode, which adds `\parskip` and one empty line holding the indent box (§1145), or no line after `\@afterheading`. `StartsWithBox`: latex.ltx `\[` and `displaymath` without amsmath, which add `\nointerlineskip` and `\makebox[.6\linewidth]{}`. Text that resumes after a display gets `Inline::DisplayResumes`, so there is no `\parskip` or `\parsep` before it (`continues_par`). The parser's `display_resumes` flag only lives for the block after the display.
- `\endtrivlist`: when the last skip is positive (e.g. a display's `\belowdisplayskip`), it becomes that skip + the list's `\parskip` − `\@outerparskip` before `\@endparenv` (`endtrivlist_skip`).


## Measured
- testmath 25.7% to 52.4% glyphs within 0.1pt. Pages 1–10 are at 99–100% (page 9 6% to 99%, page 10 28% to 100%). The page count is now 47 against 41.
- Repros `dp`, `bn`, `p4m`, `f1`, `f2` and `p9` (99.8%) all match. `sample2e`, `small2e` and `lppl` stay at 100%.
- Tests: 234 pass plus the 4 known NewCM failures. Clippy is clean.

## Open
- Page 11 onward: 96%, 96%, 95%, 23%, then a drift.
