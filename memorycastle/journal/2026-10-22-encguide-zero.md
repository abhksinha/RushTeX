# encguide at zero shift: converged reference, reference-identical errors, `.enc` parsing, TeX-exact font error
Author: abhi+claude | Date: 2026-10-22
Files: `scripts/parity/parity.py`, `src/backend/backend_ship.rs`, `src/engine/engine_font.rs`, `src/engine/engine_vf.rs`, `scripts/parity/corpus/encguide.tex`

## Context
Step 2 of `2026-10-22-remaining-corpus-plan.md`. encguide was blocked by "Font wnr10 not loadable".

## Decision
- **Recovery is by design.** `\ftable` loads each font in `\batchmode` and omits the table when it is `\nullfont`. The reference pdfTeX hits the same 12 "not loadable" errors and recovers.
  - `run_engine` now accepts engine errors only when they equal the reference log's errors: same message, same input line, same count.
  - `RT_ERRS` was raised to 200 so the full list is compared.
  - `reference_errors()` parses the `! …` lines of the reference log with their `l.N` line.
- **Font errors are TeX-exact.** `find_font` takes the control sequence being defined and prints §561's message: `Font \X=wnr10 [at …pt|scaled n] not loadable: Metric (TFM) file not found`.
- **The reference converges.** `run_pdflatex` reruns until `.aux`/`.toc`/`.lof`/`.lot`/`.out` stop changing (at most 5 passes), as latexmk does.
  - With a fixed two passes, encguide's reference was one step short: its pass-2 `.toc` said Introduction is on page 2 while the PDF printed 1.
  - The engine already reran to convergence; the reference was what was wrong.
- **`.enc` parsing follows pdfTeX's `load_enc_file`** (writet1.c):
  - from the first `[`, names start at `/` and end at a blank, `]` or the next `/`;
  - `%` ends the line.
  - Latin Modern's `/enclmec[` had produced an empty `/Differences`, so MuPDF drew glyphs from the built-in encoding (wrong glyphs, en dash shown as `\x15`).
- **Reference environment.** `tlmgr --usermode install mflogo-font`. Without the Type 1 `logo10`, pdfTeX fell back to a 600 dpi PK bitmap Type 3 font, which a standard TeX Live install does not do.

## Rejected
- Per-document error whitelists: the reference's own error list is the general rule.
- PK/Type 3 bitmap fonts for now (writet3.c): a real pdfTeX path, but only for installs that lack Type 1 fonts.

## Measured
- encguide ZERO: 30 pages, 37,153 glyphs, 2,503 paths, 0 pixels at 300 dpi.
- Corpus 14/15 at zero shift: 291 pages, 424,548 glyphs.
- 326 tests pass; clippy clean; probe suites agree; native `score --all` unchanged at 100% on its 6 documents.

## Open
- first-latex-doc: `\pdfximage` (step 3).
- PK bitmap (Type 3) fonts like writet3.c, for installs without Type 1 fonts.
- `.notdef` entries are written into `/Differences`; pdfTeX omits them. This makes no visual difference.
