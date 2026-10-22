# Document-needs gate: how much of the format a document uses
Author: abhi+claude | Date: 2026-10-22
Files: `src/engine/engine_run.rs`, `src/engine/engine_macro.rs`, `src/engine/engine_eqtb.rs`, `memorycastle/journal/progress.md`

## Context
The user wants a micro engine that reads the document first, keeps a hash map of the functions and tokens it needs, and has a secondary loader bring only those into RAM (P0b in `progress.md`). This note measures what such a loader could save before anything is built.

## Decision
`RT_PROFILE=1` on `format_smoke` (test builds only) sets one bit per control sequence whose macro `macro_call` expands. It compares that with the format's macros (`Eqtb::macro_lengths`, taken after undump).

Design that follows:
- **Engine (oracle).** Undump the format lazily. A sorted name table and one offset per macro body stay on disk or in the format bytes, and a meaning is faulted in on its first lookup. That is exact, because the lookup itself is the need.
- **The document pre-scan.** A hash map of the source's control-sequence names, used only as a prefetch hint. A static scan cannot see names built by `\csname`, catcode changes, or what package code calls, so it cannot decide alone.
- **Native path.** Modules are Rust code with needs known at generation time. The scan gives the roots, and a closure precomputed at generation time gives the module bitset.
- **Order.** Lazy undump wants macro bodies in their own token blocks, so it comes after Z3.

## Decision: loader structures (proposed to the user, 2026-10-22)
- **Name table:** one byte blob of the format's names plus `u32` offsets (SoA), indexed by a static minimal perfect hash (about 3 bits per key) and checked against the name bytes. It has no per-entry heap; it replaces a `HashMap` for the static names.
- **Meaning table:** SoA by name ID: kind `u8`, payload `u32` (primitive code, `\chardef` value or body offset), and body length. Macro bodies sit in one token blob, ordered by measured use frequency, so the hot ones share cache lines and pages.
- **Filters:**
  - **Binary fuse filter (XOR family, 8-bit fingerprints, about 9 bits per key, about 33 KB for 29.7k names):** "is this name the format's at all?" The document's own macros miss without touching the name blob. It is static, so a Bloom filter (about 1.44 times larger for the same error rate, insert support unused) is not needed.
  - **`loaded` bitset (one bit per ID, about 2.5 KB):** checked on every lookup (one load and a mask), faulting the body in when clear.
  - **Closure in CSR form** (an offsets array plus an ID array: an array of arrays without per-list heap): the names each macro body mentions literally. The source pre-scan seeds it, and a breadth-first search over it gives the `needs` bitset. One sorted pass loads those bodies. Fault on first lookup catches what static text cannot show (`\csname`, catcode changes).
  - **Per block:** a sorted ID array of the block's needs. An edit diffs the old and new arrays and loads only the difference.
- **Where bodies live:**
  - Engine path: the format file's sections are read by position (`FileExt::read_at`, std only; the whole file on wasm).
  - Native path (P7): the same tables are generated into the binary as static slices. A body that is never redefined is read in place (borrowed, zero copy, paged in by the OS on first touch), and only a redefinition gets an arena block (Z3).
- `HashMap` stays only for names a document defines at run time (`CsTable`).

## Measured
Format (LaTeX 2023 plus pdfTeX extras): 20,445 macros, 386,606 macro tokens, about 29,700 names.

| Document | Format macros used | Their tokens | Macros expanded, format and packages |
|---|---|---|---|
| small2e | 1,375 (6.7%) | 8.3% | 1,453 |
| sample2e | 1,524 (7.5%) | 8.9% | 1,634 |
| testmath | 2,035 (10.0%) | 11.5% | 2,633 |
| amsldoc | 2,645 (12.9%) | 15.6% | 4,202 |
| listings | 2,690 (13.2%) | 16.0% | 5,772 |
| array | 3,082 (15.1%) | 19.4% | 5,263 |
| fancyhdr | 3,381 (16.5%) | 21.4% | 7,928 |

So 79–92% of the format's macros are never expanded. In bytes that is about 1.2–1.4 MB of tokens, plus the names and meanings of those macros (about 2 MB more). Packages read at run time are the larger share on heavy documents: fancyhdr ends with 56,744 names and 1.53M stored tokens. Only the native path removes those.

## Open
Measure peak RSS and undump time with the lazy format once it exists. Expected: about 3 MB lower on every document, with small2e moving from 22 MB toward 19 MB.
