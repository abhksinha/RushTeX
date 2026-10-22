# SUPER IMPORTANT: RushTeX native architecture (the long-term design; living spec)
Author: abhi+claude | Date: 2026-10-22 (living spec: update in place, record each change in a dated note)
Files: `src/lib.rs`, `src/main.rs`, `src/engine/engine_mod.rs`, `src/engine/engine_node.rs`, `src/engine/engine_pack.rs`, `src/engine/engine_linebreak.rs`, `src/engine/engine_page.rs`, `src/engine/engine_mlist.rs`, `src/engine/engine_font.rs`, `src/engine/engine_ship.rs`, `src/frontend/frontend_parser.rs`, `src/expand/expand_mod.rs`, `src/layout/layout_engine.rs`, `src/layout/layout_hyphen.rs`, `Resource_Conscious_Software_Engineering.md`, `memorycastle/journal/tex-weaknesses-roadmap.md`, `scripts/bench_corpus.py`, `scripts/parity/parity.py`, `scripts/parity/engine_boxdiff.py`

## Context
**Status: SUPER IMPORTANT.** This is the main architecture RushTeX follows from now on. It is built carefully and completely, never hastily. "Looks right in Rust" is not the bar: the code must use the machine's native capability, with the least work and the least data movement.

Where it came from (2026-10-22):
- The TeX engine (`src/engine/`) reached zero shift against pdfLaTeX on 85 documents (2,137 pages) by interpreting `latex.ltx` and the package macros.
- The benchmark (`scripts/bench_corpus.py`) measured it at 2.9× faster than pdflatex, using 38% of its CPU, but with a median of 48.7 MB RAM against 31.1 MB, and 848 MB at worst.
- Macro interpretation is the cost. It also needs TeX Live's files at run time.
- The user chose a fully native design: LaTeX's behaviour written in Rust, zero runtime dependencies, bare-metal speed, real-time incremental compile. The user's own simplified preamble and document language sits on top of it.

## Decision
RushTeX implements LaTeX natively (approach A); nothing is interpreted:
- The kernel and the supported packages are Rust code that builds node lists directly into the exact typesetting core.
- Static knowledge (fonts, metrics, encodings, class and package parameters, hyphenation) is generated at build time into `static` tables.
- The TeX engine stays as a development oracle that proves exactness, and is compiled out of releases.
- Incremental real-time compile is designed in from the start.

The full specification follows. Every change to rushtex is checked against it.

### 1. Invariants (never traded away)
- **Exact output.** In compatibility mode the output is identical to pdfLaTeX: zero shift, the same pages, every glyph and path at the same place, the same links and outlines, 0 differing pixels.
- **Zero runtime dependency.** One binary. No TeX Live, no `kpsewhich`, no first-run format build, no files read except the document and its own inputs (images, `\input` files).
- **Least work, least movement, least memory**, measured on every change.
- **Values from TeX, methods ours.** tex.web, pdftex.web, `latex.ltx` and the `.sty` sources define what the numbers are. Our representation and algorithms decide how they are computed.

### 2. Layers
1. **Front ends.** Both produce the same command stream of IDs, never strings:
   - the LaTeX parser, zero-copy over the source bytes;
   - the user's simplified language.
2. **Command dispatch.** `CmdId: u16` goes to a static `[fn; N]` table. User macros (`\newcommand`, `\def`) are expanded by a minimal expander into the same stream.
3. **Native kernel**, ported from `latex.ltx` routine by routine:
   - NFSS, sectioning, lists, footnotes, marks;
   - floats and the output routine as an explicit state machine;
   - references and contents.
