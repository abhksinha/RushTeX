# paracol at zero shift: `head_for_vmode` and display alignments in lists
Author: abhi+claude | Date: 2026-10-22
Files: `src/engine/engine_box.rs`, `src/engine/engine_align.rs`, `src/engine/engine_noad.rs`, `scripts/parity/corpus/paracol.tex`

## Context
paracol.dtx (CTAN bundle, 270 pages: parallel columns, column footnotes, parallel pages)
was the coverage pick for two-column text. First run 657204/674277 glyphs.

## Decision
- §1095 `head_for_vmode`: vertical material (`\vskip`, `\hrule`, `\unvbox` ...) in a paragraph
  backs itself up behind an inserted `\par` token, whatever `\par` means then; we called
  `end_graf` directly. LaTeX's list `\par` re-sets the list's `\parshape`, so the next
  `\item` (paracol's `\Item` = `\vskip-\itemsep...\item`) lost the list indent (24.9pt).
- §800/§806/§807: an alignment in a display (`eqnarray` inside a list) shifts every row by
  `\displayindent`, and a rule row goes into a box of natural size with that shift. We
  shifted by 0.
- paracol.dtx and its parts (`man`, `ref`, `pwfnote`, `ppts`, `bgpaint`, `probs`, `impl`
  `.dtx`) added flat to the corpus (its `\input{man.dtx}` resolves beside the driver).

## Measured
- paracol: 270/270 pages, 674277 glyphs, 492 paths, 531 links, 681 dests, pixels 0.
- Gate: 45/45 at zero shift, 334 tests, probes agree, clippy clean.

## Open
- The corpus now holds generic names (`ref.dtx`, `man.dtx`, `impl.dtx`); a later document
  with the same file names needs its own directory.
