# Glyph-position parity and speed baseline for the samples and the manual
Author: abhi+claude | Date: 2026-10-19
Files: `benchmark.sh`, `docs/manual.tex`

## Context
The user asked whether rushtex output is 99% identical to pdfLaTeX on real documents, after the sample work in `memorycastle/journal/2026-10-19-footnote-insertions.md` and `memorycastle/journal/2026-10-19-tex-input-spacing-rules.md`.

## Decision
Parity on whole documents is measured as the share of pdfLaTeX's glyphs that rushtex places within 0.1pt in x and y on the same page, matched one to one (PyMuPDF `rawdict` origins). This is stricter than the harness's pixel grades: one early drift moves every later glyph.

## Measured
- Glyphs within 0.1pt: `sample2e` 3571/3572 (100.0%), `small2e` 718/748 (96.0%), `lppl` 8442/14997 (56.3%), `docs/manual.tex` 17661/44280 (39.9%). Page counts match in all four. `testmath` and `amsthdoc` do not compile yet.
- `benchmark.sh` on `docs/manual.tex` (8 runs): rushtex 6.87 ms and 6.39 MB RSS; pdfLaTeX 198.57 ms and 30.28 MB (28.9x faster). The PDF is 375.5 KB against 335.3 KB, so rushtex's is 12% larger; the script's "0.9x smaller" means larger.
- The parity harness still grades 38/38 A, but its test documents are small.

## Open
- Find the first drift in `lppl` and the manual, then fix the TeX rule behind it.
