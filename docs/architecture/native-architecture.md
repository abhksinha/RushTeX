# SUPER IMPORTANT: RushTeX native architecture (the long-term design; living spec)
Author: abhi+claude | Date: 2026-10-22 (living spec: update in place, record each change in a dated note)
Files: `src/data/data_mod.rs`, `src/data/data_blob.rs`, `src/data/data_tfm.rs`, `Cargo.toml`, `.github/workflows/ci.yml`, `src/bin/rushtex_gen.rs`, `src/engine/engine_microtype.rs`, `src/backend/backend_ship.rs`, `src/lib.rs`, `src/main.rs`, `src/engine/engine_mod.rs`, `src/engine/engine_node.rs`, `src/engine/engine_pack.rs`, `src/engine/engine_linebreak.rs`, `src/engine/engine_page.rs`, `src/engine/engine_mlist.rs`, `src/engine/engine_font.rs`, `src/engine/engine_ship.rs`, `src/frontend/frontend_parser.rs`, `src/expand/expand_mod.rs`, `src/layout/layout_engine.rs`, `src/layout/layout_hyphen.rs`, `Resource_Conscious_Software_Engineering.md`, `memorycastle/journal/tex-weaknesses-roadmap.md`, `scripts/bench_corpus.py`, `scripts/parity/parity.py`, `scripts/parity/engine_boxdiff.py`
Supersedes: 2026-10-22-native-latex-architecture.md

## Context
**Status: SUPER IMPORTANT.** This is the main architecture RushTeX follows from now on. It is built carefully and completely, never hastily. "Looks right in Rust" is not the bar: the code must use the machine's native capability, with the least work and the least data movement.

Where it came from (2026-10-22):
- The TeX engine (`src/engine/`) reached zero shift against pdfLaTeX on 85 documents (2,137 pages) by interpreting `latex.ltx` and the package macros.
- The benchmark (`scripts/bench_corpus.py`) measured it at 2.9× faster than pdflatex, using 38% of its CPU, but with a median of 48.7 MB RAM against 31.1 MB, and 848 MB at worst.
- Macro interpretation is the cost. It also needs TeX Live's files at run time.
- The user chose a fully native design: LaTeX's behaviour written in Rust, zero runtime dependencies, bare-metal speed, real-time incremental compile. The user's own simplified preamble and document language sits on top of it.

## Decision
RushTeX implements LaTeX natively first (approach A), with the engine as a whole-document fallback:
- The kernel and the most-used packages (chosen by measured usage) are Rust code that builds node lists directly into the exact typesetting core.
- Static knowledge (fonts, metrics, encodings, class and package parameters, hyphenation) is generated at build time into `static` tables.
- The TeX engine proves exactness in development. It also ships in releases as the fallback for any document the native path cannot run (§2a).
- Incremental real-time compile is designed in from the start.

The full specification follows. Every change to rushtex is checked against it.

### 1. Invariants (never traded away)
- **Exact output.** In compatibility mode the output is identical to pdfLaTeX: zero shift, the same pages, every glyph and path at the same place, the same links and outlines, 0 differing pixels.
- **Zero runtime dependency on the native path.** One binary. No TeX Live, no `kpsewhich`, no first-run format build, no files read except the document and its own inputs (images, `\input` files). Only a document that falls back to the engine (§2a) needs TeX files: its own, plus the kernel and any unported package from TeX Live or the pinned snapshot.
- **Backward compatible.** Every document pdfLaTeX compiles also compiles in RushTeX, natively or through the fallback, with identical output.
- **Least work, least movement, least memory**, measured on every change.
- **Values from TeX, methods ours.** tex.web, pdftex.web, `latex.ltx` and the `.sty` sources define what the numbers are. Our representation and algorithms decide how they are computed.

