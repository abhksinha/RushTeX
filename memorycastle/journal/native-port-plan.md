# Native LaTeX port: plan and tracker (living)
Author: abhi+claude | Date: 2026-10-22 (living tracker: update rows in place; record each phase in a dated note)
Files: `memorycastle/journal/native-architecture.md`, `src/engine/engine_run.rs`, `src/engine/engine_prim.rs`, `src/engine/engine_tables.rs`, `src/engine/engine_eqtb.rs`, `src/engine/engine_names.rs`, `src/engine/engine_macro.rs`, `src/engine/engine_cond.rs`, `src/engine/engine_token.rs`, `src/engine/engine_job.rs`, `src/engine/engine_host.rs`, `src/engine/engine_snapshot.rs`, `src/engine/engine_font.rs`, `src/engine/engine_page.rs`, `src/layout/layout_engine.rs`, `src/math/math_layout.rs`, `scripts/parity/parity.py`, `scripts/parity/engine_boxdiff.py`, `scripts/bench_corpus.py`

## Context
`native-architecture.md` (SUPER IMPORTANT) fixes the target: LaTeX written natively in Rust, exact, with zero runtime dependency. This tracker is how we get there with backward compatibility: phases, method, the inventory of what must be ported and what need not be, and the status of every kernel module and package.

## Decision
The port is a strangler migration inside the engine:
- Each LaTeX command becomes a native primitive (`Meaning::Native`) in the engine's dispatch, shadowing its `latex.ltx` definition.
- A per-command switch (`RT_NATIVE`) lets the same binary run either the macro or the native version on identical state, and `RT_BOXES` diffs the two node by node.
- When every command a document reaches is native, `latex.ltx` is no longer loaded. Its data comes from tables generated at build time.
- TeX's expansion machinery remains for the document's own macros. The old native pipeline (`src/layout`, `src/math`) is frozen and retired.

### 0a. Scope and design rule (user, 2026-10-22)
- **All of P1–P8 is in scope**, with a speed gate of at least 30–40× pdflatex on every native document.
- **Port designs, not transliterations.** A ported cluster is redesigned by `native-architecture.md` §5a: least computation, simplest algorithm, least data moved, cache lines counted per operation. Only LaTeX's observable results are kept (boxes, glyph positions, the state other code reads, written files).
  - Example: font selection becomes one cached (shape, size) → font ID lookup over the generated tables, not `\csname` building and `.fd` reading.
  - Example: counters, lengths and flags live in typed `eqtb` slots read by index, not in macro texts.
- **The per-command strangler** (`RT_NATIVE`, `kernel_diff.py`) proves each step against the macros. Once a whole cluster is native and nothing outside it reads its internals, the cluster's internal macros and their side effects are dropped. That is where the speed comes from.

### 1. The boundary: what stays and what goes
- **Leaves the product:** interpretation of `latex.ltx`, expl3 and the `.sty`/`.cls` files (their macro implementations); the TeX Live files; the first-run format build; `kpsewhich`; `.aux` files.
- **Stays in the product:** the typesetting core, and TeX's macro language for the document's own code. Documents use `\def` with delimited parameters, `\expandafter`, `\csname`, `\makeatletter`, catcode changes and `\newcommand`, so backward compatibility requires TeX's full expansion semantics (tex.web §366–§401, conditionals §487–§510). `engine_macro` and `engine_cond` already implement them. This is not "the engine" in the slow sense: it expands only the document's own macros, which are few and short. Every LaTeX command it reaches is a native call.
- `native-architecture.md` §2 and N6 are corrected accordingly (see `2026-10-22-native-port-plan.md`).

### 2. Method for one command (deterministic, no trial and error)
1. **Pick** the next command by its measured cost: `RT_PROFILE` (P0) counts expansions and time per format macro, per document.
2. **Read** its source: the `.dtx` in `latex-source/` (pinned version), with `scripts/refindex.py` for where it is defined and used.
3. **Write** the Rust primitive in the module for its kernel file (`kernel_sect.rs` for `ltsect.dtx` and so on).
   - It reads and writes kernel state through typed accessors over `eqtb` slots (§4).
   - Its constants come from generated tables.
   - Cite the macro in comments (`` `\@startsection` ``).
