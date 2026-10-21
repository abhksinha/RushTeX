# Math design sizes: testmath reaches 100% strict
Author: abhi+claude | Date: 2026-10-20
Files: `src/math/math_fonts.rs`, `src/math/math_layout.rs`, `src/math/math_tfm_faces.rs`, `src/backend/backend_fonts.rs`, `src/backend/backend_cm_fonts.rs`, `src/backend/backend_ligkern.rs`, `src/frontend/frontend_parser.rs`, `src/layout/layout_engine.rs`, `scripts/fontgen/gen_one.py`

## Context
After `2026-10-20-testmath-99-percent.md`, about 180 glyphs were still off. They were on page 11 (footnote math at 8pt), page 28 (`\Large` math), and pages 19, 20 and 31–33. The user asked for these to be fixed too.

## Decision
- `resolve_optical_face` now picks the face by point size, not by style tier. The size ranges come from the fd files (omlcmm, omscmsy, ot1cmr, amsfonts).
- New faces `MathItalic6/8/9/12` and `Symbol6/8/9` were added (`Face::COUNT` is now 40), with `Face::is_symbol` and `Face::is_math_italic` helpers.
- `math_font_parameters_sized` gives the Appendix G parameters of cmsy and cmex at the actual size.
- Operator-family widths use the cmr text master.
- Skew is the skewchar kern of the sized cmmi.
- A text hbox (`MathNode::Raise`, `ctx.text_box`) adds no math italic correction.
- A blank paragraph keeps `\addvspace` merging (`\list` does `\unskip\par`).
- A top accent is drawn before its nucleus, as in §738.

## Rejected
- Making logo letters `MathNode::Symbol` nodes. This broke the bold heading logos in lppl and sample2e.

## Measured
- testmath: 42221 → 42400/42400. sample2e, small2e and lppl stay at 100%.
- 234 tests pass, plus the 4 known NewCM failures. Clippy is clean.
- In-process median for testmath: about 4.46 ms (was 4.37 ms).

## Open
- On pages 36 and 40, equation tags are drawn after the whole display rather than after each row. The text layer therefore orders the numbers differently, although every glyph position is identical.
- `scripts/fontgen/gen_one.py` regenerates the fonts. It needs fonttools, run with `uv run --with fonttools`.
