# RushTeX progress (living checklist)
Author: abhi+claude | Date: 2026-10-22, last updated 2026-10-26 (living: updated in the commit that closes an item)
Files: `memorycastle/journal/native-architecture.md`, `memorycastle/journal/guarded-direct-lowering.md`, `docs/master.tex`, `docs/index.html`, `benchmark.sh`, `scripts/pgo_build.sh`, `src/engine/engine_doc.rs`, `src/engine/engine_linebreak.rs`, `examples/phase_count.rs`, `src/engine/engine_ams_align.rs`, `scripts/native_probes/gather.tex`, `scripts/native_probes/multline.tex`, `src/engine/engine_eqnarray.rs`, `src/engine/engine_align.rs`, `src/engine/engine_tabular.rs`, `scripts/native_probes/aligned.tex`, `scripts/native_probes/split.tex`, `src/engine/engine_noad.rs`, `assets/tables/names.rtdb`, `scripts/native_probes/genfrac.tex`, `scripts/native_probes/numberwithin.tex`, `scripts/native_probes/smash.tex`, `scripts/native_probes/modulo.tex`, `scripts/native_probes/textsymbols.tex`, `scripts/native_probes/appendix.tex`, `scripts/native_probes/overset.tex`, `scripts/native_probes/tag.tex`, `scripts/native_probes/mathrsfs.tex`, `scripts/native_probes/boldsymbol.tex`, `scripts/native_probes/substack.tex`

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
- [ ] H1. Hardware-counter tool (instructions per glyph, IPC, cache misses per page). IPC measured with `/usr/bin/time -l` cycles: rushtex 3.4–4.1, pdfLaTeX 4.2–4.5 (`2026-10-26-self-time-census.md`). Profile-guided release build, trained on the corpus only: `scripts/pgo_build.sh` (`2026-10-26-pgo-build.md`).
- [ ] B1–B5. Copies and algorithmic waste removed from the expander and the core; a layout table beside every core record.
- [ ] H2. Single-core throughput for the loops B4 keeps (`native-architecture.md` §7b).

### Phase 2. Tables and demand loading
- [x] D1. Table library: perfect hash, SoA tables, CSR lists, bitsets.
- [x] D2. `rushtex-gen`: package registry, 354 TFMs, 630 NFSS shapes, class tables, all self-checked (`2026-10-22-d2-font-tables.md`, `2026-10-22-d2-nfss-tables.md`, `2026-10-22-d2-package-registry.md`).
- [ ] D2b. Fonts: programs carried in the binary (done for the pin, `2026-10-23-carry-table-font-programs.md`, `2026-10-24-embed-plain-and-glyph-names.md`); subset per document as pdfTeX does, PDF within 5% of pdfLaTeX's on `docs/master.tex` (`2026-10-24-pdf-size-subsetting.md`). Left: per-glyph pieces at build time (about 10M instructions per document today); lig/kern pair tables at build time.
- [ ] D2c. One blob per domain with a combined fingerprint; generated `const`s.
- [x] D4b. Hyphenation memo per word (`2026-10-24-hyphenation-memo.md`).
- [x] D4c. A hyphenated word stays one node through the second pass: its form is a word entry with break rows, and only words holding a chosen break are expanded (`2026-10-25-hyphenated-word-nodes.md`). The patterns are TeX's packed trie, built by `rushtex-gen` (`2026-10-26-packed-trie.md`). Left: lazy hyphenation (stage C); hyphenating a first sighting from the word store's items without making nodes (about 3M on `native-text-61p`).
- [ ] D3, D4. Pre-scan fused into the lexer; demand loading of patterns, metrics and font programs.
- [ ] D5. Start-up, RSS and PDF size against pdfLaTeX. Dynamic-Huffman deflate tried and reverted by the user's choice: smaller than pdfLaTeX's PDFs, but about 0.6M instructions per page (`2026-10-24-deflate-dynamic-huffman.md`). Left: start-up and RSS.
- [ ] F1. Engine fallback in releases, reporting its trigger.
- [ ] F2. Fallback triggers ranked over the arXiv sample: started (`2026-10-23-arxiv-corpus.md`, `2026-10-23-native-gaps-census.md`, `2026-10-24-native-coverage-census.md`); 1 of 169 papers native today.
- [ ] I1. Session-stable interner for incremental compile.

