# Reusable parity script
Author: abhi+claude | Date: 2026-10-20
Files: `scripts/parity/parity.py`, `scripts/parity/corpus/testmath.tex`

## Context
Ad hoc scratchpad scripts cost rounds. Stale PDFs were scored after failed compiles, `echo` mangled `\t`, and TeX-only primitives broke rushtex.

## Decision
Use `scripts/parity/parity.py` (uv script, pymupdf) for all parity work. It has five commands:
- `score`: strict per-page glyph score for the corpus. Its deltas and REGRESSED pages are measured against `target/parity/last_score.json`.
- `rank`: pages by glyphs lost, each with its first drifting line.
- `lines` and `miss`: drift on one page.
- `repro`: document preamble plus a line range or `--body`, built by both compilers. `--trace pages|lists` goes into a TeX-only copy.

Every run deletes old outputs and fails loudly on compile errors. Reference PDFs are cached in `target/parity/ref`.

## Measured
Strict baseline: testmath 81.0%; sample2e, small2e and lppl 100%. `score` takes 4 s.
