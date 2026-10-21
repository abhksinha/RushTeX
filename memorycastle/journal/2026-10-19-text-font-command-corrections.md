# Text font commands: `\check@icl`, `\check@icr`, OT1 braces and `\maketitle` defaults
Author: abhi+claude | Date: 2026-10-19
Files: `src/frontend/frontend_ast.rs`, `src/frontend/frontend_parser.rs`, `src/layout/layout_engine.rs`, `src/expand/expand_mod.rs`

## Context
`\emph` inside `em` text was 0.77pt narrow in `sample2e`. `\{` and `\}` printed OT1 dashes. A `\maketitle` without `\date` printed no date. Companion note: `memorycastle/journal/2026-10-19-tex-input-spacing-rules.md`.

## Decision
Groups from `\DeclareTextFontCommand` commands (`\textbf`, `\texttt`, `\textrm`, `\emph`, `\textsc` and the others) start with `Inline::TextFontCommand`. `check_icl` applies latex.ltx's `\maybe@ic`/`\sw@slant`: when the new font is upright and the argument does not start with `.` or `,`, the italic correction of the character before (in any font) goes in front of the space. `check_icr` now applies only to these groups. `\{`/`\}` are cmsy's braces, as `\textbraceleft` is. The expander supplies `\date{\today}`, and `\maketitle` no longer suppresses the next indent.

## Rejected
- `\check@icr` on every group that starts with a declaration: `{\itshape x}` gets no correction in LaTeX.

## Measured
- `{\itshape given \emph{additional} emphasis.}`, `\textit{given} more` and a `\maketitle` without a date: exact against pdfLaTeX.
