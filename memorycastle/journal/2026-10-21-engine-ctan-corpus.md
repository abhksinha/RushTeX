# Engine path: usrguide, short-math-guide, amsthdoc, clsguide, bare_jrnl at 100%
Author: abhi+claude | Date: 2026-10-21
Files: `src/engine/engine_macro.rs`, `src/engine/engine_pdfdoc.rs`, `src/engine/engine_value.rs`, `src/engine/engine_native.rs`, `src/engine/engine_box.rs`, `src/engine/engine_page.rs`, `src/engine/engine_vf.rs`, `src/engine/engine_ship.rs`, `src/engine/engine_showbox.rs`, `src/backend/backend_ship.rs`, `scripts/engine_probes/halign.tex`, `scripts/engine_probes/prims.tex`

## Context
E4: run the rest of the CTAN corpus through the engine and match pdfLaTeX strictly.

## Decision
- §394: when a macro's parameter delimiter ends in `#{`, the brace is read twice; `align_state` is decremented once (`engine_macro.rs`). Fixed `\MakeLowercase{\NoCaseChange{..}}` in `\halign` cells (usrguide, 101 errors to 0).
- hyperref's pdfTeX extensions (`\pdfinfo`, `\pdfcatalog`, `\pdfdest`, `\pdfstartlink`, `\pdfendlink`, `\pdfobj`, `\pdfrefobj`, `\pdfoutline`, `\pdflastobj`, `\pdflastlink`) are scanned with pdfTeX's exact syntax in `engine_pdfdoc.rs`. Links and destinations are zero-size whatsits; nothing is written to the PDF yet.
- `scan_keyword` keeps up to 16 matched tokens, because pdfTeX keywords like `nonewwindow` are long.
- e-TeX `\lastnodetype` is implemented (etex.ch §424, and the page builder's `last_node_type`). LaTeX's `\par` adds `\hskip0pt` when `\lastnodetype=11`. Without it a trailing space was lost and a line break moved (short-math-guide p.4).
- With `\pdfgentounicode`, every font's ToUnicode is built from its glyph names: the `.enc` file, else the pfb's built-in encoding. The fixed tables are now only a fallback; the old MSBM table mapped letters only.
- Virtual fonts (`engine_vf.rs`) follow pdfTeX's `do_vf`, `vf_def_font` and `do_vf_packet`.
  - A font is checked lazily at its first shipout, so fonts that are never typeset cost no lookup.
  - Packets are byte ranges in one buffer, and local fonts are ordinary engine fonts.
  - The BaseFont of an embedded pfb is its own `/FontName`.
- Tooling: `RT_BOXES=<file>` dumps shipped pages as `\showbox` prints them (`engine_showbox.rs`). It diffs line by line against `parity.py boxes` logs and found the `\lastnodetype` bug in minutes.

## Rejected
- Fixing the MSBM table by hand: entries need several code points, and pdfTeX's rule is the glyph names.
- Loading `.vf` at `\font` time: a kpse lookup for every font LaTeX preloads.

## Measured
Strict parity through the engine (`parity.py score --engine`), all 100%:
- small2e, sample2e, lppl, modguide, cfgguide, testmath, fntguide
- usrguide (46749 glyphs)
- short-math-guide (41307)
- amsthdoc (21022)
- clsguide (56668)
- bare_jrnl (1115)

## Open
- amsldoc: "Missing number" at `\usepackage{hyperref}`.
- encguide: `wnr10` TFM missing.
- first-latex-doc: png missing (check whether pdfLaTeX's reference has the same problem).
- VF specials, link annotations and outlines are not written yet.
- The engine file `engine_show.rs` was overwritten by mistake and restored from Claude's file-history backup.
