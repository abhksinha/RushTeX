# TeX input rules for blanks, arguments and kerns
Author: abhi+claude | Date: 2026-10-19
Files: `src/frontend/frontend_parser.rs`, `src/layout/layout_engine.rs`, `src/main.rs`

## Context
After footnotes worked, `sample2e` still differed from pdfLaTeX by line breaks and up to 3pt in x. Each difference was a TeX input or spacing rule the parser did not follow.

## Decision
- `skip_comment` skips the next line's leading blanks (state N). After `\ `, blanks and one end of line are skipped (state S), unless a blank line follows.
- `brace_arg` matches nested braces. `skip_arg_space` skips one end of line before an argument. `macro_arg` takes a single token (`\setlength\leftmargini{2em}`).
- An empty kern word ends a shaped glyph run: `\,` and the kerns of `\dots` were lost inside runs.

## Measured
- `sample2e`: all three pages have the same lines as pdfLaTeX. Every glyph is within 0.07pt except the footnote mark (see `memorycastle/journal/2026-10-19-footnote-insertions.md`). rushtex takes 0.0055 s against pdfLaTeX's 0.116 s.
- `\,` and `\ldots`: exact. Italic corrections are in `memorycastle/journal/2026-10-19-text-font-command-corrections.md`.
- Tests 234 pass with the 4 known NewCM failures; clippy clean.

## Open
- `rushtex --expand` prints the expander's output, for debugging the samples.