### Phase 3. Native translation (GDL, census order)
- [x] P0. Native-command layer and `RT_NATIVE` (`2026-10-22-p0-kernel-native.md`).
- [ ] P1. Fonts and NFSS from tables: `\selectfont`, switches, `\normalfont`, size commands, `\rmdefault`/`\sfdefault`/`\ttdefault` native (`2026-10-22-p1-font-switches.md`, `2026-10-24-native-font-declarations.md`, `2026-10-24-native-font-families.md`). NFSS substitution (`\wrong@fontshape`) and whole font families in the pin (`2026-10-25-braces-macros-font-pin.md`). T1 encoding (`2026-10-27-native-t1-fontenc.md`). lmodern (`2026-10-28-native-lmodern.md`). Left: `\usepackage{times}` and other font packages, the cm-super programs in the pin.
- [x] P2. The native document job: small2e, sample2e, lppl at zero shift with no format and one pass (`2026-10-23-p2-native-document-path.md`, `2026-10-23-p2-3-lppl-native.md`).
- [ ] P2b. One-pass references: forward `\ref`, `\pageref`, `\tableofcontents` done (`2026-10-23-p2b-forward-refs.md`), `\cite` with `thebibliography` (`2026-10-23-native-cite.md`). Left: forward `\pageref`, BibTeX `.bbl`, lists of figures and tables.
- [ ] P2c–P2d. Kernel commands by census: class options (`2026-10-23-class-options.md`), flush environments, accents, `\\`, operators and delimiters, preamble lengths (`2026-10-23-p2d-preamble-lengths.md`), abstract and equations (`2026-10-23-abstract-equation-vspace.md`), `\centering`, `\newpage`, `\hspace`, `\noindent` (`2026-10-24-native-kernel-misc.md`). `\newtheorem` in all three forms (`2026-10-25-native-kernel-theorems.md`). `predict` passes preamble macros that step no counter (`2026-10-25-braces-macros-font-pin.md`).
- [x] GDL adopted (`guarded-direct-lowering.md`): node builders (`2026-10-24-gdl-builders-vspace-newline.md`), byte-run text (`2026-10-24-gdl-text-run-cite.md`), word table and word nodes (`2026-10-24-gdl-word-table.md`), generated name table (`2026-10-24-generated-names-table.md`); theorems, amsthm, `eqnarray` and `align` on builders and built preambles, names interned once per job, native indices by name at compile time; then every other TeX-text native (math accents, operators, `\text`, `\big`, dots, `\colon`, cases and matrices, displays, `\eqref`, text accents, `\setlength`, the contents, `abstract`, `thebibliography`, the operator install) on builders, and `tex()` deleted (`2026-10-25-gdl-debt-typed-natives.md`). Names known in advance are compared as numbers everywhere in the natives (R2).
- [ ] P3. Output routine and floats: `figure`, `table`, `\caption`, the float queue native (`2026-10-24-native-floats.md`). Left: `\clearpage` in the body, `\listoffigures`, `\suppressfloats`, two columns.
- [ ] P4. Tables, math, pictures: `tabular` with rules and spans native (`2026-10-24-native-tabular.md`); inline and display math, `equation`; `array` in math, amsmath `cases` and the six matrices (`2026-10-25-native-array-cases-matrices.md`); `eqnarray` and `eqnarray*` on the engine's `\halign` (`2026-10-25-native-eqnarray.md`); amsmath `gather`, `gather*`, `multline`, `multline*`, `aligned`, `gathered` and `split` (`2026-10-26-native-gather.md`, `2026-10-26-native-multline.md`, `2026-10-26-native-aligned.md`, `2026-10-26-native-split.md`); pictures through graphicx (P6). Left: `smallmatrix`, `\vdots`, `\ddots`, `\tag`. `p{}` columns native (`2026-10-28-native-p-columns.md`). array.sty declarations `>{}`, `<{}`, `!{}`, `@{text}` native (`2026-10-28-native-column-declarations.md`). tabularx native (`2026-10-28-native-tabularx.md`). Prediction through macros of starred headings (`2026-10-28-predict-starred-macros.md`). microtype native (`2026-10-28-native-microtype.md`). report's table of contents native (`2026-10-28-report-toc.md`). enumitem expression values (`2026-10-28-enumitem-expression-values.md`). `\href` in arguments (`2026-10-28-href-in-arguments.md`). siunitx loaded (`2026-10-28-siunitx-load.md`). UTF-8 bookmark titles (`2026-10-28-bookmark-utf8.md`). parskip native (`2026-10-28-native-parskip.md`). Engine: an expanded font is not reused (`2026-10-28-expanded-font-reuse.md`). Expanded copies' `.vf` answered by the binary (`2026-10-28-expanded-vf-absent.md`). ToUnicode CMap bodies shared per encoding file (`2026-10-28-tounicode-shared-body.md`). Encoding files read and parsed once per job (`2026-10-28-encoding-once-per-job.md`). `\dfrac`, `\tfrac`, `\binom` and `\genfrac` native (`2026-10-26-native-genfrac.md`). `\numberwithin{equation}{section}` native (`2026-10-26-native-numberwithin.md`). `\smash`, the phantoms, the `\tmspace` spaces and the long arrows native (`2026-10-26-native-smash-spaces-arrows.md`). Modulo commands and text symbols native (`2026-10-26-native-modulo-text-symbols.md`). `\appendix` native (`2026-10-26-native-appendix.md`). `\overset`, `\underset`, `\vdots`, `\ddots` native (`2026-10-26-native-overset-dots.md`). `\tag` native in equations (`2026-10-26-native-tag.md`). mathrsfs native (`2026-10-26-native-mathrsfs.md`). The bold math version and `\boldsymbol` native (`2026-10-26-native-boldsymbol.md`). `\substack`, `\xrightarrow` and `\xleftarrow` native (`2026-10-26-native-substack-xarrows.md`). `\overrightarrow`, `\overleftarrow`, `\overleftrightarrow` native (`2026-10-26-native-overarrows.md`). `smallmatrix` and `\hdotsfor` native, `\multicolumn` in an array fixed (`2026-10-26-native-smallmatrix-hdotsfor.md`). `\tag` in `align` and `gather` rows native (`2026-10-26-native-align-tag.md`). Roots, `\sqrt[n]` and the plain fraction commands native (`2026-10-26-native-roots-plain-fractions.md`). `\def` native; redefined internals guarded (`2026-10-26-native-def.md`). booktabs native (`2026-10-26-native-booktabs.md`). multirow native (`2026-10-26-native-multirow.md`). `\baselinestretch` read natively (`2026-10-26-native-baselinestretch.md`). The array package's tables native (`2026-10-26-native-array-package.md`). UTF-8 input and the common text symbols native (`2026-10-26-native-utf8-input.md`). `\c`, `\r A`, `\AA`, `\aa`, `\l`, `\L` native (`2026-10-26-native-cedilla-ring-l.md`). `\smallskip`/`\medskip`/`\bigskip`, `\em`, `\thispagestyle`, `\ensuremath`, `\columnwidth`, `\rule` native (`2026-10-26-native-kernel-small-commands.md`).
- [ ] P5. Classes beyond article: report, book, amsart, IEEEtran, revtex. report: class tables, `\chapter` and chapter numbering, `titlepage` native (`2026-10-28-report-class-design.md`); a box-0 float-output bug that dropped a line fixed with it.
- [ ] P6. Packages by real usage: amsfonts, amssymb native (`2026-10-23-native-packages-amssymb.md`). graphicx native (`2026-10-24-native-graphicx.md`: `\includegraphics` with width, height, scale, keepaspectratio; PDF and PNG). amsmath stage A, `align`/`align*` and `gather`/`gather*` and `multline`/`multline*` in one pass (`2026-10-25-native-align.md`, `2026-10-26-native-gather.md`, `2026-10-26-native-multline.md`). Package options with no effect, inputenc (`2026-10-25-native-package-options.md`). amsthm: styles, `\newtheorem*`, `proof`, `\qed` (`2026-10-25-native-amsthm.md`). geometry: keys, `\Gm@@process`, `\geometry` (`2026-10-26-native-geometry.md`). color and xcolor loaded (`2026-10-26-native-color-load.md`). `\color`, `\textcolor`, `\definecolor` (`2026-10-26-native-color-commands.md`). url.sty (`2026-10-26-native-url.md`). hyperref (`2026-10-27-native-hyperref-load.md`): page anchors, Doc-Start, catalog and Info, bookmarks and outlines, heading/caption/item/theorem/footnote/equation/amsmath display anchors, `\ref`/`\pageref`/`\eqref`/`\cite` links, `\url`/`\href`/`\nolinkurl`, `hidelinks`, `colorlinks`, link colours, `\hypersetup`, document properties; `\tableofcontents`, `\appendix`, eqnarray, `\tag` and other keys still refuse (stopped by census, `2026-10-27-handover-native-porting.md`). `\bibliography` reads `\jobname.bbl` in the one pass (`2026-10-27-native-bibliography.md`). Kernel batch (`2026-10-27-native-kernel-batch.md`): `\clearpage`, `\notin`, `\cong`, `\underbrace`, `\overbrace`, `\thanks`, `\and`, user-level primitives (`\hbox`, `\vskip`, ...) and `\rlap`, `\llap`, `\nobreak`, `\sloppy` and kin; a TeX error in the native job declines it. bm: `\bm` of math characters (`2026-10-27-bm-design.md`). textcomp, dcolumn as tables (`2026-10-27-native-package-tables.md`). float: `[H]` (`2026-10-27-native-float.md`). enumitem default lists (`2026-10-27-native-enumitem.md`) and its keys with `\setlist` (`2026-10-27-native-enumitem-keys.md`). `\fontsize` with `\selectfont` (`2026-10-27-native-fontsize.md`). `\parbox` and `minipage` (`2026-10-27-native-parbox-minipage.md`). `\fbox` (`2026-10-27-native-fbox.md`). xcolor's `\colorbox`, `\fcolorbox` (`2026-10-27-native-colorbox.md`). `\textsuperscript`, `\textsubscript` (`2026-10-27-native-text-scripts.md`). xcolor HTML colours and mixes (`2026-10-27-xcolor-expressions-design.md`). fancyhdr heads and feet, one-sided (`2026-10-28-fancyhdr-design.md`). enumitem `description` and `label=` references; master uses enumitem (`2026-10-28-enumitem-description-label-refs.md`). Prefixed counter forms (`2026-10-27-native-counter-prefix.md`). mathtools and calc (`2026-10-27-native-mathtools.md`). enumerate.sty with label templates (`2026-10-27-native-enumerate.md`). `\allowdisplaybreaks` (`2026-10-27-native-allowdisplaybreaks.md`). Theorems within a theorem's counter, titles with commands (`2026-10-27-native-theorem-within-theorem.md`). **Next, by the users' corpus** (`2026-10-27-handover-users-corpus.md`, documents made fully native among the latextemplates.com templates): fontenc T1 + lmodern, package options, report/book classes, fancyhdr, tabularx and enumitem keys, then small kernel items.
- [ ] Known small gaps (from `docs/master.tex`): `\[` after a blank line, `\verb`, `\textbackslash`, `\longrightarrow`. (`\{`/`\}` in any text font: done, `2026-10-25-braces-macros-font-pin.md`.)

