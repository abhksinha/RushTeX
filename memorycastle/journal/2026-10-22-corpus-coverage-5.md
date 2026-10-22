# Coverage corpus: rotation, landscape pages, fractions and units, stacking, column rules
Author: abhi+claude | Date: 2026-10-22
Files: `scripts/parity/corpus/chemarr.dtx`, `scripts/parity/corpus/chemarr.tex`, `scripts/parity/corpus/extarrows-test.tex`, `scripts/parity/corpus/icomma.dtx`, `scripts/parity/corpus/icomma.tex`, `scripts/parity/corpus/ltablex.tex`, `scripts/parity/corpus/multicolrule.dtx`, `scripts/parity/corpus/multicolrule.tex`, `scripts/parity/corpus/parcolumns.dtx`, `scripts/parity/corpus/parcolumns.tex`, `scripts/parity/corpus/pdflscape.dtx`, `scripts/parity/corpus/pdflscape.tex`, `scripts/parity/corpus/placeins-doc.tex`, `scripts/parity/corpus/polydemo.tex`, `scripts/parity/corpus/polynom.dtx`, `scripts/parity/corpus/polynom.tex`, `scripts/parity/corpus/rotating.dtx`, `scripts/parity/corpus/rotating.tex`, `scripts/parity/corpus/stackengine.tex`, `scripts/parity/corpus/units.dtx`, `scripts/parity/corpus/units.tex`, `scripts/parity/corpus/upgreek.dtx`, `scripts/parity/corpus/upgreek.tex`, `scripts/parity/corpus/xfrac.dtx`, `scripts/parity/corpus/xfrac.tex`

## Context
Topics not yet covered: rotated floats and pages (rotating, pdflscape), fractions and units
(xfrac, units, icomma), stacking and arrows (stackengine, extarrows, chemarr), polynomials
(polynom, polydemo), long tables with X columns (ltablex), parallel columns (parcolumns),
column rules drawn with TikZ ornaments (multicolrule), float barriers (placeins) and
upright Greek (upgreek).

## Decision
All 15 join the corpus. Twelve were at zero shift on the first run. The other three needed
`\quitvmode`, `\pdfsavepos` and `.pgf` lookups (separate notes).

## Rejected
- lscape.dtx: see `2026-10-22-pdfsetmatrix-rects.md` (pdfTeX reads uninitialised memory
  for a destination).
- abraces (`ltxdockit.cls` missing), tasks (own class missing), longdivision (hyperxmp),
  mcrule-examples (fontspec), floatrow and fr-sample (their pictures and macros are not
  shipped), grfguide (EPS figures), centernot (matha fonts missing): these fail under
  pdflatex itself here.

## Measured
- 15 documents, 212 pages, all ZERO. The corpus has 85 documents.
