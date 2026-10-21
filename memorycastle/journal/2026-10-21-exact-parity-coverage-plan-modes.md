# Exact zero-shift parity, coverage-driven corpus, legacy and modern modes
Author: abhi+claude | Date: 2026-10-21
Files: `scripts/parity/parity.py`, `scripts/parity/corpus/`, `src/engine/engine_ship.rs`, `src/backend/backend_ship.rs`, `src/backend/backend_pdftex.rs`, `src/backend/backend_optical.rs`, `src/engine/engine_pdfdoc.rs`, `src/engine/engine_whatsit.rs`, `src/engine/engine_vf.rs`, `src/lib.rs`, `memorycastle/journal/tex-weaknesses-roadmap.md`

## Context
Twelve of the fifteen CTAN corpus documents reached 100% strict parity through the engine:
- small2e, sample2e, lppl, modguide, cfgguide
- testmath, fntguide, usrguide
- short-math-guide, amsthdoc, clsguide, bare_jrnl

Together that is 222 pages and 326,746 glyphs (see `2026-10-21-engine-ctan-corpus.md`).

The user then asked four things:
- whether the problem count will shrink over time;
- what work remains for a truly identical export;
- whether the 0.1pt tolerance is acceptable;
- how to grow the corpus.

The user also stated their product plan: two modes, legacy and modern.

## Decision

### Engine convergence (why the problem count falls)
- The engine is a translation of a finite specification: tex.web, etex.ch and pdftex.web. That is about 325 TeX, 70 e-TeX and 150 pdfTeX primitives.
- LaTeX, expl3 and the packages are macros on top of these primitives. A fixed primitive rule fixes every document that uses it.
- Evidence from this session:
  - amsthdoc and clsguide passed with no specific work.
  - Each other document needed 1–3 general rule fixes (§394, `\lastnodetype`, virtual fonts).
- Fixes are guarded by the probe suites (`scripts/engine_suites.py`) and the parity corpus, so the count only falls.
- What does not converge by itself:
  - rare primitives not yet ported;
  - PDF output features;
  - missing files and fonts, a packaging problem (roadmap item 17);
  - XeTeX/LuaTeX documents (out of scope);
  - the native pipeline, which is per-feature code.
- Track coverage per primitive, not only per document.

### Parity means exact: zero tolerance
The 0.1pt glyph tolerance in `parity.py` becomes a diagnostic only. As a gate it is technical debt:
- TeX's arithmetic is integer scaled points with written rounding rules (`xn_over_d`, `store_scaled`, §625 glue rounding). Any difference, even 1sp, is a ported rule that is wrong, never noise.
- Small drifts grow: a 0.05pt glue error can tip a line or page break somewhere else.
- A tolerance lets regressions through silently.
- Incremental compile (roadmap item 4) needs bit-identical reuse.

Zero shift is reachable mathematically:
- The layout is integer sp. The only float is `glue_set`, an IEEE double in both pdfTeX and our `f64`.
  - The same operations in the same order give bit-identical results.
  - Remote edge: clang may fuse multiply-add (FMA) on arm64 and change the last bit of a double. If that ever crosses an sp boundary, reproduce it deliberately.
- PDF numbers come from pdfTeX's integer routines (`divide_scaled`, `pdf_print_real`, `\pdfdecimaldigits`), which `TextState` in `backend_pdftex.rs` already follows.
- The catch is pixels: same positions still need the same glyph outlines.
  - pdfTeX embeds the Type 1 `.pfb` programs. We embed our own Type1C/CFF conversions for CM and AMS (`backend_optical.rs`).
  - A renderer may draw them differently (hints, flex, coordinate rounding). The options: either prove the conversion lossless, or embed the same Type 1 data subset as pdfTeX does. The second costs binary size unless the fonts move to packs.

`parity.py` gains three checks, all at zero tolerance. The 0.1pt score stays beside them as a "how far off" number.
1. **Exact numeric:** every glyph's font, size, code and position, and every rule's position and size, equal to pdfTeX's content-stream numbers in PDF units. Compare the content-stream numbers directly, not floats extracted through pymupdf.
2. **Pixel:** both PDFs rendered at 300 dpi by the same renderer give zero differing pixels. This also tests the CFF-against-Type1 question.
3. **Structure:** the same link annotations, destinations, outlines, `/Info` and catalog entries.

Byte-identical PDF files are not a goal. pdfTeX's own output varies (creation date, `/ID`, object order), and matching it would mean copying pdfTeX's internals instead of our own design.

### What remains for an identical export (as of this note)
- **Documents still failing:**
  - amsldoc: an engine error "Missing number" at `\usepackage{hyperref}`, l.252.
  - encguide: the `wnr10` TFM is not found. Check whether the reference build finds it.
  - first-latex-doc: `latex-first.png` is not found. Needs `\pdfximage`.
