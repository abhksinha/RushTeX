# testmath pages 28–31: group-scoped sizes, clo display skips, split tags
Author: abhi+claude | Date: 2026-10-20
Files: `src/frontend/frontend_parser.rs`, `src/frontend/frontend_ast.rs`, `src/layout/layout_engine.rs`

## Context
Continues `memorycastle/journal/2026-10-20-testmath-accents-roots-sideset-varlim.md`. A `{\Large \[..\]}` block group leaked its size into the rest of the document, which gave 47 pages against 41.

## Decision
- The parser records the size in force when a block group opens (`block_group_sizes`). At that group's `}` it emits `Inline::FontSize` to restore the size.
- Display skips come from `FontSize::display_skips` (`size1x.clo`). Only `\normalsize`, `\small` and `\footnotesize` set them, so `\Large` keeps the earlier skips. They are layout state (`display_skips`), not derived from the size in force.
- After a display, `last_line_advance` is the display's `\baselineskip`, so a resumed paragraph in another size corrects its interline glue.
- In `equation` with `split`, amsmath's `\place@tag` hangs a number that does not fit inside the `\vcenter` box. The number gets struts, `\belowdisplayskip` follows, and prevdepth is the box's depth.
- `\newpage` clears `\@doendpe`'s noindent, because its `\par` restores `\everypar`.

## Measured
- testmath 70.7% → 85.2%, with the 0.1pt margin-rule shift on pages 29–41 compensated. Pages are 41/41.

## Open
- testmath's own `\@makecol` adds 0.1pt `\vrule`s, which shifts pages 29–41 by 0.1pt. This needs an output-routine model.
- `split` in `align` becomes `aligned` (`\split@aligned`, a vcentered box). We still lay it out as a split.
- `\delimiterfactor` and `\delimitershortfall` are parsed but ignored.
- Math design-size masters: cmmi12 at `\Large`.
