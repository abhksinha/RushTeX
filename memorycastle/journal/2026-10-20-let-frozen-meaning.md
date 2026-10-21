# \let copies a builtin's meaning, not its name
Author: abhi+claude | Date: 2026-10-20
Files: `src/expand/expand_mod.rs`

## Context
amsldoc does `\let\oldcs\cs \def\cs#1{..\oldcs{#1}..}`. `define_let` stored a by-name alias to `\cs` when `\cs` was not a macro, so the redefined `\cs` called itself, giving 347 "macro nesting too deep" errors.

## Decision
`\let` to a builtin control word stores `\<U+0001>name` (`LET_FROZEN`). On that marker, `control` calls `dispatch_builtin`, which skips the macro table. This keeps the meaning from `\let` time (tex.web §1221).

## Measured
amsldoc: 350 expansion errors went to 0. Its remaining blockers are the amsldoc/amsbook class, hyperref, imakeidx and the `\cs`/`\pkg`/`\env` macros.