4. **Native packages.** One Rust module each, with option bitflags and hooks at fixed kernel hook points.
5. **Exact typesetting core**, today's `engine_box`, `engine_pack`, `engine_chars`, `engine_linebreak`, `engine_hyph`, `engine_align`, `engine_page`, `engine_noad`/`engine_mlist`, `engine_font`/`engine_vf`, `engine_microtype`, `engine_ship`, `engine_pdfdoc`, `engine_image`: about 11k lines, already exact.
6. **Output:** PDF pages, or a direct preview for real-time mode.
7. **Oracle (dev only, cargo feature `oracle`):** interpretation of `latex.ltx`, expl3 and the `.sty`/`.cls` files, plus `engine_job`/`engine_host` and the format build.
   - **Boundary (corrected 2026-10-22, `native-port-plan.md` §1):** TeX's expansion semantics (`engine_macro`, `engine_cond`, `engine_value`, `engine_scan`; tex.web §366–§401, §487–§510) stay in the product, because documents' own macros (`\def`, `\expandafter`, `\csname`, catcodes) need them for backward compatibility.
   - Every LaTeX command they reach is a native primitive. What leaves the product is LaTeX's macro implementation and TeX Live, not the expansion machinery.

### 3. Build time, not run time
- A generator (`rushtex-gen`, dev only) reads the pinned TeX Live files once and emits `src/generated/*.rs` and compressed blobs. Everything lands in the binary's read-only data:
  - zero parse and zero allocation at start-up;
  - shared between processes;
  - only the pages a document touches are ever loaded.
- Parameters are extracted by running the oracle, not copied by hand. For example, `article` with `11pt,twocolumn` dumps `\textwidth`, `\baselineskip`, the `\@startsection` arguments and the list indents, so the values are pdfLaTeX's by construction.
- Behaviour is ported code; numbers are tables. A routine's logic never contains a magic constant that a generated table could hold.

### 4. Data structures by kind
- **Closed sets** (commands, packages, options, colours, encodings):
  - build-time perfect hash, resolved without collisions at generation;
  - a name gives a `u16` ID; the ID indexes a static table.
- **Open sets** (user macros, labels, citations, file names):
  - one string arena with interned `u32` IDs and one runtime hash map from name to ID, with our own std-only hasher;
  - after interning, only IDs move.
- **Font metrics** (`.tfm`):
  - TFM's own compact layout, kept: a packed `u32` per character indexing small width, height, depth and italic tables of `i32` sp;
  - the widths of each font are contiguous (SoA), because line breaking reads only widths;
  - `FontId: u16`.
- **Kerns and ligatures:**
  - per font, a 256-bit gate mask per left character (does any pair start here?);
  - then a flat array of pairs sorted by (left, right), searched by binary search.
- **Encodings:** `[u16; 256]` per encoding, direct index.
- **NFSS:**
  - the key is (encoding `u8`, family `u16`, series `u8`, shape `u8`) packed into one `u32`;
  - a sorted array maps keys to size ranges to `FontId`, with size functions resolved at build time;
  - a small direct-mapped cache of recent selections sits in front, so `\selectfont` is usually one compare.
- **Class and size parameters:** a `const ClassParams` struct per class × size, all `i32` sp. Glue is `(width, stretch, shrink, order)`. Section and list specs are `const` arrays indexed by level.
- **Options:** a `u64` bitflag per class and package.
- **Package hooks:** fixed arrays per hook point (`[HookList; HOOKS]`), never maps. A package registers at most a few function pointers.
- **Hyphenation:** a packed trie (`layout_hyphen`) plus a sorted exceptions array. A gate filter on word length and letters skips words that cannot hyphenate.
- **Embedded font programs:** one compressed blob plus a static (name, offset, length) table, decompressed only for fonts a page uses.
- **References:**
  - SoA columns: label to (page `u32`, number ID, block ID);
  - a reverse index from label to the blocks that cite it.