- **All five pillars together** (user, 2026-10-22): a native kernel on the exact core; data as build-time tables, self-checked against their sources; demand loading from a fused pre-scan; per-block incremental compile designed in; zero-shift proof for every ported feature. The progress checklist maps each pillar to its items.

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
7. **Engine (cargo feature `oracle`, on in releases since 2026-10-22, §2a):** interpretation of `latex.ltx`, expl3 and the `.sty`/`.cls` files, plus `engine_job`/`engine_host` and the format build. It is the oracle in development and the whole-document fallback in releases.
   - **Boundary (corrected 2026-10-22; now `guarded-direct-lowering.md` §7):** TeX's expansion semantics (`engine_macro`, `engine_cond`, `engine_value`, `engine_scan`; tex.web §366–§401, §487–§510) stay in the product, because documents' own macros (`\def`, `\expandafter`, `\csname`, catcodes) need them for backward compatibility.
   - Every LaTeX command they reach is a native primitive. What leaves the product is LaTeX's macro implementation and TeX Live, not the expansion machinery.

### 2a. Native first, whole-document engine fallback (user, 2026-10-22)
This replaces the native-only rule decided earlier the same day, under which a release refused unported packages.
- **What is native.** The kernel plus the packages and classes that measured usage ranks highest. The corpus loads about 100–150 distinct `.sty`/`.cls` files; the real set comes from a larger sample (A3, arXiv sources), and it grows as the fallback measurements say.
- **What falls back.** The pre-scan (D3) decides per document. The whole document runs on the engine when it:
  - loads a local `.sty`/`.cls`;
  - loads an unported package;
  - redefines LaTeX internals in its preamble (`\makeatletter` with `\renewcommand\@seccntformat` and the like), which native code cannot honour.
- **Why the whole document.** An unknown `.sty` expects LaTeX's internals to be macros. Native code holds them as Rust, so mixing the two in one document is not reliably exact.
- **What the fallback costs.**
  - Engine speed: about 1.4–3× faster than pdflatex today, not 30–40×.
  - No per-block incremental compile.
  - TeX files at run time: the document's own, plus the kernel and unported packages from an installed TeX Live or the pinned snapshot.
- **What it gives.** Full backward compatibility, and a measured list of fallback triggers that sets the porting order.
- **Native-path invariants unchanged.** A native document still needs no TeX Live, interprets no LaTeX macros and compiles incrementally.

### 3. Build time, not run time
- A generator (`rushtex-gen`, dev only) reads the pinned TeX Live files once and emits `src/generated/*.rs` and compressed blobs. Everything lands in the binary's read-only data:
  - zero parse and zero allocation at start-up;
  - shared between processes;
  - only the pages a document touches are ever loaded.
- Parameters are extracted by running the oracle, not copied by hand. For example, `article` with `11pt,twocolumn` dumps `\textwidth`, `\baselineskip`, the `\@startsection` arguments and the list indents, so the values are pdfLaTeX's by construction.
- Behaviour is ported code; numbers are tables. A routine's logic never contains a magic constant that a generated table could hold.

### 3a. Storage: data, constants and code (user, 2026-10-22)
Three kinds of things are stored, each in its own form.

| Kind | Examples | Form |
|---|---|---|
| Bulk data | font metrics, glyph programs, hyphenation patterns, NFSS maps, encodings, the package registry | `.rtdb` blobs (the D1 format), embedded with `include_bytes!` |
| Data the code names or branches on | `ClassParams`, option bits, `CommandId`s, hook slots, `CounterId`s | generated Rust `const`s |
| Behaviour | `\section`, `\selectfont`, amsmath environments, the output routine | handwritten Rust, one module per package |

**The rule.** If code names a value or branches on it, it is a generated `const`, so the compiler folds it into the code. If code only indexes it, it goes in a blob, read in place, with no compile-time cost.

**Data.**
- **Embedded, never loaded from files.** The OS pages in only what a document touches, which is demand loading at no cost. Nothing is opened, parsed or copied.
- **Built with the code, so the two cannot disagree.** One fingerprint compare at start-up is the only check.
- **One blob per domain:** `fonts`, `nfss`, `hyph`, `glyphs`, `pkg`.
  - All share the one format: a perfect hash, SoA columns, CSR lists and bitsets.
  - Each domain is regenerated on its own; a combined fingerprint ties them to one pin.
