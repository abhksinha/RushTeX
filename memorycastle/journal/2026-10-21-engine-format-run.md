# E4: running latex.ltx and expl3 on the engine
Author: abhi+claude | Date: 2026-10-21
Files: `src/engine/engine_run.rs`, `src/engine/engine_cond.rs`, `src/engine/engine_input.rs`, `src/engine/engine_lex.rs`, `src/engine/engine_mod.rs`, `src/engine/engine_names.rs`, `CLAUDE.md`, `src/engine/engine_macro.rs`, `src/engine/engine_value.rs`, `src/engine/engine_io.rs`, `src/engine/engine_expr.rs`, `src/engine/engine_font.rs`, `src/engine/engine_native.rs`, `src/engine/engine_show.rs`, `src/engine/engine_md5.rs`, `src/engine/engine_eqtb.rs`, `src/engine/engine_assign.rs`, `src/engine/engine_prim.rs`, `src/engine/engine_tables.rs`, `scripts/gen_engine_tables.py`, `scripts/engine_probe.py`, `src/backend/backend_tfm.rs`

## Context
Step E4 of the engine plan: class files run as macros on the engine. The first target is the
LaTeX kernel itself (latex.ltx, which loads expl3-code.tex), because every class runs on it.
Progress is measured by `format_smoke` (ignored test; `RT_FORMAT=latex.ltx`): errors, where
the run stops, and which native primitives reach the output.

## Decision
- Primitive and parameter tables are generated from pdftex.web by
  `scripts/gen_engine_tables.py` into `engine_tables.rs`: 169 parameters (TeX's codes kept,
  e-TeX and pdfTeX appended), 376 primitives with their command code. A name with no meaning
  of its own falls back to `prim()`, then `param()`, then `Meaning::Native(index)`.
- Hot primitives get `Prim` variants. The long tail of e-TeX and pdfTeX `last_item` and
  `convert` primitives (`\pdftexversion`, `\inputlineno`, `\fontcharwd`, `\pdffilesize`,
  `\pdfmdfivesum`, `\pdfescapestring` ...) is dispatched by name from the generated table in
  `engine_native.rs`. They are rare, so the `Prim` enum stays small.
- Nothing depends on the clock or the environment: dates, elapsed time, random seed and
  shell escape read as zero or empty, so a run is reproducible.
- `\write`, `\openout` and the log go to in-memory buffers; a file the run wrote and closed
  reads back (`texsys.aux`). Input comes only from `Machine::read_file`.
- Fonts (`engine_font.rs`): a font is its name, size and scaled `\fontdimen` vector; metrics
  stay in the static TFM tables (`backend_tfm::by_name`). Only the most recently loaded font
  grows its parameters (§578), which is what `l3intarray` relies on. The current font is kept
  as the meaning of a reserved name, so it is grouped with no extra machinery.
- Mu glue is its own region (`Region::Mu`); `\thinmuskip` and its two siblings stay glue
  parameters, as in TeX, and `Reg::is_mu` tells them apart. `scan_dimen` is split into
  number and units (`dimen_units`), so glue, mu glue and integer factors share one path (§448–§461).
- Fidelity fixes found by the run, each from tex.web:
  - §1215 `get_r_token` skips spaces before a defined name (`\read3 to \x`).
  - §477 `scan_toks` with expansion expands one level at a time, so a `\unexpanded` or
    `\the` that a macro produces inside `\edef`/`\expanded` is still caught.
  - §406 "non-blank" means command `spacer`: a name `\let` to a space (expl3's
    `\exp_stop_f:`) is skipped like a space.
  - §262 `print_cs` uses `\escapechar` and the current catcodes; e-TeX's `print_meaning`
    prints `\protected\long macro:`.
  - §1069 group kinds: `}` cannot close `\begingroup`, nor `\endgroup` a brace group.
- `scripts/engine_probe.py FILE LINE EXPR.. [--code LINE]` truncates a TeX Live file, runs
  the format build through pdfTeX `-ini` and through the engine, and diffs probe output.
  This is the oracle for every fix: the value comes from pdfTeX, the method from tex.web.

## Rejected
- Giving every pdfTeX primitive a `Prim` variant: hundreds of rare variants in the hot match.
- Storing `\muskip` registers in the glue table at an offset: the tables are dense vectors,
  so an offset of 32768 would allocate about 1 MB.
- A 1M-expansion runaway guard: expl3 alone needs more. Raised to 100M.

