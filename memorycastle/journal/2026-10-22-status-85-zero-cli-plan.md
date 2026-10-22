# Status: 85 documents at zero shift through the engine; next, the CLI on the engine
Author: abhi+claude | Date: 2026-10-22
Files: `src/main.rs`, `src/engine/engine_run.rs`, `scripts/parity/parity.py`, `scripts/parity/corpus/`

## Context
The user asked for a record of what is done, what was achieved and what is left, before routing the `rushtex` command through the engine and matching the 31 documents added this session end to end.

## Decision
The next step is the command line on the engine. `rushtex doc.tex` runs the document on the format (`pdflatex.rfmt`) with the same multi-run rule as `format_smoke`, and `parity.py` gains a way to score the binary's own PDF. The run loop moves out of the test into one engine function, so the test and the CLI cannot drift apart. The 31 documents of this session are the first acceptance set for the CLI path.

## Measured
Achieved so far, all through the engine (`parity.py zero --engine --pixels --all`):
- 85 of 85 corpus documents, 2,137 pages, at zero shift.
- Zero shift means the same page count; every glyph and path at exactly the same place, with no tolerance; no colour difference and no extra marks; the same links, destinations, outlines and annotations; and 0 differing pixels.
- 31 of those documents (416 pages) were added this session: tables, columns, math, lists, rotation and landscape pages, boxes, figure wrapping and floats.
- 336 unit tests pass; clippy is clean; probes agree (page 6/6, primitives 11/11, scan 4/4, whatsits 2/2).
- The largest documents are paracol (270 pages), listings (236), examdoc (127) and fancyhdr (124).

## Open
- **The CLI.** Plain `rushtex doc.tex` still uses the native pipeline, which is not at zero shift on these documents. This is the next step.
- **Images:** JPEG, PNG copy, `\pdfximagebbox`.
- **Primitives:** `\valign`; `\pdfannot`, `\pdfnames`, threads, `\pdfrunninglinkoff`.
- **Fonts and format:** font subsetting and `.notdef`, which also clears the mdotest pixels; the class snapshot (E4 g).
- **Rejected downloads:** lscape, where pdfTeX reads uninitialised memory for one destination. Others need hyperxmp, fontspec, missing classes, or EPS figures.
- **The references** come from the local TeX Live 2023.
