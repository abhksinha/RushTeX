# Engine PDF: zero-tolerance checker, rule rounding, colour stacks, links, destinations, outlines
Author: abhi+claude | Date: 2026-10-21
Files: `scripts/parity/pdfexact.py`, `scripts/parity/parity.py`, `src/engine/engine_pdfdoc.rs`, `src/engine/engine_ship.rs`, `src/engine/engine_color.rs`, `src/backend/backend_ship.rs`, `src/backend/backend_pdftex.rs`, `src/engine/engine_run.rs`

## Context
The user asked for true zero shift, not 0.1pt (see `2026-10-21-exact-parity-coverage-plan-modes.md`). This task built the zero checks and fixed what they found.

## Decision
- `parity.py zero [--engine] [--pixels]` (`pdfexact.py`):
  - content streams are interpreted with exact fractions: every glyph's rendering matrix, font and code, every path, and the fill/stroke colour;
  - pixels are rendered by MuPDF at 300 dpi;
  - structure covers links (rect to 0.001bp plus target), outlines, named dests and metadata.
- `TextState::rule` follows pdfTeX's `(h+1)/2`, which is real division truncated to scaled. That was the cause of the 0.001bp rule drift in fntguide, usrguide and clsguide.
- Colour stacks, `literal()` modes, `\special` pdf:, `\pdfsetmatrix`, `\pdfsave` and `\pdfrestore` follow utils.c/pdftex.web (`engine_color.rs`).
- Links, destinations and outlines (`engine_pdfdoc.rs`):
  - They are stored at scan time, and the whatsit `a` indexes the spec.
  - The walk tracks `cur_s` and a link stack: `do_link`, `append_link` at each hlist start for the same level, `end_link`, `set_rect_dimens`, and `do_dest` (skipped inside leaders).
  - Annotation and dest objects are rendered at shipout; outlines and the name tree are rendered at the end by pdfTeX's linking and `open_subentries` rules.
- One object counter in `PdfDoc` covers everything TeX can see: `\pdflastobj`, `\pdflastlink`, `goto num/page`, and page objects via `get_obj`. `backend_ship::write` numbers its own objects (catalog, tree, resources, contents, fonts) after `base`.
- Our own layout, not pdfTeX's: outline title and action are inline (one object per outline), and the name tree is one sorted leaf. Only the semantics must match, not the object graph.
- `\immediate\pdfobj` is written always; other `\pdfobj` objects only when a shipped page has `\pdfrefobj` (pdfTeX rule).

## Rejected
- Byte-identical object numbering with pdfTeX: not needed for any visible or structural property.

## Measured
`parity.py zero --engine --pixels` on 12 documents: 12/12 ZERO. That is 222 pages and 326,287 glyphs exact, 0 differing pixels at 300 dpi, and links, outlines, dests and meta all equal (short-math-guide 100/56/142, amsthdoc 39/16/41). Our CFF fonts render pixel-identical to pdfTeX's Type 1 in MuPDF.

Gates after the clippy fixes (two type aliases, `is_multiple_of`, and the redundant closures in `backend_ship.rs`):
- clippy `-D warnings` is clean.
- The probe suites agree.
- `cargo test --release` gives 319 passed and 3 failed. All 3 are in `src/math/fonts/mod.rs` and need `NewCMMath-Regular.otf`, which they look for only at fixed Linux paths. The file is not installed on this Mac. The failures are environmental and unrelated to this work.

## Open
- The 3 NewCM tests should skip when the font is absent, as the other tests in that file do (asked the user).
- amsldoc (hyperref "Missing number"), encguide (`wnr10`), first-latex-doc (`\pdfximage`).
- `\pdfannot`, threads, `\pdfrunninglinkoff`, struct dests and `\pdfnames` are not implemented.
- The structure check compares dest counts only, not each dest's page and position.
