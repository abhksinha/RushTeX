# E4: the format snapshot (`\dump` as our own format)
Author: abhi+claude | Date: 2026-10-21
Files: `src/engine/engine_snapshot.rs`, `src/engine/engine_eqtb.rs`, `src/engine/engine_token.rs`, `src/engine/engine_names.rs`, `src/engine/engine_font.rs`, `src/engine/engine_prim.rs`, `src/engine/engine_mod.rs`, `src/engine/engine_run.rs`

## Context
After latex.ltx ran to `\dump` with no errors and every meaning matched pdfTeX
(`2026-10-21-engine-format-run.md`), a document should start from that state instead of
running the kernel again (6.2 s). tex.web §1299–§1329 is the semantics; the form is ours.

## Decision
- `Engine::dump` / `Engine::undump` in `engine_snapshot.rs`. Each structure writes and reads its
  own private parts (`Eqtb::dump`, `CsTable::dump`, `Tokens::dump`, `Names::dump`,
  `Fonts::dump`), so no field is opened up for the format.
- Dump compacts first (`Engine::compact`): live token lists are copied into a fresh arena
  through a map from old span to new, so lists that several meanings share (`\let`) stay
  shared; dead text of redefined macros is dropped. Empty spans become `Span::default()`.
  Compaction is only safe where nothing being read points into the arena (`\dump`, between
  documents); `Eqtb::for_each_span` visits every span the equivalents hold.
- Encoding: LEB128 varints, zigzag for signed; a token is `id·2+1` (control sequence) or
  `(code·16+cat)·2` (character), so most tokens take one to three bytes. The CS hash table is
  rebuilt on load, not stored. Embedded TFMs are stored by name, file TFMs by their parts.
- A fingerprint (FNV over `Prim` names by discriminant, `PRIMITIVES`, `PARAMS`) makes a format
  from another build refuse to load. Every read is checked (lengths against the bytes left,
  spans against the arena, regions, primitive bytes), so a damaged file gives `None`.
- §1304: no dump inside a group (`Eqtb::at_top`).
- `format_smoke` with `RT_FMT=<file>` dumps, reloads and compares `\meaning` of every name.

## Rejected
- Fixed little-endian words: 5.22 MB, bigger than pdflatex.fmt; varints halve it.
- Deflating the format: our DEFLATE is an encoder only, and varints already beat pdfTeX's
  zlib-compressed format, while keeping load to one pass with no inflate step.
- Storing the CS hash slots: rebuilding them for 29k names costs well under a millisecond.

## Measured
- latex.ltx format: 38,498,056 stored tokens compacted to 368,379 live (104x).
- Size 2,510,112 bytes (pdflatex.fmt, zlib-compressed: 2,966,192). Fixed-width was 5,224,483,
  of which fonts were 2.47 MB (expl3's intarrays live in `\fontdimen`s).
- Dump 2.6 ms, load 3.5 ms (release), against 6.2 s to run latex.ltx.
- 0 of 29,117 meanings differ after loading.

## Open
- Run ltxguide.cls on the loaded format, then the class snapshot, then the bridge from engine
  output to the native pipeline.
- Hyphenation patterns (`\patterns`) and `\interaction` are not in the format yet.
