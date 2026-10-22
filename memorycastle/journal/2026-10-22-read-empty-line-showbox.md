# Empty `\read` lines keep `\endlinechar`; `show_box` as pdfTeX prints it
Author: abhi+claude | Date: 2026-10-22
Files: `src/engine/engine_io.rs`, `src/engine/engine_showbox.rs`, `scripts/parity/engine_boxdiff.py`

## Context
vwcol's fancyvrb examples lost their blank lines; finding it needed the engine's box dump
to read like pdfTeX's log.

## Decision
- §482 `read_to`: an empty line still yields `\endlinechar`. Our lexer counts a source
  without a line end as no line at all, so the read line now gets a `\n`. fancyvrb's
  `\FV@ReadLine` depends on that `^^M` and otherwise joins a blank line to the next one.
- `show_box` prints what pdfTeX prints: `glue set >20000.0fil` past the bound (§186), the
  expansion ratio after a font identifier (` (+5)`, `print_font_identifier`), and
  `(left margin)`/`(right margin)` after margin kerns.
- `engine_boxdiff.py`: a line of exactly 79 characters followed by an empty line is a wrap,
  not the end of the box (the tool used to cut page 3 of vwcol short there). `^^xx` codes
  above 127 compare as the characters pdfTeX's log prints.

## Measured
- fancyvrb repro (`VerbatimOut` then `BVerbatimInput`, blank and `%` lines): ZERO, was 2/17 glyphs.
