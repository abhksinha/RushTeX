# Coverage corpus: figure wrapping, side captions, hanging text, exam
Author: abhi+claude | Date: 2026-10-22
Files: `scripts/parity/corpus/sidecap.tex`, `scripts/parity/corpus/cutwin.tex`, `scripts/parity/corpus/hanging.tex`, `scripts/parity/corpus/examdoc.tex`, `scripts/parity/corpus/sidecap.dtx`, `scripts/parity/corpus/cutwin.dtx`, `scripts/parity/corpus/hanging.dtx`

## Context
Coverage by topic (user's priority list: tables, math, enumerations, minipages, two columns,
figure wrapping). These four add cut-out windows in paragraphs (cutwin), side captions
(sidecap), hanging paragraphs and punctuation (hanging) and exam questions/parts/points
with many nested enumerations and boxes (exam class manual, 127 pages).

## Decision
Added as found; no engine change was needed: the engine's general rules already cover them.

## Rejected
- paralist.dtx: pdflatex itself stops (`\@item` doesn't match its definition).
- floatflt.dtx: pdflatex stops (Missing `\begin{document}`).
- mdwlist.dtx: needs the whole mdwtools bundle (`mdwtools.tex`).
- empheq: not at the guessed CTAN path.

## Measured
- sidecap 10 pages 14229 glyphs; cutwin 13 pages 20110; hanging 6 pages 11572; examdoc 127
  pages 174777 glyphs, 2570 paths, 477 links: all ZERO, pixels 0, first run. Corpus 54.
