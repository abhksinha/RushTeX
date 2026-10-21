# first-latex-doc at zero shift: PNG images, origin reset at BT, \today, per-page XObjects
Author: abhi+claude | Date: 2026-10-22
Files: `src/engine/engine_image.rs`, `src/backend/backend_png.rs`, `src/backend/backend_inflate.rs`, `src/backend/backend_pdftex.rs`, `src/backend/backend_ship.rs`, `src/engine/engine_ship.rs`, `src/engine/engine_pdfdoc.rs`, `src/engine/engine_pack.rs`, `src/engine/engine_linebreak.rs`, `src/engine/engine_page.rs`, `src/engine/engine_run.rs`, `src/engine/engine_prim.rs`, `src/engine/engine_whatsit.rs`, `src/engine/engine_native.rs`, `src/engine/engine_mod.rs`, `src/backend/backend_mod.rs`, `src/expand/expand_mod.rs`, `scripts/parity/parity.py`, `scripts/parity/corpus/first-latex-doc.tex`, `src/engine/engine_snapshot.rs`

## Context
Step 3 of `2026-10-22-remaining-corpus-plan.md`: first-latex-doc, the last corpus document,
includes four RGBA PNGs with `\includegraphics[scale=.35]`. The engine reported
"File `latex-first.png' not found" and had no `\pdfximage`.

## Decision
- File lookup in `format_smoke` tries the document's own directory before `kpsewhich`
  (kpathsea searches `.` first and pdfLaTeX runs in the document's directory).
- `\pdfximage` (`scan_image`, `scale_image` ported literally, `ext_xn_over_d` as utils.c
  `extxnoverd` in doubles with half-away rounding, `one_inch` = 226 as pdftex.web sets it)
  reads only the PNG header at scan time and takes its object number then;
  `\pdflastximage` returns it; `\immediate\pdfximage` marks it written.
- `\pdfrefximage` is a whatsit `XIMAGE` (sub 18) with its width/height/depth in a
  `nodes.rules` record (`a`) and the image index in `b`. `Node` stays 12 bytes and
  hpack, vpack, line breaking (`act_width`), the page builder (`page_total` without
  `\topskip`) and `vert_break` read the dimensions like a rule's, as pdftex.web's
  "Incorporate a whatsit node" sections do.
- Shipout follows `out_image`: end text, `q`, `w 0 0 h x y cm` (scale printed with
  `print_real(…, 4)`, translation with `print_bp` relative to the moved origin), `/Im<k> Do`, `Q`;
  hlist at `base_line + depth`, vlist after `cur_v += height + depth`.
- Pixels: our own inflate (`backend_inflate.rs`) and PNG decoder (`backend_png.rs`, the five
  filters, Adam7, tRNS, 16-bit strip unless `\pdfimagehicolor` in PDF ≥ 1.5). The output
  follows writepng.c's non-copy writers: RGBA splits into `/DeviceRGB` plus a
  `/DeviceGray` `/SMask`, and pages showing an alpha image get the shared
  `/Group <</S/Transparency /CS/DeviceRGB /I true>>`. All 8 decoded streams of the corpus
  images are byte-identical to pdfTeX's (SHA-1 compared).
- `pdf_begin_text` does `pdf_set_origin(0, cur_page_height)`: our `begin_text` now moves an
  origin left displaced by `\pdfsave`/`\pdfsetmatrix`/`\pdfrestore` back before `BT`. Without
  it, text positions after a scaled graphic were rounded relative to the moved origin
  (0.01bp off).
- §241 `fix_date_and_time` on `Machine` (UTC from a Unix time). `format_smoke` uses
  `SOURCE_DATE_EPOCH` when `FORCE_SOURCE_DATE=1`, as web2c does, else the clock. The parity
  harness pins both sides to 2026-10-20 00:00 UTC (`FIXED_DATE`), so `\today` matches.
- Image XObjects go into the resources of only the pages that show them (one resources
  object per such page; the other pages share the font-only one), as pdfTeX's
  `pdf_ximage_list` does.

- Decoded rows go straight into the colour and soft-mask streams (no per-pixel
  intermediate): peak transient memory is the decoded rows plus the output.
- `engine_snapshot` accepts the pdfTeX whatsits from `DEST` on (`a` indexes a side table,
  not a token span); before, a dumped list holding a link, destination or image failed
  validation.

## Rejected
- Passing IDAT through with `/DecodeParms /Predictor 15`: pdfTeX does not copy an
  alpha PNG (or one with sBIT, gAMA≠1, tRNS…), and the alpha has to be split anyway.
  Only decoded pixels need to match; the compressed bytes do not.
- One shared resources dictionary listing every image: MuPDF treats every page whose
  resources hold a soft-masked image as transparent and blends it in a group. That gave
  ±1 colour differences on pages 1 and 6–9, where no image is shown.

- Setting the page group at `\pdfximage` time as well (writepng.c `read_png_info` does):
  pdftex.web resets `pdf_page_group_val` when `pdf_ship_out` begins, so a value set while
  scanning is always cleared before any page ships. Only `out_image` matters.

## Measured
- first-latex-doc: 9/9 pages, 11855/11855 glyphs, 619/619 paths, links 56/56, outlines
  8/8, dests 205/205, 0 pixels at 300 dpi.
- Zero check: 15/15 documents ZERO (whole corpus).
- 332 unit tests pass; clippy `-D warnings` clean; all 12 engine probe suites agree; the
  native `score` is 100.0% (+0.0) on cfgguide, lppl, modguide, sample2e, small2e, testmath.

## Open
- JPEG (DCT passthrough), PDF inclusion, JBIG2, `\pdfximagebbox`,
  `\pdflastximagepages`/`colordepth`, and `\pdfxform` (the `GPT@clipbox` path) are still
  not implemented.
- The PNG copy path (`/Predictor 15` passthrough when pdfTeX would copy) gives the same
  pixels, but smaller files would need it.
- `\showbox` prints an image node as `\whatsit18`, not `\pdfrefximage …`.
- With no `SOURCE_DATE_EPOCH`, the date is taken in UTC, not local time.
