# testmath pages 20–23: accents, roots, `\sideset`, `\varlimsup`, align `{}`
Author: abhi+claude | Date: 2026-10-20
Files: `src/math/math_layout.rs`, `src/math/math_parser.rs`, `src/math/math_ast.rs`, `src/layout/layout_engine.rs`, `src/frontend/frontend_parser.rs`, `src/frontend/frontend_ast.rs`, `src/backend/backend_cm_fonts.rs`

## Context
Continued from `memorycastle/journal/2026-10-19-testmath-page-builder-depth-ams-ops.md`. Pages 20–23 were at 99%, 37%, 3% and 4%.

## Decision
- Accents: a top accent centres on `nuc_box.w + ic` (§738 `clean_box` keeps the italic correction). amsmath `\macc@nested` gives an accent over an accented nucleus the inner character's skew. The capital accents (`\Hat`…`\Vec`) are aliases.
- amsmath `\r@@t`: the degree is raised .6(ht − dp + 1.667·`\uproot` mu) of the full radical box, shifted by `\leftroot`. The degree box keeps its italic correction (§755). Computer Modern has no LuaTeX kern clamp.
- `\dotsb` and `\dotsm` follow `\cdots`'s `\extrap@`. `\dotsi` is `\!\@cdots`.
- `DisplayPar::StartsEmpty`: `$$` right after a list with no blank line. `\@doendpe` removes the indent box, so TeX pops the empty paragraph (§1145) and adds no line.
- Verbatim ends with `\penalty\interlinepenalty`, then `\penalty-51` (`\@endparpenalty`, with `\lastskip` 0).
- `\sideset` is a `SideSetLead` Ord of width \dimen@, followed by an `OpLimits` whose nucleus is drawn `\kern-\dimen@` in. Its scripts are display limits.
- `VarLim` follows amsopn. `\ex@` uses amsgen's formula (1.0pt at 10pt). The CM arrow fills share `emit_arrow_fill` (centred `\cleaders`).
- Even align columns always start with the `{}` Ord, not only before Rel or Bin.

## Measured
- testmath 63.1% → 70.3%. Pages 20, 21, 22 and 23 are at 99–100%.
- `sample2e`, `small2e` and `lppl` are at 100%. Tests: 234 pass, plus the 4 known NewCM failures.

## Open
- Page 24 (89%) onward. Pages 28–41 drift: the page count is 47 against 41.
