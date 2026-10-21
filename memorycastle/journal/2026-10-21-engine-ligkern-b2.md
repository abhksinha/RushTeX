# Engine: ligatures, kerns and the main loop (E4 step B2)
Author: abhi+claude | Date: 2026-10-21
Files: `src/engine/engine_chars.rs`, `src/engine/engine_font.rs`, `src/engine/engine_node.rs`, `src/engine/engine_linebreak.rs`, `src/engine/engine_pack.rs`, `src/engine/engine_snapshot.rs`, `src/engine/engine_box.rs`, `src/engine/engine_run.rs`, `src/backend/backend_tfm.rs`, `src/backend/backend_tfm_data.rs`, `scripts/fontgen/gen_tfm.py`

## Context
Every word the engine set was off by its font kerns and ligatures (B1 probes 19/22,
`\halign` probe 8/9, `\hbox{abc}` 0.2778pt short).

## Decision
- The embedded `Tfm` now carries the raw lig/kern program (`lig_kern`, four-byte words as in
  the file, §545) and its `kerns`; `scripts/fontgen/gen_tfm.py` emits them. File TFMs keep
  theirs in `FileTfm` and in the format. One reader for both: `Metrics::lig_kern_start`
  (§557 with restart), `Metrics::boundary` (§573/§576 `bchar`, `bchar_label`).
- The older pairwise tables in `backend_ligkern.rs` are not used by the engine: they cannot
  express boundary characters or the `|=:|>` family, so they are not TeX-exact.
- `main_loop` (§1034–§1040) in `engine_chars.rs`: each tex.web label is a `Step`, the loop
  jumps between them as TeX does; `lig_stack` is a short vector of items (real character, or
  pseudo-ligature keeping the character it replaced). The word's ending token is obeyed
  afterwards (`reswitch`) by calling `command` once, so no lookahead is pushed back.
- New nodes: `Kind::Lig` (character in `a`, original characters in `Nodes::lig_chars`,
  count and §143 subtype in `sub`) and `Kind::Disc` (§145; `a = -1` for the empty
  discretionary §1039 puts after a hyphen character, else an index into `Nodes::discs`).
- Line breaker: ligatures measure as their character and precede legal glue breaks;
  discretionaries break per §869 (`\exhyphenpenalty` for an empty pre-break), §840
  break widths, and §881–§882 in `post_line_break` (pre-break text ends the line, post-break
  starts the next with no pruning, `\brokenpenalty`).
- `\noboundary` (§1045, `cancel_boundary`) and `scan_char_num` (§434) added.

## Measured
- Lig/kern probes vs pdfTeX: 11/11 (cmr10 ff/fi/ffi/fl, `` `` '' -- --- !` ?` ``, AV/Wa/To/P./LT
  kerns, cmti10, cmtt10 with no ligatures, `\char` inside a word, `\noboundary`, `\/`,
  hyphenated compounds breaking in a narrow `\vbox`).
- Earlier suites now: boxes 22/22 (was 19/22), `\halign` 9/9 (was 8/9), line breaker 6/6,
  primitives 7/7. Format 2,519,219 bytes, 0 meanings differ after reload.
- modguide on the format: still 25 errors (math, `\insert`, `\vadjust`, page builder); engine
  time 13.6 ms.

## Open
- Hyphenation (§891–§965) for the second pass; `\discretionary` and `\-`; accents (§1122).
- Embedded TFMs still hold 128 characters; 256-character fonts (T1/EC) need the file TFM.
