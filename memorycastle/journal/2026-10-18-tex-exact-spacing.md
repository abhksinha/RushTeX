# TeX-exact line breaking and interword spacing
Author: abhi+claude | Date: 2026-10-18
Files: src/layout/layout_linebreak.rs, src/layout/layout_engine.rs, src/layout/layout_hz.rs, src/layout/layout_hyphen.rs, src/backend/backend_scaled.rs, src/backend/backend_cm_fix.rs, src/backend/backend_shape.rs, src/backend/backend_pdf.rs, src/backend/backend_cm_fonts.rs, src/backend/backend_optical.rs, src/backend/backend_fonts.rs, src/tables/tables_layout.rs, PARITY_PRECISION_TODO.md

## Context
The user asked for spacing identical to TeX, following Resource_Conscious_Software_Engineering.md.
The paragraph breaker was a heuristic hz breaker. Widths were rounded to 1/1000 em, and every
space had a fixed 1.67pt stretch. Protrusion and expansion were on although pdfLaTeX without
microtype uses neither.

## Decision
- A port of TeX's line_break in integer scaled points became the default breaker. The hz
  breaker remains only behind `hz_microtype` (off).
- The breaker builds its own node list (24-byte nodes) in a reused scratch, so the word
  SoA is never mutated. Active nodes keep absolute totals instead of TeX's delta nodes;
  that is simpler at the same working set. A hyphenated word is cut by byte offsets in
  the line descriptor (`head_cut`/`tail_cut`, which replace the unused `num_spaces`),
  so nothing is copied.
- Widths come from raw TFM fix_words through store_scaled (generated `backend_cm_fix.rs`,
  which also holds the italic corrections). Glue is fontdimens 2/3/4/7 with the space
  factor, stored per item as `glue: [stretch, shrink]`.
- The painter applies TeX's glue ratio per space (`GlueSet`). The PDF writer emits `Td`
  between rounded positions.
- The hyphenation cut cache is keyed by letters only, so one entry serves every face
  and size: 32-byte entries, 64 KB, allocated on first use. The hz cache (512 KB) is
  now lazy too.
- p-cells and float captions use the breaker with `\sloppy`, because LaTeX's
  `\@arrayparboxrestore` ends in `\sloppy`. Titles and headings do not get it.
- The target metrics are CM (cmr/cmti TFMs, which we embed), not Latin Modern. They
  differ in kerns, for example `ll` in cmti vs lmri. Compare against pdfLaTeX without
  `lmodern`.

## Rejected
- Per-word hyphenation only near line ends: faster, but TeX hyphenates every eligible
  word in pass 2, and the result could differ.
- Caching hyphenation piece widths per (face, size): about 400 KB for a small gain.
- Mutating cell space widths to justify captions (the old approach): replaced by glue setting.

## Measured
- Mixed paragraph vs pdfLaTeX (CM): breaks 9 differences, then 0. Max horizontal error
  1.178pt, then 0.009pt. Extracted text is now identical.
- Hyphenation: 0 differences from `\showhyphens` over 4244 words.
- Harness 38/38 A. table_paragraph_col 84.9% to 99.6%, table_cline 91.3% to 99.8%,
  table_multicolumn 99.9%, table_float_caption 97.1% to 97.9%. Math probe 478/479
  (unchanged).
- manual.tex 4.84ms to 5.15ms (+6%, mostly pass-2 hyphenation of every word, as TeX does).
  calculus 2.54ms to 2.55ms. RSS 5.70MB to 5.44MB, and 4.25MB to 4.11MB.
- Tests: 223 pass; the 4 NewCM failures are as before.

## Open
- FontDescriptor ascent/descent of the masters differ from pdfTeX's (extraction bboxes only).
- Words with explicit hyphens are not pattern-hyphenated before the hyphen.
- e-TeX `\lastlinefit` is still approximated in the painter.
