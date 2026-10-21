# Engine hot paths: primitives entered once, reused name buffer; what the time really is
Author: abhi+claude | Date: 2026-10-21
Files: `src/engine/engine_mod.rs`, `src/engine/engine_prim.rs`, `src/engine/engine_run.rs`, `src/engine/engine_cond.rs`, `src/engine/engine_font.rs`, `src/engine/engine_snapshot.rs`, `src/engine/engine_input.rs`

## Context
Before continuing E4 the user asked for the small hot-path fixes found by reading the code:
primitive lookup by name string, a `String` allocated per `\csname`, active characters and
the current font found through the name hash.

## Decision
- `Engine::default` is IniTeX (§1336, §226): active character `c` is id `c` (§222
  `active_base`), the current font is id 256 (`CUR_FONT_ID`, §230 `cur_font_loc`), then every
  name of `builtin_names` (the engine's primitives, the parameters, pdfTeX's other
  primitives) is interned with `builtin(name)` as its meaning. `Machine::meaning` is then
  an array read; a never-assigned name is simply undefined. `Engine::undump` checks the
  fixed ids are where this build puts them.
- `scan_cs_name` collects `\csname`/`\ifcsname` names in one reused buffer (`name_buf`).
- `Machine::start_job` reads `\everyjob` after a format is loaded (§1337).
- `format_smoke` reports file-lookup time (a `kpsewhich` process per new file) apart from
  engine time, and `RT_FMT_IN` starts a run from a dumped format.

## Rejected
- Caching name lookups beside the meaning table: entering primitives once, as TeX does, is
  simpler and leaves a single source of meaning.

## Measured
- latex.ltx format build, harness wall time: 6.2 s before, 5.7-5.9 s after (about 6%).
- Of that, 2.55 s was the harness's 112 `kpsewhich` spawns; engine time is 3.47 s, against
  pdfTeX `-ini` 3.96 s (which also loads hyphenation patterns, which we skip). The earlier
  "6.2 s vs pdfTeX" comparison overstated our cost.
- Profile (macOS `sample`): `Input::next` self time dominates (token fetch from a 38.5M-token,
  154 MB arena: cache misses), then `macro_call`; lexing is negligible.
- Fidelity unchanged: 21529/21529 meanings agree with pdfTeX; parity's six documents at 100%.
- modguide on the loaded format: preamble (`ltxguide` class) runs with 0 errors in 11.9 ms
  engine time; errors start at `\begin{document}` (line 53), where the kernel measures boxes.

## Open
- Arena reclamation (fix 4): bounds RAM and should cut the token-fetch misses; next
  performance item.
- E4 continues into the typesetting primitives the kernel touches at `\begin{document}`:
  box registers, `\setbox`, `\hbox`/`\vbox`, `\wd`/`\ht`/`\dp`, glue and kerns.
