# Engine state and feature modules: plan
Author: abhi+claude | Date: 2026-10-20
Files: `src/engine/engine_mod.rs`, `src/engine/engine_eqtb.rs`, `src/engine/engine_names.rs`, `src/expand/expand_mod.rs`, `src/lib.rs`, `Resource_Conscious_Software_Engineering.md`


## Context
The nine new CTAN documents need class and package code (ltxguide, amsbook, IEEEtran, hyperref, csquotes, fancyvrb) to run. Today's expander rewrites strings. It has no registers beyond a `dimens` map of `f32`, no grouping, no catcodes and no real conditionals.

The user chose the hybrid route: class and package code runs as macros on a faithful engine; line breaking, math, fonts, tables and PDF stay native. The result of each class is snapshotted. Many people will work on the code later, so each feature lives in its own module.

## Decision
- tex.web gives the semantics (what a register holds, when a group restores it, how a dimension rounds). The representation is ours, following `Resource_Conscious_Software_Engineering.md`:
  - The equivalents are flat typed arrays indexed by number:
    - `ints`: count registers and integer parameters, in `i32`;
    - `dimens`: dimen registers and dimension parameters, in `i32` sp;
    - `glues`: skip registers and glue parameters, as a 16-byte `Glue`.
  - Each array grows on demand (§13), so unused registers cost nothing.
  - Group levels are `u16` arrays beside the values (hot/cold split, §8).
  - One save stack of compact entries holds (region, slot, old level, old value) and a boundary per group (§274–§283: `eq_save`, `eq_define`, `unsave`). `\global` writes straight through, as `geq_define` does.
  - Primitive parameter names (`\parindent`, `\baselineskip`, `\tolerance`) are a static sorted table found by binary search: no startup interning (§27). User names (`\newdimen\foo`) go through one interner mapping a name to a register reference.
- Feature modules, each with its own `<dir>_mod.rs` root:
  - `src/engine/`: state only: equivalents, names, save stack; later catcodes, conditionals, tokens.
  - `src/expand/`: macro expansion. It reads and writes engine state and stops owning ad-hoc maps.
  - Class and package support moves into one file per feature (`expand_lists`, `expand_counters`, `expand_refs`, `expand_theorems`, `expand_classes`) as each one is touched. No big-bang split: large files (`layout_engine.rs`, `math_layout.rs`, `frontend_parser.rs`) are split along feature lines when work next touches them, so parity can be checked at each step.
- Order of steps:
  - **E1** (now): `engine_eqtb` and `engine_names` with unit tests. The expander's `dimens` map is replaced by engine dimen registers in sp. `\newdimen`/`\newlength`/`\setlength`/`\addtolength`, `\newcount`/`\advance` and grouping by `\begingroup`/`\endgroup` and environments are wired through it.
  - **E2**: a catcode table in the equivalents, and tokens as `u32` (catcode and char, or a control-sequence id) in one arena, so macro bodies stop being re-scanned text (§9).
  - **E3**: conditionals (`\if`, `\ifx`, `\ifnum`, `\ifdim`, `\ifcase`) on tokens, plus `\csname`, `\expandafter`, `\futurelet` and `\afterassignment`, all per tex.web.
  - **E4**: class files run on the engine (ltxguide first, which unblocks three documents), then the class snapshot.
- Each step is checked by `cargo test`, then `parity.py score --all` and `exact --all`. No regression is allowed on the six passing documents.

## Rejected
- Native ports per package: endless hand work and drift from the real package code.
- One `HashMap<String, f32>` per register kind: the `f32` is lossy, it moves strings, and it has no grouping.
- Copying TeX's `mem` array and linked nodes for the equivalents: index-based typed arrays are smaller and have no pointer chasing.
- A big-bang module split now: there is a lot of uncommitted work, and parity cannot be checked mid-move.

## Open
- The snapshot format (E4): a flat sorted table in the binary or a pack, per roadmap item 17.
- A module map for collaborators in `CLAUDE.md` once the `engine` module exists.

## Measured
E1 is done:
- `src/engine/` holds `engine_eqtb` (equivalents and save stack), `engine_names` (a static sorted parameter table of 55 integer, 21 dimension and 15 glue parameters, plus user registers) and `engine_scan` (`scan_dimen_ref`, `scan_glue` with `fil` orders).
- The expander's `dimens` `HashMap<String, f32>` is gone. `\newcount`, `\newdimen`, `\newskip` and `\newlength` allocate engine registers. `\setlength` and `\addtolength` assign in sp. `\begingroup`, `\endgroup` and every environment open and close an engine group.
- The `\let` frozen-builtin marker (`LET_FROZEN`) must be stripped wherever a body is read as text; `resolve_env_alias` broke lppl's `\let\LPPLicense\document` until it did.
- Results:
  - parity is unchanged on all six documents (strict 100%; exact as before);
  - 250 tests pass; the 3 failures need a system NewCMMath and fail at HEAD too;
  - testmath: under 10ms, 6.6MB peak RSS; the binary is 3.5MB.
- The expander still scans em/ex as 11pt/5.5pt (`parse_dimension_pt`'s old defaults). This should come from the current font once font selection lives in the engine.