### 5. Choosing a structure (decision procedure)
1. **Can the set be known at build time?** Then a static table or a perfect hash, never a runtime map.
2. **Is the key a small integer, or can it become one?** Then a direct index (an array), not a hash.
3. **Is the data ordered or ranged** (sizes, kern pairs, dependencies)? A sorted flat array, searched by binary search or merged; no trees, no pointers.
4. **Is the key dynamic** (user names)? Intern once into a `u32`, then use rule 2.
5. **Do most lookups miss?** Put a gate in front:
   - a bitmask for small key spaces (kern masks, "has hyphenation exceptions", "is this character active");
   - a Bloom filter for large ones (for example "could this name be a user macro" before the user-macro map, or a block's commands against a changed definition).
   - A gate only says "no" cheaply. A "maybe" always goes to the exact structure.
   - A gate is added only when measured misses dominate.
6. **Does a lookup repeat with the same key?** A direct-mapped cache of 1–64 entries, with no eviction logic.
7. **SoA or AoS:** decided by the hot loop's access pattern.
   - **SoA** when a loop reads one or two fields of many records: character widths, glue widths in line breaking, label pages.
   - **AoS** when every access reads the whole record: a node being packed, one glue spec, a `ClassParams`.
   - Hot fields are split from cold ones. Records are sized to share cache lines (nodes of 12 bytes today; `Block`/`Inline` of at most 32 bytes).
8. **Representation:** the smallest that holds the domain. `u16` IDs; `i32` sp for every length (no floats except glue ratios, as TeX); `u8` enums; packed keys in one machine word.

### 6. Moving as little data as possible
`Resource_Conscious_Software_Engineering.md` §9 says to reduce movement. These are RushTeX's concrete rules:
- **Borrow the source; never copy it.** The parser holds `&str` slices. A token, command or argument is a range or an ID, not a `String`.
- **IDs cross module boundaries, not data.** A font is a `u16`, a label a `u32`, a box an index. A function that needs a font's widths reads the static table through the ID.
- **Build output in place.** Node lists are appended into one arena, and boxes refer to ranges of it. No intermediate vectors per paragraph, line or page, and no clone-then-modify.
- **One representation per phase, converted once** (§17). Parse, then commands, then nodes, then PDF operators. Each step consumes the previous one in a single streaming pass; nothing is round-tripped through text.
- **Stream pages out.** A shipped page is written to the PDF writer and its nodes are freed or recycled. The whole document is never held as nodes.
- **No `.aux` files.** References live in memory; a re-layout touches only the affected pages.
- **Reuse buffers.** Scratch vectors for line breaking (active lists, break candidates), packing and PDF text are owned by the long-lived context, cleared and never reallocated in steady state.
- **Copy-on-write for snapshots.** An incremental checkpoint stores what changed since the previous one (counters, the font, the float queue), not a full state copy.
- **Free what dies.** A replaced definition, a finished paragraph's scratch data or a shipped page's nodes are released at once. Memory is bounded by the working set, never by document history (the token store's append-only growth is the counter-example to remove).
- **Measure it.** Each change reports bytes moved per useful result where relevant: peak RSS, allocations, and bytes copied in the hot path.

### 6a. Memory: zero garbage collection, predictable growth
The native path never traces, never compacts and never moves live data. Every byte has an owner whose lifetime is known when it is allocated:
- **Typed pools at fixed addresses.** Each kind of record (node, box, glue, rule, ligature, discretionary, insertion, mark) lives in its own pool, SoA where a phase reads one field across many records (widths in packing, penalties in line breaking) and AoS where a record is read whole. An index into a pool never changes while the record lives.
- **One owner per list or record, freed where TeX frees it** (model M, `2026-10-22-zero-gc-core-model.md`). Moves transfer ownership; `\copy` deep-copies (§204); a list is freed at `\shipout`, a `\setbox` overwrite, a group restore and when discarded (§202). Freed span blocks return to size-class free lists (exact 1–16, then powers of two), fixed-size records to one free list per table.
- **Build buffers are the regions.** The list being built (`cur`) and the line-break scratch are reused buffers reset per paragraph or box; they own no arena memory.
- **Replaced definitions return to a free list by size class.** The document's own macros, the only interpreted text, are stored in size-class blocks. Redefinition frees the old block at once (TeX §275 `eq_destroy`), so a loop that redefines a macro reuses one block.
- **Capacity is reserved up front.** Each region starts from a capacity estimated from the source size (pages, paragraphs, definitions) and grows by doubling only past it. Peak RAM is about the format tables plus the document region plus one page region; it does not grow with the number of pages.
- **The oracle is the exception.** The TeX engine stays exact to tex.web and keeps a tracing collector (`engine_gc`) as a dev tool. Its numbers are not the product's.

### 6b. Load only what the document demands (user, 2026-10-22)
- A document's cost scales with what it uses, never with what RushTeX supports.
- Every static table (commands, packages, classes, fonts, encodings, hyphenation languages) is reached through IDs and touched only when needed. Generated tables sit in the binary's read-only data, so the OS pages in only what a document touches. Tables are ordered by measured use, so a typical document's needs share pages.
- The source pre-scan gives a module `needs` bitset through the package registry and its `\RequirePackage` closure (CSR). A lookup of an unloaded ID loads it, so the result stays exact when names are built at run time (`\csname`).
- **Name lookup:** a static perfect hash with an 8-bit fingerprint per slot gives membership and ID in one probe. It replaces a separate XOR filter plus hash: one cache miss fewer, and smaller.
- **Sets:** `loaded` and `needs` are bitsets by ID; per-block needs and the reverse index (command to blocks) are Roaring-style compressed bitmaps, exact, and they feed §9.
- Measured: documents expand 7–17% of the format's macros (`2026-10-22-document-needs-measure.md`).

### 6c. Speed target (user, 2026-10-22)
- At least 20× pdflatex on every document, aiming for 50×, with zero shift. The oracle is 1.4–3× today; the gap is macro interpretation, which only native translation removes.
- `bench_corpus.py` reports the ratio per document. A commit that lowers any document's ratio is a regression.

### 7. Hot loops: minimal or none
- **Eliminate before optimising** (§1). Every per-character, per-node or per-line loop must justify existing.
- Prefer:
  - a table lookup over a loop (§22): kerns, encodings, NFSS;
  - computing once at build time over computing per document;
  - computing once per document over computing per page.
- **No search loops in hot paths.** Use direct indexes, perfect hashes, binary search over sorted arrays, and gates that answer "no" without touching memory.
- **Never rescan.** Each byte of the source is read once by the parser, and each node is visited once per phase. A loop that re-walks a list it already walked is a design bug.
- **No allocation, no hashing of strings, no dynamic dispatch, no formatting** inside per-character or per-node loops.
- **Branch-light inner loops.** Common cases are specialised (§16): an ASCII text run, a font without ligatures, a paragraph without math. Cold paths (errors, rare node kinds) live out of line (§26).
- **Batch at boundaries.** One PDF text operator per run of glyphs; one hyphenation call per word; one call to the page builder per paragraph.
- **Profile, then prove.** Every hot loop that survives is listed in its module's comments with its measured cost.

### 7a. Hardware model and budgets (user, 2026-10-22: design for the conservative floor, not the dev machine)
Design budgets come from the weakest common machine, not from the Apple M5 the work is done on.

| | Floor (design target: older Intel/AMD laptop, e.g. Skylake or Zen 2 class) | Apple M5 (dev) |
|---|---|---|
| L1 data per core | 32 KB | 128 KB |
| L2 per core | 256 KB–512 KB | 16 MB shared |
| L3 shared | 4–8 MB | system cache |
| Cache line | 64 B | 128 B |
| Memory page | 4 KB | 16 KB |
| DRAM latency | about 80–120 ns | about 100 ns |
| Instruction set | x86-64 baseline (SSE2 only guaranteed), ARMv8 NEON, wasm32 | |

**Budgets:**
- the working set of a per-character or per-node loop fits in 32 KB;
- a paragraph's or page's working state fits in 256 KB;
- the static tables a typical document touches fit in 4 MB;
- every table's layout is decided for 64 B lines;
- data written by different threads is kept 128 B apart, which covers Intel's adjacent-line prefetch and Apple's lines;
- table ordering by use assumes 4 KB pages.

**Portability rules:**
- No `-C target-cpu=native` in releases.
- `std::arch` intrinsics need `unsafe`, which the crate forbids, so wide work uses portable SWAR (integer bit tricks on `u64`/`u128`) and loops simple enough for the compiler to vectorise on baseline SSE2/NEON. `std::simd` is nightly and not used.
- Generated blobs are little-endian by declaration (`from_le_bytes`), never by assumption.
- On x86 with hyper-threading, two threads share a core's L1 and L2, so worker threads number at most the physical cores.

**Order of thought for every feature:**
1. remove the work (build time, demand, incremental);
2. shrink the data (smallest types, hot/cold split);
3. access it in order (contiguous spans, allocation order follows use order, SoA where one field is scanned);
4. move nothing that can be borrowed;
5. only then tune the inner loop:
   - the common case as the straight path, `#[cold]` errors;
   - no allocation, string hashing, formatting or `dyn` per item;
   - slice iteration so bounds checks are hoisted;
   - TeX's integer arithmetic kept exactly.
- Output stays byte-identical whatever the machine or thread count. Parallel work (page stream compression, image decoding, font subsetting, the pre-scan) is written back in page order.

**Measured, not guessed:** hardware counters per document (instructions, cycles, IPC, L1 and last-level misses, branch mispredicts): `perf stat` on x86 Linux (CI's runners and a floor-class machine), `xctrace` CPU Counters on macOS. Budgets per document are instructions per output glyph and cache misses per page, next to the speed ratio. Every surviving hot loop states its per-item work, its working set against the floor budgets, and its measured cost in its module comment.

### 8. Exactness method (the oracle)
- Every native routine is developed against the oracle's box output:
  - `RT_BOXES` against the native node lists, diffed node by node by `scripts/parity/engine_boxdiff.py`;
  - then `parity.py zero --pixels` against pdfLaTeX.
- Porting is deterministic: the first differing node names the routine and parameter to fix. No trial and error.
- **Gates for every change:**
  - `cargo test`;
  - `cargo clippy -- -D warnings`;
  - the zero-shift corpus (85 documents today, through the oracle until native covers them);
  - `scripts/bench_corpus.py` (time, CPU, RSS);
  - binary size.
  - No regression is allowed on any of them.

### 9. Real-time incremental compile (designed in, not added later)
- **Blocks.** The source is split into blocks: paragraph, display equation, table, float, list item, heading.
- **State.** Each block records its input state: a small typed struct of counters, the current font, list depth, the float queue and the references read. It also records its output state.
- **Results are cached** by (block content hash, input state hash).
- **Dependencies** are sorted small `u16` arrays per block (labels, counters, user macros read), with reverse indexes for invalidation. They are exact; a Bloom filter may only gate them.
- **Early cutoff.** After an edit, the block is re-laid-out. If its output state is unchanged, nothing after it is.
- **Repagination** starts at the first changed page and stops when the page breaks again match the previous ones.
- **Pages whose cited labels changed** are re-laid-out through the reverse index.
- **Only changed pages are shipped:** a preview update, or an appended PDF update.
- **Target:** under 16 ms (one frame) from keystroke to updated page for a typical edit, independent of document length.

### 10. The user's simplified language
- It is a second front end onto the same IDs and tables. For example, a font setting resolves at parse time to an NFSS key and a `FontId`, and a layout setting to class option bits and a `ClassParams`.
- Classic LaTeX preambles and the simplified syntax produce identical output from one core.
- Superset features are opt-in and never change compatibility-mode output.

### 11. Order of work (each step closed by the gates and a dated note)
- **N0:** fix the oracle's two regressions so it stays a sound reference: rerun only while the auxiliary files change, and free replaced token lists.
- **N1:** the native pipeline emits the core's nodes and runs its exact algorithms. The approximate native layout is retired step by step.
- **N2:** `rushtex-gen` with fonts, metrics, encodings, NFSS, class parameters and hyphenation as static tables.
- **N3:** native kernel: small2e, sample2e, lppl exact natively, then the kernel-only corpus documents.
- **N4:** packages by usage frequency, each oracle-diffed.
- **N5:** in-memory references with one layout pass where possible, then the block-level incremental pipeline.
- **N6:** the oracle (`latex.ltx` interpretation, format build, `engine_host`) out of the release. One native binary that keeps TeX's expansion for the document's own macros.
- **Detailed phases P0–P8, inventory and trackers:** `native-port-plan.md`. The port grows inside the engine as native primitives, verified per command against the macro version. The old native pipeline is frozen and retired at P7, replacing N1's migration of it.

### 12. Review checklist (every change)
1. Does any work run at run time that could run at build time?
2. Does any string, `Vec` or node list get copied where an ID or a range would do?
3. Is there a loop that searches, rescans or allocates in a hot path? Which cache lines does it touch per item, is the access sequential, and does its working set fit the floor budgets (§7a: 32 KB, 256 KB, 4 MB, 64 B lines)?
4. Is each structure chosen by §5, and each SoA/AoS choice justified by the access pattern?
5. Does memory stay bounded by the working set? Is every allocation owned by a region or pool with a known lifetime, with no tracing, compaction or moving of live data?
6. Is the output still zero shift, and are the benchmark and RSS numbers equal or better?
7. Is the § or macro of the source cited in a code comment (`// §1057`, `` `\@startsection` ``)?

## Rejected
- **Approach B (engine plus native fast paths for hot macros) as the product.** It keeps every package working, but it pays for token interpretation on every non-shortcut macro. Its checkpoints are megabytes (the whole equivalents table and token store), and any macro can change any state, so dependencies cannot be tracked exactly. Real-time incremental compile would degrade with document size. The engine is kept only as the oracle.
- **Runtime ingestion of `.cls`, `.sty`, `.fd` and `.tfm` files**, and a first-run format build. Both are build-time work done at run time, and both create a dependency on TeX Live.
- **Generic runtime `HashMap`s for static data.** Perfect hashes and direct indexes are smaller and faster, and never allocate.
- **Bloom filters as the source of truth for dependencies.** They do not pinpoint (see `tex-weaknesses-roadmap.md`). They are allowed only as gates in front of exact structures.
- **Haste.** Porting a routine "to look right" without the oracle's node-level proof, or optimising before the design removes the work.

## Measured
Baseline at adoption (2026-10-22, `scripts/bench_corpus.py`, 85 documents, 2,137 pages):
- **Oracle engine:** 129.4 s wall time, 120.5 s CPU; median peak RSS 48.7 MB, maximum 847.8 MB; 222 runs.
- **pdflatex:** 380.2 s wall time, 314.9 s CPU; median peak RSS 31.1 MB, maximum 40.9 MB; 206 runs.
- **Core and interpreter size:** the typesetting core is about 11.3k lines; the interpreter about 9.3k lines.
- **What must be ported:** `latex.ltx` is 22,469 lines; the corpus loads about 100–150 distinct `.sty`/`.cls` files.

## Open
- Which expl3 parts the native kernel must mirror: `latex.ltx` now relies on `l3kernel` for hooks, keyval and text case.
- Package coverage list and order: measure usage frequency over a large sample of real documents (arXiv sources), not only the corpus.
- **Unported package (decided 2026-10-22, the user's native-only rule):** a release gives a clear error naming the package; the oracle fallback exists only in dev builds.
- Licensing record for the generated tables (LPPL permits redistribution; keep the attribution and pinned versions in the binary's notices).