### Speed (alongside Phase 3)
- [x] `benchmark.sh`: instructions retired, pdfLaTeX's real run count, fixed and per-page cost fitted (`2026-10-24-benchmark-instructions.md`); benchmarks of 10 pages or more (`2026-10-24-benchmark-10-pages.md`).
- [x] Type 1 subsetting: scoped searches, charstrings encrypted as copied; font work per document 21.2M to 11.5M (`2026-10-25-type1-subset-speed.md`).
- [ ] Per page 4.6–5.3× against the 40× floor; fixed cost 28–45× (`2026-10-25-braces-macros-font-pin.md`, benchmark at `af079e2`). The book alone is 38.2× one run. Next: fewer tokens per page, per-word ship, line breaking with prefix sums and an index-linked active list.

### Phase 4. Product
- [x] Pinned TeX Live snapshot 3 (whole font families), release `texlive-2026-pin-3`, named in `ci.yml` (`2026-10-25-braces-macros-font-pin.md`). The workflow is not in the public tree, so GitHub runs no CI.
- [ ] P7. Native by default: no `latex.ltx` or TeX Live for a native document; the speed floor and zero shift on every corpus document.
- [ ] P8. Real-time incremental: block checkpoints, early cutoff, only changed pages shipped; under 16 ms per typical edit.

