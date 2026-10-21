# CTAN corpus work plan: source-driven, engine state first
Author: abhi+claude | Date: 2026-10-20
Files: `src/expand/expand_mod.rs`, `src/frontend/frontend_parser.rs`, `src/config/config_mod.rs`, `scripts/refindex.py`, `scripts/parity/parity.py`

## Context
The ten new CTAN documents do not compile. `parity.py gaps` lists 122 unknown commands, 13 environments and 6 error kinds. Most of them are engine state, not missing macros:
- length registers (`\textwidth`, `\baselineskip`);
- font selection (`\fontfamily`, `\selectfont`);
- conditionals (`\if..\fi`);
- primitives (`\hskip`).

The user asked whether to engineer from `tex.web` or to use trial and error, and to prepare the tooling so future work is easy.

## Decision
- Source-driven only. Each gap is looked up with `scripts/refindex.py`, which covers:
  - the tex.web §, procedure, primitive and handler;
  - the executed `latex.ltx` and the commented `.dtx`;
  - the exact class, clo and fd files the corpus loads.
- Each fix is checked with `parity.py boxes` and `lines`, and guarded with `score --all`.
- Experiments are only for narrowing down a drift (`repro`), never for choosing a fix.
- Code comments cite the § or macro, so `refindex.py` maps TeX back to rushtex.
- Order of work:
  1. Engine state, modelled on tex.web's equivalents table:
     - regions §222–§247: count, dimen, skip and toks registers and parameters;
     - grouping with `eq_define` (§277), `eq_save` (§276) and `unsave` (§281);
     - assignments in `prefixed_command` (§1211), `do_register_command` (§1236) and `scan_dimen` (§448).
     It replaces the expander's ad-hoc `dimens` map and the hard-coded names. Layout config reads it.
  2. Font selection from `ltfssbas.dtx` and `ltfsstrc.dtx` (`\selectfont`, `\fontfamily`) over the existing design-size faces.
  3. The ltxguide class (252 lines): `\LaTeXe`, `\copyright`, `decl` and `\m`/`\arg` from ltxguide.cls and `ltlogos.dtx`. This unblocks five documents.
  4. The expander depth errors (amsldoc `\cs`, short-math-guide `\expandafter`): these are probably catcode or `\csname` semantics, not real recursion.
  5. Environments: `picture`, `tabbing`, `minipage`, `tabularx`.
  6. IEEEtran (6347 lines, two columns, Times), last.
- Fonts (T1 EC, Latin Modern, Times) are one batch after step 3. Until then, documents that use those fonts score on positions only.

## Rejected
- Trial and error: per-document guesses conflict across documents and build up per-case tweaks, which the math blueprint forbids. It also cannot find missing machinery such as registers.
- Adding unknown command names one by one: the list never ends (122 already), and every new document adds more.

## Open
- To decide with the user before step 1:
  - how far class files run as macros on the new engine state;
  - which parts are reimplemented natively (line breaking, math and PDF stay native for speed).
- `parity.py gaps` and `score --all` should be rerun after each step. Each step's note should record the change in documents blocked.
