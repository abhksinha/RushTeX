# Zero-GC core: TeX ownership on size-class pools (model M)
Author: abhi+claude | Date: 2026-10-22
Files: `src/engine/engine_node.rs`, `src/engine/engine_chars.rs`, `src/engine/engine_hyph.rs`, `src/engine/engine_image.rs`, `src/engine/engine_microtype.rs`, `src/engine/engine_snapshot.rs`, `src/engine/engine_xform.rs`, `src/engine/engine_box.rs`, `src/engine/engine_page.rs`, `src/engine/engine_eqtb.rs`, `src/engine/engine_gc.rs`, `src/engine/engine_linebreak.rs`, `src/engine/engine_run.rs`, `src/engine/engine_job.rs`, `Cargo.toml`, `memorycastle/journal/native-architecture.md`, `memorycastle/journal/progress.md`
Supersedes: 2026-10-21-engine-boxes-b1.md

## Context
The user requires truly zero garbage collection with predictable growth. Native code builds its node lists into the engine's typesetting core, so the core's memory model comes first: the user chose "core first, then P1". The b1 note's "`\copy` shares the box" depends on tracing to free anything, and is reversed here; the rest of b1 stands.

## Decision
Model M: every node list and record has one owner, and is freed at the point TeX frees it.
- **Storage.** Lists stay contiguous spans of `Nodes::list`, with no pointer chasing. Span blocks are allocated by size class (exact lengths 1–16, then powers of two). A freed block goes onto its class's free list, and live addresses never move. `BoxRec`, `Glue`, rules, discretionaries and insertions each keep one free list of fixed-size slots.
- **Ownership.** `\box`, `\unhbox`/`\unvbox`, `\lastbox` and `\vsplit` move what they take. `\copy`, `\unhcopy` and `\unvcopy` deep-copy (§204 `copy_node_list`).
- **Freeing (§202 `flush_node_list`).** At `\shipout` (§638), a `\setbox` overwrite, a group restoring a box register (§283), and discarded nodes (§1105, line-break discards).
- **Build buffers.** `cur` and the line-break scratch are reused buffers reset per list. They are §6a's regions; no arena memory belongs to a region.
- **Checker.** An ownership-check build poisons freed slots and panics on reading one, and the 85-document gate runs with it.

## Rejected
- Tracing collectors (`collect_nodes`, `collect_tokens`): deleted once M covers every site.
- Region-owned arena memory (paragraph and page regions holding nodes): `\setbox` mid-page followed by `\box` into the page crosses regions and would need a copy or leave a dangling index.
- Reference counts on boxes: every holder would have to count, and copying is what TeX does anyway.
- Linked lists (as in tex.web): pointer chasing, already rejected in b1.

## Open
Z3 (document macros in size-class token blocks, §275 `eq_destroy`, §390 reference held while reading a body) follows Z1 under the same rules.

## Measured
Z1a, allocator half (2026-10-22):
- `Nodes::store` allocates by `size_class`, and `Free` keeps per-class block starts plus one slot list per side table (`add_box`, `add_disc`, `add_insert`, `add_rule`, `glue_node`).
- `lig_node_at` moves the characters into a freed block of their class.
- Every direct `push` onto a side table now goes through these (`engine_chars`, `engine_hyph`, `engine_microtype`, `engine_page`, `engine_image`, `engine_xform`).
- Until Z1d deletes it, the oracle's `collect_nodes` keeps whole blocks live (`Nodes::block`) and clears `Free`.
- Nothing is freed yet. Gate: 85/85 zero shift, tests and suites pass. The ownership-check build is still to come.

Z1c, first free points (2026-10-22):
- Free points wired:
  - `ship_out` flushes the shipped box;
  - `set_box` and `unsave` release replaced boxes per §277/§279/§283 through `kill_box` and `take_dead_boxes`, and the machine flushes them between commands (`flush_dead_boxes`);
  - `unpackage` frees the moved list's block and record;
  - `alter_box_dimen` and `with_shift` change the record in place;
  - `vsplit` frees the split box;
  - `prune_page_top` flushes its discards;
  - `post_line_break` flushes break glue, discards and a discretionary's replaced nodes, freeing its texts' blocks (`free_disc`);
  - `line_break` flushes the final glue (§816);
  - `\unskip` flushes;
  - page discards (§1000) are flushed;
  - `fire_up` frees consumed insertions (`free_insert`) and the class box;
  - `box_error` cases are flushed.
- `just_box` is no longer a root.
- Leak audit (`RT_LEAKS`, `node_leaks`, and in the checker build `leak_sites` grouping by the `#[track_caller]` origin of each block, box and glue). On listings, before → after: boxes 6,581 → 201, glues 6,206 → 30, discretionaries 133 → 0, insertions 6 → 0.
- Checker build: all 85 documents run with no double free and no use-after-free. Gate 85/85.
- Still leaking (listings): math lists (`engine_noad.rs:532`), alignment unset rows and entries, `box_end`, ligature characters.
