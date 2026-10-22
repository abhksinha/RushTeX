# Algorithm audit: what to replace, what to keep
Author: abhi+claude | Date: 2026-10-22
Files: `src/engine/engine_macro.rs`, `src/engine/engine_linebreak.rs`, `src/engine/engine_regex.rs`, `src/engine/engine_token.rs`, `src/engine/engine_hyph.rs`, `src/engine/engine_mlist.rs`, `src/engine/engine_job.rs`, `src/engine/engine_font.rs`, `src/backend/backend_deflate.rs`, `memorycastle/journal/progress.md`

## Context
The user asked whether any algorithm in use is truly unoptimised. This is the answer from reading the code. It feeds B5, A2, H2 and D5 in `progress.md`.

## Decision
**Replace:**
- macro interpretation, by native code (P1–P6);
- whole-document reruns in `engine_job`, by single-run references (P2b);
- `collect_tokens`, by token model M (Z3);
- per-use width recomputation in `passes` (a font lookup per node, per pass), by per-paragraph SoA widths with prefix sums (H2);
- the `END_MATCH` scan of the parameter text on every `macro_call`, by a pattern length stored in `Macro` (B5);
- mid-vector `insert`/`remove`/`splice` in protrusion and math (B5);
- per-conversion `Vec<Node>` copies in math (B1);
- `\pdfmatch`'s backtracking matcher, exponential in the worst case, by a linear-time matcher keeping pdfTeX's match rules (A2);
- static names in `CsTable`, by the fingerprinted perfect hash (D1);
- per-occurrence hyphenation, by a word cache (D4b);
- run-time TFM parsing, by static tables (P1).

**Keep:**
- Knuth–Plass (optimal, and exactness requires it);
- the hyphenation trie, built per used language with sorted children and binary search (Liang);
- size-class node pools;
- flat `eqtb` arrays;
- the `ls-R` index;
- the single-probe 32 KB DEFLATE hash table: fast and L1-resident on the floor machine. Its compression ratio is to be compared with pdflatex's PDF sizes in D5.

**Measure before deciding:** ligature and kern program walks (§557). A per-font 256-bit start gate is used only if B4 shows them hot.
