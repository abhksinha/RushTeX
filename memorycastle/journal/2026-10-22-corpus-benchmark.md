# Corpus benchmark: rushtex against pdflatex on 85 documents (time, CPU, RAM)
Author: abhi+claude | Date: 2026-10-22
Files: `scripts/bench_corpus.py`, `src/engine/engine_job.rs`, `src/engine/engine_token.rs`

## Context
The user asked for a script that compiles all 85 zero-shift corpus documents with both tools and compares RAM, CPU and time.

## Decision
`scripts/bench_corpus.py` compiles each document as a user does, from no auxiliary files.
- **rushtex:** one `rushtex doc.tex`, which runs the document again internally until it is stable.
- **pdflatex:** run again until `.aux`, `.toc`, `.lof`, `.lot` and `.out` stop changing, at most 5 runs (latexmk's rule).
- **Measurement:** each process's `wait4` rusage gives wall time, user and system CPU (summed over runs) and peak RSS (the largest run). `--repeat N` keeps the fastest timing.
- **Output:** a per-document CSV in `target/parity/bench_corpus.csv`.

## Measured
All 85 documents, 2,137 pages, one timing each:
- **Wall time:** rushtex 129.4 s, pdflatex 380.2 s; 2.9× faster.
- **CPU time:** rushtex 120.5 s, pdflatex 314.9 s; 38% of pdflatex's CPU. System time is 3.2 s against 151.9 s.
- **Runs:** rushtex 222, pdflatex 206.
- **Peak RSS:** rushtex median 48.7 MB, maximum 847.8 MB (fancyhdr); pdflatex median 31.1 MB, maximum 40.9 MB. rushtex uses less RAM on 10 of 85 documents, all of them small.
- **Small documents:** 5–8× faster and 0.8× the RAM (small2e 0.066 s against 0.41 s).
- **encguide:** pdflatex took 42 s, spent running font-generation scripts for a font that is not installed.

## Open
- **RAM.** The token store (`Tokens::store`) is append-only: a redefined macro's old text is never freed. cases holds 24.6M stored tokens (about 220 MB of RSS), and fancyhdr reaches 848 MB. tex.web frees a list when its reference count drops to zero (`flush_list`, `delete_token_ref`). This is the next fix.
- **Extra runs.** `engine_job::run` stops only when every written file repeats, while pdflatex's rule checks just the five auxiliary files. Several documents take 5 runs where pdflatex takes 2 or 3: array, vwcol, listings, multicolrule, fancyhdr. That costs array its whole speed lead (1.0×). Restrict the check to the files LaTeX reads back, or find out which file keeps changing.
