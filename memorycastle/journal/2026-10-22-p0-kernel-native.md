# P0/P1: the native-command layer, and `\get@external@font` as its first command
Author: abhi+claude | Date: 2026-10-22
Files: `src/engine/engine_kernel.rs`, `src/engine/engine_prim.rs`, `src/engine/engine_run.rs`, `src/engine/engine_page.rs`, `src/engine/engine_show.rs`, `src/engine/engine_snapshot.rs`, `src/engine/engine_mod.rs`, `src/data/data_nfss.rs`, `assets/tables/nfss.rtdb`, `scripts/parity/kernel_diff.py`, `.github/workflows/ci.yml`

## Context
After D2, the native port begins (`native-port-plan.md` §2, the strangler method). P0 owed the per-command switch (`RT_NATIVE`), a state dump (`RT_STATE`) and a macro-against-native gate. The first command proves the whole loop and gives the D2 NFSS table its consumer. On advice, the first command is `\get@external@font`, not `\selectfont`: its contract is small (it reads `\f@encoding`…`\f@size` and defines `\external@font`), and `NfssTable` already computes it, proved on 12,027 cases.

## Decision
- **`Meaning::Kernel(u16)`**, separate from `Native` (pdfTeX extras):
  - unexpandable, and dispatched in the main loop's `command` through a static table of `fn(&mut Machine, Token)` (`engine_kernel::KERNEL`);
  - it has a snapshot tag, and `\meaning` prints its name.
- **Binding per job, not in the format.** `start_job` calls `bind_kernel`. `RT_NATIVE=name,…|all` rebinds each name to its kernel meaning after the format loads.
  - The replaced macro is kept under `rt@orig@<name>`, so the token collector keeps its span current.
  - The native code expands that macro for every case it does not settle itself.
  - A document or package that redefines the name simply replaces the kernel meaning, so overrides cost no check.
  - Package-defined commands (P6) will need binding at definition time instead.
- **`\get@external@font` natively only where nothing else happens.** Conditions:
  - `\curr@fontshape` has the format's text (one slice comparison);
  - the four shape macros and `\f@size` hold characters only;
  - the shape's spec is the table's: the spec control sequence's name hash equals `nf.h`, checked once per spec ID and cached;
  - the first entry holding the size uses `""`, `s`, `gen`, `sgen`, `genb` or `sgenb` with no scale (`NfssTable::direct`).

  Then `\external@font` is defined locally, with the catcodes the macro's `\edef` gives. Every other case (substitution, `sub` chains, scales, `fixed`, unknown shapes) writes a message, loads an `.fd` or sets `\font@submax`, so the macro runs it.
- **The scratch rule.** LaTeX's temporaries are left out of the state comparison (`SCRATCH`: `reserved@a–f`, `font@info`, `mandatory@arg`, `lower@bound`, `@tempdimb`…). Native code does not reproduce scratch assignments. The call sits inside `\define@newfont`'s group, so they are gone at `\endgroup` anyway.
- **`RT_STATE=<file>`:** at each `\shipout` and at the end, every control sequence's `\meaning` and every nonzero register, as sorted text. Every name a kernel command can take is skipped in both runs.
- **The gate is `scripts/parity/kernel_diff.py`.** Each corpus document runs on the macros and with `RT_NATIVE`, and the PDF bytes and state dumps must be identical. This is stronger than a box diff, so `engine_boxdiff.py` needs no engine-against-engine mode.
- **New pin.** `texlive-2026-pin-2` adds the TFM files that D2's font table reads. CI's regeneration step failed on pin-1, which predates them.

## Rejected
- **Binding in the format:** the format is pdfLaTeX's oracle, and the switch must compare both on identical state.
- **Native for the substitution and `sub` paths now:** they write log messages and load files. That is P1's `\selectfont` work, where the `.fd` loads go away as a whole.
- **Keeping the original `Macro` in `Machine`:** its span would go stale when the token arena is collected.

## Measured
- `kernel_diff.py all`: 85/85 documents identical in PDF bytes and state. 10,692 of 12,776 calls (84%) were settled natively; the rest went to the macro.
- A document that redeclares `OT1/cmr/m/n` as `cmtt10` gives identical output: the spec check sends that shape to the macro.
- `nfss.rtdb` is 61,240 bytes (was 56,184, plus the 8-byte spec hashes). Tests: 350 pass; clippy clean in three configurations.
- Without `RT_NATIVE` the engine is unchanged: one `is_some` test per `\shipout` and per job.

## Open
- **Payoff timing.** Strangling one command proves correctness but removes little interpretation. The cost goes away when the whole ltfss cluster is native and no macro reads `\f@size`, `\curr@fontshape` or the shape macros, so the `.fd` loads can stop. The cluster boundary needs a trace of which macros read those names.
- **Next:**
  - `\selectfont`'s body;
  - `\set@fontsize` and the size commands;
  - `\define@newfont`, with the `.fd` loading taken from the table (family code from `nf.fr`);
  - then P2.
- Log messages are not part of the gate. Native code takes only the cases that write none.
