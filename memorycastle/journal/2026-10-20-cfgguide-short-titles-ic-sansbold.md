# cfgguide: argument blanks, short titles, `\/`, TOC depth, cmssbx10
Author: abhi+claude | Date: 2026-10-20
Files: `src/frontend/frontend_parser.rs`, `src/frontend/frontend_ast.rs`, `src/index/index_mod.rs`, `src/index/index_toc.rs`, `src/layout/layout_engine.rs`, `src/layout/layout_pagebuilder.rs`, `src/math/math_layout.rs`, `src/backend/backend_fonts.rs`, `src/backend/backend_scaled.rs`, `src/backend/backend_shape.rs`, `src/backend/backend_cm_fonts.rs`

## Context
cfgguide (ltxguide class) is the second CTAN corpus document. It stopped at `\section[..]%` followed by `{..}` on the next line.

## Decision
- `skip_arg_blanks` (§393): before an undelimited argument, `require_group` skips spaces, comments and one line end.
- `\section[short]{long}`: `optional_inlines` parses it. The short title sits in front of `Inline::ShortTitleEnd` in the same title `Vec`, because `Block` has no room. `section_title_parts` splits it: the TOC gets the short title, the heading gets the long one.
- `\/` is `Inline::ItalicCorrection` (§1113): a kern equal to the last character's italic correction.
- TOC entries measure their line box. `prev_depth` is the line's depth, so the page's natural height includes it (`\@makecol`).
- At a page top, verbatim adds no `\@topsep` or `-\parskip` skips.
- A math `Variable` digit drew "x" (`\LaTeXe`'s 2). It now uses `ASCII_GLYPH_STRS`.
- `Face::SansBold` is cmssbx10 (ot1cmss.fd `bx/n <-> cmssbx10`), built with `scripts/fontgen/gen_one.py`. Sans faces now have their own bold, mono and roman axes.
- The page builder traces `%v fire` (overflow and shrink).

## Measured
- cfgguide: from not compiling to 98.1% strict. Pages 2 and 4–9 and 12 are at 100%.
- modguide, testmath, sample2e, small2e and lppl are unchanged.

## Open
- TOC entry titles lose `\sffamily` (painted roman).
- `\textit{..\LaTeX{}}`: the logo is a math box, so `\check@icr` misses X's italic correction (1.58pt). Page 3 and page 10 lines drift by up to 1.46pt.
- Page 11: `\pageref` is drawn as "??".
- TS1 ©. `Face` has 41 of its 64 values, so the fixed enum will not hold the T1/EC fonts. A TFM-keyed font table is needed.
