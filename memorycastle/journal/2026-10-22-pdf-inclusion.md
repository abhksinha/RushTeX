# PDF inclusion (`\pdfximage` of a PDF page) and hot-loop removals
Author: abhi+claude | Date: 2026-10-22
Files: `src/backend/backend_pdfread.rs`, `src/backend/backend_mod.rs`, `src/engine/engine_image.rs`, `src/engine/engine_ship.rs`, `src/backend/backend_pdftex.rs`, `src/engine/engine_native.rs`, `src/engine/engine_pdfdoc.rs`, `src/backend/backend_png.rs`, `src/engine/engine_chars.rs`, `src/engine/engine_node.rs`, `src/engine/engine_run.rs`, `scripts/parity/pdfexact.py`

## Context
Batch 2 item 9: fancyhdr.dtx includes `example-image.pdf`; the engine only read PNG. The
spec is in `2026-10-22-pdf-inclusion-spec.md`. Also part of the hot-loop sweep of the batch work.

## Decision
- New `backend_pdfread.rs`: one byte buffer, objects parsed on demand (`PdfFile::get`),
  streams kept as raw byte ranges and copied undecoded; only object streams and xref streams
  are decoded, once each. Classic xref tables, xref streams (`/W`, `/Index`), `/Prev`,
  `/XRefStm`, FlateDecode with PNG predictors (reusing `backend_png::unfilter`). Page tree
  walk by `/Count`, inherited Resources/MediaBox/CropBox/Rotate, xpdf box normalising and
  clipping, boxes as C `float`.
- `write_form` follows pdftoepdf.cc `write_epdf` key order; `Copier` is `inObjList`
  (`addOther` numbering, FIFO `writeRefs`). Fonts are copied verbatim (no replacement by
  pdfTeX's own embedding): marks compare equal, subset tags are ignored by the checker.
- Sizes by writeimg.c `bp2int` per value (width, height, origin separately); `scale_image`
  takes the size as is for a PDF. `\pdflastximagepages` is the page count.
- Placement: pdfTeX `out_image` for PDF, scale `ext_xn_over_d(w,10^6,img_w)` to 6 places,
  shifted by the box origin scaled the same way (`TextState::form`).
- Box choice: keyword, else `\pdfpagebox`, `\pdfoptionalwaysusepdfpagebox` overrides, default crop.
- `pdfexact.py` resolves an indirect `/BaseFont` (pdfTeX writes `/BaseFont N 0 R` for a font
  it replaced in an included PDF).
- Hot loops: ligature stack reused across characters (`Machine::lig_buf`), `pack_lig` writes
  ligature characters straight into `lig_chars` (`Nodes::lig_node_at`), PNG `unfilter`
  chooses the filter once per row instead of per byte.

## Rejected
- Decoding and re-encoding content streams: changes bytes for nothing; pdfTeX copies a single
  stream raw, and so do we. Only an array of content streams is joined and deflated again.
- A full xpdf-like object cache: the included page touches a few dozen objects, each parsed
  once by `write_refs`; a cache would only hold memory.

## Measured
- Fixture (`\includegraphics` width, scale+angle, page+height of example-image{,-a,-b}):
  ZERO, glyphs 12/12, paths 9/9, pixels 0.
- Gate before this step: 41/42 at zero shift (fancyhdr was the PDF one).

## Open
- fancyhdr now stops at `\pdfxform` (pgf shadings): next step.
- `\pdfximagebbox`, `named` pages, JPEG, `\pdfinclusioncopyfonts` font replacement.
