# Engine E2b: a token-level macro processor beside the text expander
Author: abhi+claude | Date: 2026-10-21
Files: `src/engine/engine_mod.rs`, `src/engine/engine_eqtb.rs`, `src/engine/engine_prim.rs`, `src/engine/engine_input.rs`, `src/engine/engine_macro.rs`, `src/engine/engine_run.rs`, `src/expand/expand_mod.rs`, `CLAUDE.md`

## Context
E2a (`2026-10-20-engine-catcodes-tokens.md`) added catcodes, `u32` tokens and the tokenizer. In the text expander (`src/expand/expand_mod.rs`), macro bodies are still text, read again with the catcodes in force at use. Class code breaks on this: a body defined under `\makeatletter` and used after `\makeatother`. About 200 call sites in the expander read bytes (`peek`, `bump`, `read_arg`, `top_rest`), so moving it to tokens in place would rewrite the whole file and could not be checked against parity step by step.

## Decision
- Build the token-level processor in `src/engine/`, beside the text expander, not by converting it. It follows tex.web; the representation is ours:
  - `engine_prim`: a static sorted primitive table, looked up by name only when a control sequence has no meaning of its own. Nothing is interned at startup (§27).
  - Meanings are kept per control-sequence id in `Eqtb`, grouped through the same save stack (`Saved::Meaning`).
  - Active characters use their own reserved names.
  - `engine_input`: the input stack (§300–§325). It holds the file `Lexer` and token-list frames. The parameter stack (§308) is one scratch token array with spans, freed in stack order. Exhausted lists are popped before a macro is fed (§390), so tail recursion uses no stack.
  - `engine_macro`: `scan_toks` for `\def`/`\gdef`/`\edef`/`\xdef` (§473–§479), and `macro_call` (§389–§400), including delimited parameters with partial-match backtracking (§397), brace stripping (§400) and skipping spaces before undelimited parameters.
  - `engine_run`: `get_x_token` and the main loop. Unexpandable tokens and control sequences the engine does not define go to an output list, which is shown as text for the existing parser.
- The text expander keeps the document body for now. Class and package code (E4) goes through the processor first. The document moves over feature by feature, each guarded by `parity.py`.

## Rejected
- Converting `expand_mod.rs` to tokens in place: about 200 byte-level call sites, a big-bang change with no parity checkpoint.
- Quoting control-sequence names in text bodies (as `LET_FROZEN` does): it keeps re-scanning text and only patches the catcode-at-definition bug.
- Eager parameter substitution into a fresh vector at each call: it copies the body each time; TeX's parameter stack does not.

## Measured
- Built as designed: `engine_prim` (18 primitives), meanings in `Eqtb`, `engine_input`, `engine_macro` and `engine_run` (`Machine`).
- What runs:
  - `\def`, `\gdef`, `\edef`, `\xdef`, with the prefixes `\global`, `\long`, `\outer` and `\protected`;
  - `\let`, `\expandafter`, `\noexpand`, `\csname`, `\string`, `\catcode`;
  - `\begingroup`, `\endgroup` and brace groups;
  - active characters;
  - TeX's errors, worded as TeX words them: "doesn't match its definition", "Paragraph ended before", extra `}`, runaway recursion.
- 15 new tests cover:
  - delimited parameters and §397 partial matches;
  - `#{`, `##`, and brace stripping;
  - catcodes fixed at definition;
  - tail recursion, which runs 3000 deep with no stack growth.
- 600k macro calls with arguments take 47ms in release, about 78ns per call, tokenizing included.
- Parity on the six documents is unchanged (strict 100%); 277 tests pass, plus the same 3 environmental failures. The binary is still 3.52MB: nothing calls `Machine` from `main` yet.

## Open
- Old macro texts stay in `Tokens` when a macro is redefined. A compaction pass is needed once class code loops through redefinitions (`\@tempa`). It can mark live spans from the meanings, the save stack and the running frames.
- E3: conditionals; internal integers in `scan_int` (`\count`, `\chardef`); `\afterassignment`; `\futurelet`.
- E4: run ltxguide.cls through `Machine`. Its output is shown as text to the existing parser.
