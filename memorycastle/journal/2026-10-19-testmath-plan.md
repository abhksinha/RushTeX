# testmath to 100%: work plan and ordered fix list
Author: abhi+claude | Date: 2026-10-19
Files: `src/expand/expand_mod.rs`, `src/math/math_parser.rs`, `src/layout/layout_engine.rs`, `src/frontend/frontend_parser.rs`

## Context
After `memorycastle/journal/2026-10-19-testmath-logo-text-formula.md`, `testmath` is at 9.0% of glyphs within 0.1pt (46 pages vs pdfLaTeX's 41). Each fix moves everything after it, so later drifts only become visible once earlier ones are fixed. The user agreed to this order on 2026-10-19.

## Decision
Work in page order, one general rule from latex.ltx/amsthm.sty/tex.web per fix:
1. amsthm `\thm@headsep` (5pt plus 1pt minus 1pt) after the theorem head, instead of a fixed 0.5em (page 2, "Notation.").
2. `\eqref` is `\textup{\tagform@{..}}`: `\check@icl` puts the previous character's italic correction before the space ("of (2)", 0.78pt on page 2).
3. Theorem body text after a display inside a plain-style theorem must stay italic (page 2, "where per B is ...").
4. Display 0.42pt too short on page 3 (equation 7, `b^{(\lambda)}` with `_{ij}` on the next source line).
5. Then compare pages one by one; add the amsmath commands as they appear: `\hdotsfor`, `\iiiint`, `\varliminf`/`\varlimsup`, `\numberwithin`, `\pmb`, `\leftroot`/`\uproot`, capital accents (`\Hat`, `\Tilde`, ...); `\multlinegap`, `\delimiterfactor`, `\hfill` for `\qed`.
6. cmbxsl10 and cmbxti10 masters; compare page breaks once content matches.

## Rejected
- `\@makecol` margin rules (testmath redefines the output routine): needs a TeX interpreter; stays a known difference unless the user decides otherwise.
- Wide-accent glyphs reported unmatched by `miss.py`: a ToUnicode/PyMuPDF artefact, rendering is identical.

## Open
- `\sectionmark` under `headings` and twoside `\leftmark` (not needed by `testmath`).
