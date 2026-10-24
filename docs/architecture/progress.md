# RushTeX progress (living checklist)
Author: abhi+claude | Date: 2026-10-22 (living: updated in the commit that closes an item)
Files: `memorycastle/journal/native-architecture.md`, `memorycastle/journal/guarded-direct-lowering.md`, `docs/master.tex`, `benchmark.sh`

## Context
The project keeps three guidance documents, and only three (user, 2026-10-24):
- **`progress.md`** (this file): what is done and what is next. One line per item, pointing to the dated note that holds its decisions and numbers. An item is ticked in the commit that passes its gate.
- **`native-architecture.md`**: the long-term rules (invariants, layers, storage, data structures, memory, hot loops, hardware budgets, exactness, incremental compile, resource rules, math, superset policy) and the review checklist.
- **`guarded-direct-lowering.md`**: how every native feature is built (the GDL pipeline, rules R1–R9, the porting recipe, what is ported and what stays, overridable internals, gates, speed budget).

Dated notes in `memorycastle/journal/` hold why each decision was made; `CLAUDE.md` holds the commands. Nothing else is a plan or a to-do list.

**The gate** (every item): `cargo test --release`; clippy ×3 (default, `--no-default-features`, `--features gen`); `scripts/native_probes.sh` (the probes and `docs/master.tex`, box by box against the engine); `parity-testing zero --engine --pixels --all` 85/85, also with `RT_EMBED_FIRST=1`; `scripts/engine_suites.py`; `scripts/leak_audit.py` 85/85; instruction counts with `benchmark.sh` on the `scripts/bench/` documents (10 pages or more).

## Decision

### Standing rules
- **Speed floor:** 40–50× fewer instructions than one pdfLaTeX run, long documents too (`2026-10-24-direct-path-guideline.md`), judged per page by the ×1/×4/×10 fit (`2026-10-24-benchmark-instructions.md`).
- **Identical output:** zero shift, every glyph, path, link and pixel; a feature without that proof is not shipped.
- **No `.aux`, one pass:** references in memory (`2026-10-23-p2b-forward-refs.md`).
- **Native first, engine fallback:** a document the pre-scan cannot prove, or one with its own `.sty`/`.cls`, runs whole on the engine (`native-architecture.md` §2a).
- **No garbage collection, no copies on hot paths, structures by access pattern, load only what the document uses** (`native-architecture.md` §5–§6d, §11b).
- **One implementation per job:** the old approximate pipeline (`src/layout`, `src/math`, `src/tables`, `--native`) is frozen and leaves at P7; `#![forbid(unsafe_code)]` stays.

### The five pillars (user, 2026-10-22)
1. Native kernel on the exact core. 2. Data as build-time tables (`rushtex-gen`, each generator self-checked). 3. Demand loading from the pre-scan. 4. Per-block incremental compile designed in (state reachable through IDs and arrays). 5. Zero-shift proof for every ported feature.

### Done before the native port
- [x] 85 CTAN documents at zero shift through the engine (2,137 pages); the CLI runs the engine on the LaTeX format with a kpathsea index (`engine_host`).
- [x] Runs only while written files change, as pdfLaTeX (`2026-10-22-runs-until-stable.md`).
- [x] Model M memory on the node core; leak audit 85/85 clean (`2026-10-22-zero-gc-core-model.md`).
- [x] The testing framework in Rust, `parity-testing` (`2026-10-23-rust-parity-checker.md`).

### Phase 1. Foundations
- [x] A0. `oracle` cargo feature: the product builds without `latex.ltx` interpretation.
- [x] A1, A1+. Pinned TeX Live 2026 snapshot; CI gates zero shift, the leak audit and the engine suites on it (`2026-10-22-a1-texlive-pin.md`).
- [ ] A1b. One generated load probe per `.sty`/`.cls`, proving each native package against its macro version.
- [ ] A2. Security for untrusted documents: file-read policy, resource caps, parser fuzzing, an overflow-checked corpus run, a linear-time `\pdfmatch`. Required before a public release.
- [ ] A3. 15–20 real papers added to the zero-shift gate.
- [ ] Z2–Z4. Buffers reserved once, token model M, capacity from the source size.
- [ ] H1. Hardware-counter tool (instructions per glyph, IPC, cache misses per page).
- [ ] B1–B5. Copies and algorithmic waste removed from the expander and the core; a layout table beside every core record.
- [ ] H2. Single-core throughput for the loops B4 keeps (`native-architecture.md` §7b).

