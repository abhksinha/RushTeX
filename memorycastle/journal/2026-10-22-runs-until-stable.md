# Engine runs a document up to 5 times, as latexmk; empheq and supertabular added
Author: abhi+claude | Date: 2026-10-22
Files: `src/engine/engine_run.rs`, `scripts/parity/corpus/empheq.tex`, `scripts/parity/corpus/supertabular.tex`, `scripts/parity/corpus/empheq.dtx`, `scripts/parity/corpus/supertabular.dtx`

## Context
empheq.dtx (math boxes, 23 pages) drifted on page 16: our text read "Figure 2 on the
following page." where pdfLaTeX's read "Figure 2.", which moved the figure 0.167pt and the
text below 0.5pt. The reference (`run_pdflatex`) reruns until the files it reads back stop
changing, up to 5 times; `format_smoke` stopped at 3, before varioref settled.

## Decision
`format_smoke` reruns while the written files change, up to 5 runs, the reference's rule.
A document that settles earlier still stops earlier, so the corpus pays nothing.

## Rejected
- Suspecting picture mode or float glue: the page's box tree, compared line by line with
  pdfTeX's `\tracingoutput` (whatsits, `\setlanguage`, `\ETC.` and 79-column wraps
  normalised), was identical in a run with the same aux state; the text came from the aux.

## Measured
- empheq: ZERO, 23 pages, 32100 glyphs, 391 paths (was 30924 glyphs, 335 paths).
- supertabular.dtx: ZERO, 24 pages, 36804 glyphs, first run.
- Gate: 56/56 at zero shift, 336 tests, probes agree, clippy clean.
