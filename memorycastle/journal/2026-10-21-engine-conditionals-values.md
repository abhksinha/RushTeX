# Engine E3: conditionals, internal quantities, register assignments
Author: abhi+claude | Date: 2026-10-21
Files: `src/engine/engine_cond.rs`, `src/engine/engine_value.rs`, `src/engine/engine_assign.rs`, `src/engine/engine_prim.rs`, `src/engine/engine_run.rs`, `CLAUDE.md`

## Context
Step E3 of `2026-10-20-engine-state-plan.md`. Class code (E4) is mostly conditionals and register arithmetic. `Machine` (E2b, `2026-10-21-engine-macro-processor.md`) had neither.

## Decision
- `engine_cond` follows §487–§510:
  - the condition stack is one byte per open conditional, holding its limit (§489 `if_limit`);
  - `pass_text` skips by meaning, without expanding, so `\newif`'s `\let` copies of `\iffalse` nest correctly;
  - `\else`, `\or` and `\fi` are checked against the limit; `insert_relax` (§510) uses a frozen `\relax` that no definition can change and that never reaches the output;
  - all 19 `\if` tests, plus e-TeX's `\unless`, `\ifdefined` and `\ifcsname`.
- Boxes, `\read` streams and modes belong to the native pipeline. So `\ifvoid` is always true, `\ifeof` is always true, and the mode is a public `Machine::mode` field (vertical by default) that the pipeline will set.
- `engine_value` follows §402–§470:
  - `scan_int`, `scan_dimen` (exact §458 ratios, `round_decimals`, `fil` orders) and `scan_glue`;
  - `scan_keyword` puts back what it read, using a fixed stack buffer rather than an allocation;
  - `scan_something_internal`: parameters, `\count`/`\dimen`/`\skip`, `\chardef` constants, `\catcode`;
  - `\the`, `\number`, `\romannumeral`, `print_scaled` and `print_spec`.
- `em`/`ex` are cmr10's until font selection lives in the engine.
- Parameter names fall back after the primitive table (`Meaning::Reg`), so `\tolerance=200` assigns with nothing interned at startup.
- `engine_assign` covers:
  - register and parameter assignment;
  - `\advance`, `\multiply` and `\divide`, with §1239 glue order rules and overflow errors;
  - `\chardef`, `\mathchardef`, `\countdef`, `\dimendef`, `\skipdef` (the name means `\relax` while its value is read);
  - `\futurelet` and `\afterassignment`.
- A name `\let` to `\relax` goes to the output as `\relax` itself. The native parser knows no aliases.

## Rejected
- Expanding while skipping a false branch: TeX's `pass_text` doesn't expand, and expanding would run macros' side effects.
- Ending `\skip3=1pt \the\skip3` before `\the`: TeX expands `\the` while looking for `plus` and prints the old value. The engine does the same; the test documents it.

## Measured
- There are 57 primitives. Tests: 292 pass, including 15 new E3 tests, among them plain.tex's `\loop` counting down. The same 3 environmental failures remain.
- Parity on the six documents is unchanged (strict 100%). Throughput: 600k macro calls take 24ms.
- Smoke runs through `Machine`:
  - ltxguide.cls: 0.3ms, 18 errors;
  - plain.tex: 0.5ms, 61 errors, 934 names, 4466 stored tokens.
- Every error cascades from TeX primitives the engine doesn't know. They go to the output without their operands, and the operands are then read as commands; for example `\write\m@ne{…}` makes `\m@ne` an assignment.

## Open
- E4 first: a static table of operand signatures for the primitives the native pipeline owns (`\write`, `\immediate`, `\mathcode`, `\lccode`, `\font`, `\fontdimen`, `\toks`, `\hbox`, `\vskip`, `\penalty` and the rest). Their operands get scanned (expanded where TeX expands them) and emitted together.
- Then `\toks` registers and `\the\toks`, then ltxguide.cls on latex.ltx's kernel macros.
- Old macro texts are still not reclaimed (see `2026-10-21-engine-macro-processor.md`).
