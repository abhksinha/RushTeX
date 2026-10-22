# A0: the `oracle` cargo feature as the product/oracle boundary
Author: abhi+claude | Date: 2026-10-22
Files: `Cargo.toml`, `src/main.rs`, `src/engine/engine_mod.rs`, `src/engine/engine_job.rs`, `src/engine/engine_host.rs`, `src/engine/engine_snapshot.rs`, `src/engine/engine_run.rs`, `src/engine/engine_eqtb.rs`, `src/engine/engine_font.rs`, `src/engine/engine_token.rs`, `src/engine/engine_names.rs`, `src/engine/engine_hyph.rs`, `src/engine/engine_tounicode.rs`, `.github/workflows/ci.yml`, `memorycastle/journal/progress.md`

## Context
First item of Phase 1 (`progress.md`, `native-architecture.md` §2 layer 7). The product and the oracle share one crate, and nothing stops product code from depending on `latex.ltx` interpretation, the format build or TeX Live lookup. That is the entanglement that would make P7 (oracle out of the release) a rewrite instead of a flag flip.

## Decision
- **Feature:** `oracle` in `Cargo.toml`, in `default` until P7. Until the native port lands, the engine on the LaTeX format is the only exact path the `rushtex` command has, and releases must not lose it. At P7 the default flips; the boundary itself exists from now on.
- **Behind the feature:**
  - `engine_job` (runs until written files repeat);
  - `engine_host` (kpathsea, `kpsewhich`);
  - `engine_snapshot` (format dump and load), plus every per-module `dump`/`undump` (`Eqtb`, `Fonts`, `CsTable`, `Tokens`, `Names`, `Hyph`, `GlyphUnicode`, `Table`);
  - `compile_engine` and the format build in `main.rs`;
  - the `format_smoke` harness.
- **Stays in the product:** the typesetting core, and the expansion machinery for the document's own macros (`engine_macro`, `engine_cond`, `engine_value`, `engine_scan`, `engine_run`), with `Engine::default` as the initial state.
- **Without the feature,** the command falls back to the existing path the product has today (`--native`).
- **CI enforces it:** `cargo build --no-default-features`, `cargo clippy --no-default-features -- -D warnings` and `cargo test --no-default-features --lib`, beside the default build. A product file that reaches oracle code fails to compile.

## Rejected
- **Off by default now:** it would drop exact output from releases before native code replaces it.
- **A separate crate for the oracle now:** it moves 20k lines and every private field access across a crate boundary, a large churn that the feature gate gets for a fraction of the cost. It can follow if the gate proves leaky.
- **Gating only `engine_job`/`engine_host`:** the format dump and load is oracle work too (the native product has no format), and leaving it compiled would keep the snapshot codec alive as accidental product code.
