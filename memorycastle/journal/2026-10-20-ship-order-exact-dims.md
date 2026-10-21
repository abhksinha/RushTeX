# testmath exact: TeX ship order, exact TFM dims, fraction rule width
Author: abhi+claude | Date: 2026-10-20
Files: `src/math/math_layout.rs`, `src/math/math_matrix.rs`, `src/layout/layout_engine.rs`, `scripts/parity/parity.py`, `src/backend/backend_shape.rs`, `src/config/config_mod.rs`, `src/frontend/frontend_ast.rs`, `src/math/math_ast.rs`

## Context
testmath exact was stuck at 99.89%. The residuals were baseline drifts of a few sp and ±655sp x offsets (one TJ kern unit), which come from pdfTeX's TJ state.

## Decision
- Ops are emitted in TeX's ship-out order, because pdfTeX's TJ state depends on it:
  - fraction: numerator, rule, denominator;
  - `\left` before the array;
  - `\@frameb@x` (`\boxed`): top rule, left vrule, content, right vrule, bottom rule;
  - `\ext@arrow`: sup, arrow, sub;
  - `\arrowfill@`: left end, leaders, right end.
- A numbered alignment nested in `gather` records its rows in `row_ends`, so that `order_display_rows` interleaves the tags.
- The fraction rule spans only the num/den width, without the `\nulldelimiterspace` pads (§748). Before this fix it was 1.2pt too long on each side, a visible bug that glyph scores could not catch.
- Exact TFM `glyph_dims` replace the lossy metric tables:
  - `\xrightarrow` widths and height;
  - `layout_varlim`;
  - unshaped (typewriter) text in `line_extent`.
- `\pmb` kerns are each rounded on their own.
- `align_shift_and_sep` works in integer sp with truncating `\divide`, as amsmath does.

## Rejected
Giving the strut 550502sp was wrong: `\strutbox` is 550500sp (8.39996pt).

## Measured
- testmath exact went from 99.89% to 99.98%, with 8 glyphs left (pages 15, 19, 22, 24, 33, 35).
- Strict is 100% on all six documents.
- mathbox oracle: 324/328; the 4 failures are formulas wider than 256pt, where f32 loses 2sp.

## Open
- The page-33 tag (67) is −197sp; the rest are ±655sp TJ units.
- Rules are not scored, so a rule-geometry comparison is needed in `parity.py`.
- f32 math coordinates lose sp beyond 256pt.
