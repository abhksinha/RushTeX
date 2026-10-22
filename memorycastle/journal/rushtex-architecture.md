# RushTeX architecture: how a document becomes a PDF, today and in the native pipeline (living reference)
Author: abhi+claude | Date: 2026-10-22
Files: `src/main.rs`, `src/lib.rs`, `src/engine/engine_snapshot.rs`, `src/engine/engine_lex.rs`, `src/engine/engine_token.rs`, `src/engine/engine_macro.rs`, `src/engine/engine_run.rs`, `src/engine/engine_cond.rs`, `src/engine/engine_chars.rs`, `src/engine/engine_font.rs`, `src/engine/engine_vf.rs`, `src/engine/engine_pack.rs`, `src/engine/engine_linebreak.rs`, `src/engine/engine_hyph.rs`, `src/engine/engine_microtype.rs`, `src/engine/engine_page.rs`, `src/engine/engine_noad.rs`, `src/engine/engine_mlist.rs`, `src/engine/engine_ship.rs`, `src/engine/engine_pdfdoc.rs`, `src/engine/engine_job.rs`, `src/data/data_tfm.rs`, `src/backend/backend_ship.rs`, `src/backend/backend_deflate.rs`

## Context
This is the user's top-level reference: a plain explanation of the whole compiler, written for reading and note-taking. The binding design details live in `native-architecture.md` (the native architecture), `progress.md` (the checklist) and `native-port-plan.md` (the port order). This note explains; those decide.

## Decision
RushTeX keeps TeX's typesetting rules exactly, because those rules decide where every glyph lands, and changes how the work is done. The rules kept are Knuth–Plass line breaking, Appendix G math and the page builder's costs. What changes:
- each piece of work is done once, as early as possible (build time, then load, then use);
- only what the document needs is loaded;
- data is borrowed in place, never copied;
- only what an edit changed is recomputed.

Steps 1, 2, 3 and 8 below disappear or become Rust code for a native document. Steps 4–7 and math keep their rules on faster data. The TeX engine proves each native piece identical in development. In releases it runs any document the native path cannot (a local `.sty`/`.cls`, an unported package, redefined LaTeX internals) as a whole-document fallback (`native-architecture.md` §2a).

## The pipeline in one picture
```
.tex file
  -> 1. start: load prepared LaTeX state
  -> 2. read: characters to tokens
  -> 3. expand: macros run (LaTeX's code)
  -> 4. boxes: characters to boxes (fonts, ligatures, kerns)        <- math branches off here
  -> 5. lines: paragraph to lines (Knuth-Plass, hyphenation)
  -> 6. pages: lines to pages (footnotes, floats, headers)
  -> 7. PDF: page content, fonts embedded
  -> 8. rerun when references changed
```
In one sentence: characters become tokens, tokens expand, expansion builds boxes, boxes break into lines, lines into pages, and pages become PDF.

## Today: the engine pipeline (the default command, pdfLaTeX's exact output)

### 1. Start: the format
- LaTeX's kernel (`latex.ltx`, about 22,000 lines) is slow to read every time. It is run once, and the resulting state is saved as the format in `~/.cache/rushtex`.
- The format holds macro definitions, registers, catcodes and the loaded fonts.
- Each compile loads this snapshot (`engine_snapshot`) instead of reading LaTeX again, like resuming a saved game instead of replaying it.

