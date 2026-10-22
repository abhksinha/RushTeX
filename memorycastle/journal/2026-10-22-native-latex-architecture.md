# SUPER IMPORTANT: RushTeX goes fully native; the TeX engine becomes the exactness oracle
Author: abhi+claude | Date: 2026-10-22
Files: `memorycastle/journal/native-architecture.md`, `src/engine/engine_mod.rs`, `src/lib.rs`, `src/main.rs`, `Resource_Conscious_Software_Engineering.md`, `CLAUDE.md`
Supersedes: 2026-10-20-engine-state-plan.md

## Context
The engine reached zero shift on 85 documents by interpreting `latex.ltx` and the package macros. The benchmark showed what interpreting costs: only 2.9× faster than pdflatex, a median of 48.7 MB RAM against 31.1 MB, and TeX Live required at run time. The user wants bare-metal speed, zero dependencies, real-time incremental compile, and their own simplified language on top. The effort is accepted; haste is not.

## Decision
- LaTeX's kernel and the supported packages are written natively in Rust (approach A). They feed the existing exact typesetting core.
- Static data is generated at build time into tables.
- The macro interpreter becomes a development oracle and is compiled out of releases.
- Incremental compile is designed in.
- The full binding specification is the living spec `memorycastle/journal/native-architecture.md`: data structures, data-movement rules, hot-loop rules, the incremental design, gates and the review checklist.
- This replaces the 2026-10-20 hybrid route, in which class and package code ran as macros on the engine.

## Rejected
- Approach B (the engine plus native fast paths) as the product. Checkpoints cost megabytes, dependencies cannot be tracked exactly, and everything without a shortcut is still interpreted.
- Runtime ingestion of TeX Live files.

## Measured
Baseline at adoption: the oracle took 129.4 s against pdflatex's 380.2 s over 85 documents, with a median peak RSS of 48.7 MB against 31.1 MB (`scripts/bench_corpus.py`).

## Open
Work order N0–N6 is in the spec. N0, fixing the oracle's extra runs and freeing replaced token lists, comes first.