- **Joined by IDs.** `FontId`, `FamilyId`, `EncodingId`, `PackageId`, `CommandId`, `LanguageId` and `CounterId` connect the tables. A name becomes an ID once, through a perfect hash; tables refer to each other only by ID.
- **Ordered by popularity,** so a typical document's data shares pages, as the font tiers do.
- **Committed to the repository.** Blobs are generated by `rushtex-gen` from the pinned TeX Live, so a build needs no TeX installation. CI regenerates them and requires identical bytes, so a table cannot drift from its source.

**Code.**
- **Layout:** `src/kernel/`, `src/pkg/<package>/`, `src/cls/<class>/`. Behaviour only; a module's numbers live in tables.
- **Dispatch:** a name gives a `CommandId` through the perfect hash, once, when the document is read. The `CommandId` indexes a static array of function pointers: one indirect call per use. No trait objects, no maps of closures, no interpretation.
- **Loading a package** sets its bit in the loaded bitset and runs its setup function. All code is compiled in; pages never used are never touched.
- **No plugins or dynamic libraries:** they cost load time and break WASM and the security model. Cargo features may trim the package set for a small WASM build later.
- **No hidden state.** Document state lives in central arrays by ID (counters, lengths, toggles as a bitset, the current font). A package owns behaviour, not state.
- **User macros** (`\newcommand` and `\def` in the document) go through the minimal expander with its own token arena. Package internals are never interpreted.

**How it serves the three goals, and what each still needs.**

| Goal | Given by this design | Still needed |
|---|---|---|
| Incremental compile | immutable tables (never checkpointed), state as arrays by ID, IDs fixed at build time, a checkpoint as a few small arrays or a length | per-block dependency bitsets (D3, P8); an interner kept across recompiles so user names keep their IDs |
| No garbage collection | static tables and code, append-only arenas, fixed-size state | token model M for user macros, where a redefinition frees the old text at once (Z3) |
| No interpretation | native packages, static dispatch, names resolved once | the minimal expander, for the document's own macros only |

**Unsafe code stays forbidden** (`#![forbid(unsafe_code)]`).
- Bounds checks are already removed by verifying lengths once and by iterators.
- `from_le_bytes` compiles to a plain load on little-endian CPUs.
- Undefined behaviour would weaken both the security of untrusted documents (A2) and the meaning of a zero-shift proof.
- The one future exception: explicit SIMD in a loop where a profile shows a large measured gain, isolated in one small module with a safe interface and tested under Miri.

### 4. Data structures by kind
- **Closed sets** (commands, packages, options, colours, encodings):
  - build-time perfect hash, resolved without collisions at generation;
  - a name gives a `u16` ID; the ID indexes a static table.
- **Open sets** (user macros, labels, citations, file names):
  - one string arena with interned `u32` IDs and one runtime hash map from name to ID, with our own std-only hasher;
  - after interning, only IDs move.
- **Font metrics** (`.tfm`): one 16-byte record per character per loaded (font, size), scaled at load; `FontId: u16`. See §4a; this replaces the earlier choice of TFM's own `char_info` indirection with separate width, height, depth and italic arrays.
- **Kerns and ligatures:** a per-font pair table compiled at build time, where (left, right) gives the first matching program step. One probe per pair; see §4a.
- **Encodings:** `[u16; 256]` per encoding, direct index.
- **NFSS:**
  - the key is (encoding `u8`, family `u16`, series `u8`, shape `u8`) packed into one `u32`;
  - a sorted array maps keys to size ranges to `FontId`, with size functions resolved at build time;
  - a small direct-mapped cache of recent selections sits in front, so `\selectfont` is usually one compare.