### The master document
- [x] `docs/master.tex`: the project's description and growing native test, one section per native feature (`2026-10-24-master-doc.md`), restructured as the RushTeX book (`2026-10-25-master-restructure.md`). Each newly native feature adds its section; this file and `package-porting-progress-list.md` are updated in the same commit.

## Measured
2026-10-26, after the word key, allocation cuts, miss-path buffers, break pool, word ship bytes, packed trie, `param!` and the family-font memo (`2026-10-26-*.md`). Measured on one Mac, one run each, best of three, plain release build:
- `native-text-61p`: 218.4M instructions against pdfLaTeX's 3,278M (15.0×; 245.7M on 2026-10-25).
- `native-29p`: 150.4M against 3,808M (25.3×; 165.0M).
- `native-math-20p`: 102.3M against 3,241M (31.7×; 112.5M).
- `native-tables-29p`: 129.3M against 2,796M (21.6×; 140.2M).
- `native-master-95p`: 300.2M against 4,740M (15.8×).
- Against XeLaTeX (with its PDF driver): 13.8× to 25.4× fewer instructions.
- The profile-guided build (`scripts/pgo_build.sh`) is 4% to 7% fewer again on these documents, which are held out of its training.
- **The 40–50× floor is not met**, and the long text documents are furthest from it, at 15–16×. Where they spend their instructions is in `2026-10-26-self-time-census.md`. The cuts left are each under 3M: the kernel's per-job install (about 1.5–2M), the hyphenation miss path's node round trip (about 3M on 61p), and a byte-identical faster deflate (1–2M).
- 91/91 PDFs byte-identical throughout; corpus 85/85 zero shift; leak audit clean.

