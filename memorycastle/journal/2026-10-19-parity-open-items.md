# pdfLaTeX parity: open items tracker
Author: abhi+claude | Date: 2026-10-19
Files: `src/layout/layout_engine.rs`, `src/math/math_layout.rs`, `src/math/math_fonts.rs`, `src/backend/backend_optical.rs`, `src/frontend/frontend_parser.rs`, `src/expand/expand_mod.rs`

## Context
Open items were scattered across the `## Open` sections of dated notes. Notes are never edited, so items stayed "open" after a later note fixed them. This note collects the current ones. It is updated in place (append-only within the day) and superseded by a newer tracker on a later date. Already closed: the fixed interword stretch item in `memorycastle/journal/2026-10-18-design-sizes-colour-booktabs.md` was fixed by `memorycastle/journal/2026-10-18-tex-exact-spacing.md` (glue from fontdimens 2/3/4/7 with the space factor).

## Decision
Open parity items live in one dated tracker. A later tracker supersedes it, so a fixed item stops ranking as open.

## Measured
- Glyphs within 0.1pt of pdfLaTeX: `docs/manual.tex` 39.9%, `lppl` 56.3%, `small2e` 96.0%, `sample2e` 100.0% (`memorycastle/journal/2026-10-19-sample-parity-baseline.md`).
- First drift in the manual: a list after `\subsection` sits 8.7pt low (page 3). The `sample2e` footnote mark is 0.25pt off.

## Open
- **Math design sizes.** LaTeX sets 11pt and 12pt scripts, `\footnotesize` and `\large` math in cmmi8/cmsy8/cmr8, cmmi6/cmr6, cmmi12 and so on. rushtex scales the 10/7/5pt fonts. This affects `docs/manual.tex` (11pt), `testmath`, and the `sample2e` footnote mark (0.25pt). Approach: select masters by (face, size), not new `Face` values (`memorycastle/journal/2026-10-18-design-sizes-colour-booktabs.md`).
- **Skip after a heading.** A list right after `\subsection` is 8.7pt too low on page 3 of `docs/manual.tex`. LaTeX's `\addvspace` should take the larger of the heading's after-skip and the list's `\topsep`, but `layout_section` does not record its after-skip in `last_addvspace`. This is the first drift in the manual (39.9% of glyphs within 0.1pt).
- **`lppl` drift** (56.3%). It starts in the title block. Cause not found yet.
- **`small2e`** (96.0%). One line differs.
- **`testmath`** stops at `\AmS`. amsmath gaps: capital accents (`\Hat`, `\Tilde`, ...), `\hdotsfor`, `\iiiint`, `\varliminf`/`\varlimsup`, `\numberwithin`, `\pmb`, `\leftroot`/`\uproot`.
- **TeX interpreter** (needs the user's decision). `testmath` redefines `\@makecol` with box and dimension registers. `amsthdoc` sets `\catcode`\|=0`. Both need token-level expansion, registers and boxes, which the borrowed-AST parser cannot provide.
- **Footnotes** inside floats or minipages are dropped.

- **Stress document page starts** drift from page 56 on (`memorycastle/journal/2026-10-19-vertical-mode-page-builder.md`). The page count matches (267). pdfLaTeX shrinks page 55 to fit a heading after a verbatim block; rushtex breaks before it.
- **No cmbxti10.** `\textbf{\textit{..}}` falls back to CMBX10 with WinAnsi encoding; pdfLaTeX's manual uses CMBXTI10 (`memorycastle/journal/2026-10-19-real-world-samples.md`).
- **`helvet`** switches the whole document to Helvetica; in LaTeX it only changes `\sfdefault`.
- **PDF size:** rushtex's manual is 375.5 KB against pdfLaTeX's 335.3 KB.

## Open (update, end of 2026-10-19)
- **Closed:**
  - `lppl` is now 100% (`memorycastle/journal/2026-10-19-lppl-list-skips-trace.md`).
  - `small2e` is now 100%.
  - The `sample2e` footnote mark is fixed by `math_text_master`.
  - `testmath` compiles (`memorycastle/journal/2026-10-19-testmath-compiles.md`).
- **`testmath`** is at 5.2%. Next fixes, in order:
  1. The `\AmS` italic correction.
  2. `\pagestyle{headings}` running heads.
  3. Comparing pages one by one.
- **`amsthdoc`** does not compile yet.
- **`\item\label`** in vertical mode should be a whatsit (`Block::VMark`), so the next `\parskip` is a breakpoint. The expander drops it today.
- **Not modelled:**
  - `\delimiterfactor`;
  - `\multlinegap` (`\setlength` in groups);
  - `\hfill` (so `\qed` is not flush right);
  - `\@makecol` margin rules.
