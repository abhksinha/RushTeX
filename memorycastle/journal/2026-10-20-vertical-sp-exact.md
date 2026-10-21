# Vertical positions exact to the sp: paper, line boxes, page shrink
Author: abhi+claude | Date: 2026-10-20
Files: `src/config/config_mod.rs`, `src/layout/layout_engine.rs`, `src/layout/layout_pagebuilder.rs`, `src/layout/layout_float.rs`, `src/backend/backend_shape.rs`, `src/tables/tables_layout.rs`, `src/math/math_layout.rs`, `src/frontend/frontend_parser.rs`

## Context
testmath exact stalled near 99.4%: many glyphs 0.001bp off in y because baselines were a few sp off and pdfTeX's 3-decimal rounding flipped.

## Decision
Paper sizes are sp constants (`A4_SP`, `LETTER_SP`, as `scan_dimen` reads 297mm and 11in); `paper_sp` recovers them from the f32 fields, and `margin_bottom` is computed in sp. pdfTeX's vertical origin is `pdf_v_origin` = 4736287sp, not 1in. Line box height and depth come from TFM (`TextFont::height_depth_sp`, verbatim via `glyph_dims`), not the 1/1000-em tables (6.94458 vs 6.94444pt showed up wherever `\lineskip` applied). Page shrink follows `vlist_out` (§625): round(glue_set × accumulated shrink in sp), glue_set a double. Caption hbox height includes its formulas; float-page fil glue is rounded in sp. amsmath `\,` is .1667em; math `\quad`/text-mode skips use the surrounding text font's quad; amsgen `\ex@` is `\compute@ex@`.

## Rejected
Nearest-baseline row assignment for display ops (see `2026-10-20-testmath-exact-math.md`).

## Measured
testmath exact 99.19% -> 99.51%; baselines off by >=1sp: 459 -> 298. Strict 100% on all six documents.

## Open
Remaining sp drifts on shrunk pages (overflow still from f32 sums) and lists; remove the `RT_TRACE_V` debug prints.
