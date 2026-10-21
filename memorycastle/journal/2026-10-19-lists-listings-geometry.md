# LaTeX list, verbatim and lstlisting geometry
Author: abhi+claude | Date: 2026-10-19
Files: `src/layout/layout_engine.rs`, `src/layout/layout_listing.rs`, `src/backend/backend_pdf.rs`, `src/config/config_mod.rs`, `src/lib.rs`

## Context
The Computer Modern default exposed wrong `lstlisting` defaults and list spacing; the 100k stress document was 10 pages longer than pdfLaTeX's. Companion notes: `memorycastle/journal/2026-10-19-latex-commands-lists-listings.md`, `memorycastle/journal/2026-10-19-vertical-mode-page-builder.md`.

## Decision
- Lists follow `\list`/`\@item`/`\endtrivlist` with the size10/11/12.clo values (`list_level_params`): topsep, partopsep, parsep, itemsep as glue with stretch and shrink, `\@itempenalty` and `\@beginparpenalty` -51, labels right-aligned in `\labelwidth`, `Layouter::addvspace` at both ends.
- `verbatim` is the `\@verbatim` trivlist. `lstlisting` uses listings' defaults: document font, `columns=fixed`, `\medskipamount` (6pt plus 2pt minus 2pt at every size) above and below, `\penalty-50`, a `\strut` in every line, no `\parskip` before it, `\lastskip` 0 after it.

## Rejected
- Scaling `\medskipamount` with the class size: size11.clo and size12.clo keep 6pt; it put listings 0.57pt low at 11pt.
- Merging a heading's skip with the listing's below skip: pdfLaTeX adds both (measured).

## Measured
- Listing then heading and listing then verbatim at 10/11/12pt with `\parskip` 5pt: within 0.02pt of pdfLaTeX. Lists exact at 10 and 11pt, x within 0.07pt at 12pt.

## Open
- Item label style follows the overall list depth, not the depth per list kind.
