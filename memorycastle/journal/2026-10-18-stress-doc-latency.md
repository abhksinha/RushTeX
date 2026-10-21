# Cutting the 100k-word stress document from 31 ms toward 15 ms
Author: abhi+claude | Date: 2026-10-18
Files: `src/lib.rs`, `src/layout/layout_linebreak.rs`, `src/layout/layout_engine.rs`, `src/backend/backend_pdf.rs`, `src/math/math_fonts.rs`

## Context
The user asked for the 100k-word stress document (`scripts/generate_100k_doc.py`, 262 pages, Times) to compile in about 15 ms; it took about 31 ms of CLI wall time. Profiled with macOS `sample` on an in-process loop; pprof's own signal handling distorted the leaf attribution, so `sample` was used for decisions.

## Decision
- `push_hyphenated` caches the discretionary widths of a hyphenated word per (word, face, family, size) in a direct-mapped 1024-entry table (`HyphenWidths`, 72 bytes). Words without hyphenation points are cached too. Before this the second pass shaped each word 2k+2 times. The slot index must fold face and size in before taking the low bits; without that the faces evicted each other and the hit rate was 79% on a 298-word vocabulary (92.5% after).
- `PageStreamAccumulator::push_page` clears the page's `ops` and `text_pool` instead of replacing them with new vectors, so `recycle_page` actually reuses the capacity.
- The streaming path writes pages (content stream and DEFLATE) on a second thread: a `sync_channel` of 4 pages to the writer and a return channel for recycled pages, inside `std::thread::scope`. It is used only for sources of at least 32 KiB, with more than one CPU, and never on wasm32. The output is byte-identical to the serial path.
- `explode_inlines_into_family_sf` measures words with the shaper it already resolved, instead of looking it up again per word.
- `try_break` is `#[inline(always)]`; the call overhead of 200k calls was measurable.
- `ParagraphSoA::line_extent` reads per-face ASCII height/depth tables (`ascii_height_depth_table`, built once) instead of calling `char_height_depth` per character. No measurable time change; kept because it is simpler per character.

## Rejected
- A division-free "line still too short" reject in `try_break` (comparing 297t against a precomputed ratio times s). It was exact but made the pass slower (14.6 vs 14.8 ms): the extra branches cost more than the division they skip.
- Rank-based integer maxima in `line_extent`: no gain over the f32 table.
- A 2048-entry width cache: 0.16 ms better than 1024 on the stress document, for 72 KB more.
- Folding the `\tableofcontents` checks into `needs_expansion`: about 0.1 ms, not worth the API change.
- Lazy pass-1 node building: a failing pass 1 processes only 35% of its nodes, but the build is about 0.2 ms in total.

## Measured
- Stress document, in-process `compile_str` median: 27.8 ms before, 14.8 ms after. CLI wall (15 runs, median) 30.3 ms before, 18.3 ms after; `benchmark.sh` 18.5–19.9 ms.
- CPU time (user+sys): serial build of the same code 25.4 ms, pipelined 27.2 ms (+7% for the thread hand-offs). Before this task about 30 ms. RSS 7.0 MB either way.
- 176-page math document: wall 24.4 ms to 13.9 ms.
- Line breaking on the stress document: 1406 paragraphs, 1089 need pass 2; about 200k `try_break` calls with 3.3 active nodes on average.
- After the changes the main thread is about 46% line breaking, 22% word measuring, 7% `line_extent`, 6% parsing. The writer thread is not on the critical path.
- All 38 regression documents byte-identical; harness 38/38 A; tests 223 pass with the 4 known NewCM failures; clippy clean.

## Open
- 15 ms wall needs about 3 ms more off the main thread. The remaining lever is to explode and break paragraph N+1 on a worker while the main thread paints and page-builds paragraph N. That needs the paragraph parameters (measure, indent, hang, tolerance, font state) captured into a snapshot and validated, a large change to `layout_para_full_inner`.
- Parsing on its own thread would save about 1 ms.