### Phase 2. Tables and demand loading
- [x] D1. Table library: perfect hash, SoA tables, CSR lists, bitsets.
- [x] D2. `rushtex-gen`: package registry, 354 TFMs, 630 NFSS shapes, class tables, all self-checked (`2026-10-22-d2-font-tables.md`, `2026-10-22-d2-nfss-tables.md`, `2026-10-22-d2-package-registry.md`).
- [ ] D2b. Fonts: programs carried in the binary (done for the pin, `2026-10-23-carry-table-font-programs.md`, `2026-10-24-embed-plain-and-glyph-names.md`); subset per document as pdfTeX does, PDF within 5% of pdfLaTeX's on `docs/master.tex` (`2026-10-24-pdf-size-subsetting.md`). Left: per-glyph pieces at build time (about 10M instructions per document today); lig/kern pair tables at build time.
- [ ] D2c. One blob per domain with a combined fingerprint; generated `const`s.
- [x] D4b. Hyphenation memo per word (`2026-10-24-hyphenation-memo.md`).
- [ ] D3, D4. Pre-scan fused into the lexer; demand loading of patterns, metrics and font programs.
- [ ] D5. Start-up, RSS and PDF size against pdfLaTeX; dynamic-Huffman deflate.
- [ ] F1. Engine fallback in releases, reporting its trigger.
- [ ] F2. Fallback triggers ranked over the arXiv sample: started (`2026-10-23-arxiv-corpus.md`, `2026-10-23-native-gaps-census.md`, `2026-10-24-native-coverage-census.md`); 1 of 169 papers native today.
- [ ] I1. Session-stable interner for incremental compile.

### Phase 3. Native translation (GDL, census order)
- [x] P0. Native-command layer and `RT_NATIVE` (`2026-10-22-p0-kernel-native.md`).
- [ ] P1. Fonts and NFSS from tables: `\selectfont`, switches, `\normalfont`, size commands, `\rmdefault`/`\sfdefault`/`\ttdefault` native (`2026-10-22-p1-font-switches.md`, `2026-10-24-native-font-declarations.md`, `2026-10-24-native-font-families.md`). Left: `\usepackage{times}` and other font packages (not in the pin), T1 encoding.
- [x] P2. The native document job: small2e, sample2e, lppl at zero shift with no format and one pass (`2026-10-23-p2-native-document-path.md`, `2026-10-23-p2-3-lppl-native.md`).
- [ ] P2b. One-pass references: forward `\ref`, `\pageref`, `\tableofcontents` done (`2026-10-23-p2b-forward-refs.md`), `\cite` with `thebibliography` (`2026-10-23-native-cite.md`). Left: forward `\pageref`, BibTeX `.bbl`, lists of figures and tables.
- [ ] P2c–P2d. Kernel commands by census: class options (`2026-10-23-class-options.md`), flush environments, accents, `\\`, operators and delimiters, preamble lengths (`2026-10-23-p2d-preamble-lengths.md`), abstract and equations (`2026-10-23-abstract-equation-vspace.md`), `\centering`, `\newpage`, `\hspace`, `\noindent` (`2026-10-24-native-kernel-misc.md`).
- [x] GDL adopted (`guarded-direct-lowering.md`): node builders (`2026-10-24-gdl-builders-vspace-newline.md`), byte-run text (`2026-10-24-gdl-text-run-cite.md`), word table and word nodes (`2026-10-24-gdl-word-table.md`), generated name table (`2026-10-24-generated-names-table.md`). Left: the 25 `tex()` natives move to builders (R1).
- [ ] P3. Output routine and floats: `figure`, `table`, `\caption`, the float queue native (`2026-10-24-native-floats.md`). Left: `\clearpage` in the body, `\listoffigures`, `\suppressfloats`, two columns.
- [ ] P4. Tables, math, pictures: `tabular` with rules and spans native (`2026-10-24-native-tabular.md`); inline and display math, `equation`. Left: amsmath environments, `array`, pictures.
- [ ] P5. Classes beyond article: report, book, amsart, IEEEtran, revtex.
- [ ] P6. Packages by real usage: amsfonts, amssymb native (`2026-10-23-native-packages-amssymb.md`). **Next, Tier A order:** graphicx, amsmath (`align`, `\text`, `equation*`, `\eqref`), package options, UTF-8 input, hyperref, BibTeX, amsthm, natbib, booktabs, geometry, xcolor.
- [ ] Known small gaps (from `docs/master.tex`): `\{`/`\}` in `\texttt`, `\[` after a blank line, `\verb`, `\textbackslash`, `\longrightarrow`.

### Speed (alongside Phase 3)
- [x] `benchmark.sh`: instructions retired, pdfLaTeX's real run count, fixed and per-page cost fitted (`2026-10-24-benchmark-instructions.md`); benchmarks of 10 pages or more (`2026-10-24-benchmark-10-pages.md`).
- [ ] Per page 6.0× against the 40× floor (3.2M against 19.4M instructions); fixed cost 70×. No stage is above 16%: fewer tokens per page, per-word ship, line breaking with prefix sums and an index-linked active list.

### Phase 4. Product
- [ ] P7. Native by default: no `latex.ltx` or TeX Live for a native document; the speed floor and zero shift on every corpus document.
- [ ] P8. Real-time incremental: block checkpoints, early cutoff, only changed pages shipped; under 16 ms per typical edit.

### The master document
- [x] `docs/master.tex`: the project's description and growing native test, one section per native feature (`2026-10-24-master-doc.md`). Each newly native feature adds its section.

## Measured
`docs/master.tex` (12 pages, native, zero shift at 300 and 600 dpi). Ten-page version: 76.6M instructions against pdfLaTeX's 3,261.5M one run (42.6×) and 10,461.5M over its 3 runs (136.5×); fit per page 3.2M against 19.4M (6.0×), fixed 43.7M against 3,082.1M (70.5×). Corpus 85/85 zero shift. arXiv sample: 1 of 169 papers native.