- **PDF output in the engine path, scanned but not yet written:**
  - colour stacks, `\pdfliteral` and `\special`;
  - `\pdfsetmatrix`, `\pdfsave`, `\pdfrestore`;
  - link annotations;
  - `\pdfdest`, `\pdfoutline`;
  - `\pdfinfo`, `\pdfcatalog`, `\pdfobj`;
  - `\pdfpageattr`, `\pdfpagesattr`, `\pdfpageresources`.
- **Missing outright:**
  - `\pdfximage`, `\pdfrefximage`, `\pdfxform` (images, forms);
  - `\pdfsavepos`/`\pdflastxpos` (they return 0);
  - font subsetting with pdfTeX's six-letter tag;
  - specials inside VF packets.
- **Engine:** `\valign` (E4 e), the class snapshot (E4 g), a primitive coverage table, and the arena leak.
- **Delivery:** the `rushtex` binary still runs the native pipeline. It needs packs (roadmap 17), the class snapshot, a WASM check, then the user-perspective benchmark.
- **Order agreed:**
  1. the exact numeric check, then the pixel check;
  2. amsldoc;
  3. colour and matrices;
  4. links, destinations, outlines and info;
  5. images (encguide, first-latex-doc);
  6. subsetting;
  7. the coverage table and a larger corpus;
  8. the binary switch with packs.

### Corpus growth: coverage, not quantity
New documents are chosen for the features they exercise. A document that passes proves a rule; a failing one names the next rule.

The user's first priority, two groups:
1. **Text articles:**
   - plain articles with images (graphicx with PNG, JPEG and PDF);
   - two-column layout (`twocolumn`, IEEEtran or article two-column, `multicol`);
   - minipages, floats and captions (figure and table placement, `subcaption`, `wrapfig`);
   - lists.
2. **Mathematical documents:** amsart and amsmath-heavy papers, theorem environments, equation numbering and cross-references, and math font setups (lmodern, newtx, mathptmx).

Later groups:
- classes: report, book, memoir, KOMA, llncs, elsarticle, revtex, beamer;
- tables: longtable, tabularx, multirow, siunitx;
- bibliography and index, shipped with their pre-generated `.bbl`/`.ind` so BibTeX/Biber/makeindex are not under test;
- fonts and encodings: T1 with lmodern, VF-based families;
- real arXiv papers with pdfLaTeX builds.

TikZ and pgf stay a separate later milestone.

Mechanics:
- Target about 40–50 documents in total.
- Each new document goes in its own directory (untrusted download), with the pdfLaTeX reference PDF.
- Rank blockers with `parity.py gaps --all` and fix them from tex.web/latex.ltx as before.

### Product plan: legacy mode and modern mode (the user's plan)
- **Legacy mode:** full LaTeX backward compatibility. This is the current work: the engine runs latex.ltx and the packages, with pdfLaTeX-identical output.
- **Modern mode:** a simplified language with no preamble required. Work on it starts once legacy is solid on the mainstream groups above. Legacy does not need to be at 100% everywhere first.
- **Mode selection on the first line:**
  - an explicit marker such as `%!rushtex modern`;
  - no marker but `\documentclass` present: legacy, so old `.tex` files work unchanged;
  - neither: modern.
- **Architecture:** both modes share one engine: boxes, line breaker, page builder, fonts, virtual fonts and PDF shipout.
  - Modern mode is a different front end (syntax plus a prebuilt default setup loaded from a snapshot), not a second typesetter.
  - Every legacy fix therefore also improves modern mode, and modern documents get TeX-quality output.
  - The default setup (fonts, page geometry, math) comes from the class snapshot (E4 g), so there is no preamble cost at startup.

## Rejected
- Keeping 0.1pt as the pass/fail bar: it hides real rule bugs and lets regressions through. It stays only as a secondary number.
- Byte-identical PDF files as a target: pdfTeX itself is not reproducible byte for byte.
- Growing the corpus by count: more documents of an already-passing kind add little.
- Holding modern mode until legacy is at 100% everywhere.
- A separate typesetter for modern mode: the two would drift apart and duplicate every fix.

## Measured
Strict 0.1pt parity through the engine: 12/15 documents at 100%, 222 pages, 326,746 glyphs, 0 mismatches. Exact zero-tolerance parity has not been measured yet.

## Open
- Does our CFF conversion of the CM/AMS Type 1 fonts render pixel-identically to pdfTeX's embedded `.pfb`? The pixel check will answer it. If not, choose between proving the conversion lossless and embedding Type 1 subsets.
- The exact marker syntax for modern mode, and the default setup it loads.
- Which concrete documents to download for the text-article and math groups. Pick by features, with pdfLaTeX references.
