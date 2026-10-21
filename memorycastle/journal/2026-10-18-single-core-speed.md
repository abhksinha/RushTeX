# Single-core speed: serial default, TJ displacements, rejected work removals
Author: abhi+claude | Date: 2026-10-18
Files: `src/lib.rs`, `src/main.rs`, `src/backend/backend_pdf.rs`, `src/layout/layout_linebreak.rs`, `src/backend/backend_deflate.rs`, `src/backend/backend_shape.rs`

## Context
The pipelined writer thread (see `memorycastle/journal/2026-10-18-stress-doc-latency.md`) brought the 100k-word stress document to about 18 ms of wall time but used two cores. The user decided that one core is better for real use (batch builds, editors, CI runners already run many jobs in parallel), and asked for more single-core speed, but only where the output stays near-identical. The earlier "50x faster than pdfLaTeX" figure came from commit d99e588, whose inexact engine produced 279 pages where pdfLaTeX produces 267, so it is not a fair baseline.

## Decision
- Serial is the default. `CompileOptions { compress, threads }` with `compile_into_with` and `compile_file_with` in `src/lib.rs`; the CLI flag `--threads` turns on the pipelined writer. The output is byte-identical either way.
- `build_stream_into` now tracks where the viewer's text position is after each run, using the same widths the font dictionary declares (`TextFont::glyph_units` for shaped runs; `pdf_simple_widths` gives the embedded Computer Modern `/Widths` for unshaped math runs), the TJ kerns, and `Tw` per space byte. A run on the same baseline is appended to the open `TJ` array with a displacement (`OpenArray`) instead of getting its own `Td`. A font change emits only `Tf` and continues with a displacement. It falls back to `Td` on a baseline change, a run of unknown width (non-ASCII or raw glyphs, Base-14 unshaped runs), or after any non-text operator. This is what pdfTeX does.
- The cursor must use the values as written to the stream: the font size and `Tw` rounded to 0.01. With the unrounded `Tw` the drift reached 0.07 pt by the end of a justified Times line.

## Rejected
- Building the hyphenated node list once and having pass 1 step over the hyphenation discretionaries (`run_pass` with a `hyphenated` flag). It was byte-identical, but m200b went from 17.9 to 18.5 ms because the 23% of paragraphs that pass at `\pretolerance` paid for hyphenation and pass 1 walked more nodes. Reverted.
- Shaping each word once and having the writer copy kern-free words. `TextFont::shape` is already bitmap-filtered for ligatures and kerns, and the writer's share is about 0.8 ms, so it would save under 0.5 ms for new plumbing from word attributes to `Paint::GlyphRun`.
- A faster DEFLATE. `backend_deflate` is a greedy single-probe fixed-Huffman encoder at about 500 MB/s, already in LZ4 territory; anything faster would trade ratio or add complexity for a fraction of a millisecond.
- Line height computed during explode, and a tighter line-breaker layout: both were estimated at under 1 ms each, and the try_break experiments in the earlier note showed that extra branches cost more than they save.

## Measured
- Glyph positions against the pre-change output, every character in 23 documents (5.8M characters): max deviation 0.0057 pt, which is the old `Td` rounding to 0.01 pt. Text is identical except for spaces that MuPDF synthesizes from the larger displacements.
- Content stream size: stress document 2.2 MB to 1.99 MB uncompressed (36.5k to 15.1k `Td`); survey (CM, math) 125 KB to 102 KB (2944 to 1169 `Td`). Compressed PDF: stress 1.06 MB to 0.96 MB, stress_40k 335 KB to 284 KB.
- Time did not change measurably: in-process `compile_str` median 23.0 ms on the stress document, 17.9 ms on m200b.
- CLI, median of 15: stress document serial 27.0 ms wall, 26.0 ms CPU, 5.8 MB RSS; `--threads` 20.9 ms wall, 29.3 ms CPU. m200b serial 23.2 ms, `--threads` 16.3 ms. Survey 3.7 ms. pdfLaTeX takes about 1034 ms on the stress document, 38x the serial time.
- Serial profile: deflate 19%, `run_pass` 19%, `build_stream_into` 11%, explode 8% plus word shaping 6%, `build_nodes` 6%, `emit_shaped_parts` 6%, `layout_para_full` 5%, `line_extent` 4%.
- Corpus byte-identical after the revert; harness 38/38 A; tests 223 pass with the 4 known NewCM failures; clippy clean.

## Open
- On one core, the remaining time is spread over line breaking, word measuring, the stream writer and DEFLATE with no single hot spot; further gains need either more threads or real algorithmic change (for example caching whole-paragraph breaks across recompiles in an editor).