4. **Respect overrides.** If the document or a package has redefined the command, or any internal macro it would call, the primitive yields to that macro (§5).
5. **Prove it:**
   - `RT_NATIVE=<cmd>` against the macro run: `RT_BOXES` identical, state dump identical (`RT_STATE`, P0), written files identical;
   - then `parity.py zero --pixels` on every corpus document that uses it.
6. **Measure:** `bench_corpus.py` (time, CPU, RSS) and the count of format-macro expansions (the progress metric, target 0).
7. **Record:** update this tracker's row, and write a dated note per module.

### 3. Inventory: what is code and what is not
The 85 corpus documents load 243 distinct `.sty`/`.cls` files. They fall into five classes; only (b), (c) and (d) are port work.
- **(a) Macro-programming libraries, subsumed by native code; zero port.**
  - etoolbox (internals), keyval, kvoptions, kvsetkeys, ltxcmds, intcalc, bigintcalc, bitset, pdftexcmds, infwarerr, stringenc, pdfescape;
  - iftex, ifpdf, ifluatex, ifxetex, ifvtex, etexcmds, letltxmacro, trimspaces, environ, auxhook, atveryend, refcount, hycolor, gettitlestring;
  - expl3 and xparse internals.
  - About 40% of the list. They exist only to implement packages in macros.
- **(b) Programming commands documents call directly; port as primitives.**
  - `\ifthenelse` (ifthen); calc expressions in `\setlength`;
  - etoolbox's user commands (`\newtoggle`, `\AtBeginEnvironment`, `\patchcmd`, `\ifdefempty`);
  - `\NewDocumentCommand` (xparse, now in the kernel);
  - xkeyval/pgfkeys key handling where documents use it.
- **(c) Feature packages; port.** By corpus frequency: hyperref (with nameref, url, bookmark), multicol, color/xcolor, array, amsmath (amstext, amsbsy, amsopn, amsgen), textcomp, graphics/graphicx, calc, booktabs, microtype, amssymb/amsfonts, fancyvrb, enumitem, colortbl, verbatim, varioref, longtable, csquotes, listings, footmisc, tabularx, float, caption, geometry, …
- **(d) Classes; port.** article (75 of 85), ltxdoc and doc (54 and 49, the `.dtx` manuals), ltxguide, scrartcl (KOMA), book, report, amsart, IEEEtran, exam, l3doc.
- **(e) Font packages; data only.** lmodern, mathptmx, mathpazo, helvet, courier, fourier, libertine, bera, tgpagella/tgheros, fontenc, textcomp's TS1 encoding. These become NFSS tables and metrics generated by `rushtex-gen`; no behaviour.

**Port order comes from real usage, not the corpus.** 52 of the 85 documents are `.dtx` manuals, so doc/ltxdoc and hyperref (pulled in by hypdoc) are overweighted. P0 measures package frequency over a sample of real documents (arXiv sources). The 85 remain the acceptance gate.

### 4. Kernel state: `eqtb` slots with typed accessors, not a parallel struct
- Packages and documents read kernel state by name: `\value{section}`, `\the\textwidth`, `\c@page`, `\@tempdima`, `\thesection`.
- Native code reads and writes the same `eqtb` slots through build-time `const` slot indices and typed wrappers (for example `kernel::counter(SECTION)`, `kernel::len(TEXTWIDTH)`).
- There is one store and no bridging. Group restore (`\begingroup`) works for native and macro code alike.
- This satisfies the spec's typed-state rule: flat typed arrays, integer IDs.

### 5. Overridable internals (backward compatibility)
- Documents and packages redefine kernel internals routinely:
  - output formatting: `\@makecaption`, `\@seccntformat`, `\@biblabel`, `\@maketitle`, `\@evenhead`/`\@oddhead`, `\@listi`, `\@begintheorem`, `\@footnotetext`, `\@mkboth`;
  - labels and numbering: `\thesection`, `\theequation`, `\labelenumi`, `\labelitemii`, `\arraystretch` (18 times in the corpus), `\baselinestretch`;
  - name strings: `\figurename`, `\bibname`, `\refname`.
- Rule: every internal the kernel calls keeps its control-sequence name.
  - At format time the name is bound to its native meaning.
  - A native primitive calls an internal through its current meaning: still native means a direct Rust call; redefined means the macro is expanded.
  - The check is one comparison of the meaning word, no string work.
