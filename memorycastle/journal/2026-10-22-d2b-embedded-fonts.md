# D2b tier 1: the common font programs carried in the binary, byte-exact
Author: abhi+claude | Date: 2026-10-22
Files: `src/data/data_embed.rs`, `src/data/data_mod.rs`, `src/engine/engine_host.rs`, `src/bin/rushtex_gen.rs`, `assets/tables/embed.set`, `assets/tables/embed.rtdb`, `Cargo.toml`, `.github/workflows/ci.yml`

## Context
The user's goal is the path of least resistance for the 80–90% of users whose documents use the common fonts. Government and academic documents mostly use Times; STEM papers mostly use Computer Modern and Latin Modern; Helvetica, Courier and Palatino follow. The metrics were already in the binary (`fonts.rtdb`). The font programs, which hold the glyph outlines and are where the size is, came only from a TeX installation.

## Decision
- **An embedded file layer under the engine's file lookup.** `Kpse::read` serves a file from `data_embed` when the disk has none; `RT_EMBED_FIRST=1` serves it from the binary first. The bytes are the pinned file's exactly, so output cannot change, and the same layer serves a binary with no TeX Live once the format no longer needs one (P7).
- **Type 1 stored decrypted.** A `.pfb`'s eexec section (key 55665) and each charstring (key 4330) are decrypted before compression and encrypted again on read. Both are deterministic, so the round trip gives the original bytes; the segment headers stay in place.
- **The set is a committed pattern file** (`assets/tables/embed.set`) matched against the pin:
  - Computer Modern and the AMS fonts (`cm*`, `msam*`, `msbm*`, `eu*`);
  - Latin Modern (`lm*`);
  - the URW base fonts (Times `utm`, Helvetica `uhv`, Courier `ucr`, Palatino `upl`, Symbol `usy`, Dingbats `uzd`);
  - their encodings (`lm-*.enc`, `8r.enc`).
- **Layout:** a perfect hash of file names, three words per file (offset, length, kind), one zlib stream per file. A read is one probe, one inflate and one re-encryption, once per font per document.
- **Cargo feature `fonts-core`, on by default.** Without it the table is empty (WASM or minimal builds).
- **Self-check:** the generator reads every file back and compares the bytes. CI regenerates the table from the pin and requires identical bytes.

## Rejected
- **Deflating the `.pfb` as is:** encrypted bytes look random. Across the pin's 206 programs, 17.9 MB only becomes 17.4 MB (zlib -9).
- **eexec decryption only:** 13.5 MB. Charstring decryption as well gives 8.4 MB.
- **Converting to CFF**, as the old native path's `assets/cm_fonts` does: a little smaller, but no longer pdfTeX's bytes, and every output path would need a CFF writer.
- **Carrying all 206 pinned programs:** cm-super (42 sizes, 3 MB) and Libertine (1.2 MB) serve few documents. They stay on disk, for a later tier.

## Measured
- Tier 1 as the pin holds it: 135 files, 7,468,612 bytes stored as 4,342,618 (all 135 read back exactly); generation takes 0.6 s.
- The compressor is ours, with fixed Huffman codes only. zlib -9 would store the same set in about 3.5 MB (estimate from the measurements above).
- Release binary: 5,428,280 → 9,805,640 bytes.
- small2e and testmath: the PDF is byte-identical with fonts from disk and from the binary.
- Gate with every tier-1 file served from the binary (`RT_EMBED_FIRST=1`): zero shift 85/85 with pixels; leak audit 85/85; 352 tests; clippy clean in three configurations.

## Open
- **The pin holds only what the corpus reads.** The full families (all Latin Modern and Computer Modern sizes, Times bold italic and so on) need `pin-3` with an extra file list beyond the corpus.
- **A dynamic-Huffman encoder** (about −0.8 MB here) also shrinks every PDF RushTeX writes; this is D5.
- **Per-glyph storage and cached subsets** (§4a, the next D2b item) would let a document decompress only the glyphs it uses.
- **`pdftex.map` lines** for the tier-1 fonts, so that a binary with no TeX Live finds them (P7).
