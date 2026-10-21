# PDF inclusion (`\pdfximage` of a PDF): specification from pdftoepdf.cc, and makecell/colortbl
Author: abhi+claude | Date: 2026-10-22
Files: `src/engine/engine_image.rs`, `src/engine/engine_io.rs`, `scripts/parity/corpus/colortbl.dtx`, `scripts/parity/corpus/makecell.dtx`, `scripts/parity/corpus/fancyhdr.dtx`

## Context
Coverage corpus round 2 (tables, headers): colortbl (19 pages) and makecell (16) added; fancyhdr (124) added and blocked on `example-image.pdf` (PDF 1.5, object streams, XRef stream), i.e. Batch 2 item 9.

## Decision
- `open_bytes` compares written-file names with any `./` prefix removed on both sides: docstrip in a `.dtx` writes `./makecell.sty`, which `\usepackage{makecell}` then loads in place of the installed package (pdfTeX does the same). This was makecell's only gap (12 glyphs: the dtx's `\eline` lacks the installed one's `\@temptokena{}` reset).
- PDF inclusion follows pdftoepdf.cc (vendored `texlive-source/texk/web2c/pdftexdir/`):
  - Box: keyword, else `\pdfpagebox`, else cropbox; xpdf defaults (Media `[0 0 612 792]`, Crop=Media, others=Crop), inherited, normalised x1<=x2, clipped to MediaBox. Values are C `float`; `bp2int(p) = round(p * 65781.76)` for width, height, orig_x=x1, orig_y=y1 separately.
  - `/Rotate` (inherited int, mod 360): 90/270 swap w,h; `/Matrix` for 90 `[0 -1 1 0 x1-y1 y1+x2]`, 180 `[-1 0 0 -1 x1+x2 y1+y2]`, 270 `[0 1 -1 0 x1+y2 y1-x1]`, `%.8f` with zeros stripped.
  - Form dict order: attr, `/Type /XObject /Subtype /Form /FormType 1`, `/PTEX.FileName (path)`, `/PTEX.PageNumber n`, `/PTEX.InfoDict N 0 R` (if indirect Info), `/Matrix`, `/BBox`, page keys LastModified/Metadata/PieceInfo/SeparationInfo, `/Group` (page group object), `/Resources << ... >>` (inherited, categories inline), then content: a single stream is copied raw with its /Filter and /DecodeParms; an array is decoded, joined with newlines and re-deflated.
  - Copied objects: re-printed from parsed values (reals via `convertNumToPDF`, 6 decimals, zeros stripped), new numbers allocated depth-first in print order, written FIFO; streams copied raw. Default `\pdfinclusioncopyfonts=0` replaces embedded Type 1 fonts found in the map file with pdfTeX's own (fd + encoding + font objects).
  - Placement: `q w/img_w 0 0 h/img_h x-wd*orig_x/img_w y-h*orig_y/img_h cm /Im<n> Do Q`, `pdf_print_real(...,6)`; `/Im<n>` n = `\pdfximage` count.
  - `\pdflastximagepages` = page count; `\pdfximagebbox` prints orig and orig+size.
  - LaTeX (pdftex.def) passes `attr{/Group<</S/Transparency/K false/I false>>} page N cropbox {file}` and reads the size back from a box.

## Measured
- colortbl ZERO on first run (28850 glyphs, 748 paths); makecell ZERO after the `./` fix (16293 glyphs).

## Open
- Implement the PDF reader (xref table and stream, object streams, Flate with PNG predictors) and writer above; then fancyhdr.
- Parity checker compares marks inside forms, so content and font names must match; byte layout matters only for faithfulness.