- **Class and size parameters:** a `const ClassParams` struct per class × size, all `i32` sp. Glue is `(width, stretch, shrink, order)`. Section and list specs are `const` arrays indexed by level.
- **Options:** a `u64` bitflag per class and package.
- **Package hooks:** fixed arrays per hook point (`[HookList; HOOKS]`), never maps. A package registers at most a few function pointers.
- **Hyphenation:** a packed trie (`layout_hyphen`) plus a sorted exceptions array. A gate filter on word length and letters skips words that cannot hyphenate.
- **Embedded font programs:** split at build time into per-glyph byte slices behind a static (font, glyph) table, decrypted once at build time; see §4a.
- **References:**
  - SoA columns: label to (page `u32`, number ID, block ID);
  - a reverse index from label to the blocks that cite it.

### 4a. Fonts: each piece of work done once, at the earliest time it can be (user, 2026-10-22)
Fonts are a large, permanent part of the work, so they are designed from first principles rather than by keeping TeX's 1980s layout. TeX's layout stores the fewest bytes but touches the most cache lines per operation. Every piece of work moves to the earliest point where its inputs are known: build time, then font load, then use. Exactness comes from TeX's arithmetic (the same scaling, the same first-match rule), not from TeX's data layout.

**Per character: one 16-byte record.**
- Each loaded (font, size) has one array of `[width, height, depth, italic]` `i32` records, indexed by character code minus the font's first character.
- An absent character is marked in the record (a sentinel width), with no separate existence lookup.
- A 128-character font is 2 KB, resident in L1. A lookup reads one record, not `char_info` plus four arrays (up to five cache lines).
- Scaling is TeX's (§571), done once per record at font load.
- The PDF widths array is the width column of these records, with no further lookup.

**Per pair: one probe.**
- `rushtex-gen` compiles each font's lig/kern program into a pair table: a small open-addressed table per font, key `left << 8 | right` (the boundary character as 256), value the first program step TeX's walk (§545, §1040) would reach.
- This is exact because the walk's result depends only on the two characters.
- Kern values are stored as indices and scaled into the loaded font's kern array at load.

**Expanded fonts (microtype):** an expanded copy is another record array, with its widths, italic corrections and kerns scaled by `1000 + expand` once, when the copy is made. Nothing is scaled on use.

**Font programs in the PDF.**
- At build time each Type 1 font is decrypted and split into per-glyph charstring slices plus a fixed header.
- At runtime each font keeps a 256-bit used-glyph set, set with one OR per character.
- A subset is the header followed by the used glyphs' bytes, concatenated with no parsing.

**Incremental compile.**
- Loaded fonts and their arrays are append-only, so a checkpoint is a length.
- A font's PDF bytes are cached per (font, used-glyph set) and reused unchanged when an edit adds no glyph.
- Font numbers stay stable across recompiles (P8).

**Where each piece of work happens.**
- Build time: parsing TFM files, compiling pair tables, splitting font programs.
- Font load: scaling records and kerns, making expanded copies.
- Use: one record read, one pair probe, one OR into the used-glyph set.

**Rejected for fonts.**
- **Pre-scaling every common size at build time:** it saves microseconds of load-time work but adds about 3 MB of table. More data to save less computation loses (§5a).
- **Walking the lig/kern program at run time:** repeated decoding per pair.
- **Scaling expanded fonts on use:** a multiply and a divide per character.
- **Parsing font programs per compile:** the same bytes, parsed again on every run.

### 4b. Font delivery and binary size: a small core, font packs for the rest (user, 2026-10-27)
Detailed design: `2026-10-27-font-packs-blueprint.md` (supersedes the tiering of
`2026-10-26-font-delivery.md`, keeps its rules: content-addressed, fetched before
typesetting, offline mode, licences, privacy).

**Size budget.**
| Delivery | Size | Contents | Templates fully covered offline |
|---|---|---|---|
| Core binary (CLI and WebAssembly) | at most 5 MB (aim 4.5) | code about 2.5 MB, tables about 0.5 MB, Computer Modern text and maths and the AMS fonts about 1.5-2 MB, the pack catalog | about 20%; the rest download once per family |
| WebAssembly transfer | about 3-3.5 MB compressed | the same core | packs fetched into browser storage |
| Desktop installer | about 15 MB | core plus the common packs: cm-super core, Latin Modern, URW Times/Helvetica/Courier/Palatino, TeX Gyre | about 80% |
| All packs (optional) | tens of MB | every pinned font | about 99% |
If code and tables do not fall to about 3 MB, the embedded Computer Modern sizes are cut
(10/11/12pt text and the maths kept) rather than exceed 5 MB.

