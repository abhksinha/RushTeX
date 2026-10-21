# Incremental recompile: session caches for line breaks and page streams
Author: abhi+claude | Date: 2026-10-19
Files: `src/lib.rs`, `src/main.rs`, `src/backend/backend_hash.rs`, `src/backend/backend_pdf.rs`, `src/layout/layout_linebreak.rs`, `src/layout/layout_engine.rs`

## Context
Single-core trimming of a fresh compile had run out (see `memorycastle/journal/2026-10-18-single-core-speed.md`). Most real compiles are recompiles after a small edit, so the user agreed to make recompiles reuse unchanged work, with output byte-identical to a fresh compile.

## Decision
- `Session` in `src/lib.rs` owns the caches between compiles; `Session::compile_into` runs a compile with them, and `compile_into_with` is unchanged (no hashing cost for one-off compiles). The CLI gets `--watch`: it polls the source's modification time every 50 ms, recompiles through one session, and writes the PDF to `<out>.part` and renames it over the output so viewers never see half a file.
- Line breaks are cached per paragraph by `break_key`, a 128-bit hash of everything `tex_line_break` reads (widths, glue, attributes, texts, size runs, `BreakParams`, `LineShape`, base size). A hit copies the stored `HzLineDescriptor`s and pass number. Only the main TeX-breaker call in `layout_para_full_inner` uses it; table cells and the hz breaker do not.
- Page content streams are cached by `page_key`, a 128-bit hash of the page's `Paint` ops, text pool, font family and compression flag. A hit skips both `build_stream_into` and DEFLATE. An earlier version keyed on the built stream and skipped only DEFLATE; keying on the ops was worth another 25%.
- `ContentHasher` (`src/backend/backend_hash.rs`) is two 64-bit lanes with a folded 128-bit multiply. The hash stands in for comparing inputs, so it is 128 bits wide; it is not meant to resist inputs crafted to collide. `GenerationCache` marks entries used by each compile and `sweep` drops the rest after a successful compile, so the cache stays the size of the current document's working set; a failed compile keeps the previous entries.
- Documents with `\tableofcontents` or lists of figures or tables take the AST path and are compiled without the caches.

## Rejected
- Keying the page cache on the uncompressed stream: correct, but it still built every stream (about 30% of a recompile).

## Measured
- The machine ran about 1.85× slower than on 2026-10-18 during these runs (the old binary went from 27 ms to 50 ms on the stress document), so compare ratios. In-process medians: stress document fresh 43.7 ms, session recompile 19.3 ms (−56%), the same for a one-word edit. m200b fresh 33.9 ms, recompile 19.3 ms (−43%). `--watch` on the stress document: first compile 61 ms, recompiles after edits 22.6–23 ms.
- Remaining recompile profile: `layout_para_full` 23%, explode 18%, word shaping 13%, `push_page` 12% (font-usage scan and hashing), `line_extent` 10%, parsing 4%.
- Byte identity: 27 documents through one shared session, compressed and uncompressed, serial and `--threads`, 108/108 identical to fresh compiles; unit test `session_recompiles_match_fresh_compiles` covers edits, an error compile and an inserted paragraph. Fresh-compile corpus output unchanged.

## Open
- The next level is reusing painted paragraphs: explode, shaping, `line_extent` and painting are now about 64% of a recompile. Their output holds absolute positions, so a cached paragraph would need to be stored relative to its first baseline and relocated, with the page builder state checked at the paragraph boundary.
- The wasm32 target is not installed on this machine; the wasm build of `--watch` (stubbed) and the caches was not compiled.