2026-10-25, hyphenated words kept as word nodes (`2026-10-25-hyphenated-word-nodes.md`):
- Instructions, one run: `native-text-61p` 245.7M (260.5M before), `native-29p` 165.0M (183.5M), `native-math-20p` 112.5M (125.4M), `native-tables-29p` 140.2M (144.4M); `docs/master.tex` 184.6M (184.7M). 91/91 PDFs byte-identical, also with `RT_WORD_CHECK=1`; corpus 85/85 zero shift (also with `RT_EMBED_FIRST=1`); leak audit 85/85.

2026-10-25, after GDL debt part 2 (`tex()` 50 → 0 sites, the function deleted):
- Instructions, one run: `docs/master.tex` 191.9M (199.6M before); `native-29p` 193.5M (199.8M); `native-math-20p` 133.8M (136.5M). `docs/master.tex` box-identical to the engine and 28/28 pages pixel-identical to pdfLaTeX at 150 and 600 dpi; corpus 85/85 zero shift (also with `RT_EMBED_FIRST=1`); leak audit 85/85.

2026-10-25, after GDL debt part 1 (`tex()` 93 → 50 sites, runtime name interning gone):
- `docs/master.tex`: 199.6M instructions for one run (201.0M before); box-identical to the engine, 28/28 pages pixel-identical to pdfLaTeX at 150 and 600 dpi. Corpus 85/85 zero shift.

2026-10-25, after `eqnarray` (`c430374`):
- `docs/master.tex`: 28 pages, native, box-identical to the engine and pixel-identical to pdfLaTeX at 150, 300 and 600 dpi; 200.1M instructions for one run.
- Benchmarks at `af079e2` (`benchmark.sh`, one RushTeX run against one pdfLaTeX run):
  - book, 25 pages: 38.2×;
  - math, 20 pages: 22.8×;
  - mixed, 29 pages: 18.5×;
  - tables, 29 pages: 17.9×;
  - prose, 61 pages: 11.7×.
- Corpus: 85/85 zero shift.
- arXiv sample: 1 of 169 papers native; 164 still have 11 or more blockers. The top blockers are package options (129), non-ASCII input (109), hyperref (105), bibliography (95) and journal classes (94).

Earlier (2026-10-24): master 12 pages, ten-page version 76.6M against pdfLaTeX's 3,261.5M (42.6×).
