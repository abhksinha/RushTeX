# Engine: line breaker, outer lists, small primitives (E4 step B3a)
Author: abhi+claude | Date: 2026-10-21
Files: `src/engine/engine_linebreak.rs`, `src/engine/engine_box.rs`, `src/engine/engine_run.rs`, `src/engine/engine_eqtb.rs`, `src/engine/engine_value.rs`, `src/engine/engine_native.rs`, `src/engine/engine_prim.rs`, `src/engine/engine_snapshot.rs`, `src/engine/engine_node.rs`

## Context
modguide on the latex.ltx format halted with 330 errors from `\maketitle` on. Found
causes from evidence: a ring of the last 16 commands is printed with each error when
`RT_TRACE` is set (`Machine::recent`, `Machine::trace`).

## Decision
- When a run starts from a format (`RT_FMT_IN`), the engine owns the outer lists
  (`Machine::typeset`); `building()` is true and `new_graf` on the main vertical list
  always gets `\parskip` (§1091). Without a format the native pipeline still owns them.
- `scan_left_brace` checks the meaning, not the token: `\bgroup` is `\let` to `{`. This one
  bug caused most of the cascade ("Missing { inserted", then `\vskip` in an `\hbox`).
- New primitives: `\ignorespaces` (§1045), `\/` (§1113, explicit kern), `\parshape` (§1248;
  shapes kept in `Eqtb::shapes`, the register is an index + 1, saved by `Saved::Shape`,
  in the format), `\prevgraf` (§422, §1244, on `Level`), and e-TeX's
  `\parshapelength`/`\parshapeindent`/`\parshapedimen` now read the shape.
- `line_break` (§813–§890) in `engine_linebreak.rs`. Own representation: the active list is
  a vector in TeX's order; each active node keeps an `origin` (running total at its break
  minus `break_width`), so a line's widths are `total - origin`, the same integer sums TeX's
  delta nodes produce. Feasibility, fitness classes, demerits, tie order (`<=` keeps the
  later), the three passes, `\looseness`, the final-pass last resort (§854) and
  `post_line_break` (rightskip at the break, leftskip unless zero, club/widow/interline
  penalties, discarding after a break) follow tex.web.

## Measured
- 6/6 pdfTeX probes agree: plain paragraphs, hanging indentation, `\parshape`,
  `\looseness=1`, ragged right with `\leftskip`; box heights and `\prevgraf`.
- Earlier batch (`\aftergroup`, families, `\the\font`, `\ifincsname`, `\spacefactor`,
  `\prevdepth`, `\iffontchar`): 7/7.
- modguide on the format: 330 errors to line 400, now 30. latex.ltx meanings still 0
  differ after reload; format 2,519,178 bytes.

## Open
- No hyphenation in the second pass (§891–§965), no discretionaries, no math nodes, so
  LaTeX paragraphs that need hyphens will break differently.
- Remaining modguide errors: math lists, `\halign`, `\insert`/`\vadjust`, the page builder
  (`\deadcycles`, `\end`).