**Tiers, one lookup order** for the native job (`carried`) and the engine (`Kpse::read`):
embedded, then packs in the cache (user `~/.cache/rushtex/packs`, then the installer's
system directory; memory-mapped, as cheap as embedded), then a verified download, then a
local TeX Live, then an error naming the pack (never a different PDF).

**Rules.**
- A pack is one font family (large ones split by shape, each under 3 MB) in the `embed.rtdb`
  container: TFM records, `.vf`, map lines, `.enc`, Type 1 programs with their stored layout,
  and its licence. Named `<pack>-<sha256>.rtpk`, immutable; the catalog in the binary maps
  every pinned font file to its pack and hash.
- The embedded set and the packs are generated by `rushtex-gen` from the pin, the embedded
  set chosen by measured coverage of the users' corpus under the budget, never by hand.
- The pre-scan predicts a document's packs; missing ones download in one parallel batch before
  typesetting; a missed prediction fetches on demand (rare, counted).
- Embedding is a build option; the pack reader does not depend on a file system, so the
  WebAssembly build fetches packs into the Cache API or IndexedDB.
- A font gives the same PDF bytes from every tier, tested by forcing each tier.

**Why.** Measured 2026-10-27 on 702 templates: 88% use Computer Modern, cm-super or Latin
Modern; Computer Modern and AMS alone (61 programs, 1.96 MB) cover 20% fully, 4 MB of the best
fonts 48%, about 13 MB 80%: the long tail belongs in packs. A font outside the binary costs
about 230M instructions through kpathsea and about 1M from a pack. The stripped binary was
22.8 MB (code 2.9 MB, embedded fonts 16.4 MB, other tables 1.7 MB).

### 5a. Design heuristic: less work and less data movement win without a benchmark (user, 2026-10-22)
The project cannot test every alternative. When one design does strictly less computation, has a simpler algorithm and moves less data per operation, it is chosen without benchmarking the others.
- **Count data movement per operation, not in total.** One cache miss costs dozens of arithmetic operations. When less computation and less data conflict, the design that touches fewer cache lines per hot operation wins.
- **The heuristic chooses the design; the gates prove it exact.** The zero-shift gate and the leak audit still run on every change. Measurements record what changed; they are not needed to justify a design the heuristic already decides.
- **Benchmark only when the heuristic does not decide:** when one option saves computation but moves more data, or saves data but adds computation, and the cost model is not clear.

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
§11b says to reduce movement. These are RushTeX's concrete rules:
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
- **Floor (user, 2026-10-24): 40–50× fewer CPU instructions than one pdflatex run on every native document, long ones included**, with zero shift. Budgets and method: `guarded-direct-lowering.md` §5. (Was: at least 30–40× pdflatex, aiming for 50×; 2026-10-22.) The oracle is 1.4–3× today; the gap is macro interpretation, which only native translation removes.
- `bench_corpus.py` reports the ratio per document. A commit that lowers any document's ratio is a regression.

### 6d. Computation and data-movement ledger (user, 2026-10-22)
For each stage of the native pipeline: what work is required, what is eliminated, and how.

