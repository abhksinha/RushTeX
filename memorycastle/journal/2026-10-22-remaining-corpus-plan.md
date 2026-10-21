# Plan: the last three corpus documents to zero shift (amsldoc, encguide, first-latex-doc)
Author: abhi+claude | Date: 2026-10-22
Files: `scripts/parity/pdfexact.py`, `scripts/parity/parity.py`, `src/engine/engine_pdfdoc.rs`, `src/engine/engine_ship.rs`, `src/backend/backend_ship.rs`, `src/engine/engine_font.rs`, `scripts/parity/corpus/amsldoc.tex`, `scripts/parity/corpus/encguide.tex`, `scripts/parity/corpus/first-latex-doc.tex`

## Context
12 of the 15 CTAN corpus documents pass `parity.py zero --engine --pixels`: 222 pages, 326,287 glyphs, 964 paths, 0 pixels differing (`2026-10-21-engine-pdf-structure-zero.md`). The user asked for a plan to finish the other three, written down before starting.

## Decision
The order of work, each step closed by its gates (`cargo test`, clippy, probe suites, native `score --all`, `zero` on all documents) and a journal note:
0. Prep:
   - `memcastle graph --code`;
   - tighten the structure check so each named destination's page and exact position are compared, not only their count;
   - rerun the 12.
1. amsldoc: "Missing number, treated as zero" at `\usepackage{hyperref}` (l.252).
   - Trace with `RT_TRACE` to the primitive scanning `{` as a number, and map it to tex.web/pdftex.web.
   - Fix the rule, add a probe, then run `zero`.
2. encguide: the `wnr10` TFM is not found.
   - Find where the reference build gets it (`kpsewhich`, its log, `pdftex.map`).
   - Install the package in the user tree, or fix our lookup rule. Then run `zero`.
3. first-latex-doc: image support from pdftex.web/writeimg.c in our own form.
   - `\pdfximage` (dimensions, page, attr), `\pdflastximage`, `\pdfrefximage`;
   - a whatsit placed with `cm` and `Do` as `out_image` does;
   - PNG read for size and resolution, its zlib data passed through as an image XObject.
   - graphicx's pdftex driver unchanged. Check with `zero --pixels`.
4. The full gates; 15/15 documents identical. Then the coverage corpus (text articles, math articles).

Working rule for every step:
- `memcastle search --sections` before the design;
- `memcastle links` on every file before its first edit, shell edits included;
- note and save after.
The user asked for this explicitly after it slipped in the previous task.

## Open
- Whether `wnr10` exists in the reference TeX Live at all; encguide may have been built with a fallback font.
