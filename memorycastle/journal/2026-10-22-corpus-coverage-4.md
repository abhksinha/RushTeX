# Coverage corpus: math notation, reversed and outline lists, mini boxes, picture wrapping
Author: abhi+claude | Date: 2026-10-22
Files: `scripts/parity/corpus/cancel.tex`, `scripts/parity/corpus/cases.tex`, `scripts/parity/corpus/esdiff.tex`, `scripts/parity/corpus/esdiff.dtx`, `scripts/parity/corpus/etaremune.tex`, `scripts/parity/corpus/etaremune.dtx`, `scripts/parity/corpus/faktor.tex`, `scripts/parity/corpus/faktor.dtx`, `scripts/parity/corpus/mathdots.tex`, `scripts/parity/corpus/mathdots.dtx`, `scripts/parity/corpus/minibox.tex`, `scripts/parity/corpus/moreenum-doc.tex`, `scripts/parity/corpus/outlines.tex`, `scripts/parity/corpus/picinpar-en.tex`

## Context
Topic coverage, not count: math notation (cancel, cases, esdiff, faktor, mathdots), list
kinds (etaremune reversed enumerations, moreenum numbering styles, outlines), boxes
(minibox) and text around pictures (picinpar). All come from CTAN and compile under
pdflatex as published; several use KOMA-Script's scrartcl and lmodern.

## Decision
All ten join the corpus. Each was at zero shift on its first engine run, so no engine
change was needed.

## Rejected
- boxedminipage.dtx and delimset.dtx: need `hyperxmp`, which tlmgr cannot install in
  user mode.
- ctable.dtx: its documentation needs fontspec (XeTeX or LuaTeX).
- mdotest.tex (mathdots' size test): every glyph and path is exact, but 7 pixels differ at
  300dpi. The engine embeds its built-in CFF Computer Modern where pdfTeX embeds a Type 1
  subset of the `.pfb`, and the rasterizer anti-aliases them differently at 6pt. This is
  for the font-embedding work (roadmap Batch 4), not a placement error.

## Measured
- 10 documents, 43 pages, 53692 glyphs, all ZERO. The corpus now has 70 documents.
