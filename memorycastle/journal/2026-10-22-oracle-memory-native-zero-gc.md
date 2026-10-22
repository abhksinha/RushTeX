# Oracle memory: node collection, format and file bytes dropped; native path is zero-GC
Author: abhi+claude | Date: 2026-10-22
Files: `src/engine/engine_gc.rs`, `src/engine/engine_eqtb.rs`, `src/engine/engine_align.rs`, `src/engine/engine_page.rs`, `src/engine/engine_run.rs`, `src/engine/engine_job.rs`, `src/engine/engine_host.rs`, `src/main.rs`, `memorycastle/journal/native-architecture.md`

## Context
User targets: under 50 MB worst case, 30–35 MB typical. Then the user insisted on truly zero garbage collection with predictable growth, and chose "native first": the zero-GC design goes into the native path; the engine (the oracle) keeps its collector as a dev tool.

## Decision
Native: `native-architecture.md` §6a now binds the native path. Records sit in typed pools at fixed indices, and memory is freed by region (paragraph, page, float queue, document). Replaced definitions return to size-class free lists. Nothing traces, compacts or moves live data.

Oracle, for measurability:
- `collect_nodes` marks one live bit per entry of each node table from `for_each_node_root`, keeps live entries in place (`Live::keep`) and remaps indices by rank.
- Shipped boxes are kept only when the harness asks (`keep_shipped`).
- The format is loaded per run and dropped once undumped (`engine_job::run`).
- `Kpse::read` memoizes paths, not file contents.

## Rejected
Zero-GC in the engine now (TeX's reference-counted token lists with free lists, deep `\copy`): every holder of a list or box in about 20k lines would need counting, for code that becomes dev-only.

## Measured
Peak RSS of the heavy seven, before → after:

| Document | Before | After |
|---|---|---|
| fancyhdr | 74 MB | 63 MB |
| multicolrule | 73 MB | 60 MB |
| listings | 65 MB | 56 MB |
| array | 53 MB | 45 MB |
| cases | 47 MB | 38 MB |
| polydemo | 37 MB | 30 MB |
| small2e | 23 MB | 23 MB |

Median 43 MB, max 60 MB (pdflatex: 34 and 40). The 7 documents take 15.4 s against pdflatex's 28.1 s. The array heap at its peak: token arena capacity 8.2 MB, fonts 3 MB, hyphenation 2 MB, names and meanings 2.5 MB.

## Open
The oracle's remaining floor is the format's interpreter state (names, meanings, macro texts, fonts, patterns). Only the native path, with tables generated at build time, removes it.
