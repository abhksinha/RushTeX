# Design-size fonts, colour in cells/headings/math, booktabs spacing
Author: abhi+claude | Date: 2026-10-18
Files: src/backend/backend_optical.rs, src/backend/backend_cm_optical.rs, src/backend/backend_fonts.rs, src/backend/backend_shape.rs, src/backend/backend_pdf.rs, src/layout/layout_engine.rs, src/layout/layout_hz.rs, src/layout/layout_hyphen.rs, src/tables/tables_layout.rs, src/tables/tables_parser.rs, src/math/math_ast.rs, src/math/math_parser.rs, src/math/math_layout.rs, src/frontend/frontend_ast.rs, src/frontend/frontend_parser.rs, PARITY_PRECISION_TODO.md

## Context
Follow-up parity work approved by the user: Computer Modern design sizes instead of
scaling the 10pt fonts, colour outside body paragraphs, and booktabs rule spacing.

## Decision
- Design sizes are a separate dimension from `Face`, not new faces. `Face` is packed in
  5 bits (`PackedWordAttr`) and its u32 masks are full at 32 values. `OpticalMaster::select(family, face, size)`
  is a pure function, so layout widths, fontdimens, lig/kern, hyphenation pieces and the
  PDF writer all agree without storing anything per word. The size ranges follow
  `ot1lmr.fd`, which give the same masters as `ot1cmr.fd` at LaTeX's standard sizes.
- The PDF writer applies masters only to shaped glyph runs and text label ops. Math glyph
  runs (unshaped) keep the TFM font their metrics came from. The Tf switch tracks the
  master as well as the face and size, because a math run and a text run can share both.
- The hyphenation cache key gains a master byte (taken from padding; the entry stays 64 B).
  Piece widths now use the word's own size from the size runs.
- Math colour is `MathNode::Colored`, transparent for atom kinds and italic correction.
  `\textcolor` in math wraps its argument as an Ord atom, as the braces do in TeX.
- Booktabs rules follow booktabs.sty: em/ex widths, `\aboverulesep`/`\belowrulesep`,
  `\doublerulesep` between adjacent rules, and the `\cmidrule` sharing rules.

## Rejected
- Widening `Face` to 64 values (u64 masks, 6-bit attr): this touches every packed
  structure for a property that is derivable from (face, size).
- Applying masters by face only, for every glyph run including math: math at `\large` would
  switch fonts under metrics measured with cmr10.

## Measured
- Word x positions at 8, 9, 12, 14.4 and 17.28pt (lmodern article) were within 0.05pt of
  pdfLaTeX.
- table_booktabs 63.7% to 99.8% coverage. The other table tests did not change.
- Harness 38/38 A. Math probe 478/479 within 0.02pt (unchanged).
- Tests: 221 pass. The same 4 tests fail as before, because they need the NewCM OpenType fonts.
- manual.tex median 5.00ms before, 4.95ms after. calculus_and_quantum 2.65ms before,
  2.74ms after (within noise). The binary grows by about 190 KB for the 17 font masters.
- The page number was 8pt left of pdfLaTeX, because it was centred on the page instead of
  the text block. It is fixed.

## Open
- Line breaking uses a fixed 1.67pt interword stretch per space, so it ignores the font
  and size. TeX uses the fontdimen 3/4 of each word's font.
- Bold italic and small caps have a single master (cmbxti10, cmcsc10), as in ot1cmr.fd.
- `page.used_faces` still marks the 10pt face when only a master is used. The result can
  embed one unused font.
