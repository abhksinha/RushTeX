# RushTeX progress (living checklist)
Author: abhi+claude | Date: 2026-10-22
Files: `memorycastle/journal/native-architecture.md`, `memorycastle/journal/native-port-plan.md`, `src/engine/engine_gc.rs`, `src/engine/engine_node.rs`, `src/engine/engine_token.rs`

## Context
The one place to see what is done and what is next. Tick an item when its gate passes (tests, clippy, `parity.py zero --engine --pixels --all` at 85/85, `bench_corpus.py`) and its dated note is saved; the next unticked item is the work. Detail lives elsewhere:
- `native-architecture.md` holds the rules;
- `native-port-plan.md` holds the method and the kernel and package tables;
- the dated notes hold decisions and measurements.

## Decision
### Standing rules (every item is checked against them)
- **No garbage collection.** Memory is owned and freed at known points (pools, regions, free lists; §6a of the spec). No tracing, compaction or moving of live data.
- **No hot loops.** Every surviving per-token, per-node or per-line loop is measured with `scripts/profile_doc.py` and justified in its module's comments.
- **Structures by access pattern.** Every table is SoA where a phase reads one field across many records, and AoS where a record is read whole; the choice is written next to the type.
- **Borrow, don't copy.** Pass `&[T]`, ranges and IDs; no `.to_vec()`, `.clone()` or `String` building on a per-token or per-node path.

### Done
- [x] 85 CTAN documents at zero shift through the engine (2,137 pages)
- [x] CLI runs the engine on the LaTeX format; kpathsea index (`engine_host`)
- [x] Corpus benchmark (`scripts/bench_corpus.py`)
- [x] Native architecture spec and port plan
- [x] N0a: reruns only while written files change (`last_versions`), run counts equal pdflatex
- [x] N0b (oracle only): token arena reclaimed (`collect_tokens`); fancyhdr 848 MB to 88 MB
- [x] Oracle node arena reclaimed (`collect_nodes`); format and file bytes no longer held. Seven heaviest documents: median 43 MB, max 60 MB
- [x] Profiler that tells the truth (`scripts/profile_doc.py`): `get_token` 3.3 ns/token, token volume is the cost
- [x] Zero-GC memory rules written into the spec (§6a)

### Z. Zero-GC exact core, before P1 (model M: `2026-10-22-zero-gc-core-model.md`)
- [ ] Z1a. Pools: size-class span blocks and free lists for each side table in `Nodes`, plus the ownership-check build (poisoned slots).
- [ ] Z1b. `flush_list` (§202) and `copy_list` (§204); `\copy`/`\unhcopy`/`\unvcopy` deep-copy.
- [ ] Z1c. Free points: `\shipout`, `\setbox` overwrite, group restore, `\box` and `\unvbox` moves, discards (§1105, line break, page builder).
- [ ] Z1d. `collect_nodes` deleted; 85/85 under the checker; RSS per page flat on listings.
- [ ] Z2. Build buffers (`cur`, line-break scratch) with capacity reserved once and reset per list.
- [ ] Z3. Document macros in size-class token blocks: redefinition frees the old block (§275 `eq_destroy`); a body being read holds a count (§390). `collect_tokens` deleted.
- [ ] Z4. Capacity reserved from the source size; RSS growth per page measured flat on listings (236 pages).

### B. Borrowing and layout audit (the oracle's hot paths; also the core's)
- [ ] B1. Remove the 19 `toks.get(..).to_vec()` / `nodes.get(..).to_vec()` copies: read through the span, or push the span as an input frame.
- [ ] B2. Audit the remaining `.to_vec()`/`.clone()` sites in `src/engine` (36), keeping only cold paths.
- [ ] B3. Layout table of every core record, with its size, its fields, and SoA or AoS and why (`Node` 12 B, `BoxRec`, `Glue`, `InsRec`, `Noad`, eqtb `Table`), written beside each type.
- [ ] B4. Profile the 85 documents with `profile_doc.py`; list every function above 2% self time with its cost per call.

### P. Native port (per `native-port-plan.md`)
- [ ] P0 rest: `RT_PROFILE` (expansions and time per format macro, which picks the next command to port), `RT_STATE`, `RT_NATIVE=cmd,…`, engine-against-engine `engine_boxdiff.py`, `rushtex-gen` skeleton, arXiv package sample, pinned LaTeX version
- [ ] P1. Fonts and NFSS as generated tables
- [ ] P2. Kernel core (small2e, sample2e, lppl at zero format expansions)
- [ ] P3. Output routine and floats
- [ ] P4. Tables, math, pictures
- [ ] P5. Classes
- [ ] P6. Packages by real usage
- [ ] P7. Zero dependency: no `latex.ltx`; the native path is the default
- [ ] P8. Real-time incremental compile
