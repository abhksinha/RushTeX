# Font arena: metrics scaled once, no allocation per font, fonts found by key
Author: abhi+claude | Date: 2026-10-22
Files: `src/engine/engine_font.rs`, `src/engine/engine_chars.rs`, `src/engine/engine_mlist.rs`, `src/engine/engine_hyph.rs`, `src/engine/engine_microtype.rs`, `src/engine/engine_box.rs`, `src/engine/engine_value.rs`, `src/engine/engine_noad.rs`, `src/engine/engine_ship.rs`, `src/engine/engine_vf.rs`, `src/engine/engine_show.rs`, `src/engine/engine_run.rs`, `src/data/data_tfm.rs`, `src/backend/backend_optical.rs`, `src/backend/backend_pdfread.rs`, `.github/workflows/ci.yml`

## Context
The user (2026-10-22) asked for zero overhead and zero garbage collection when reading fonts, and for fonts not to hurt incremental compile. After D2:
- every `\font` allocated a `String` name and two `Vec`s of parameters;
- every character's dimensions built a `Scaler` and scaled four fix_words, as did every kern;
- `find_font` compared name strings against every loaded font.

## Decision
- **One append-only arena of scaled values** (`Fonts::info`), as TeX's `font_info` (§549–§576).
  - At load, a font appends its widths, heights, depths, italic corrections and kerns, scaled once (§571–§575, with the same `Scaler`), then its parameters.
  - Parameters come last so that §579's growth of the last-loaded font's `\fontdimen` appends in place.
  - A `Font` holds bases into the arena; a character's dimensions are its `char_info` word plus four indexed loads, and a kern is one load.
- **No allocation per font in the steady state.** A pinned font's name is a `&'static str` borrowed from the table (`Cow::Borrowed`). Only a font outside the table, or an expanded copy, owns its name. The record, the arena and the key map grow with amortized pushes.
- **Found by key.** `find_font` looks up (hash of name, size) in a map, then checks the name, keeping the first match (§1260); no scan.
- **Incremental (pillar 4).** The arena and the font list are append-only, so a checkpoint is their two lengths and a restore truncates. Session-stable font numbers (a font keeps its number across recompiles) are wired in P8, by seeding the key map from the previous compile; nothing here prevents it.
- **Microtype's expanded copies** (`push_expanded`, pdfTeX's `auto_expand_font`) share the base's scaled arrays and get their own parameters. `Font::expanded` applies `1000 + expand` on use, exactly as before.
- **Bounds:** `Font::kern` returns zero past the font's kerns, never the next array, as the old `try_get` did.
- **Format:** `undump` loads each font again through `Fonts::push`, so the arena is rebuilt, never stored, and the format layout is unchanged.

## Rejected
- **Scaling on every use** (the D2 state): a multiply-and-divide sequence per character and per kern on the hottest path.
- **A `Vec` of parameters per font:** one allocation per `\font`, and §579's growth would reallocate.
- **Interning every name:** pinned fonts need no copy at all; the table already holds the name.
- **Growing the arena by doubling after the format.** The LaTeX format holds 48 fonts, one of which has about 610,000 `\fontdimen` words: expl3 keeps data in a font's parameters. Doubling took the arena to 1.1M words (4.4 MB), +2 MB RSS against D2's exact `Vec`. Now `undump` fits it exactly and reserves `ARENA_HEADROOM` (65,536 words) for the document's own fonts.

## Measured
- Arena: small2e loads 96 fonts in total; their scaled arrays are 7,658 words. The rest of the arena's 620,510 words is that one font's parameters.
- Instructions retired (`/usr/bin/time -l`), against D2 built from `dee56bd`. The machine was loaded by another process, so wall times are not comparable.
  - testmath: 2.776G down to 2.732G (−1.6%).
  - listings: 66.34G down to 65.87G (−0.7%).
  - small2e: flat.
- Peak RSS, median of 5 runs:
  - small2e: 21.6 MB to 21.2 MB;
  - testmath: 24.5 MB to 23.8 MB;
  - listings: 46.4 MB to 45.4 MB.
- Tests: 347 pass, including `arena_reuses_and_scales_once`.
- Clippy is clean with `--all-targets`, with `--no-default-features` and with `--features gen`. `--all-targets` had already been failing in `backend_optical.rs` and `backend_pdfread.rs` (a test module before other items); both are fixed, and CI now runs `--all-targets` and `gen`.
- Gate: zero shift 85/85; leak audit 85/85 under `RT_NO_TFM_READS=1`.

## Open
- Session-stable font numbers for incremental compile (P8): seed `by_key` from the previous compile.
- Fonts outside the table still use a linear §1260 search, because their design size is unknown until loaded. They are rare and none are in the corpus.
