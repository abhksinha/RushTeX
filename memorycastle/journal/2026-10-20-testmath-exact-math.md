# testmath exact: math from TFM integers and TeX's list order
Author: abhi+claude | Date: 2026-10-20
Files: `src/math/math_layout.rs`, `src/math/math_fonts.rs`, `src/math/math_tfm.rs`, `src/backend/backend_tfm.rs`, `src/backend/backend_scaled.rs`, `src/backend/backend_shape.rs`, `src/backend/backend_pdf.rs`, `src/layout/layout_engine.rs`, `src/frontend/frontend_parser.rs`, `scripts/parity/mathbox.py`, `examples/math_box.rs`

## Context
User asked to match testmath (CTAN) 100% exact. Strict was 100% but exact 65.8%.

## Decision
Every math dimension comes from the TFM tables (`Tfm`, `GlyphDims`) and TeX's integer operations in sp: `mu_sp` (§716), `cm_var_delimiter_sp` and `emit_cm_delimiter` (§706–§713), `left_right_target_sp` (§762), §737 radicals, amsmath `\big@size`, amsgen `\compute@ex@` (\ex@ is 1pt at 10pt, not .2326ex), amsmath `\,` = .1667em, `\nulldelimiterspace` 78643sp, a list nucleus keeps its last italic correction, overline/underline span the italic correction. Math fonts' `/Widths` come from their TFM (`ExactWidths`). `\accent` is §1125 exact, accent before base. Ops are drawn in TeX's list order (left delimiter before content, surd/rule before radicand, sup/op/sub for limits, page head first, row tags after their row) because the order decides pdfTeX's string grouping and rounding.

## Rejected
Nearest-baseline assignment of display ops to rows: limits reaching toward the neighbouring row land in the wrong row; row op boundaries must come from the alignment layout itself.

## Measured
testmath exact 65.81% -> 99.19%; math box oracle over testmath formulas 321/328; strict 100% on all six documents throughout. cfgguide 99.95, sample2e 99.92 exact.

## Open
Row op boundaries from the matrix loop; `\pmb`, `\xrightarrow` +5sp; non-amsmath `sfixed*cmex10` choice; remove old `MathFontParameters`/`FixMetrics` tables.
