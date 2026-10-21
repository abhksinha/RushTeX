# PDF widths at first-use size, amsthm openbox, \hfil glue, margin-rule order
Author: abhi+claude | Date: 2026-10-20
Files: `src/backend/backend_scaled.rs`, `src/backend/backend_pdf.rs`, `src/backend/backend_pdftex.rs`, `src/math/math_layout.rs`, `src/math/math_parser.rs`, `src/expand/expand_mod.rs`, `src/frontend/frontend_parser.rs`, `src/layout/layout_engine.rs`, `src/layout/layout_linebreak.rs`, `scripts/parity/parity.py`

## Context
testmath exact stuck at 99.63%. Content streams were identical, but glyph x positions were off by 0.007bp; then TJ splits differed with identical h values.

## Decision
- `/Widths` follow writefont.c: dividescaled(width scaled at the font's size, `pdf_font_size`, 4). The size is the size the font was first used at (`FontSizes`, tracked per face and master in both PDF paths), not its design size. `width_tenths` takes that size.
- pdfTeX's TJ state (`pdf_tj_start_h`) resets at every BT, so draw order and ET/BT matter. amsthm `\qedsymbol` is `\openbox` (four rules, `MathNode::OpenBox`), not msam `\square`; `\qed` is `\unskip\nobreak\hfill\quad\openbox`.
- `\hfil`, `\hfill`, `\hfilneg`, `\hspace{\fill}`: `Inline::HFil`, stored sparsely in `ParagraphSoA::fils`, with fil-order glue in the line breaker. `line_fil` sets the line by the highest order (§659). `\unskip` is `Inline::Unskip`.
- `make_column` draws the left margin rules before the column and the right ones after it.
- `set_font` begins the text object before checking the current font, as `pdf_begin_string` does.
- `parity.py hlist` recurses into nested hboxes and rounds as web2c `zround` (half away from zero).

## Rejected
- `\pdfhorigin` = 4736287sp (pdftex.web's (one_hundred_inch+50) div 100): `pdftexconfig.tex` sets `1 true in` = 4736286sp. The margins stay at 1in; exact dropped 99.82% to 99.78% with 4736287.
- Python `round` in the replay tools: it is banker's rounding and gave false 1sp glue errors.

## Measured
testmath exact 99.63% to 99.89%; strict 100% on all six documents; CM `/Widths` now equal pdfTeX's in all six (EC/SFRM fonts still differ).

## Open
Display row heights a few sp off (pages 18, 19, 21, 23, 28) from f32 math layout; EC `/Widths` and chars above 127.
