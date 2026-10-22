# `\pdfsavepos`, `\pdflastxpos`, `\pdflastypos`
Author: abhi+claude | Date: 2026-10-22
Files: `src/engine/engine_whatsit.rs`, `src/engine/engine_ship.rs`, `src/engine/engine_page.rs`, `src/engine/engine_pdfdoc.rs`, `src/engine/engine_native.rs`, `src/engine/engine_run.rs`

## Context
multicolrule (through KOMA's `\scr@savepos`), zref, TikZ's `remember picture` and eso-pic
need the position of a point on the shipped page. pdfTeX records it during the ship walk,
and `\write`s later in the same walk read it.

## Decision
The ship walk records each `\pdfsavepos` position (`cur_h`, page height − `cur_v`) in walk
order. `ship_out` now runs `out_whats` twice: `\pdfliteral shipout` before the walk, then
the files after it. Each `SAVE_POS` whatsit takes the next recorded position into
`last_pos`. Both traversals are depth first in list order and skip leaders, so they stay
in step. `\pdflastxpos` and `\pdflastypos` are integers, as in pdfTeX (they were dimensions
reading 0).

## Rejected
- Tracking positions inside `out_whats` itself: that would duplicate the walk's geometry
  (glue setting, leaders, expansion), which would then have to stay in step by hand.
- Expanding `\write`s inside the walk: the walk borrows the node store while expansion
  needs the whole machine.

## Measured
- A repro with four positions across a line, `\hfill`, an `\fbox` and a second page writes
  pdfTeX's numbers exactly (`11459191,46515936` ...), at zero shift.
