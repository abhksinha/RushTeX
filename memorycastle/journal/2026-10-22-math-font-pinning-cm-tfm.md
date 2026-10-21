# OpenType math fonts: pin the NewCM release, Computer Modern stays TFM-driven
Author: abhi+claude | Date: 2026-10-22
Files: `src/math/fonts/mod.rs`, `src/math/fonts/vector_paths.rs`, `src/math/fonts/delimiters.rs`, `src/math/math_layout.rs`, `src/math/math_tests.rs`

## Context
Three `math::fonts` tests failed ever since the machine migration (the "known NewCM failures"). They looked for `NewCMMath-Regular.otf` only at fixed Linux paths. The user asked to complete and patch them, not skip them.

## Decision
- **Resolver.** `math_font_bytes` searches these locations, in order:
  - `$RUSHTEX_FONT_DIR`;
  - `~/.rushtex/fonts`;
  - every texmf tree: `$TEXMFHOME`, `$TEXMFDIST`, `~/texmf`, `~/Library/texmf`, `/usr/local/texlive/<year>/texmf-dist` (newest first), `/usr/share/texlive/texmf-dist`, `/usr/share/texmf`;
  - the system font directories.
- **NewCM release pinned.** The glyph-id tables (`get_vector_glyph_path` script paths, `NEWCM_*_VARIANTS`, operator metrics, test GIDs) come from the NewCM in TeX Live 2023: 7601 glyphs, `\int`=832, script A=2229.
  - Every later release renumbers the glyphs. Glyph counts: TL2022 7041, TL2024 7668, TL2025 7831, TL2026 8603.
  - So `NEWCM_MATH_FILE` carries `Some(7601)`, and any other release is skipped.
  - Mixing a newer font's GIDs with these tables would paint the wrong glyphs.
- **Computer Modern has no OpenType math font.** `active_math_font` and `active_opentype_math_font` return `None` for `FontFamily::ComputerModern`, so its math comes from cmmi/cmsy/cmex TFMs, as in TeX.
  - Measured: with NewCM present, the old routing changed native output. testmath dropped 8 glyphs on pages 21 and 25. `test_mlist_c_radical_degree_parity` failed because CM roots became vector glyphs.
  - Native parity had been calibrated with NewCM absent, so `None` is the validated behaviour, and it is now independent of what is installed.
  - `newcm_opentype_math_font()` gives NewCM explicitly. It is the fallback of the OpenType families.
- **The three tests patched to that rule:**
  - CM resolves to no OpenType font;
  - the NewCM checks use the pinned font explicitly;
  - the VectorGlyph layout check uses an OpenType family (`mathptmx`, STIX Two Math).
- **This machine:**
  - `tlmgr --usermode install newcomputermodern stix2-otf firamath asana-math` (TL2026 releases into `~/Library/texmf`);
  - the TL2023 NewCMMath was copied from the TeX Live historic archive into `~/.rushtex/fonts/`.

## Rejected
- Updating the test GIDs to the installed TL2026 font: the code's static tables would then disagree with the font.
- Skipping the tests when the font is absent: the user asked for them to run.
- Keeping CM routed to NewCM: it makes output depend on installed fonts and breaks pdfLaTeX parity.

## Measured
- `cargo test --release`: 322 passed, 0 failed (was 319/3).
- clippy `-D warnings` clean.
- Native `score --all`: unchanged, 100% on cfgguide, lppl, modguide, sample2e, small2e, testmath.
- Engine `zero --pixels`: 12/12.

## Open
- CI must provide the TL2023 NewCMMath (Ubuntu 24.04's texlive 2023, or `RUSHTEX_FONT_DIR`), or the NewCM tests fail there.
- The long-term home of these fonts is packs (roadmap 17), with the release embedded rather than found on disk.
