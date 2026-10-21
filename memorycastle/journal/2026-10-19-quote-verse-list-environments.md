# quote, quotation and verse as latex.ltx lists; per-kind list depth
Author: abhi+claude | Date: 2026-10-19
Files: `src/layout/layout_engine.rs`, `src/frontend/frontend_ast.rs`, `src/frontend/frontend_parser.rs`

## Context
`sample2e` uses `quote`-like environments, `verse`, and an enumerate inside an itemize. `quote` had an ad-hoc 25pt indent, `verse` was unsupported, and the nested enumerate was labelled (a) instead of 1.

## Decision
`QuoteData` has a `QuoteKind` and `in_vmode`. `layout_quote` follows latex.ltx:
- **All three** are a one-item `\list` with `\rightmargin\leftmargin`, the level's `\topsep`/`\partopsep`/`\parsep`, and `\@beginparpenalty`/`\@endparpenalty` -51.
- **quotation** uses 1.5em `\itemindent`/`\listparindent` and `\parsep` 0pt plus 1pt.
- **verse** adds 1.5em to `\leftmargin` with -1.5em indents. Each `\\` ends a paragraph, as `\@centercr` does; its `\addvspace{-\parskip}` cancels the next `\parsep`.

List labels follow `\@enumdepth`/`\@itemdepth` (`enum_depth`, `item_depth`), not the overall nesting.

## Measured
- `sample2e` verse, nested lists and every list label: within 0.01pt of pdfLaTeX.
