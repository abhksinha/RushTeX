# TS1 text symbols as design-size masters; font data moved into tables
Author: abhi+claude | Date: 2026-10-20
Files: `src/backend/backend_ts1.rs`, `src/backend/backend_optical.rs`, `src/backend/backend_fonts.rs`, `src/backend/backend_pdf.rs`, `src/frontend/frontend_parser.rs`, `src/frontend/frontend_ast.rs`, `src/layout/layout_engine.rs`, `src/index/index_toc.rs`, `src/math/math_layout.rs`, `src/math/math_fonts.rs`, `scripts/fontgen/gen_enc.py`, `scripts/fontgen/gen_ts1_symbols.py`, `src/backend/backend_cm_fix.rs`, `src/backend/backend_cm_fonts.rs`, `src/backend/backend_scaled.rs`, `src/backend/backend_shape.rs`, `src/backend/backend_mod.rs`, `assets/cm_fonts/tcrm1000.flate`, `scripts/parity/baseline.json`

## Context
modguide and cfgguide page 1 missed the `\thanks` asterisk and cfgguide's itemize bullets: LaTeX 2020+ takes `\textasteriskcentered`, `\textbullet`, `\textcopyright`, `\textdagger`, `\S`, `\P` from TS1 (latex.ltx `\DeclareTextSymbolDefault{..}{TS1}`), set in tcrm (cm-super SFRM) at the `ts1cmr.fd` design sizes. The cfgguide TOC leaders drifted 0.1pt.

## Decision
One `Face::TextComp` (tcrm1000) plus 11 `OpticalMaster` rows `Tcrm0500`..`Tcrm2488` in `MASTERS`, chosen by `ts1cmr.fd` sizes in `OpticalMaster::select`. Each tcrm font is a compact 128-code subset (`gen_enc.py` first_slot -1); `TS1_SLOT_CODE` maps slots to codes, shared `TS1_TOUNICODE`, masters with their own ToUnicode take a 4th PDF object. Text commands come from the generated sorted `TS1_TEXT_SYMBOLS` (107 rows, binary search), `LogoKind::Ts1(slot)`; `\@fnsymbol` and list markers use the same slots. Leader box width is §716 `math_kern` arithmetic plus the TFM width of the period. Dead `flatten_into`/`FlatRun` removed.

## Rejected
Upper-half TS1 layout (codes = slots 128..255) with a separate `TextComp12` face per size: one face per size does not scale to 12 sizes and wastes the 6-bit face space; masters already solve design sizes.

## Measured
Strict parity: all six documents 100.0% (cfgguide 17287/17287, modguide 11247/11247, lppl, sample2e, small2e, testmath 42400/42400); baseline saved. Speed: 2.4 to 8.5ms against pdfLaTeX 94 to 187ms (22x to 40x one run). Binary 3.4MB; tcrm assets 165KB.

## Open
TS1 bold/italic/slanted (tcbx, tcti, tcsl) not bundled. Write-time glyph subsetting would shrink the 13KB per TS1 size embedded. A `\label` inside a paragraph records its first line's page.
