# Engine E2a: catcodes, tokens, tokenizer
Author: abhi+claude | Date: 2026-10-20
Files: `src/engine/engine_catcode.rs`, `src/engine/engine_token.rs`, `src/engine/engine_lex.rs`, `src/engine/engine_eqtb.rs`, `src/engine/engine_scan.rs`, `src/expand/expand_mod.rs`

## Context
Step E2 of `2026-10-20-engine-state-plan.md`: class and package code needs real catcodes and tokenized macro bodies before conditionals (E3) and class files (E4) can run.

## Decision
- `CatTable` is a fixed 256-byte array starting in latex.ltx's state: the §232 IniTeX defaults, then latex.ltx lines 307–316, with `~` active and `@` other. Characters above 255 read as other.
- `Eqtb` groups catcodes through the same save stack as the registers (`Saved::Cat`).
- A `Token` is one `u32`:
  - character tokens are `cat << 21 | char`; control sequences are `CS_FLAG | id`;
  - `out_param`, `match` and `end_match` reuse catcodes 5, 9 and 14, which the tokenizer never emits.
- Active characters stay character tokens, not control sequences.
- `CsTable` interns each name once: one text buffer, a span per id, an open-addressing table of `u32` ids (FNV-1a, load at most 1/2).
- Token lists live in one `Tokens` arena, addressed by `Span`.
- `Lexer` is `get_next` §343–§357:
  - a reused line buffer; trailing spaces dropped and `\endlinechar` appended (§31);
  - the three states;
  - `^^` reduction, including inside names (§352, §355);
  - `\par` on a blank line;
  - catcodes read at scan time.
- `scan_int` follows §440–§445: signs, `` ` `` constants, octal, uppercase-only hex, internal integers, one optional space.
- Expander wiring:
  - `at_letter` is gone; `\makeatletter`, `\makeatother` and `\catcode` assign the engine table, locally.
  - `Input` keeps a copy of the table, refreshed after each change and each group end. `control`, `read_token` and conditional skipping scan control words with it.

## Rejected
- Catcode 13 for `match`, as TeX does: our active characters are catcode-13 character tokens.
- A `HashMap<Box<str>, u32>` for names: it stores each name twice; the arena plus id table stores it once.
- Per-character catcode levels for characters above 255: pdfTeX allows only 0–255.

## Measured
- Parity on the six passing documents is unchanged:
  - strict 100%;
  - exact: cfgguide 99.95%, lppl 100%, modguide 99.99%, sample2e 99.97%, small2e 100%, testmath 99.98%.
- 264 tests pass; the same 3 fonts tests fail at HEAD (they need a system NewCMMath).
- testmath: 0.01s, 6.6MB peak RSS; the binary is 3.52MB.

## Open
- E2b: the expander's `Input` should read tokens from `Lexer` and store macro bodies as `Span`s. Bodies are still text, re-scanned with the catcodes current at use, not at definition.
- `define_let`, `wrap_dimen` and the `LET_FROZEN` path still treat `@` as a letter.
- The corpus blockers are now `\NewDocumentCommand` (clsguide, usrguide), `\and` (encguide), `picture`, `minipage` and the short-math-guide depth.
