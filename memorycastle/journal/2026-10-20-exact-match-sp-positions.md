# Zero-shift work: pdfTeX page description, positions in scaled points
Author: abhi+claude | Date: 2026-10-20
Files: `src/backend/backend_pdftex.rs`, `src/backend/backend_pdf.rs`, `src/backend/backend_scaled.rs`, `src/backend/backend_ligkern.rs`, `src/backend/backend_shape.rs`, `src/layout/layout_engine.rs`, `src/layout/layout_float.rs`, `src/layout/layout_pagebuilder.rs`, `src/layout/layout_hz.rs`, `src/layout/layout_linebreak.rs`, `src/math/math_layout.rs`, `src/math/math_ast.rs`, `src/tables/tables_layout.rs`, `src/index/index_toc.rs`, `src/frontend/frontend_parser.rs`, `src/frontend/frontend_ast.rs`, `src/config/config_mod.rs`, `scripts/parity/parity.py`, `scripts/fontgen/kern_index.py`, `src/math/math_tests.rs`

## Context
The user asked for an identical match with zero shift, not 0.1pt. Strict was 100% on all six documents but glyphs sat 0.01pt to 0.1pt off.

## Decision
- **PDF output.** The content stream follows pdftex.web's page description in `backend_pdftex.rs` `TextState`. It uses `divide_scaled` as a single 64-bit division, integer TJ displacements, and Td from rounded absolute positions.
- **Positions.**
  - Page positions are `Sp(i32)`: `Paint` x/y, `Rule` w/h, `Layouter.y`, page builder baselines, float, table and footnote tops.
  - f32 points are exact only below 256pt.
  - Math is set about its own origin and moved in `Sp` (`place_ops`).
- **Line output.** Lines replay §625 `cur_g` rounding with the breaker's exact width (`line_w_sp`).
- **Kerns and lengths.** Kerns are TFM fix_word index tables. `scan_dimen`, `em_times` and `EM_*` constants cover em lengths. `\textellipsis` uses `FontDimenKern(3)`.
- **Debug tools.** `parity.py exact`, `tj` and `hlist` (the last with `RUSHTEX_TRACE_H` / `RUSHTEX_TRACE_G`).

## Rejected
Keeping the coordinates in f32 points and rounding at output: positions above 256pt have only 2 to 4sp resolution, so the TJ and Td decisions flip.

## Measured
Exact (residual under 0.002pt), from the start of this work to now:

| document | start | now |
|---|---|---|
| cfgguide | 7.8% | 99.57% |
| lppl | 16.5% | 99.95% |
| modguide | 3.8% | 99.77% |
| sample2e | 0.3% | 98.38% |
| small2e | 0.4% | 100% |
| testmath | 9% | 43.25% |

- Strict is still 100% on all six documents.
- Speed: 4.2 to 14.5ms against pdfLaTeX's 153 to 308ms. RSS 3 to 6MB. Binary 3.4MB.

## Open
- **Math metrics.** They are packed in 2^-14 em (`math_tfm.rs`), which costs a few sp per glyph. This is the cause of the logos' (`tex_logo`) drift and of most of testmath. Plan: TFM's own char_info packing (width, height, depth and italic indices plus small fix_word tables). That is exact and smaller than today's 8 bytes per glyph and `FixMetrics`.
- **Logo raise.** The `\LaTeX` `A` raise needs exact TFM heights.
- **Page geometry.** The config page size and margins are still f32.
- **Vertical glue setting.** It should follow §634 rounding.

## Decision (addendum: review fixes)
- `Sp` arithmetic (`Add`, `Sub`, their assign forms, `Neg`) saturates at the `i32` range instead of wrapping, as TeX goes on with a clamped value after an arithmetic error (§104). Debug builds assert, since an overflow means a sentinel such as an infinite page goal leaked into positions. Test `sp_sums_saturate`. Debug-build runs of all six corpus documents and the debug test suite hit no overflow.
- `test_milestone_8_comprehensive_modern_math_certification` was flaky: a wall-clock throughput threshold (1,500 docs/s) under the parallel test runner, measured at 1,179 docs/s under load against 6,500 alone. The throughput is now reported, not asserted; speed is gated by `benchmark.sh` and `examples/bench_loop.rs`. Not a regression: the same test measured 5,093 docs/s at HEAD and 6,500 with this work, both alone.
