# Engine profile: the OS sampler, not pprof; token volume is the hot path
Author: abhi+claude | Date: 2026-10-22
Files: `scripts/profile_doc.py`, `src/engine/engine_run.rs`, `src/engine/engine_input.rs`, `src/engine/engine_node.rs`

## Context
The user asked for flame graphs to find hot loops and keep data in L1–L3 caches.

## Decision
Profile with `scripts/profile_doc.py DOC [--callers FUNC]`, which runs the release `rushtex` under macOS `sample` (1 ms) and prints self time per function (inlined callees included). Before optimising any function, confirm its cost with a direct counter or timer: profiles of inlined Rust code mislead.

## Rejected
pprof (dev-dependency, flamegraph feature) inside `format_smoke`: on macOS it attributed 40% of listings' time to `Nodes::store` copying nodes; a timer on `Nodes::store` measured 7 ms for 3.2M nodes over 3 runs (0.2%). `cargo flamegraph` needs dtrace with root on macOS.

## Measured
listings (236 pages, 3 runs, 2.4 s sampled): `get_token` 47.5% self (input stack and lexer inlined), `macro_call` 9.9%, `meaning` 6.4%, `get_x_token` 4.4%, `CsTable::probe` 3.0%, memmove 2.7%, `collect_tokens` 1.9%. `get_token` runs 351.7M times (117M per run), about 3.3 ns each: the per-token path is near its floor; the cost is the volume of tokens the listings package expands per character.

## Open
Interpreter micro-tuning can give tens of percent at most; removing token volume is the native path's job (`native-architecture.md`, kernel and package commands as Rust). Profile the native path the same way as it lands.
