# E4/B1: boxes in the engine (node model, packaging, box registers)
Author: abhi+claude | Date: 2026-10-21
Files: `src/engine/engine_node.rs`, `src/engine/engine_pack.rs`, `src/engine/engine_box.rs`, `src/engine/engine_eqtb.rs`, `src/engine/engine_prim.rs`, `src/engine/engine_run.rs`, `src/engine/engine_value.rs`, `src/engine/engine_cond.rs`, `src/engine/engine_native.rs`, `src/engine/engine_snapshot.rs`, `src/engine/engine_mod.rs`, `scripts/engine_probe.py`

## Context
modguide's preamble ran on the format, but `\begin{document}` measures boxes. The user agreed
the engine gets TeX's own box and node model (tex.web semantics, our representation), with
native layout to consume the nodes later. Step B1 of three: B1 boxes, B2 ligatures and kerns
(TFM lig/kern programs), B3 paragraphs, page, `\output` and the bridge to native layout.

## Decision
- `engine_node`: a node is 12 bytes (kind, subtype, font, two words); box records, glue specs
  and rule dimensions are side tables. A finished list is a span of one node arena and never
  changes: `\copy` shares the box, and `\wd n=`, a shift, or `\lastbox` make a new record
  (copy on write). Lists being built sit back to back in one buffer (`Machine::cur`), one
  segment per nest level (§211), so building a box allocates nothing per box.
- `engine_pack`: `hpack` §649, `vpackage` §668 with `\boxmaxdepth` §669, glue setting
  §657–§667 with `\badness` and the §666 overfull rule, `append_to_vlist` §679, `badness` §108.
- `engine_box`: `\setbox` §1241 (prefix `\global`, `\afterassignment` token inside the box),
  `begin_box`/`package`/`box_end` §1079–§1087 (`to`/`spread`, `\everyhbox`/`\everyvbox`,
  `\vtop` §1087, adjusted hbox group), `\box`/`\copy` voiding at the register's own level,
  `\lastbox`, `\raise`/`\lower`/`\moveleft`/`\moveright`, leaders with boxes or rules, glue,
  `\kern`, `\penalty`, rules §1056, `\unhbox`/`\unvbox`/copies §1110, `\unskip`/`\unkern`/
  `\unpenalty` §1105, `\char`, interword glue with the space factor §1034/§1041–§1044,
  `\indent`, `\wd`/`\ht`/`\dp` as values and assignments §1247, `\lastskip`/`\lastkern`/
  `\lastpenalty`/`\badness`, `\ifvoid`/`\ifhbox`/`\ifvbox`.
- Box registers are an eqtb table grouped through the save stack; the node arena is in the
  format. The outer vertical and horizontal lists remain the native pipeline's until B3: a
  box appended there is dropped, and material inside a box that the engine cannot build yet
  is one error per kind ("Not in the engine's boxes yet").
- A paragraph inside a `\vbox` is packed as one line with an error until the line breaker
  moves into the engine (B3).
- `engine_probe.py` accepts a path to one's own test file (`--format NAME` with options before
  the positionals).

## Rejected
- New `Prim` variants only for the hot box commands, the rest dispatched by name: every box
  command is hot inside LaTeX's measuring macros.
- Linked node lists as in tex.web: pointer chasing; immutable spans plus copy-on-write records
  give `\copy` for free.

## Measured
- 19/22 pdfTeX probes agree on a box test file; the three that differ are text needing
  ligatures and kerns (B2). Locked as `engine_box::tests::boxes_measure_as_pdftex`.
- Pitfall: an earlier "21/21" was invalid: zsh `echo` turned `\t` of `\the` into a tab, so the
  probes printed plain text. Write probe files with `printf '%s\n'`.
- latex.ltx fidelity unchanged: 21529/21529 meanings; format 2,518,020 bytes, load 3.5 ms.
- modguide on the format: `\begin{document}` passes; errors now start at `\maketitle`
  (line 55), 86 errors (was 117 from line 53).

## Open
- Next small primitives that `\maketitle` needs: `\aftergroup`, `\iffontchar`, `\ifincsname`,
  `\spacefactor`/`\prevdepth` values, control space `\ `, `\textfont`/`\scriptfont` families.
- Larger: `\halign`/`\valign` (§768), math lists in boxes, B2 ligatures/kerns, B3 line breaker
  and page builder in the engine.
- TeX's `glue_set` type in web2c (float or double) decides shipped positions; check
  `texmfmp.h` before B3.
- The node arena, like the token arena, is never reclaimed during a run.