- Patching commands (`\patchcmd`, `\pretocmd`, `\apptocmd`, `\xpatchcmd`) on a native command are recorded as hooks before or after it where the patch is a pure prepend or append. Otherwise they report a clear unsupported-patch error naming the command.
- The list of overridable internals is generated from the corpus and the arXiv sample (P0) and kept in `kernel_overrides.rs`.

### 6. Phases
Each phase is closed by the gates: `cargo test`, clippy, `zero --pixels` on all corpus documents, `bench_corpus.py`, the format-expansion count, and a dated note.

- **P0. Tooling and a sound oracle (N0).**
  - `engine_job` reruns only while the auxiliary files change (pdflatex's rule); replaced token lists are freed (`delete_token_ref`, `flush_list`).
  - `RT_PROFILE`: expansions and time per format macro.
  - `RT_STATE`: counters, lengths and fonts dumped at each `\shipout`.
  - `RT_NATIVE=cmd,…`: a per-command switch.
  - `engine_boxdiff.py` for engine-against-engine box diffs.
  - `rushtex-gen` skeleton.
  - An arXiv package-frequency sample. The pinned LaTeX version is a dated TeX Live 2026 snapshot (`progress.md` A1).
- **P1. Fonts and NFSS as generated tables.**
  - `ltfssbas`, `ltfssini`, `ltfssdcl`, `ltfsstrc`, `ltfntcmd`, `ltfssaxes`, `ltoutenc`, `lttextcomp`.
  - `.fd` files, TFM metrics, encodings and size functions become static tables.
  - `\selectfont`, `\fontsize`, `\usefont`, `\textbf`/`\textit`/`\emph`, `\normalsize` and the size commands become primitives.
  - First because every document runs them constantly.
- **P2. Kernel core:** `ltcounts`, `ltlength`, `ltspace`, `ltboxes`, `ltpara`, `ltlists`, `ltsect`, `ltxref`, `ltmiscen`, `ltdefns` (`\newcommand` and kin), `ltcmd` (`\NewDocumentCommand`), `ltkeys`, `lthooks`, `ltfiles` (`\input`, `\include`), `ltbibl`, `ltidxglo`, `ltthm`, `ltpage`, `ltcntrl`. Target: small2e, sample2e and lppl at zero format expansions and zero shift.
- **P3. Output routine and floats:** `ltoutput` (6,674 lines), `ltfloat`, `ltmarks`, `ltshipout`.
  - The hardest part, driven by pdfTeX's `\tracingpages` and the oracle's box traces.
  - Float placement is an explicit state machine (queues, `\@toplist`/`\@botlist`, the placement parameters).
- **P4. Tables, math and pictures:** `lttab` (`tabular`, `array`), `ltmath`, `ltpictur`.
- **P5. Classes:** article, report, book (and their `.clo`); then ltxdoc/doc for the `.dtx` corpus; then amsart, scrartcl, IEEEtran, exam.
- **P6. Packages** in measured real-usage order, class (b) and (c) only; each one oracle-diffed.
- **P7. Zero dependency.**
  - `latex.ltx` is no longer loaded: the native "format" is generated tables plus native bindings.
  - Fonts, metrics and hyphenation are embedded; the CLI default is the native path.
  - The interpreter keeps only user-macro expansion; `engine_host` and the format build become dev-only.
- **P8. Real-time incremental:**
  - block checkpoints, which are small because the token store holds only user macros;
  - caching by (content, state) hash; early cutoff; repagination until the breaks match;
  - shipping only changed pages.
- **The old native pipeline** (`src/layout`, `src/math`, `src/tables`): frozen now, with no new features. `--native` keeps working until P7, then switches to the new path and the old code is removed module by module.

### 7. Kernel tracker
Status values: `macro` (interpreted), `native` (written), `verified` (node-diff and zero-shift proven). Lines are `.dtx` lines; a phase number in the status means the module is planned for that phase and not yet written.

| Kernel file | Lines | Phase | Status | Proving documents |
|---|---|---|---|---|
| ltfssbas, ltfssini, ltfssdcl, ltfsstrc, ltfssaxes, ltfsscmp | 2550+2459+2236+2054+4544+378 | P1 | macro; `\get@external@font` verified (`2026-10-22-p0-kernel-native.md`) | all |
| ltfntcmd, ltoutenc, lttextcomp | 806+3861+2778 | P1 | macro | all |
| ltcounts, ltlength, ltspace, ltboxes, ltpara, ltpar | 1028+184+1543+1728+1354+189 | P2 | macro | small2e |
| ltdefns, ltcmd, ltkeys, lthooks, ltcmdhooks | 2837+6817+740+8140+1858 | P2 | macro | all |
| ltlists, ltsect, ltxref, ltmiscen, ltthm, ltbibl, ltidxglo | 1227+1103+709+1899+334+481+218 | P2 | macro | sample2e, lppl |
| ltfiles, ltfilehook, ltclass, ltpage, ltcntrl, ltpageno | 1842+1841+4171+438+326+101 | P2 | macro | all |
| ltoutput, ltfloat, ltmarks, ltshipout | 6674+1783+2409+2272 | P3 | macro | float, afterpage, placeins-doc |
| lttab, ltmath, ltpictur | 1837+1483+2061 | P4 | macro | array, testmath |
| ltplain, ltalloc, lthyphen, ltlogos, ltvers, ltexpl, ltfinal, lterror, ltdirchk, ltmeta, ltproperties, ltsockets, lttagging, lttemplates, ltluatex | rest | as reached | macro or not needed (luatex, dirchk) | — |

### 8. Package tracker
Rows are added as P0's real-usage sample ranks them. Classes (a) and (e) are listed once and never ported as code.

| Package | Class | Corpus documents | Phase | Status |
|---|---|---|---|---|
| article.cls (+ size10/11/12.clo) | d | 75 | P5 | macro |
| hyperref (+ nameref, url, bookmark) | c | 67 | P6 | macro |
| multicol | c | 56 | P6 | macro |
| doc / ltxdoc / hypdoc | d | 54 | P5 | macro |
| color / xcolor | c | 52 / 14 | P6 | macro |
| fontenc, textcomp, lmodern | e | 25 / 16 / 12 | P1 (data) | macro |
| array | c | 18 | P4 | macro |
| amsmath family | c | 17 | P6 | macro |
| graphics / graphicx | c | 13 | P6 | macro |
| ifthen, calc | b | 13 / 13 | P2 | macro |
| booktabs | c | 10 | P6 | macro |
| microtype | c | 9 | P6 | macro |
| amssymb / amsfonts | e | 9 | P1 (data) | macro |
| fancyvrb, enumitem, colortbl, verbatim, varioref, longtable, csquotes, listings, footmisc, tabularx, float, caption, geometry, … | c | 3–8 each | P6 | macro |

## Rejected
- **Starting from the old native pipeline (the spec's former N1).** Making 56k lines of approximate layout exact means rewriting it against the core anyway. Porting inside the engine gives an in-process oracle on identical state, so every step is diffable and the corpus never breaks.
- **Kernel state in a parallel typed struct.** Every `\value`, `\the\textwidth` or `\c@page` read would need a bridge. Typed accessors over `eqtb` slots give both type safety and compatibility.
- **Porting class (a) libraries.** Native code replaces what they exist for.
- **Ordering ports by corpus frequency alone.** The corpus is 61% package manuals.
- **Removing TeX's expansion semantics.** Documents' own macros would break; that is backward compatibility lost.

## Measured
Inventory, 2026-10-22:
- The corpus loads 243 distinct `.sty`/`.cls` files.
- The `latex-source/base` kernel `lt*.dtx` files total 98,635 lines, documentation included. The largest are `lthooks` (8,140), `ltcmd` (6,817), `ltoutput` (6,674), `ltfssaxes` (4,544), `ltclass` (4,171) and `ltoutenc` (3,861).
- The baseline oracle benchmark is in `2026-10-22-corpus-benchmark.md`.

## Open
- **Unported packages in a document: decided 2026-10-22.** A release gives a clear error naming the package; the oracle fallback exists only in dev builds (`native-architecture.md`).
- **The pinned LaTeX version: decided 2026-10-22.** A dated TeX Live 2026 snapshot with a checksummed manifest is pinned for references, generated tables and CI. The user chose 2026 over 2023: the zero-shift proof is against pdfTeX 1.40.29 of TeX Live 2026, and an old kernel would be ported only to be rewritten. It is upgraded yearly as one deliberate step (`progress.md` A1).
- **expl3:** which l3 functions documents call directly (via `\ExplSyntaxOn` in preambles) and must therefore be primitives.