## Measured
- latex.ltx smoke: 857 errors (stopped by `\eTeXversion` at line 117) → 345 → 95 → 2979
  (expl3 running far past where it failed before) → 33 → 15 → 7 → 5. The run now passes
  l3fp and stops in l3unicode's data-file loop at expl3-code.tex line 35568.
- 334961 expansions in 0.3 s for everything up to l3fp (release).
- Engine tests: 59 pass.

## Open
- l3unicode reads `GraphemeBreakProperty.txt` through `\ior_str_map_inline:Nn`; the run loops
  there. Next to fix.
- `\currentiftype`, `\currentifbranch` need the if type on the condition stack.
- Token arena grows without reclaiming (1.4M stored tokens); compaction still open.
- After latex.ltx: the snapshot (`\dump`), then ltxguide.cls on it, then the bridge to the
  native pipeline.

# Update (same day): latex.ltx runs to `\dump` with no errors

## Decision
- §367/§358 `\noexpand`: the token goes back marked (`DONT_EXPAND`, bit 30 of the token, only
  ever in the input); the read that meets it clears the mark and, if the token is expandable,
  gives `Meaning::DontExpand` (acts as `\relax`, unequal to `\relax` under `\ifx`). expl3's
  V-expansion test `\expandafter\ifx\noexpand#1#1` depends on this.
- pdfTeX is 8-bit: `Machine::read_file` returns bytes, and text is read one character per
  byte. LaTeX's `utf8.def` then works on bytes as in pdflatex (UTF-8 decoding in the engine
  had turned `’` into code 8217).
- Fonts outside the embedded set are read from `<name>.tfm` through the same resolver
  (`parse_tfm`, §560); `Metrics` is `Static` (embedded) or `File`.
- Printing follows §59: to a file or the terminal (`\write`, `\message`) a character prints
  in `^^` form when unprintable (§49 with pdflatex's `cp227.tcx`: 9–11, 32–126, 128–255
  printable) and `\newlinechar` ends the line; to a string (`\meaning`, `\detokenize`)
  characters print as they are. `\string` honours `\escapechar` (§263 `sprint_cs`).
- The primitive table now also reads the change files pdfTeX is built with (tex.ch,
  tracingstacklevels.ch, partoken*.ch, showstream.ch, pdftex.ch, SyncTeX) and one-character
  primitives (`\ `, `\/`, `\-`), and leaves out MLTeX (`\charsubdef` and kin), which pdflatex's
  format does not enable. latex.ltx tests `\ifx\charsubdef\@undefined`; with it defined, the
  whole utf8 set-up was skipped.
- `\nullfont` is `Meaning::Font(0)` (§553); `\meaning` of `\topmark` and its kin ends in `:`
  (§296), e-TeX's `\topmarks` family does not.
- `\read` from the terminal is fatal (§484), not an endless loop.
- Oracle: `engine_probe.py` runs pdfTeX with `-translate-file=cp227.tcx` (as fmtutil builds
  pdflatex), `--exprs-file` for thousands of probes, `--quiet` for differences only. The
  `format_smoke` harness memoizes `kpsewhich` lookups (`\pdffilesize` is called often).

## Rejected
- Keeping `\noexpand` as a special case in `get_x_token` only: `\expandafter\ifx\noexpand`
  never reaches `get_x_token`, so the mark must travel with the token.
- Decoding input as UTF-8: changes what `\catcode`, `\lccode` and `utf8.def` see.

## Measured
- latex.ltx: 0 errors to `\dump` (line 22467); 49.1M expansions in 6.2 s (release),
  29117 names, 38.5M tokens stored (the arena never reclaims; see Open).
- Meanings of all 21529 plainly named control sequences at the end of latex.ltx compared
  with pdfTeX: 98.0% → 99.4% after the printing and table fixes → 100% (21529/21529) after
  leaving out MLTeX.
- Rendering unchanged: parity `score --all` keeps its six documents at 100%. Three
  `math::fonts` tests fail on this machine because TeX Live basic has no `NewCMMath-Regular.otf`
  (environment, not this change).

## Open
- Token arena compaction: 38.5M stored tokens (154 MB) for one format build; TeX frees a
  macro's old text on redefinition.
- Speed: 6.2 s against pdfTeX's ~1 s for the same format build.
- `\currentiftype`, `\currentifbranch`.
- Then the snapshot (`\dump`), ltxguide.cls on it, and the bridge to the native pipeline.