| Stage | Required | Eliminated | How |
|---|---|---|---|
| Read source | read the bytes once | copies into `String`, re-reading | borrow slices of one buffer |
| Pre-scan and block split | one pass giving block boundaries and command IDs per block | a separate pre-scan pass; an AST of the whole document | fused into the lexer's single pass |
| Commands | dispatch by ID | expansion of LaTeX's internal macros, string compares, run-time option parsing | native code; options as build-time bitflags |
| Fonts | metrics of the characters used | TFM parsing at run time, unused fonts, rescaling per use | static tables; scaled once per used (font, size), cached by ID |
| Characters to nodes | ligature and kern program per pair | hashing per character | direct index into per-font tables |
| Hyphenation | only when a paragraph's first pass fails (TeX's rule) | every word; unused languages; repeated words | cache by (language, `\lefthyphenmin`, `\righthyphenmin`, word), cleared when `\hyphenation` or patterns change |
| Line breaking | Knuth–Plass over feasible breaks | re-accumulating widths per candidate; a second pass after a successful first | prefix sums; TeX's pass order |
| Math | Appendix G conversion | node vectors copied between steps | conversion in place into the build buffer |
| Page building | once per paragraph's lines | rebuilding unchanged pages | incremental early cutoff (§9) |
| References | resolve each `\ref`, `\pageref`, `\cite` | pdflatex's 2–3 whole-document runs | one run with references in memory; only pages whose resolved values changed are laid out again |
| PDF output | operators for glyphs and rules; compression | re-embedding fonts per page, generic number formatting, holding all pages, recompressing unchanged objects | fonts subset once at the end, a custom integer and fixed-point formatter, pages streamed, compression on a worker thread in page order |

| Data | Representation | Moves |
|---|---|---|
| Source | one buffer, borrowed slices | read once, never copied |
| Commands and tokens | `u16`/`u32` IDs | no strings after lexing |
| Nodes | written once into the arena | moved by span index, freed at shipout; copied only by `\copy` (§204) |
| Paragraph working data | SoA arrays plus prefix sums in a reused buffer | built once per paragraph |
| Font metrics | static tables through IDs | never copied |
| Page to PDF | streamed into a reused buffer | written once, in page order |
| Static tables | the binary's read-only data | the OS pages in only what is touched |

**Budgets per page, measured by H1:** bytes allocated, bytes copied, instructions per glyph and cache misses. A change that raises one must justify it in its note.

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

### 7b. Single-core throughput: SIMD and vectorisation (user, 2026-10-22)
Constraints: safe Rust only (`#![forbid(unsafe_code)]`), std only, stable compiler, one binary for every machine of its architecture. SIMD speeds up kernels by roughly 1.3–2× overall; removing interpretation stays the main lever (§6c).

**Tools, in order of preference:**
1. **Better algorithm first.** Example: line breaking keeps prefix sums of the paragraph's widths, so the width between two breakpoints is one subtraction, not a re-accumulation per candidate.
2. **Compiler auto-vectorisation** (SSE2 on x86, NEON on ARM). Loops are written so LLVM can vectorise them:
   - one field per array (SoA) for a field scanned across many records;
   - no branches in the body (`min`/`max`/select instead of `if`) and no early exit;
   - `chunks_exact(N)` plus a remainder, so bounds checks are hoisted and the loop unrolls;
   - struct fields copied into locals before the loop, so aliasing does not force a reload each iteration;
   - integer arithmetic where TeX's is integer, so reordering is exact.
3. **SWAR** (SIMD within a register, `u64` for 8 bytes and `u128` for 16, plain bit tricks):
   - the lexer and the pre-scan skip runs of ordinary bytes, stopping only at `\`, `{`, `}`, `%`, `$`, `&`, `#`, `^`, `_`, `~` or a newline;
   - DEFLATE match length by XOR plus `trailing_zeros`;
   - name hashing 8 bytes per multiply;
   - PDF string escaping checks.
4. **Instruction-level parallelism:** several independent accumulators, combined at the end, instead of one dependency chain. This covers box packing (widths, heights, stretch and shrink per order), Adler-32 and CRC, and bitset and Roaring unions, which compile to 128- or 256-bit operations.
5. **Build level:**
   - LTO and one codegen unit (already on);
   - a PGO release build trained on the corpus (`-Cprofile-generate`/`-Cprofile-use`, typically 10–20% on branchy code);
   - an optional x86-64-v3 (AVX2) build beside the baseline one, chosen at install, because run-time dispatch to `target_feature` code needs `unsafe`. ARM always has NEON.

