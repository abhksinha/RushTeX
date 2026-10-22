# D2 (fonts first): `rushtex-gen` and the TFM table, one metrics representation in the engine
Author: abhi+claude | Date: 2026-10-22
Files: `src/bin/rushtex_gen.rs`, `src/data/data_tfm.rs`, `src/data/data_csr.rs`, `src/data/data_mod.rs`, `src/engine/engine_font.rs`, `src/engine/engine_chars.rs`, `src/engine/engine_mlist.rs`, `src/engine/engine_microtype.rs`, `src/engine/engine_hyph.rs`, `Cargo.toml`, `assets/tables/fonts.rtdb`, `memorycastle/journal/progress.md`

## Context
Pillar 2: font metrics as build-time tables. P1 consumes them first. Today the engine has three metrics sources:
- `Metrics::Static`: the frozen pipeline's Rust arrays in `backend_tfm_data.rs`, 128 characters of the CM fonts;
- `Metrics::File`: a TFM parsed into `Vec`s per load;
- the format's copy of the `File` arrays.

That is three representations of one thing.

## Decision
- **`rushtex-gen`.** A dev binary (`src/bin/rushtex_gen.rs`, `required-features = ["gen"]`). `rushtex-gen fonts` reads the pinned TFMs (the `.tfm` rows of `scripts/texlive/files.tsv`) and writes `assets/tables/fonts.rtdb`.
- **Layout, by access pattern** (user, 2026-10-22: "are we writing font files truly in an optimised way?"). A font is read whole, never one array across fonts.
  - A perfect hash (`data_phf`) maps the TFM name (`cmr10`) to a font ID.
  - One 64-byte record per font, one cache line: `bc`, tier, design size, and nine (offset `u32`, length `u16`) into one word region.
  - The word region is laid out font after font, so a font's arrays are contiguous.
  - An array equal to one already written is not written again; the record points at the first. This covers a family's lig/kern program at another size, and equal depth and italic arrays.
  - Words are little-endian quads read with `u32::from_le_bytes`, a plain load.
- **Popularity order and tiers** (user: "font_package_1 (n1 most popular), font_package_2 ..., load only what is needed").
  - IDs follow how many corpus documents load the font (`RT_FONTS` in `format_smoke`, kept in `scripts/texlive/font-use.tsv`), then name. The fonts most documents use share the first pages.
  - Tier 1: loaded by 2 or more documents. Tier 2: loaded by 1. Tier 3: unused. The tier is in each record.
  - In RAM, tiers inside one embedded blob gain nothing over popularity order: the OS pages in only what is touched.
  - Tiers pay in shipping: tier 1 embedded uncompressed, later tiers as separate or compressed blobs fetched on demand (WASM and web download size). That split comes with D5, against a size budget; the record's tier field makes it a generator change, not a format change.
- **Self-check.** After writing, the generator re-reads the blob through the runtime reader. For every font it compares every word with `parse_tfm` of the source file, and fails on any difference.
- **One word layout, two owners.**
  - `Metrics` is `None`, `Table(TfmView<'static>)` or `File(Rc<FileTfm>)`. Both views hold `[[u8; 4]]` quads, each a word's value in little-endian order.
  - `TfmView` holds the nine resolved row slices, taken once at `\font` load (`Csr::row_slice`). The per-character paths never see the CSR.
  - `FileTfm`'s arrays become `Vec<[u8; 4]>`, for a TFM outside the table.
  - `lig_kern()` matches once and returns two quad slices; the four walkers then read `u32::from_le_bytes` with no further branch. There is no per-word match: do not "fix" it into one.
  - `Metrics::Static` goes from the engine; `backend_tfm_data.rs` stays only for the frozen pipeline (retired at P7).
- **Byte order.** TFM files are big-endian; `parse_tfm` yields values, and every store keeps the value as a little-endian quad. `exten(r)` returns `u32::from_le_bytes(q).to_be_bytes()`: §546's top, mid, bot, rep byte order.
- **Format identity.** The format stores a table font by its ID, and a file font's words. The blob carries a 64-bit content ID (section `fonts.id`, a hash computed by the generator). `engine_snapshot::fingerprint` takes it in, so a regenerated blob refuses old formats and they are rebuilt; IDs cannot silently shift.
- **Generator input and check.** The generator takes a list of TFM paths (`--list FILE`); for the pin, that is the `.tfm` rows of `files.tsv` or the unpacked snapshot. `rushtex-gen fonts --check` re-reads the blob and compares it with the sources: the pillar 2 gate, which CI can run on the snapshot. The gate adds `cargo clippy --release --features gen -- -D warnings`.
- **Proof.** Zero shift 85/85, plus `RT_NO_TFM_READS=1` in `format_smoke`, which panics when a `.tfm` is read through the file callback. Every corpus document must run without one.

## Rejected
- **Copying table rows into `Vec`s on first use.** That is a parse by another name, with a copy per used font.
- **An enum of `File` and `Table` in the hot walkers.** That puts a branch per lig/kern word; one quad representation has none.
- **Generated Rust arrays (like `backend_tfm_data.rs`).** That is the second table format D1 rejected.
- **Compressing the blob now.** Compression is measured in D5 per tier, against binary size; tier 1 stays uncompressed.
- **Nine CSR pools by font (pool-major, the first cut).** It was SoA across fonts, the wrong axis. A font's first use touched about 9 distant regions, and identical arrays were stored per font (1.38 MB).
- **Physical tier blobs now.** With every tier embedded they buy nothing; they come with the download and compression work.

## Measured
- `rushtex-gen fonts`: 354 fonts. The pinned corpus TFMs plus 57 Computer Modern TFMs the engine had taken from `backend_tfm_data.rs`; those are now in `files.tsv` and listed in `scripts/texlive/fonts.txt`.
- Every word is checked against its TFM.
- Size:
  - pool-major first cut: 1,375,440 bytes;
  - font-major with shared arrays: 895,616 bytes (487,592 bytes saved, 35%).
- Popularity over the 85 documents (`RT_FONTS`): 353 of the 354 fonts are loaded by some document. Tiers 238 / 115 / 1.
  - Microtype's expanded copies (`ec-lmr10+20`) are counted under their base font.
- Gate:
  - tests 346 pass; clippy clean with and without `oracle` and with `gen`;
  - zero shift 85/85;
  - leak audit 85/85 under `RT_NO_TFM_READS=1`, so no corpus document reads a `.tfm` file. A TFM looked for and not found still passes, as in encguide.
- Time is unchanged (listings 2.89 s, testmath 0.136 s).
- Listings' peak RSS was 48.8 MB against 45 MB in the Z1 note, measured another day. The table can explain at most 0.9 MB of the difference. Recheck in P1's measurements.

## Open
- Next, as the user asked (2026-10-22): zero overhead and zero GC on the font path, plus stable font numbers for incremental compile. This means a `font_info`-style append-only arena of scaled values (§571, scaled once per font and size), fonts found by (table ID, size), and session-stable font numbers. See `2026-10-22-font-arena.md`.
- The `.fd` tables are the next D2 slice (NFSS), then the package registry. They are deferred here, not dropped.
