# Engine math lists: noads, Appendix G, displays, accents, discretionaries
Author: abhi+claude | Date: 2026-10-21
Files: `src/engine/engine_mlist.rs`, `src/engine/engine_noad.rs`, `src/engine/engine_chars.rs`, `src/engine/engine_node.rs`, `src/engine/engine_pack.rs`, `src/engine/engine_linebreak.rs`, `src/engine/engine_hyph.rs`, `src/engine/engine_align.rs`, `src/engine/engine_box.rs`, `src/engine/engine_run.rs`, `src/engine/engine_font.rs`, `src/engine/engine_prim.rs`, `scripts/engine_probes/math.tex`, `scripts/engine_probes/accent.tex`

## Context
E4 step b: the engine had no math, so every LaTeX formula failed ("Not in the engine's boxes yet: $"). sample2e, modguide and cfgguide had 8, 5 and 18 math errors, and testmath had 742 errors.

## Decision
- **Representation (ours, TeX's semantics).** A math list is built in the build buffer like any list. A noad is a 12-byte `Kind::Noad` node whose `a` indexes `Machine::noads`; its fields are a `Field` enum (`Empty`, `Char`, `TextChar`, `Box`, `Mlist(span)`). A finished subformula is a span of the node arena.
  - A fraction keeps its numerator in `supscr` and its denominator in `subscr`, as tex.web does. A choice uses its four fields; an accent's character is in `extra`.
  - Delimiters are TeX's 24-bit code.
  - The noad table is cleared when no math level is open.
- **Math-group state.** §1139's `saved` words live in a small `math_saved` stack: the target `noad*4+field`, the `\mathchoice` count, and the `\leqno` flag.
  - In math mode `aux` is `incompleat_noad` (−1 for none).
  - e-TeX's `delim_ptr` is a new `Level::delim`, an offset into the list. It stays valid after `\over` moves the list into the numerator span.
- **`mlist_to_hlist` (§719–§767).** It copies the mlist to a local `Vec` (cheap, since nodes are 12 bytes). The first pass builds each noad's `new_hlist` back to back in one local buffer, as a span stored in the noad. The second pass joins the spans with the §764 spacing table and the penalties.
  - Boxes made here are fresh records, so `rebox`, `make_op` and the scripts adjust them in place.
  - `clean_box` copies a sub_box record before changing it, because `\copy` shares boxes.
- **Measuring without packing.** `measure_h` was factored out of `hpack`. `check_dimensions` and `make_scripts` measure a list without storing it.
- **e-TeX behaviour followed exactly.**
  - `\middle` is a right noad with subtype 1.
  - `make_left_right` resets `cur_style`.
  - After `\right`/`\middle`, `r_type` becomes left and the style is reset.
  - `fin_mlist` cuts the numerator at `delim_ptr`.
- **Kinds in the rest of the engine.** `Kind::Math` (math-on/off, `\mathsurround`) is wired through:
  - `hpack`, `vpack`, break widths (§837) and `kern_break` at a math node (§866);
  - pruning after a break and the zero width at a break (§879, §881);
  - no hyphenation between math-on and math-off (`auto_breaking`).
- **Displays.** `line_break(display)` implements e-TeX's `line_break(d)`: `\displaywidowpenalty` applies, and `just_box` is recorded.
  - `pre_display_size` follows §1146; the line size follows §1149.
  - Equation numbers (`\eqno`/`\leqno`) follow §1199–§1205, with the squeeze, the centring and the short skips.
  - `pre_display_direction` is 0, since there is no TeXXeT.
- **Display alignments.** `\halign` inside `$$` follows §774/§1206.
  - The alignment takes `prev_depth` from the enclosing vertical list.
  - `do_assignments` runs after the alignment.
  - The display's penalties and skips go around the rows.
- **`\globaldefs` (§1214).** It now applies to every assignment, including `\gdef` with a negative value. LaTeX's `\glb@settings` sets the math families under `\globaldefs=1`. Without this, every formula was deleted with "Insufficient symbol fonts".
- **Text commands.**
  - `\accent` follows §1123–§1125, with slant-corrected kerns and the accent raised in a box.
  - `\discretionary` and `\-` follow §1117–§1121, using `disc_saved` and `DISC_GROUP`.
- **Font metrics.** The engine's `FileTfm` now keeps the extensible recipes (dumped in the format). `Metrics::tag`/`exten` serve `var_delimiter` for both the static and the file TFMs.

## Rejected
- A per-noad `Vec` for `new_hlist`. One buffer per `mlist_to_hlist` call with spans allocates once per formula level.
- Packing with `hpack` just to measure: it would store a dead list in the arena for every noad.

## Measured
- New `math` probe suite: 44 probes vs pdfTeX -ini, all identical. It covers:
  - scripts, fractions (`\over`, `\atop`, `\above`), radicals, big operators with and without limits;
  - `\left`/`\middle`/`\right`, math accents with skew, `\mathchoice`, `\overline`/`\underline`, `\vcenter`, mu glue and kerns;
  - displays with `\eqno`/`\leqno`, a display `\halign`, and `\globaldefs`.
- New `accent` suite: 11 probes vs pdfTeX -ini, all identical (accents, discretionaries, breaks at `\-`).
- All 11 suites agree (136 probes). The latex.ltx format shows 0 meanings differing.
- Engine runs on the format: lppl, small2e, sample2e (3 pages), modguide (7) and cfgguide (11) have 0 errors.
- testmath went from 742 errors to 268, all from one construct (`align*` inside `gather`, l.2132).

## Open
- A reduced gather/align* file loops forever. Find the cause, and add TeX's 100-error stop (§82; the count is reset by `end_graf`).
- cfgguide gives 11 pages against pdfTeX's 12.
- Arena reclamation: sub-mlists and measuring boxes stay in the node arena until the arena work lands.

# Update (same day): testmath at 0 engine errors

## Decision
- §442: a brace given as an alphabetic constant does not count in `align_state`. amsmath's `\iffalse{\fi\ifnum0=`}\fi` around `align*` in `gather` needs it. This was the single cause of the remaining 268 errors.
- §82: the run stops at 100 errors in one paragraph (`end_graf` resets the count). A broken document had looped forever.

## Measured
- testmath runs on the engine with 0 errors and pdfTeX's 41 pages. The `halign` suite gained 5 probes, including the brace-constant trick: 20/20.
- Later the same day, testmath reached 100.0% strict through the engine PDF; see `2026-10-21-engine-pdf-bridge.md`.