**Not used:**
- `std::arch` intrinsics (`unsafe`), nightly `std::simd`, and `-C target-cpu=native` in releases.
- Vectorising TeX's order-sensitive arithmetic (badness, glue setting, division): exactness wins.

**Expected gains per kernel:**

| Kernel | Share of native-path time (estimate) | Gain |
|---|---|---|
| Lexing and pre-scan | 5–10% | 3–6× |
| DEFLATE compression | 10–25% | 2–4× |
| Image decoding (PNG unfiltering, colour conversion) | depends on the document | 3–8× |
| Packing and bitsets | small | 2–4× |
| Line-break widths (prefix sums) | 5–15% | 2–5×, algorithmic |
| Whole program, PGO | | 1.1–1.2× |

**Verified, not assumed:**
- instructions per glyph and IPC from H1's counters;
- the emitted vector instructions checked with a dev-only assembly viewer (`cargo-show-asm`);
- per-kernel micro-benchmarks with `std::hint::black_box`.
- A kernel is vectorised only after B4 shows it survives and matters.

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
- **N6:** native is the default; the engine stays in the release only as the whole-document fallback (§2a). A native document needs no TeX Live, and TeX's expansion is kept for the document's own macros.
- **Phases P0–P8 and status:** `progress.md`; the inventory and the override rule: `guarded-direct-lowering.md` §7–§8. The port grows inside the engine as native primitives, verified per command against the macro version. The old native pipeline is frozen and retired at P7, replacing N1's migration of it.

### 11a. How native work is built: Guarded Direct Lowering, GDL (user, 2026-10-24)
`guarded-direct-lowering.md` is the binding working guideline: pre-scan proof, direct compiler, typed node builders, exact core. Native code never emits TeX text, names are IDs, and fixed state is generated. Every change follows its rules R1–R8, its porting recipe and its gates.

### 11b. Resource rules (folded from `god_instruction.md` and `Resource_Conscious_Software_Engineering.md`, 2026-10-24)
Do less work, move less data, allocate less memory, touch less memory, do nonessential work later; then measure that it helped.
1. Borrow existing data; do not copy it to pass it between layers. Hand ownership on rather than cloning.
2. Transform in place when safe, instead of input, intermediate and output buffers.
3. Allocate once and reuse on hot paths (buffers, pools, scratch).
4. Compact binary representations between internal layers; no serialising or re-parsing.
5. Layout follows the access pattern: SoA for scanning a few fields, AoS for whole records; hot data apart from cold.
6. Bounded pipelines: every queue, buffer and cache has a capacity.
7. Maintenance work (cleanup, logging, compaction) off the latency path.
8. Optimise hot code only where a profile shows the cost; keep cold paths simple.
9. Before every operation ask "must this happen now?": defer, precompute at build time, cache, batch or remove it.

### 11c. Mathematics: rules, not tweaks (folded from `RushTeX_Math_Architecture_Blueprint.md`)
Math parity comes from Appendix G and `mlist.c` applied to the font's own parameters (TFM `fontdimen`s, OpenType MATH constants, per-glyph italic corrections, kerns and extensible recipes), with style as a state machine (display, text, script, scriptscript, each cramped or not) and atom classes driving spacing. Never a per-formula or per-symbol adjustment, and never an empirical proportion where the font carries the value.

### 11d. Beyond LaTeX: superset behaviour is opt-in (folded from `tex-weaknesses-roadmap.md`)
Compatibility mode reproduces pdfTeX's output exactly; anything "better" than TeX (global page optimisation, interpolated design sizes, Unicode-first input) is opt-in and never the default. TeX's values are kept, its methods are ours (no `mem` array or digit-loop arithmetic transliterated). One binary, data split from code: no dynamic libraries (no LTO across them, none on wasm). Hash maps and Bloom filters only where keys are dynamic or most lookups miss, and only when measured; encodings are 256-entry arrays, known names a generated perfect hash.

