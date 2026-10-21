# testmath page 10: float residue in vertical mode, caption depth, float pages
Author: abhi+claude | Date: 2026-10-19
Files: `src/layout/layout_float.rs`, `src/layout/layout_engine.rs`, `src/tables/tables_layout.rs`

## Context
Page 9 broke one item too early, and page 10 (a float page) was at 28%. Display work: `memorycastle/journal/2026-10-19-testmath-display-paragraphs-float-pages.md`.

## Decision
- A float ended in vertical mode leaves `\addpenalty\interlinepenalty` and then `\penalty10000`, from `\@specialoutput`'s put-back. So `\lastskip` is 0 afterwards (`add_vmode_float`).
- A caption's first line includes the label's depth, as `render_caption` now computes it.
- `\@tryfcolumn` runs at every page start, before top floats (`try_float_page`). If the collected `p` floats fill more than `\floatpagefraction` of the page, they make a float page spread by the `\@fptop`/`\@fpsep`/`\@fpbot` fil glue. A failed float blocks later floats of its type (`FLOAT_TABLE_KIND`).

## Rejected
- `git stash` to compare against the old binary. The work tree holds many uncommitted files, and the stash also leaves a stale build behind. Use a git worktree instead.

## Measured
- `\showoutput` repro with one and two figures: the vertical lists match, and positions match at 100%.
- testmath 28.5% to 52.4%. Page 10 went from 28% to 100%, and page 9 from 6% to 99% (together with the display note).
