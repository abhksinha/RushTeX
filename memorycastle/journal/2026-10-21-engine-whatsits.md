# Engine whatsits: \write, \openout, \closeout, \special, \setlanguage, pdfTeX literals
Author: abhi+claude | Date: 2026-10-21
Files: `src/engine/engine_whatsit.rs`, `src/engine/engine_node.rs`, `src/engine/engine_snapshot.rs`, `src/engine/engine_page.rs`, `src/engine/engine_io.rs`, `src/engine/engine_chars.rs`, `src/engine/engine_box.rs`, `src/engine/engine_run.rs`, `src/engine/engine_prim.rs`, `src/engine/engine_mod.rs`, `src/engine/engine_linebreak.rs`, `src/engine/engine_pack.rs`

## Context
E4 step d. LaTeX's l3backend maps `\__kernel_backend_literal:e` to `\special` and colour to
`\pdfliteral`/`\pdfcolorstack`; inside the engine's boxes these were unsupported, so every
document's shipout hooks errored.

## Decision
- One node kind `Whatsit`: `sub` names it (open, write, close, special, language, literal,
  late literal, colour set/push/pop/current, setmatrix, save, restore), `font` the stream,
  literal mode or colour stack, `a..b` a token span (file names are stored as character
  tokens, so no string side table and the snapshot codec stays uniform).
- `\special`, `\setlanguage`, `\pdfliteral`, `\pdfcolorstack`, `\pdfsetmatrix`, `\pdfsave`,
  `\pdfrestore` became engine primitives; outside the engine's lists they still go to the
  native pipeline unchanged. Non-immediate `\write`/`\openout`/`\closeout` make nodes only
  while the engine builds a list.
- §1366 `out_what` runs from `ship_out` over the shipped box depth first (leader boxes
  skipped): streams open and close, `\write` text expands then with `mode_zero` (§1370),
  `\pdfliteral shipout` text expands and the node becomes an ordinary literal in place.
- §1376 `fix_language` runs at the start of a word in a paragraph; `new_graf` sets
  `clang` (§1091). Language whatsits carry `\lefthyphenmin`·64+`\righthyphenmin` for the
  coming hyphenation step.

## Measured
- Whatsit probes vs pdfTeX: 2/2 (deferred write expansion order across two shipped pages,
  `\noexpand`, mode-0 `\lastkern`/`\lastpenalty`, writes in an unshipped box dropped,
  `\setlanguage` in a box, colour stack and literal syntax).
- latex.ltx 21529/21529; parity six 100.0%.
- Engine on the format, shipped pages vs pdfTeX: modguide 7/7, lppl 8/8, sample2e 3/3,
  small2e 1/1, cfgguide 11 vs 12. lppl and small2e now run with 0 engine errors; the rest
  error only on math.

## Open
- `\pdfcolorstackinit` numbering is not checked (any stack number is accepted).
- Math lists (E4 b) and hyphenation (E4 c) are next.
