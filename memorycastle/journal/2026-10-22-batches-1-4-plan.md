# Plan: batches 1–4 (housekeeping, figures, coverage corpus, deferred engine work)
Author: abhi+claude | Date: 2026-10-22
Files: `.github/workflows/ci.yml`, `CLAUDE.md`, `scripts/parity/parity.py`, `src/engine/engine_image.rs`, `src/engine/engine_showbox.rs`, `src/backend/backend_png.rs`, `src/backend/backend_ship.rs`, `src/engine/engine_ship.rs`, `src/engine/engine_pdfdoc.rs`, `src/engine/engine_align.rs`, `src/engine/engine_snapshot.rs`

## Context
After the corpus reached 15/15 at zero shift (`2026-10-22-first-latex-doc-images.md`), the
user asked for all four batches at once, source-driven (tex.web, pdftex.web, writeimg.c,
writejpg.c, writet3.c, epdf), with little trial and error, minimal data movement and no hot
loops, following `Resource_Conscious_Software_Engineering.md`.

## Decision
Order: each batch ends with the gates (`cargo test`, clippy `-D warnings`, the 12 probe
suites, `parity.py zero --engine --pixels --all`, native `score` on the six baseline
documents) and a journal note.

Batch 1, housekeeping:
1. CI: drop the stale native harness step; install the TL2023 NewCMMath for the font tests;
   build, test, clippy gate. Parity needs the same TeX Live as the references, so it runs as
   a separate, non-gating job until CI pins a TeX Live.
2. Rebuild every reference with `FIXED_DATE` (one `--build`).
3. CLAUDE.md: engine bullet lists the new modules; memorycastle section from `memcastle init`.
4. `\showbox`: an image whatsit prints as pdfTeX prints `pdf_refximage_node`.
5. `\pdflastximagepages`, `\pdflastximagecolordepth`, `\pdfximagebbox`.
6. PNG copy path (writepng.c `copy_png`): IDAT bytes moved once, with `/DecodeParms`
   `/Predictor 15`, when pdfTeX would copy.

Batch 2, figures:
7. JPEG (writejpg.c): SOF header for size, colour, bits; JFIF/Exif density for resolution;
   the file is the stream (`/DCTDecode`), never decoded.
8. `\pdfxform`, `\pdfrefxform`, `\pdflastxform` (form XObjects from a box), and the
   `out_form` placement.
9. PDF inclusion (pdftoepdf.cc): a small PDF reader (xref table and streams, object
   streams, `/Pages` tree, page boxes), copying the page's content and the objects it
   reaches, renumbered; no re-encoding of streams.

Batch 3, coverage corpus: six documents chosen for coverage (floats, two columns, minipages,
amsart theorems, bibliography, figures of all three kinds). Each gap is fixed from the source.

Batch 4, deferred engine work: PK fonts as Type 3 (writet3.c), `\valign` (tex.web §785,
§1130), the class snapshot (E4 g), font subsetting and `.notdef` in `/Differences`,
`\pdfannot`, `\pdfnames`, threads, `\pdfrunninglinkoff`.

Resource rules for all of it: decoders read slices of the input file and write into one
output buffer each; images are decoded only when a shipped page shows them; nothing is
copied twice; tables are built once.

## Open
- Batch 3 documents are picked by coverage; their gaps are not known ahead.

## Measured
- Batch 1 closed (commit `835f51e`): items 1–4 done; all 15 references rebuilt with
  `FIXED_DATE`, still 15/15 ZERO; 332 tests, clippy, 12 probe suites agree.
- Items 5–6 move into the XImage redesign of batch 2. The PNG copy path has no gate in the
  corpus (every corpus PNG has `sBIT`, so pdfTeX never copies them): file size only, last.
- Reordered at the user's request: the coverage corpus (tables, mathematics, enumerations,
  minipages, two columns, wrapped figures, from CTAN documentation) comes before figures.
