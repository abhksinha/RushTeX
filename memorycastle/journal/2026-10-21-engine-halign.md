# Engine: \halign (E4 step B3b)
Author: abhi+claude | Date: 2026-10-21
Files: `src/engine/engine_align.rs`, `src/engine/engine_input.rs`, `src/engine/engine_run.rs`, `src/engine/engine_prim.rs`, `src/engine/engine_mod.rs`, `src/engine/engine_box.rs`, `src/engine/engine_pack.rs`, `src/engine/engine_linebreak.rs`

## Context
modguide's `\maketitle` uses `tabular`, which is `\halign`. The engine had no alignment.

## Decision
- tex.web §768–§812 and §1126–§1133 in `engine_align.rs`, own representation: an `Align`
  holds the columns in a vector (templates are spans of the engine token arena, shared
  when a periodic preamble grows), the tabskip glue in another, and each column's spanned
  widths as a small sorted vector instead of span nodes. An unset entry is a box node with
  subtype `UNSET_ENTRY` whose stretch/shrink sit in `Align::unsets`; a row is a box node
  with subtype `UNSET_ROW`. `fin_align` packs a prototype row and resets every row and
  entry (glue ratio `f64`, as web2c's `glueratio` is `double` in pdfTeX).
- `align_state` lives in `Input`: `next` counts explicit braces, `back` uncounts them
  (§325), and the end of a ⟨u_j⟩ frame (`Frame::u_template`) sets it to 0 (§324). The check
  that inserts ⟨v_j⟩ (§342) is in `Machine::get_token` and runs only when the state is 0.
- `\endtemplate` is a frozen name, id `END_TEMPLATE_ID` (305), named `\0endtemplate` so it
  cannot be typed.
- New prims: `\halign`, `\span`, `\cr`, `\crcr`, `\omit`, `\noalign`. `\valign` is not done.
- A slot test caught `\globaldefs` (43, not 34); the line breaker's slots got the same test.

## Measured
- 8/9 pdfTeX probes agree (periodic preambles, `\omit`, `\span`, `to`/`spread`, `\noalign`
  rules with running widths, tabskip changes inside the preamble, a misplaced `&`). The
  ninth differs only by the TFM kern between `b` and `c` (0.2778pt), which the engine does
  not apply yet (B2); plain `\hbox{abc}` differs the same way.
- modguide on the format: 30 errors to line 400, now 25 (math, `\insert`, `\vadjust`,
  `\deadcycles`/`\end`). Engine time for modguide 19.8 ms, file lookups excluded.

## Open
- B2: ligatures and kerns from the TFM lig/kern programs (affects every word).
- Displays (`$$\halign`), `\valign`, `\vadjust`/`\insert` material in rows (`cur_tail`).
- Templates stay in the token arena until arena reclamation exists.
