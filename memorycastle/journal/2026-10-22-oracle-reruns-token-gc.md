# Oracle reruns by final file content; in-place token-arena collection
Author: abhi+claude | Date: 2026-10-22
Files: `src/engine/engine_job.rs`, `src/engine/engine_gc.rs`, `src/engine/engine_token.rs`, `src/engine/engine_run.rs`, `src/engine/engine_input.rs`, `src/engine/engine_align.rs`, `src/engine/engine_page.rs`, `src/engine/engine_whatsit.rs`, `src/engine/engine_mod.rs`

## Context
P0/N0 of `native-port-plan.md`: the engine took extra runs (5 on array, vwcol, listings, multicolrule, fancyhdr) and its append-only token arena kept every replaced macro text (cases stored 24.6M tokens; fancyhdr peaked at 848 MB). User target: under 50 MB worst case, 30–35 MB typical.

## Decision
`run` in `engine_job` compares each written file's last version (`last_versions`) with what the run read, as latexmk sees files on disk. Token lists are reclaimed by `collect_tokens` (`engine_gc`): when the arena reaches twice the last live size (floor `MIN_COLLECT`), between commands of `Machine::run`, `for_each_root` gathers live spans, merges them into sorted ranges, rebases every span by `partition_point`, and `Tokens::slide` moves them down in place. No refcounts, no map, no second arena. This collector is oracle-only: the native path (`native-architecture.md`) keeps kernel macros out of a token arena, so it has nothing to collect by construction.

## Rejected
Copying live lists into a fresh arena with a `HashMap` of moved spans: peak RSS rose (listings 176 to 367 MB) because old and new arenas coexist and the allocator kept the freed blocks; it also broke sharing of nested spans (a frame reading from the middle of a macro body).

## Measured
Runs now equal pdflatex's (array 2, vwcol 2, listings 3, multicolrule 3, fancyhdr 3). Peak RSS: fancyhdr 848 to 88 MB, listings 176 to 94 MB, array 96 to 55 MB, cases 48 MB, small2e 23 MB. cases stored tokens 24.6M to 1.75M. 7 heavy documents: 21.2 s against pdflatex 42.1 s.

## Open
What remains is the node arena: `Nodes::list` is append-only too (listings ends with 1.09M nodes, 24 MB, plus boxes and glues 9 MB), so shipped pages' nodes are never freed. Next: reclaim at `\shipout`. The 23 MB floor is the undumped format plus the binary; reaching 30–35 MB typical needs both.

Functions changed: `for_each_span` (`Eqtb`, `Align`, `Page`), `for_each_list` (`Input`), `holds_tokens`, `slide`, `maybe_collect`, `for_each_root`; diagnostics `report` (`RT_MEM`) and `capacities` in `format_smoke`.
