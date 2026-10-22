# `\quitvmode` in every mode; `\input` lookups of unregistered suffixes
Author: abhi+claude | Date: 2026-10-22
Files: `src/engine/engine_run.rs`

## Context
ltxcmds' `\ltx@leavevmode` is `\quitvmode`, and chemarr and pdflscape stopped on it. TikZ's
pgfornament inputs `.pgf` files, which `kpsewhich` cannot find without a format.

## Decision
- `\quitvmode` (pdfTeX `start_par`, code 2) starts an indented paragraph in vertical mode
  and is a no-op in horizontal and math modes (no `indent_in_hmode`).
- In `format_smoke`'s file lookup, a name `kpsewhich` misses is looked up again with
  `-format=tex`, as `\input` and `\openin` use `kpse_tex_format`. Font files are excluded,
  so misses on those cost nothing extra.

## Measured
- chemarr and pdflscape: errors to zero shift. multicolrule (with TikZ ornaments) at zero
  shift, together with `\pdfsavepos`.
