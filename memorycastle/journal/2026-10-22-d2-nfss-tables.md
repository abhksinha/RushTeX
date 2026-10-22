# D2 (NFSS): font shapes as a generated table, proved against LaTeX's own selection
Author: abhi+claude | Date: 2026-10-22
Files: `src/data/data_nfss.rs`, `src/data/data_mod.rs`, `src/bin/rushtex_gen.rs`, `src/engine/engine_job.rs`, `assets/tables/nfss.rtdb`, `.github/workflows/ci.yml`

## Context
D2's second slice, after the fonts (`2026-10-22-d2-font-tables.md`): the `.fd` files that map an NFSS shape and size to a TFM name. It is built in the storage form of `native-architecture.md` §3a. P1's native `\selectfont` will read it. A census of the 75 pinned `.fd` files found that 21 of them hold macro code, not only declarations:
- scale options set by their package (bera's `\fve@@Scale`, libertine's `@scale`);
- shape generators (`\EC@family` in `t1cmr.fd`);
- conditionals.

A static parser of `.fd` text would therefore be wrong for those files.

## Decision
- **Evaluate, don't parse.** `rushtex-gen nfss` runs the engine on the LaTeX format and reads every `.fd` file with `\DeclareFontFamily` and `\DeclareFontShape@` wrapped. Each declaration is written out after its own definition has run.
  - The size spec is recorded as the control-sequence name LaTeX makes of it (`\csname #5\endcsname`, so expanded at declaration), and the series as `\DeclareFontShape@` normalises it.
  - Only the spec's small fixed grammar is parsed: `<size>` and `<lo-hi>`, then `[fn*][[scale]]arg`.
- **Layout (§3a, least data per selection):**
  - shape and family names go through perfect hashes (`nfsh`, `nffa`), with shape IDs in name order so a family's shapes are adjacent;
  - each shape's entries are contiguous 16-byte records, four to a cache line: `lo`, `hi` (or a single size's text), the scale factor, the argument, the size function and the kind;
  - one string pool, each string stored once.

  A selection reads a hash slot, one row offset, a shape's 1–14 entries (one to four lines) and one name.
- **Exact semantics from ltfsstrc.dtx.** `NfssTable::external` follows `\get@external@font`:
  - entries are tried in declaration order: a single size by `\ifdim` equality, a range as `lo <= size < hi` with an upper bound of 0 meaning `\maxdimen`;
  - with no match, the nearest single size is taken, the first of equals, and found again by its text (`\try@size@substitution`, `\try@simple@size`);
  - all 13 size functions, `gen` and `genb` names built from the size's text, the scale computed with §455's arithmetic, and `sub`/`ssub`/`alias` chains followed.
- **Family loading code.** `\hyphenchar` and `\skewchar` (decimal, octal, `\m@ne`, with or without `=`) are read into numbers. Other code (`\qpl@set@spacing`, `\SetFourierSpace`) is kept as text, for a native package or the engine fallback to run.
- **Self-check against behaviour, not text.** The generator runs LaTeX's own `\get@external@font` on every shape at the standard sizes, each declared size and bound, and sizes between and beyond them, then compares every `\external@font` string with the table's.
- **CI regenerates both blobs** (fonts and NFSS) from the pin in the parity job and requires identical bytes (D2c). A file missing at its recorded path is found by name through kpathsea, so the generator runs on the unpacked snapshot.
- **`engine_job::run` now gives `inspect` the last run's written files** in `closed`. Before, they were already moved out, so `inspect` saw nothing.

## Rejected
- **A static `.fd` parser:** wrong for 21 of the 75 files.
- **Storing the spec text and parsing it at run time:** that is work at use that build time can do.
- **Resolving names to font IDs in the table:** many `.fd` names are fonts the corpus never loads, so they are not in `fonts.rtdb`. A selection looks the name up once, and P1 caches it.
- **Folding `\fontsubfuzz` warnings and `\do@subst@correction` into the data:** they are selection behaviour and belong to P1.

## Measured
- 75 `.fd` files, 75 families, 630 shapes, 1,646 entries; `assets/tables/nfss.rtdb` is 56,184 bytes, 5,747 of them strings.
- Self-check: 12,027 (shape, size) cases equal LaTeX's `\get@external@font`, with no case where no font is found. They cover `genb` (`ecrm1095`), nearest-size substitution (`ecrm1095 at10.95pt` for 11pt), `ssub` chains, ranges (`ec-lmr12` at 11pt) and the `[.92]` scale.
- Generation, both runs: 0.74 s. Regeneration is byte-identical, also with every recorded path moved, through kpathsea.
- Tests: 349 pass; clippy clean with `--all-targets`, without `oracle` and with `gen`.

## Open
- **Consumer:** P1's native `\selectfont`, with a front cache of recent selections. Until then the engine reads `.fd` files as before, so this slice cannot change any output.
- **D2 still owes the package registry** (each `.sty`/`.cls`: options, `\RequirePackage` edges, defined commands). It is the third D2 slice, and it is needed by D3's pre-scan and F1's fallback decision.
- The format fingerprint does not include the NFSS table, because the engine does not read it. It will at P1.
