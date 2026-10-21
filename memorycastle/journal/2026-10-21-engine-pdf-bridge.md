# E4 f: engine pages to PDF, and the six parity documents at 100% through the engine
Author: abhi+claude | Date: 2026-10-21
Files: `src/engine/engine_ship.rs`, `src/backend/backend_ship.rs`, `src/backend/backend_optical.rs`, `src/backend/backend_pdf.rs`, `src/engine/engine_page.rs`, `src/engine/engine_run.rs`, `src/engine/engine_io.rs`, `src/engine/engine_value.rs`, `src/engine/engine_chars.rs`, `scripts/parity/parity.py`, `scripts/engine_probes/accent.tex`, `scripts/engine_probes/halign.tex`

## Context
The engine typeset every corpus document (0 errors, pdfTeX's page counts) but its boxes never became a PDF, so parity could only count pages. E4 f is the bridge from shipped boxes to PDF.

## Decision
- **Shipout writes the content stream directly.** `engine_ship.rs` translates §619–§640 as pdfTeX does them (`pdf_hlist_out`, `pdf_vlist_out`, `pdf_ship_out`).
  - It covers glue rounding through `cur_g`/`cur_glue` and `vet_glue` (§625, §634), box and rule leaders (§626, §635, with the 10sp compensation), and running rule dimensions.
  - It writes through the existing `backend_pdftex::TextState`, pdfTeX's own `Td`/`TJ` raster. No list of positioned glyphs is kept.
  - It runs at every `ship_out` and keeps one content buffer per page, plus a bitset of the fonts used.
  - The page size and origin come from `\pdfpagewidth`, `\pdfpageheight`, `\pdfhorigin`, `\pdfvorigin` and `\hoffset`/`\voffset`, as "Calculate page dimensions" in pdftex.web.
- **Fonts by file name, not by `Face`.** `backend_ship.rs` writes the document: catalog, page tree, one shared resources dictionary, and one PDF font per TFM name.
  - Several sizes of one font share that PDF font. `/Widths` come from the first size used, as pdfTeX does.
  - `backend_optical::program_by_name` finds the embedded Type1C program among the 38 base programs and the 31 design-size masters. A font with no program is written by name, unembedded.
  - `tounicode_by_name` picks the ToUnicode map by font family (OT1 text, typewriter, OML, OMS, OMX, AMS) or a TS1 master's own map.
  - This is the first piece of roadmap item 18: the engine path needs no `Face` enum at all.
- **The format is built from `pdflatex.ini`, not `latex.ltx`.** `pdftexconfig.tex` sets the A4 page and the 1in origins. The format is `target/parity/pdflatex.rfmt`.
- **LaTeX's multi-run protocol in the harness.** `format_smoke` reruns the document, at most 3 runs, while the files it writes change. `.aux` and `.toc` from the previous run read back through `closed`. This is how cfgguide gets its table of contents and pdfTeX's 12 pages.
- **tex.web fixes found by the PDF comparison:**
  - §455: `em` and `ex` are `\fontdimen6`/`\fontdimen5` of the current font. They had been fixed cmr10 constants, which shifted every `\quad` in a non-10pt font (section numbers).
  - §1125: the accent's order is kern(delta), accent, kern(−a−delta), character. The engine put the accent first.
  - §442: a brace given as an alphabetic constant (`` `} ``) does not count in `align_state`. amsmath's `\iffalse{\fi\ifnum0=`}\fi` in `gather`/`align*` relies on this. It caused all 268 remaining testmath errors.
  - §1374: `\immediate\openout` closes a stream that is still open.
  - §1378: every open `\openout` file closes when the job ends.
  - §82: the run stops after 100 errors in one paragraph, instead of looping forever on a broken document.
- **Tooling.** `parity.py score --engine` typesets through the engine and scores `target/parity/engine/*.pdf`. It never writes the baseline. `format_smoke` gained `RT_PDF=<file>`, `RT_PAGES=1` (the first and last text of each page) and `RT_ONCE` (a single run).

## Rejected
- Mapping engine glyphs into native `Paint::GlyphRun`s. `Face` is a fixed enum of 42 faces, with design-size masters chosen by size, so arbitrary TFM names would need a reverse mapping. That deepens the font-architecture problem roadmap item 18 is meant to remove.
- Writing a positioned-glyph list and then the content stream: an extra pass and allocation per page for no gain.
- Writing without ToUnicode, as pdfTeX does. Text extraction of ligatures then differs (`ﬀ` against `f f`), so the strict score drops about 1%. The maps the native path ships are kept.

## Measured
Strict parity (0.1pt) through the engine, `parity.py score --engine`:

| Document | First PDF | After `em`/`ex` | After ToUnicode and accent order |
|---|---|---|---|
| small2e | 96.7% | 99.7% | 100.0% (748/748) |
| sample2e | 98.2% | 99.6% | 100.0% (3572/3572) |
| lppl | 99.3% | 99.6% | 100.0% (14997/14997) |
| modguide | 96.2% | 99.2% | 100.0% (11247/11247) |
| cfgguide | 97.7% | 98.8% | 100.0% (17287/17287, 12/12 pages) |
| testmath | 98.2% | 99.8% | 100.0% (42400/42400) |

- Engine runs have 0 errors on all six.
- Gates:
  - The native pipeline is unchanged at 100% on all six.
  - All 11 probe suites agree (138 probes).
  - Clippy is clean.
  - 317 tests pass. Three `math::fonts` tests fail only because TeX Live basic lacks `NewCMMath-Regular.otf`.

## Open
- The `rushtex` binary still uses the native pipeline. Running the engine from the binary needs the format and the TeX files without `kpsewhich`: roadmap item 17 (packs).
- fntguide gives 47 pages against 46. usrguide has 102 engine errors. short-math-guide has 4 errors and 0 pages.
- `\valign` (E4 e). T1/EC fonts (E4 h). The class snapshot (E4 g).
- Colour and `\special`/`\pdfliteral` whatsits are not written to the content stream yet; the six documents have none.
