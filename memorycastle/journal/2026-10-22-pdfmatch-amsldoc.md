# `\pdfmatch` and `\pdflastmatch`: our own POSIX ERE, BSD rules; amsldoc at zero shift
Author: abhi+claude | Date: 2026-10-22
Files: `src/engine/engine_regex.rs`, `src/engine/engine_native.rs`, `src/engine/engine_run.rs`, `src/engine/engine_mod.rs`, `scripts/engine_probes/prims.probes`, `scripts/parity/pdfexact.py`, `scripts/parity/corpus/amsldoc.tex`

## Context
amsldoc stopped at `\hypersetup{pdfborder=0 0 0}` (l.252) with "Missing number, treated as zero (at {)". Bisected to 4 lines.
- hyperref's `\Hy@Match` does `\ifcase\pdfmatch{pattern}{subject}`.
- Our `\pdfmatch` was a stub that read one text and gave nothing, so `{subject}` reached `\ifcase`.

## Decision
- `engine_regex.rs`: a std-only POSIX ERE matcher over bytes.
  - Parsed tree, backtracking, leftmost-longest as `regexec`. The first-found among equal-length matches keeps the subexpressions.
  - A work budget, so no pattern can hang.
  - icase via sets; `[:class:]` in the C locale.
- `\pdfmatch [icase] [subcount n] {p}{s}` and `\pdflastmatch n` follow utils.c `matchstrings`/`getmatch`:
  - `1`, `0`, or `-1` for a bad pattern;
  - the default subcount is 10;
  - `pos->text` or `-1->`;
  - a bad pattern leaves the previous match state alone.
- POSIX-undefined cases follow the reference pdfTeX's (macOS/BSD) regex library, measured with `pdftex -ini` probes:
  - errors: the empty pattern; an empty branch (`a|`, `|a`, `a||b`, `(|a)`); a repetition with no operand (`*a`, `^*a`, `{1}a`); a second operator (`a**`, `a+?`, `a{1}*`); a bad interval (`a{3,2}`, `a{1x}`); an unknown class;
  - accepted: `()`;
  - literal: `{` not followed by a digit (`a{,2}`, `x{}`).
  - Consequence: hyperref's driverfallback pattern `^(|dvipdfm|…)$` is -1 here, as in the reference build. glibc would accept it.
- Step 0 of the plan: the structure check compares each named destination's page and exact point, not only the count.

## Rejected
- glibc semantics (empty branches allowed): the reference PDFs come from the BSD-regex pdfTeX, and parity follows the reference.
- Linking a system regex: zero-dependency rule, and the output would vary by platform.

## Measured
- amsldoc ZERO on the first run after the fix: 39 pages, 61,108 glyphs, 147 paths, 108 links, 56 outlines, 162 dests (page and point exact), 0 pixels at 300 dpi.
- The corpus is now 13/15 at zero shift: 261 pages, 387,395 glyphs.
- prims probes 11/11, including 3 `\pdfmatch`/`\pdflastmatch` probes; regex unit tests 3/3.

## Open
- POSIX subexpression choice among equal-length matches is not proven to match BSD in all cases. Only cases met so far are probed.
