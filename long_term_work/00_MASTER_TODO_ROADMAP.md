# RushTeX Long-Term TeX Parity: Master To-Do Roadmap

This master document tracks implementation progress across all **5 Phases (26 Steps)** of the long-term TeX compatibility roadmap, derived from Victor Eijkhout's *TeX by Topic: A Systematizer's Reference*.

---

## Master Progress Dashboard

| Phase | Description | Reference Chapters | Steps | Completed | Status |
| :--- | :--- | :--- | :---: | :---: | :---: |
| **Phase 1** | [Paragraph Construction & Knuth-Plass Line Breaking](#phase-1-paragraph-construction--knuth-plass-line-breaking) | Ch 15–20 | 7 | 7 / 7 | **100% (COMPLETE)** |
| **Phase 2** | [Page Construction, Gallies & Vertical Breaks](#phase-2-page-construction-gallies--vertical-breaks) | Ch 26–29 | 6 | 0 / 6 | 0% (PENDING) |
| **Phase 3** | [Macro Expansion, Token Lists & Grouping](#phase-3-macro-expansion-token-lists--grouping) | Ch 10–14 | 5 | 0 / 5 | 0% (PENDING) |
| **Phase 4** | [Alignment & Tabular Gallies](#phase-4-alignment--tabular-gallies) | Ch 25 | 4 | 0 / 4 | 0% (PENDING) |
| **Phase 5** | [Registers, Arithmetic & File I/O](#phase-5-registers-arithmetic--file-io) | Ch 7–9, 30–31 | 4 | 0 / 4 | 0% (PENDING) |
| **Total** | **Comprehensive TeX Parity Roadmap** | **Full System** | **26** | **7 / 26** | **26.9%** |

*(Note: Mathematical typesetting from Chapters 21–24 is already fully implemented and verified via the `mlist.c` / Modern Math architecture).*

---

## Phase 1: Paragraph Construction & Knuth-Plass Line Breaking

Detailed specification and test records: [`long_term_work/01_PARAGRAPH_PARITY_AUDIT.md`](file:///home/triton/Coding/rushtex/long_term_work/01_PARAGRAPH_PARITY_AUDIT.md)

- [x] **Step 1A: Space Factor Table (`\sfcode`) & Punctuation Spacing** *(TeX by Topic Ch 20)*
  - Zero-cost ASCII lookup table for character space factors ($A..Z = 999$, $a..z = 1000$, punctuation $1250 \dots 3000$).
  - Dynamic inter-word stretch/shrink scaling and `\frenchspacing` reset.
- [x] **Step 1B: Exact Demerit & Fitness Class Invariants** *(TeX by Topic Ch 19.1)*
  - 4 distinct Knuth-Plass line fitness classes (Tight, Decent, Loose, Very Loose).
  - Edge penalties: `\adjdemerits`, `\doublehyphendemerits`, `\finalhyphendemerits`.
- [x] **Step 1C: Knuth-Liang Discretionary Hyphenation Integration** *(TeX by Topic Ch 19.3–19.4)*
  - Embedded 4,461 canonical Plain TeX US English hyphenation pattern engine (`layout_hyphen.rs`).
  - Discretionary soft hyphens `\-`, compound break penalties, and trailing hyphen glyph emission.
- [x] **Step 1D: `\parshape` Multi-Line Matrix Support** *(TeX by Topic Ch 18.2.2 & 18.3)*
  - Arbitrary $N$-line indentation and measure specifications with repetition for line $k \ge N$.
  - Precedence over `\hangindent` / `\hangafter` and exact one-shot lifecycle clearance.
- [x] **Step 1E: Three-Pass Line Breaking (`\pretolerance`, `\tolerance`, `\emergencystretch`)** *(TeX by Topic Ch 19.2.1)*
  - Pass 1 (unhyphenated under `pretolerance`), Pass 2 (hyphenated under `tolerance`), Pass 3 (`emergencystretch`).
  - Supported `\sloppy` and `\fussy` modes in both linear pass and optimal DP breaker.
- [x] **Step 1F: Paragraph Stretch Tuning & Line Count Constraints (`\looseness`, `\leftskip`, `\rightskip`)** *(TeX by Topic Ch 18.2, 19.2.2)*
  - Target line count modification via DP demerit biasing (`-50000.0 * looseness`).
  - Persistent dimension margins `\leftskip` and `\rightskip` across all line measures and layout items.
- [x] **Step 1G: Paragraph Termination & Ragged/Justified Tuning (`\parfillskip`, `\prevgraf`, `\lastlinefit`)** *(TeX by Topic Ch 17.1, 19.1.4)*
  - Terminal glue `\parfillskip` with zero-stretch full-justification enforcement.
  - Active line counter `\prevgraf` propagation to display math vertical spacing.
  - Short final line tuning via `\lastlinefit`.

---

## Phase 2: Page Construction, Gallies & Vertical Breaks

Reference chapters: *TeX by Topic* Chapters 26–29 (`Page Shape`, `Page Breaking`, `Output Routines`, `Insertions`).
Detailed specification: *To be created at `long_term_work/02_PAGE_CONSTRUCTION_AUDIT.md` upon phase kickoff.*

- [ ] **Step 2A: Vertical Penalty Modeling & Cost-Driven Breaking** *(TeX by Topic Ch 27.4)*
  - Implement penalty evaluation for legal page breaks: `\interlinepenalty` (default 0), `\clubpenalty` (default 150), `\widowpenalty` (default 150), and `\displaywidowpenalty` (default 50).
  - Explicit break commands: `\pagebreak`, `\nopagebreak`, `\break`, `\nobreak`.
- [ ] **Step 2B: `\vsplit` Engine** *(TeX by Topic Ch 27.5)*
  - Support splitting a vertical box (`\vbox`) into two boxes where the first has an exact specified target height.
  - Natural height computation, break search at glue/penalties, and trailing glue discard.
- [ ] **Step 2C: `\topskip`, Page Height/Depth Bookkeeping & Baseline Snapping** *(TeX by Topic Ch 26.2–26.3)*
  - Page head baseline alignment: ensure the baseline of the first box on a page sits exactly `\topskip` below the top margin.
  - Maintain page depth constraints (`\maxdepth`, default 4pt).
- [ ] **Step 2D: Output Routine Infrastructure (`\output`, `\box255`)** *(TeX by Topic Ch 28.1–28.2)*
  - Assembly of completed page contents into `\box255`.
  - Configurable page delivery pipeline with header/footer expansion (`\headline`, `\footline`).
- [ ] **Step 2E: Marks System (`\mark`, `\topmark`, `\firstmark`, `\botmark`)** *(TeX by Topic Ch 28.3)*
  - Tracking running header/footer marks across page splits.
  - Correct split-point resolution of `\topmark`, `\firstmark`, and `\botmark`.
- [ ] **Step 2F: Insertions Engine (`\insert`, Footnotes & Floats)** *(TeX by Topic Ch 29)*
  - Support insertion items contributing to separate vertical pools (footnotes, floating figures, and tables).
  - Page-capacity reduction bookkeeping per insertion class.

---

## Phase 3: Macro Expansion, Token Lists & Grouping

Reference chapters: *TeX by Topic* Chapters 10–14 (`Grouping`, `Macros`, `Expansion`, `Conditionals`, `Token Lists`).

- [ ] **Step 3A: Delimited Macro Parameter Text Parsing & Substitution** *(TeX by Topic Ch 11.5)*
  - Arbitrary parameter pattern matching (`\def\foo#1.#2/{...}`).
  - Undelimited vs. delimited arguments and brace preservation rules.
- [ ] **Step 3B: Token Lists & Hook Registers (`\toks`, `\everypar`, `\everymath`)** *(TeX by Topic Ch 14)*
  - Token list register allocation and concatenation.
  - Automatic injection of `\everypar` at paragraph starts and `\everymath` / `\everydisplay` at math shifts.
- [ ] **Step 3C: Controlled Expansion Primitives** *(TeX by Topic Ch 12.3–12.5)*
  - Expansion control: `\expandafter`, `\noexpand`, `\edef`, and `\the`.
  - Non-expansion sentinel `\relax` semantics.
- [ ] **Step 3D: Grouping State Stack (`\begingroup`, `\endgroup`)** *(TeX by Topic Ch 10)*
  - Scoped variable restoration stack (semi-simple groups vs. ordinary brace groups).
  - `\global` prefix support across assignments and macro definitions.
- [ ] **Step 3E: Conditionals Engine** *(TeX by Topic Ch 13)*
  - Evaluation of conditionals: `\if`, `\ifx`, `\ifcat`, `\ifnum`, `\ifdim`, `\ifvoid`, `\ifcase`.
  - Macro `\newif` Boolean switch support and nesting parser (`\else`, `\fi`).

---

## Phase 4: Alignment & Tabular Gallies

Reference chapters: *TeX by Topic* Chapter 25 (`Alignment`).

- [ ] **Step 4A: `\halign` Preamble Parsing & Column Template Expansion** *(TeX by Topic Ch 25.3)*
  - Preamble template specification with `&` separators, `\cr` terminators, and repeated tail templates.
- [ ] **Step 4B: `\tabskip` Glue Distribution & Multi-pass Column Width Measurement** *(TeX by Topic Ch 25.3.4)*
  - Column natural width resolution and inter-column `\tabskip` spacing.
- [ ] **Step 4C: Column Spanning & Omission (`\omit`, `\span`)** *(TeX by Topic Ch 25.4.2–25.4.3)*
  - Template override with `\omit` and multi-column width amalgamation with `\span`.
- [ ] **Step 4D: `\noalign` Inter-Row Material Insertion** *(TeX by Topic Ch 25.2.3)*
  - Insertion of horizontal rules and vertical skips between alignment rows without disturbing column alignments.

---

## Phase 5: Registers, Arithmetic & File I/O

Reference chapters: *TeX by Topic* Chapters 7–9, 30–31 (`Numbers`, `Dimensions and Glue`, `Rules and Leaders`, `File Input and Output`, `Allocation`).

- [ ] **Step 5A: Register Allocation Pools** *(TeX by Topic Ch 31)*
  - Dynamic register allocation table for `\count`, `\dimen`, `\skip`, `\muskip`, and `\box`.
- [ ] **Step 5B: TeX Arithmetic Operations** *(TeX by Topic Ch 7.5 & 8.1.5)*
  - Direct integer and fixed-point scaled arithmetic: `\advance`, `\multiply`, `\divide`.
- [ ] **Step 5C: Rule & Glue Leaders** *(TeX by Topic Ch 9.2)*
  - Leader repetitions: `\leaders`, `\cleaders`, and `\xleaders` for table of contents dots and rule fills.
- [ ] **Step 5D: Stream I/O & Whatsits** *(TeX by Topic Ch 30)*
  - File reading and writing streams (`\openin`, `\read`, `\openout`, `\write`, `\immediate`).
  - Auxiliary file emission and pipeline whatsits.