### 12. Review checklist (every change)
1. Does any work run at run time that could run at build time?
2. Does any string, `Vec` or node list get copied where an ID or a range would do?
3. Is there a loop that searches, rescans or allocates in a hot path? Which cache lines does it touch per item, is the access sequential, and does its working set fit the floor budgets (§7a: 32 KB, 256 KB, 4 MB, 64 B lines)?
4. Is each structure chosen by §5, and each SoA/AoS choice justified by the access pattern?
5. Does memory stay bounded by the working set? Is every allocation owned by a region or pool with a known lifetime, with no tracing, compaction or moving of live data?
6. Is the output still zero shift, and are the benchmark and RSS numbers equal or better?
7. Is the § or macro of the source cited in a code comment (`// §1057`, `` `\@startsection` ``)?
8. Can the state this change adds be checkpointed at a block boundary, diffed and replayed per block (§9, pillar 4)? Is it reached through IDs and arrays, with no hidden global mutation?
9. Is this the design that does the least work and moves the least data per operation (§5a)? Is each value stored in the form §3a gives it? Work at use that could move to load, or work at load that could move to build time, is a defect.
10. Does native code emit TeX text, intern a known name, or parse table text at run time (`guarded-direct-lowering.md` R1–R3)? Each is a defect.

## Rejected
- **Approach B (engine plus native fast paths for hot macros) as the product.** It keeps every package working, but it pays for token interpretation on every non-shortcut macro. Its checkpoints are megabytes (the whole equivalents table and token store), and any macro can change any state, so dependencies cannot be tracked exactly. Real-time incremental compile would degrade with document size. The engine is kept only as the oracle.
- **Runtime ingestion of `.cls`, `.sty`, `.fd` and `.tfm` files**, and a first-run format build. Both are build-time work done at run time, and both create a dependency on TeX Live.
- **Generic runtime `HashMap`s for static data.** Perfect hashes and direct indexes are smaller and faster, and never allocate.
- **Bloom filters as the source of truth for dependencies.** They do not pinpoint (§11d). They are allowed only as gates in front of exact structures.
- **Haste.** Porting a routine "to look right" without the oracle's node-level proof, or optimising before the design removes the work.
- **Native-only releases** (decided and reversed 2026-10-22). Refusing unported packages would block real documents that ship their own `.sty`/`.cls` or journal classes. Replaced by the whole-document fallback (§2a).
- **Mixing native and interpreted code inside one document.** An unknown package expects LaTeX's internals as macros, so the result cannot be proved exact. The whole document falls back instead.
- **Generating tables in `build.rs`.** Every build would need the pinned TeX Live. Committed blobs plus CI regeneration give the same guarantee without it.
- **Plugins and dynamic libraries for packages:** load time, ABI coupling, no WASM, a weaker security model.
- **Unsafe code for speed:** it removes checks, not work; see §3a.

## Measured
Baseline at adoption (2026-10-22, `scripts/bench_corpus.py`, 85 documents, 2,137 pages):
- **Oracle engine:** 129.4 s wall time, 120.5 s CPU; median peak RSS 48.7 MB, maximum 847.8 MB; 222 runs.
- **pdflatex:** 380.2 s wall time, 314.9 s CPU; median peak RSS 31.1 MB, maximum 40.9 MB; 206 runs.
- **Core and interpreter size:** the typesetting core is about 11.3k lines; the interpreter about 9.3k lines.
- **What must be ported:** `latex.ltx` is 22,469 lines; the corpus loads about 100–150 distinct `.sty`/`.cls` files.

## Open
- Which expl3 parts the native kernel must mirror: `latex.ltx` now relies on `l3kernel` for hooks, keyval and text case.
- Package coverage list and order: measure usage frequency over a large sample of real documents (arXiv sources), not only the corpus.
- **Unported package:** decided 2026-10-22 as a release error, then replaced the same day by the whole-document engine fallback (§2a).
- **How often documents fall back.** To be measured on A3 and a larger arXiv sample: the triggers, ranked, set the porting order.
- Licensing record for the generated tables (LPPL permits redistribution; keep the attribution and pinned versions in the binary's notices).
