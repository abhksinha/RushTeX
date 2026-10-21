# Engine page builder, insertions, marks, \vadjust, \vsplit, \output
Author: abhi+claude | Date: 2026-10-21
Files: `src/engine/engine_page.rs`, `src/engine/engine_node.rs`, `src/engine/engine_pack.rs`, `src/engine/engine_box.rs`, `src/engine/engine_align.rs`, `src/engine/engine_linebreak.rs`, `src/engine/engine_run.rs`, `src/engine/engine_io.rs`, `src/engine/engine_value.rs`, `src/engine/engine_snapshot.rs`, `src/engine/engine_assign.rs`, `src/engine/engine_mod.rs`, `src/engine/engine_native.rs`, `src/engine/engine_prim.rs`

## Context
E4 step a: the engine needed tex.web parts 44 and 45 (§967–§1028), §1097–§1101 and §1054
so class code can ship pages: `\insert`, `\vadjust`, `\mark`/`\marks`, `\vsplit`,
`build_page`, `fire_up`, the output routine, `\shipout`, `\end`/`\dump`, the page
parameters (`\pagegoal`..`\pagedepth`, `\deadcycles`, `\insertpenalties`).

## Decision
- New node kinds `Ins` (side table `InsRec` in `Nodes::inserts`), `Mark` (token span in
  `a..b`, class in `font`), `Adjust` (node span). Codec values 8–10.
- The contribution list is the bottom segment of the shared build buffer `Machine::cur`
  (the main vertical list); `take_contrib`/`put_contrib` move it and shift the nest's
  `start`s, so `build_page` works while a paragraph is open (`new_graf`'s
  `if nest_ptr=1 then build_page`). The current page is its own `Vec<Node>` in `Page`.
- Page-insertion records (§981) are a small vector sorted by box number; node pointers
  become page indices (`last_ins`, `best_ins`, `broken_ins`). Marks are never-freed token
  spans, so no reference counts; e-TeX mark classes are a short `(class, [Option<Span>;5])`
  vector.
- §655 adjust migration: `Machine::adjust: Option<Vec<Node>>` is opened just around the
  `hpack` calls that TeX gives an `adjust_tail` (`post_line_break` §889, adjusted hboxes
  §1085/§1076, alignment entries §796 collected per row and appended in `fin_row` §799).
- `\output` keeps its braces (§1226); the routine runs in an internal vertical nest under
  OUTPUT_GROUP; its `}` calls `resume_page_builder` (§1100).
- §1370: `\write` text expands with `mode_zero` set, so `\lastkern`/`\lastskip`/
  `\lastpenalty` read 0, `\prevgraf` 0, and `\spacefactor`/`\prevdepth` are improper.
  Found by the page probes, not by reading.
- `RT_TYPESET=1` makes `format_smoke` own the outer lists without a format, so
  `scripts/engine_probe.py FILE N --format FILE` can probe the page builder.

## Measured
- Page-builder probes vs pdfTeX: 11/11 (two suites: 14 output-routine firings covering
  marks, split insertions with `\count`≠1000, `\holdinginserts` with `\unvbox255`,
  `\vadjust` in paragraphs and `\halign` rows, `\vsplit` with split marks, page totals).
- latex.ltx probes 21529/21529; format 0 meanings differ; 49.1M expansions (unchanged).
- Parity six unchanged at 100.0%.
- Engine run on the format: modguide 25 errors to 6 (math and `\special` only); shipped
  pages vs pdfTeX: cfgguide 12/12, small2e 1/1, modguide 8 vs 7, lppl 7 vs 8, sample2e 4
  vs 3 (hyphenation and math still missing, so lines differ).

## Open
- Whatsits (`\special`, non-immediate `\write` at shipout), hyphenation, math lists.
- `\end` path only exercised through documents, not probed in isolation.
