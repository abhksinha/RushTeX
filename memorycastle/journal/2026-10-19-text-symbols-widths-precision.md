# Text symbols from latex.ltx and one-decimal PDF widths
Author: abhi+claude | Date: 2026-10-19
Files: `src/backend/backend_pdf.rs`, `src/backend/backend_scaled.rs`, `src/backend/backend_shape.rs`, `src/frontend/frontend_parser.rs`, `src/layout/layout_engine.rs`

## Context
Glyphs drifted along lines in `docs/manual.tex` and `small2e` against pdfLaTeX.

## Decision
- PDF `/Widths` of fonts with TFM data are written to 0.1 unit, rounded half down as pdfTeX does (343.75 prints 343.7). `exact_widths` finds a width table's TFM by identity; `width_tenths` gives the value, and `TextFont::glyph_tenths` uses the same numbers for the text cursor.
- `\_`/`\textunderscore` in Computer Modern is latex.ltx's `\kern.06em\vbox{\hrule\@width.3em}` (`UNDERSCORE_TEXT`, painted as a rule). `\textbackslash` is cmsy's backslash (OMS), also in `\texttt`. The Base-14 families keep their own glyphs.
- `check_icr` follows `\maybe@ic`: the italic correction of the last character in any font (cmbx10's `y`) when the font after the group is upright.
- A list right after a heading gets no `\topsep` (`\@nbitem`); heading lines count as vertical mode for `\partopsep`.

## Rejected
- Rounding `/Widths` to nearest: 11 of 1528 widths in the manual then differ from pdfTeX's; half down leaves 2.

## Measured
- `small2e` 96.0% to 100.0% glyphs within 0.1pt; `lppl` 56.3% to 61.2%; `sample2e` 100.0%; manual 39.9% to 47.4% (before the page-builder note).
- Tests 234 pass with the 4 known NewCM failures; clippy clean.
