# The `rushtex` command runs the TeX engine; 85 documents at zero shift end to end
Author: abhi+claude | Date: 2026-10-22
Files: `src/main.rs`, `src/engine/engine_job.rs`, `src/engine/engine_run.rs`, `src/engine/engine_mod.rs`, `scripts/parity/parity.py`, `scripts/differential_parity_harness.py`, `scripts/benchmark_100k.py`, `scripts/benchmark_phase1.py`, `scripts/compare_benchmark.py`, `CLAUDE.md`

## Context
The engine reached zero shift on 85 documents, but only through the `format_smoke` test. Users running `rushtex doc.tex` still got the native pipeline.

## Decision
`rushtex doc.tex` runs `engine_job::run` on the LaTeX format whenever a TeX installation is found. The format is made once from `pdflatex.ini` into the cache directory, and made again when an older build's format no longer loads. Without TeX Live, or with `--native`, the native pipeline compiles the document. `format_smoke` and the command share `engine_job`. `parity.py --engine` now scores the command's own PDF, so the gate checks what users get.

The run loop keeps the PDF of the run whose written files repeat what it read. The old harness ran that same input once more, so every document saves a full run.

## Rejected
- Making the engine opt-in (`--engine`). Users would keep getting the inexact output by default.
- Moving the native-pipeline tools onto the engine. `parity.py score`, the differential harness and the benchmark scripts measure the native pipeline, so they now pass `--native`. `benchmark.sh` keeps the default and measures what users get.

## Measured
- Through the command (`parity.py zero --engine --pixels --all`): 85 of 85 documents at zero shift. 338 tests pass, clippy is clean, probes agree, and native `score` on sample2e, small2e and lppl is still 100%.
- The 31 documents added this session, 416 pages: rushtex 16.2 s against pdflatex 63.7 s with the same number of runs per document (79 runs). That is 3.9× faster end to end, including file lookups and PDF writing.
- small2e takes 0.036 s (2 runs; it was 3 runs before). polydemo, 69 pages, takes 0.39 s (2 runs).
- Peak RSS: small2e 23.8 MB against pdflatex's 30.4 MB; polydemo 48.0 MB against pdflatex's 30.9 MB.
- Making the format the first time takes 4.1 s; the format is 4.5 MB.

## Open
- **Memory on large documents.** polydemo uses 48 MB against pdflatex's 31 MB. Each run undumps the whole format, and the node and token arenas grow. Measure where it goes before changing anything.
- **Watch and stdin modes** still use the native pipeline.
- **No `.aux` file is written to disk**, so every invocation repeats the runs from scratch. Seeding a run from the previous invocation's files would make a stable document compile in 1 run.
