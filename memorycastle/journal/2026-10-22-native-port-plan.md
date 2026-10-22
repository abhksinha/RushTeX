# Native LaTeX port: strangler migration inside the engine, phases P0–P8
Author: abhi+claude | Date: 2026-10-22
Files: `memorycastle/journal/native-port-plan.md`, `memorycastle/journal/native-architecture.md`, `src/engine/engine_run.rs`, `src/engine/engine_prim.rs`, `src/engine/engine_eqtb.rs`, `src/engine/engine_job.rs`

## Context
The user asked for a plan to port LaTeX to Rust with backward compatibility, under `native-architecture.md`.

## Decision
- **How:** each LaTeX command becomes a native engine primitive shadowing its `latex.ltx` definition. A per-command switch (`RT_NATIVE`) diffs native against macro output node by node on identical state.
- **State:** kernel state stays in `eqtb` slots with typed accessors.
- **Overrides:** internals keep their names, so documents can redefine them.
- **Expansion stays:** TeX's expansion semantics remain for documents' own macros. `latex.ltx` loading ends at P7.
- **Old pipeline:** the old native pipeline is frozen.
- **Phases and trackers:** in `native-port-plan.md`. The spec's boundary is corrected.

## Rejected
- Making the old native pipeline exact (the former N1).
- A parallel typed kernel state.
- Porting the macro-programming libraries.
- Ordering ports by corpus frequency alone.

## Measured
- The corpus loads 243 distinct `.sty`/`.cls` files; about 40% are programming libraries that need no port.
- The kernel `lt*.dtx` files total 98,635 lines.

## Open
P0 (sound oracle and tooling) is next. The unported-package policy and the pinned LaTeX version are to be decided before P7.