### 2. Reading: characters to tokens
- The tokenizer (`engine_lex`) reads characters. Each has a category code (catcode): letter, `\` escape, `{` group, `$` math, `%` comment, and so on.
- The output is tokens: a character with its category, or a command name such as `\section`.
- A token is a `u32`; names are interned once (`engine_token`), so no strings are copied.

### 3. Expansion: macros run
- A macro (`\section`, `\textbf`) is replaced by its definition's body (`engine_macro`).
- Conditionals (`\ifx`, `\ifnum`) pick a branch (`engine_cond`).
- All of LaTeX, its classes and its packages are TeX code that runs here, on our engine (`engine_run`, the `Machine`).
- Expansion ends at primitives: commands that do something real, such as adding a character, a space or a box, or setting a value.

### 4. Boxes: characters, fonts, ligatures and kerns
- A character in a font becomes a node with a width, height and depth (`engine_chars`).
- Metrics come from the generated font table `assets/tables/fonts.rtdb` (354 fonts, built from TFM files at build time); no font file is read at run time.
- When a font loads, its values are scaled once to its size into the font arena (`engine_font`).
- Ligatures (f + i to "fi") and kerns (A and V moved closer) come from each font's lig/kern program.
- A space becomes glue: a width that can stretch and shrink.
- The result is a horizontal list of nodes in one node arena with fixed owners (model M: no garbage collection).

### 5. Lines: Knuth–Plass line breaking
- At a paragraph's end, every possible break is considered: spaces, hyphens, penalties (`engine_linebreak`).
- Each candidate line gets a badness: how much its spaces had to stretch or shrink. Badness becomes demerits, with extra demerits for hyphens and for a tight line next to a loose one.
- It chooses the breaks with the lowest total demerits for the whole paragraph. This is TeX's quality: the paragraph is optimised as a whole, not line by line.
- **Hyphenation** (`engine_hyph`): when the paragraph cannot be set well without it, words are split at the points Liang's patterns allow ("hy-phen-ation"), and line breaking runs again.
- **Microtype** (`engine_microtype`), when a document loads it:
  - protrusion hangs punctuation slightly into the margin;
  - expansion stretches letters very slightly so word spacing is more even.
- Each line is packed (`hpack`, `engine_pack`) into a box of the text width.

### 6. Pages: the page builder
- Lines stack into a vertical list with glue between them (`engine_page`).
- When a page is full, the best page break is chosen by cost, the same idea as line breaking but one page at a time.
- Insertions (footnotes, floats) are collected and space is reserved for them.
- LaTeX's output routine (`\output`, LaTeX code) then adds the header, the footer, the page number and the floats into the final page box.

### 7. PDF: shipping the page
- The page box is walked (`engine_ship`). Each character becomes a PDF text instruction at an exact position, and each rule becomes a filled rectangle.
- Fonts are embedded as subsets: only the glyphs used, mostly Type 1 Computer Modern and Latin Modern.
  - ToUnicode maps make text copyable and searchable.
  - Virtual fonts (`engine_vf`) are fonts built out of other fonts.
  - PK bitmap fonts are the fallback.
- Links, bookmarks, images and colour come from `engine_pdfdoc`, `engine_image` and `engine_color`.
- Streams are compressed by our own DEFLATE encoder (`backend_deflate`).

### 8. Reruns: references
- `\ref` and `\cite` need values that only appear later in the document. LaTeX writes them to files such as `.aux`; we keep those files in memory.
- `engine_job` runs the document again until those files stop changing: at most 5 runs, usually 2.

### Math (a branch of step 4)
- `$...$` switches to math mode. The content becomes noads (`engine_noad`): atoms with a type (ordinary, large operator, binary operator, relation, opening, closing, punctuation, fraction, radical, accent).
- When the math ends, TeX's Appendix G rules (`engine_mlist`) turn noads into boxes:
  - **spacing** by the pair of atom types: a medium space around `+`, a thick one around `=`;
  - **superscripts and subscripts**, placed from the math fonts' parameters (axis height, raise and drop amounts, rule thickness);
  - **fractions, radicals, large operators and delimiters**, made from the font's larger variants, or assembled from pieces to any size;
  - **three styles**, text, script and scriptscript, each with its own fonts per family.
- The result is an ordinary horizontal list that goes back into line breaking.

### How exactness is proved
- The references are pdfLaTeX's output on pinned TeX Live 2026.
- The gate: 85 documents (2,137 pages) match pdfLaTeX at zero shift, pixel for pixel, and the leak audit finds no leak.
- CI repeats both on a clean machine with only the pinned snapshot.

## The native pipeline: what stays and what changes

TeX's rules decide what the output is, and they stay; changing one shifts the output. Our design decides how the work gets done, and that changes almost everywhere.

| Step | Today (engine) | Native pipeline | Rules |
|---|---|---|---|
| 1. Start | load the saved LaTeX state (format) | no format: LaTeX's settings are Rust code and build-time tables in the binary | step removed |
| 2. Reading | every character becomes a token with a catcode | the document is read once into commands and IDs; tokens only for user macros | mostly removed |
| 3. Expansion | LaTeX's macros interpreted token by token | each command (`\section`, `\textbf`, `\selectfont`) is a Rust function; class and package parameters come from tables | replaced by native code |
| 4. Boxes and fonts | font records, the lig/kern program walked per pair | one 16-byte record per character, a pair table (one probe per pair), expanded fonts scaled once (`native-architecture.md` §4a) | same rules, new data |
| 5. Lines | Knuth–Plass and hyphenation patterns | same algorithm; buffers reused, widths contiguous, a gate skips words that cannot hyphenate, the pattern trie built at build time | same rules, less work |
| 6. Pages | page builder, then LaTeX's `\output` macros | same page-break rules; the output routine (headers, footers, floats) in Rust | rules same, `\output` native |
| 7. PDF | font subsets rebuilt every compile | font programs split per glyph at build time, subsets cached per used-glyph set, pages streamed | same output, less work |
| 8. Reruns | run again until references settle (at most 5) | no reruns and no `.aux`: references in memory; only the pages that cite a changed reference are laid out again | step removed |
| Math | noads, then Appendix G | same Appendix G rules; math font parameters from tables | same rules, less work |

### What is entirely new: per-block incremental compile
- The document is divided into blocks (paragraphs, sections).
- Each block records its input state (counters, the current font, the float queue, the references it read) and its output.
- After an edit, only the changed blocks and the blocks that depend on them are redone: a one-word edit lays out one paragraph again, not the whole document.
- Everything is designed for this from the start: append-only arenas (a checkpoint is a length), IDs instead of copies, font numbers stable across recompiles.

### Native first, engine fallback (decided 2026-10-22)
- **Native.** The kernel plus the most-used packages, about 100–150 `.sty`/`.cls` files by the corpus, chosen in the end by measured usage. These documents get native speed, no TeX Live and incremental compile.
- **Fallback.** Anything else runs as a whole document on the engine: exact output, engine speed (1.4–3× faster than pdflatex today), no incremental compile, and TeX files needed at run time.
- **Result:** backward compatibility. Every document pdfLaTeX compiles also compiles in RushTeX. The fallback triggers, measured, decide what to port next.

### How things are stored (`native-architecture.md` §3a)
- **Bulk data** (fonts, glyphs, hyphenation, NFSS, the package registry): binary tables embedded in the binary, read in place.
- **Values the code names** (class parameters, option bits, command IDs): generated Rust constants.
- **Behaviour:** one Rust module per package, called through a static table of functions.
- **Joined by IDs;** all changing state in small arrays by ID, which makes checkpoints cheap.

### The five pillars (binding on every feature)
1. **Native kernel:** LaTeX's kernel and packages written in Rust over the exact typesetting core.
2. **Build-time tables, self-checked:** fonts, metrics, NFSS, class parameters and hyphenation are generated by `rushtex-gen`, which checks each entry against its source file.
3. **Demand loading:** a pre-scan of the document decides what to load; nothing else is touched.
4. **Per-block incremental compile:** designed in, never added later.
5. **Zero-shift proof per ported feature:** each native piece matches the engine (the oracle) exactly before it replaces it.

### The design rule (`native-architecture.md` §5a)
A design that does less computation, is simpler and moves less data per operation wins, and is chosen without benchmarking the alternatives. When saving computation and saving data movement conflict, the design that touches fewer cache lines per operation wins. The gates still prove every change exact.

## Rejected
- **Approach B, the engine plus native shortcuts for hot macros, as the product.** Every other macro still pays for interpretation, its checkpoints are megabytes, and dependencies cannot be tracked exactly. The engine is kept as the oracle and as the whole-document fallback, never mixed with native code in one document.
- **Changing TeX's rules for speed:** any change to line breaking, Appendix G or page costs shifts the output. Speed comes from data and timing, never from different rules.

## Open
- This note is updated whenever a step's design changes; the details stay in `native-architecture.md`.
- The order of work: D2 (`.fd`/NFSS tables and the package registry), D2b (fonts by §4a), then the native port P1–P8 (see `progress.md`).
