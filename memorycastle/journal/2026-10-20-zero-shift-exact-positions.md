# Zero shift: exact glyph positions against pdfLaTeX
Author: abhi+claude | Date: 2026-10-20
Files: `src/backend/backend_pdftex.rs`, `src/backend/backend_pdf.rs`, `src/backend/backend_ligkern.rs`, `src/backend/backend_shape.rs`, `src/backend/backend_scaled.rs`, `src/layout/layout_engine.rs`, `src/layout/layout_hz.rs`, `src/layout/layout_linebreak.rs`, `src/config/config_mod.rs`, `src/index/index_toc.rs`, `src/frontend/frontend_parser.rs`, `scripts/parity/parity.py`, `scripts/fontgen/kern_index.py`, `scripts/fontgen/ligkern_lib.py`

## Context
The user asked for an identical match with zero shift; the 0.1pt strict score hid systematic sub-0.1pt error. New `parity.py exact` pairs every reference glyph with ours (same text, nearest origin) and buckets the residual; "exact" is below 0.002pt, pdfTeX's own output raster (`\pdfdecimaldigits` 3 in bp). Baseline was 0.3% to 16% exact. The user's rule: take pdfTeX's values, not its methodology; keep binary and RAM minimal.

## Decision
- Content stream: `TextState` (`backend_pdftex.rs`) writes text on pdfTeX's raster: no global `cm`, coordinates in bp, `Tf` in bp to 4 decimals, integer `TJ` displacements from a viewer-position tracker, `Td` from the line start, rules as pdfTeX draws them. Our arithmetic: `divide_scaled` is one 64-bit division, property-tested equal to the digit loop. Interword spaces are displacements, not `Tw`.
- Kerns: `LigKernTable` pairs index a per-font table of distinct TFM fix_words (TFM's own layout). Same 4 bytes per pair, exact kerns; `kern_index.py` regenerates, asserting every old 1/10000 em value.
- Lines: words placed in sp with §625 cumulative glue rounding (`cur_g`); glue_set from the breaker's exact natural width, now `HzLineDescriptor::line_w_sp` (i32, same size as the f32 it replaced).
- Dimensions: `scaled::scan_dimen` is TeX's §448–§458 arithmetic; `em_mul` gives `2.5em` as TeX does (25.00003pt with cmr10's 10.00002pt quad). Label, section and equation number widths come from TFM widths of the bytes the writer emits.
- `\$` and the TS1 text symbols come from TS1 (latex.ltx 2020+).

## Rejected
- Recomputing a line's natural width in layout: it disagreed with the breaker by 0.75pt on hyphenated lines with math (`testmath` page 6); the breaker's sp sum is the truth.
- Dropping ToUnicode from CMEX to match pdfTeX's extracted text: positions are what matters, and our Unicode text is a superset; `exact` reports it as `other-text`.

## Measured
`exact` before, then after (strict stays 100.0% on all six):
small2e 0.40% to 100.00%; lppl 16.50% to 99.91%; modguide 3.80% to 99.73%; cfgguide 7.75% to 99.35%; sample2e 0.34% to 98.29%; testmath 9.04% to 42.42%.

## Open
Positions above 256pt lose odd sp in f32 `Paint` coordinates (fix: `Sp` coordinates). Vertical `self.y` accumulates in f32 (most testmath residuals are dy). Math layout is still f32 points.
