# RushTeX progress (living checklist)
Author: abhi+claude | Date: 2026-10-22
Files: `memorycastle/journal/native-architecture.md`, `memorycastle/journal/native-port-plan.md`, `src/engine/engine_gc.rs`, `src/engine/engine_node.rs`, `src/engine/engine_token.rs`

## Context
The one place to see what is done and what is next. Tick an item when its gate passes (tests, clippy, `parity.py zero --engine --pixels --all` at 85/85, `scripts/leak_audit.py` at 85/85 clean, `bench_corpus.py`) and its dated note is saved; the next unticked item is the work. Detail lives elsewhere:
- `native-architecture.md` holds the rules;
- `native-port-plan.md` holds the method and the kernel and package tables;
- the dated notes hold decisions and measurements.

## Decision
### Standing rules (every item is checked against them)
- **No garbage collection.** Memory is owned and freed at known points (pools, regions, free lists; §6a of the spec). No tracing, compaction or moving of live data.
- **No hot loops.** Every surviving per-token, per-node or per-line loop is measured with `scripts/profile_doc.py` and justified in its module's comments.
- **Structures by access pattern.** Every table is SoA where a phase reads one field across many records, and AoS where a record is read whole; the choice is written next to the type.
- **Borrow, don't copy.** Pass `&[T]`, ranges and IDs; no `.to_vec()`, `.clone()` or `String` building on a per-token or per-node path.
- **Load only what the document demands** (user, 2026-10-22). Every static table (macros, packages, fonts, hyphenation languages) is read through IDs and loaded or touched only when the document needs it. A document's cost scales with what it uses, never with what RushTeX supports. Structures: `2026-10-22-document-needs-measure.md`.
- **Speed target: at least 20× pdflatex on every document, aiming for 50×** (user, 2026-10-22), with zero shift. Only native translation reaches it: interpretation of LaTeX's macros is the cost (`get_token` about 50%). `bench_corpus.py` reports the ratio per document, and no commit lowers it.
- **No technical debt.** Each phase leaves one implementation per job, one table format, one pinned TeX Live, and a gate that catches drift (see "Debt guards").

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
- [x] Z1: model M on the node core; `collect_nodes` deleted; `scripts/leak_audit.py` 85/85 clean; listings RSS 55 → 45 MB
- [x] `RT_PROFILE` first cut: documents expand 7–17% of the format's 20,445 macros

### Debt guards (each is a gate or a rule, not a hope)
- **One implementation per job.** The old approximate pipeline (`src/layout`, `src/math`, `src/tables`, `--native`) is frozen: no new work, retired at P7. Typesetting is the engine core only.
- **One product/oracle boundary,** enforced by the compiler: the `oracle` cargo feature holds `latex.ltx` interpretation, the format build and `engine_host`. CI builds without it.
- **One pinned TeX Live** for the references, the generated tables and CI. Generated files carry a checksum, and CI regenerates and compares.
- **One table format library** (D1) for every generated table, versioned. No ad-hoc formats per module.
- **Every native command is proven twice** against its macro version (`RT_NATIVE` with identical boxes, state and written files), then corpus zero shift.
- **Kernel internals stay overridable** (port plan §5): a native routine checks one meaning word and yields to a redefinition.
- **Unported package in a release:** a clear error naming the package. The oracle fallback exists only in dev builds.
- **Budgets per commit:** time ratio, peak RSS, binary size, leak audit, zero shift.

### Phase 1. Foundations (the core and the boundary; everything later stands on them)
- [ ] A0. `oracle` cargo feature: the product builds and passes its unit tests without `latex.ltx` interpretation, `engine_job`, `engine_host` or the format build.
- [ ] A1. Pin one TeX Live (2023, as in CI): references, local runs and generation from it; record the version in the binary.
- [ ] Z2. Build buffers reserved once: the per-line adjustment list, per-word hyphenation scratch, per-paragraph `par`/`breaks`/`carry`/active lists. Allocator share measured before and after.
- [ ] Z3. Token model M: macro texts and token lists in size-class blocks with one owner, or a count where TeX shares (§200 `token_ref_count`: `\let`, `\toks` copies, a body being read §390). Redefinition frees (§275 `eq_destroy`); `collect_tokens` deleted. This is the product's expander for user macros, so it stays.
- [ ] Z4. Capacity reserved from the source size; RSS flat per page on listings.
- [ ] B1–B2. Copies and clones removed from the expander and the core (`toks.get(..).to_vec()`, `nodes.get(..).to_vec()`, the math `Vec<Node>` returns).
- [ ] B3. Layout table beside every core record (size, fields, SoA or AoS and why).
- [ ] B4. Hot-loop census of the core and the expander over 85 documents (`profile_doc.py`): every function above 2% with its cost per call, each one removed or justified.

### Phase 2. Tables and demand loading (shared by the oracle now and native later)
- [ ] D1. Table library, std only, with tests and size reports: a static perfect hash with an 8-bit fingerprint per slot (name to ID and membership in one probe), SoA tables by ID, CSR lists (offsets plus IDs), bitsets, Roaring-style compressed bitmaps.
- [ ] D2. `rushtex-gen` skeleton plus the package registry. For each `.sty`/`.cls`/`.clo`/`.fd`, recorded from an oracle run: its class (a–e of the port plan), options, `\RequirePackage` edges (CSR), the control sequences it defines, and its parameters. Generated, checksummed.
- [ ] D3. Document pre-scan: one pass over the source gives its command names, through the D1 hash, to a module `needs` bitset with its closure. Per-block ID arrays of what each block needs and reads (P8's dependency index from day one).
- [ ] D4. Demand loading in the core: hyphenation patterns only for the languages used, font metrics on first use, embedded font programs decompressed per used font.
- [ ] D5. Measure: startup time and RSS per document against everything loaded.

### Phase 3. Native translation (per `native-port-plan.md`, each command oracle-proven)
- [ ] P0 rest: `RT_PROFILE` time per macro, `RT_STATE`, `RT_NATIVE=cmd,…`, engine-against-engine `engine_boxdiff.py`, arXiv package-frequency sample.
- [ ] P1. Fonts and NFSS as generated tables (`.fd`, TFM, encodings, size functions); `\selectfont` and the size commands native.
- [ ] P2. Kernel core (counters, lengths, spacing, boxes, paragraphs, lists, sectioning, references, `\newcommand`, `\NewDocumentCommand`, keys, hooks, files): small2e, sample2e and lppl at zero format expansions.
- [ ] P3. Output routine and floats as an explicit state machine.
- [ ] P4. Tables, math, pictures.
- [ ] P5. Classes (article, report, book, their `.clo`; ltxdoc/doc; amsart, scrartcl, IEEEtran, exam): parameters as generated `const` tables, behaviour native.
- [ ] P6. Packages by real usage. Each `.sty` becomes data (tables generated by D2) plus behaviour (one Rust module with option bitflags and fixed hook slots). Class (a) libraries are not ported; native code subsumes them.

### Phase 4. Product
- [ ] P7. Zero dependency: no `latex.ltx`, tables and fonts in the binary, native is the default. Speed gate at least 20× pdflatex on every corpus document, zero shift 85/85.
- [ ] P8. Real-time incremental: block checkpoints, (content, state) cache, early cutoff, repagination until the breaks match, only changed pages shipped. Under 16 ms per typical edit.
