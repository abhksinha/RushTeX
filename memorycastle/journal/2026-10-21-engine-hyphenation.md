# Engine hyphenation: \patterns, \hyphenation, hyphenate, reconstitute
Author: abhi+claude | Date: 2026-10-21
Files: `src/engine/engine_hyph.rs`, `src/engine/engine_linebreak.rs`, `src/engine/engine_chars.rs`, `src/engine/engine_box.rs`, `src/engine/engine_snapshot.rs`, `src/engine/engine_mod.rs`, `src/engine/engine_prim.rs`, `src/engine/engine_run.rs`, `scripts/engine_suites.py`, `scripts/engine_probes/`

## Context
E4 step c: tex.web §891–§966. Before this, `\patterns` and `\hyphenation` were native
commands whose tokens streamed to the output list during the format build (1.67M out
tokens), and the engine's line breaker never hyphenated.

## Decision
- Own storage, TeX's results. Each language keeps its patterns as records (`k`, the `k`
  lc-mapped letters, the `k+1` digits nibble-packed); that is what the format stores, one
  raw blob per language (a copy at load). A language's trie is built on its first
  hyphenation: children contiguous and sorted, op lists shared between patterns with the
  same digits, later duplicates replacing earlier (§963 `trie_o[q]:=v`). A document builds
  only the languages it uses.
- Exceptions: a hash map from letters + language to positions; a later `\hyphenation` of
  the same word replaces the earlier one (TeX Live's tex.ch behaviour).
- `line_break` is split into `passes()`; between the first and second pass the whole
  paragraph is hyphenated (`hyphenate_par`). Equivalent to §894's word-at-a-time
  hyphenation inside pass 2 because each word depends only on its own nodes and on the
  language whatsits before it, which the pre-pass tracks in order (§1362, §1363).
- `new_graf` packs language and hyphen minima into the paragraph's `prev_graf` (§1091);
  `line_break` reads them before `pop_nest` (§891).
- `reconstitute` (§905–§911) and §913–§918 are ported label for label; the font's lig/kern
  program is borrowed while ligature nodes are made (disjoint borrows of fonts and nodes).
- Bug found by the probes: §1036 `main_loop_move+2` appends the `lig_stack` node (the
  character read), not `cur_l`; after a `=:` ligature ours appended the ligature character,
  so `lig_ptr` lists were wrong (widths were not, which is why the lig/kern probes passed).
- Probe suites moved from the scratchpad into `scripts/engine_probes/` with
  `scripts/engine_suites.py` (a `%% typeset` first line sets `RT_TYPESET=1`).

## Rejected
- Storing the built trie in the format: bigger than the records and makes every document
  pay for every language.
- Per-byte varint codec for pattern bytes: load went 3.5 ms to 8.2 ms; raw blobs restored it.

## Measured
- Hyphenation probes vs pdfTeX 10/10 (ligature words in cmr10 and cmti10, quotes and
  parentheses before words, exceptions, `\uchyph`, `\lefthyphenmin`, `\hyphenchar=-1`,
  `\language` switches, box and kern boundaries). All 9 suites: 85/85.
- Format with ~20 languages' patterns: 2.52 MB to 4.99 MB raw, 4.43 MB nibble-packed
  (pdflatex.fmt is 2.97 MB zlib-compressed); load 3.6 ms (was 3.5 ms before patterns).
- Format build output tokens 1.67M to 144K; time unchanged (6.2 s).
- latex.ltx 21529/21529; parity six 100.0%; engine docs: lppl 8/8 pages and small2e 1/1
  with 0 errors, modguide 7/7, sample2e 3/3, cfgguide 11 vs 12 (math still missing).

## Open
- e-TeX `\savinghyphcodes`/`hyph_index` not implemented (no TeX Live file sets it).
- Format size: patterns could stay in the format file and be read per language on demand.
