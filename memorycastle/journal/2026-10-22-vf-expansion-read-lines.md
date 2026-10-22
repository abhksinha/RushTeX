# Expanded virtual fonts, empty `\read` lines; tabulary, shortlst, arydshln, vwcol
Author: abhi+claude | Date: 2026-10-22
Files: `src/engine/engine_vf.rs`, `src/engine/engine_ship.rs`, `src/engine/engine_microtype.rs`, `src/engine/engine_io.rs`, `src/engine/engine_showbox.rs`, `scripts/parity/engine_boxdiff.py`, `scripts/parity/corpus/tabulary.tex`, `scripts/parity/corpus/shortlst.tex`, `scripts/parity/corpus/arydshln.tex`, `scripts/parity/corpus/vwcol.tex`

## Context
Coverage round for tables (tabulary, arydshln dashed rules), lists (shortlst) and
variable-width columns (vwcol). Three were ZERO on the first run. vwcol (microtype with
mathpazo's old-style-figure fonts, which are virtual) drew its text as Type 3 bitmaps and
lost blank lines in its fancyvrb examples.

## Decision
- pdftex.web `auto_expand_vf` in `engine_vf.rs`: an automatically expanded copy of a
  virtual font is virtual too. It shares the base's packets (`Rc<[..]>`, no copy) and its
  local fonts are the locals' copies expanded by the same ratio (`get_expand_font`).
- pdftex.web `vf_def_font`: when the virtual font has expansion parameters, each local font
  gets them (`set_expand_params`, now its own function in `engine_microtype.rs`), so the
  copies are automatic and drawn through the scaled text matrix.
- `classify_fonts`: an expanded copy (`name+e`, never in the map) takes its base font's
  scalable or bitmap status, as the map lookup of the base name does.
- §482 `read_to`: an empty line still yields `\endlinechar`. Our lexer counts a source
  without a line end as no line at all, so the read line now gets a `\n`. fancyvrb's
  `\FV@ReadLine` depends on that `^^M` and otherwise joins a blank line to the next one.
- `show_box` prints what pdfTeX prints: `glue set >20000.0fil` past the bound (§186), the
  expansion ratio after a font identifier (` (+5)`, `print_font_identifier`), and
  `(left margin)`/`(right margin)` after margin kerns.
- `engine_boxdiff.py`: a line of exactly 79 characters followed by an empty line is a wrap,
  not the end of the box (the tool used to cut page 3 of vwcol short there). `^^xx` codes
  above 127 compare as the characters pdfTeX's log prints.

## Rejected
- Copying the base virtual font's packet buffer for each expanded copy. Expanded copies
  are numerous (one per step and per size), and the packets never differ.

## Measured
- tabulary 16 pages, shortlst 12, arydshln 64 (135442 glyphs), vwcol 23 (38442 glyphs):
  all ZERO.
- Before: vwcol had 1283 of 38442 glyphs and 22 of 23 pages.
- Gate: 60/60 at zero shift, 336 tests, probes agree, clippy clean.
