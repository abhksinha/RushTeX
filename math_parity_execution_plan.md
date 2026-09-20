# RushTeX Mathematical Parity Execution Plan & Master Todo

**Status:** Active Implementation Plan  
**Target:** 100% Proven Algorithmic & Geometric Parity with TeX Live / pdfLaTeX  
**Guiding Architecture:** [`Resource_Conscious_Software_Engineering.md`](file:///home/triton/Coding/texrs/Resource_Conscious_Software_Engineering.md) & [`AGENTS.md`](file:///home/triton/Coding/texrs/AGENTS.md)

---

## 1. Actionable Milestones & Todo List

- [x] **Milestone 1: Verify Math Spacing & Muskip Constants against `plain.tex` / `tex.web`**
  - [x] 1.1 Verify thinmuskip (3mu), medmuskip (4mu), thickmuskip (5mu) values in text and display.
  - [x] 1.2 Verify script/scriptscript suppression rules in the 64-byte spacing matrix.
  - [x] 1.3 Verify Rule 5 atom normalization (`Bin` $\to$ `Ord` transitions).

- [x] **Milestone 2: TFM Extraction & Engine Optical Sizing Wiring**
  - [x] 2.1 Build an offline Python/Rust extraction script utilizing system `tftopl` to parse canonical TeX Live TFMs:
        `cmr10/7/5`, `cmmi10/7/5`, `cmsy10/7/5`, `cmex10`, `msam10`, `msbm10`.
  - [x] 2.2 Extract the 22 canonical font dimensions ($\sigma_1 \dots \sigma_{22}$) for Family 2 (`cmsy`) and Family 3 (`cmex`) into compile-time `.rodata` (`FontDimen22`).
  - [x] 2.3 Extract discrete delimiter chains (`char_list`) and extensible delimiter recipes (`top, mid, bot, rep`) from `cmex10.tfm`.
  - [x] 2.4 Wire the optical size resolver (`resolve_optical_face`) into radicals, delimiters, and fractions in `math_layout.rs`, replacing remaining linear scale factors (`0.70x / 0.50x`) with authoritative font selections.

- [x] **Milestone 3: Canonical 12-Bit `\mathcode` Table Extraction & Parser Unification**
  - [x] 3.1 Extract all `\mathcode` assignments from `plain.tex` lines 49–78 and `\mathchardef` lines 744–820.
  - [x] 3.2 Pack the data into a static, 512-byte array `pub static MATHCODE_TABLE: [u16; 256]` (fitting entirely in L1 cache).
  - [x] 3.3 Replace scattered hardcoded character matching in `math_parser.rs` and `math_fonts.rs` with the unified table lookup.

- [x] **Milestone 4: Automated Ground-Truth Differential Parity Test Harness**
  - [x] 4.1 Construct a test corpus of 50+ representative math expressions covering:
        fractions, nested radicals, limits, scripts, multi-integrals, matrices, and cases.
  - [x] 4.2 Build an automated harness script that runs the corpus through `pdflatex`, extracts exact box dimensions, baselines, and glyph coordinates from TeX box dumps.
  - [x] 4.3 Assert that RushTeX's measurements match pdfTeX ground truth across all test cases.

- [x] **Milestone 5: Display Math Vertical List & Page Builder Integration (`mlist_to_hlist` to Vertical Builder)**
  - [x] 5.1 Track preceding paragraph geometry (`last_line_w`, `last_line_indent`, `has_preceding_text`, `prev_graf`) in `Layouter`.
  - [x] 5.2 Wire TeX's canonical `\predisplaysize` calculation (`DisplayMathParagraphBreaker`, `EmptyParagraphDisplayFilter`).
  - [x] 5.3 Implement TeX's dynamic display skip selection (`PreDisplayClearanceComparator`):
        select `\abovedisplayshortskip` (0pt natural) vs `\abovedisplayskip` (1.0 * base font size) and `\belowdisplayshortskip` (0.6 * base font size) vs `\belowdisplayskip` (1.0 * base font size).
  - [x] 5.4 Connect display penalties and page break guards:
        discard vertical glue at top-of-page breaks following TeX's discardable boundary rule; enforce `EquationTagPageBreakGuard::INF_PENALTY` on multiline equation number splits (`must_split_line`).
  - [x] 5.5 State restoration:
        restore horizontal mode state via `DisplayResumeStateRestorer::restore_state`.

- [x] **Milestone 6: Type 1 `/ToUnicode` CMap Generation for Computer Modern Math Searchability**
  - [x] 6.1 Compile-time `.rodata` static Unicode mappings (`CMR_TO_UNICODE`, `CMMI_TO_UNICODE`, `CMSY_TO_UNICODE`, `CMEX_TO_UNICODE`, `MSAM_TO_UNICODE`, `MSBM_TO_UNICODE`) in `backend_cm_fonts.rs`.
  - [x] 6.2 Implement `emit_tounicode_cmap` in `backend_pdf.rs` generating Adobe `/CIDInit /ProcSet findresource begin ... end` streams.
  - [x] 6.3 Shared CMap stream references across font sizes (`CMR10`, `CMR7`, `CMR5` share the single OT1 CMap object ID), preventing redundant PDF stream bloat.
  - [x] 6.4 Packed parameter object `CmFontObjectIds` (16 bytes, zero allocation) and automated PDF text extraction test (`pdftotext`) verifying searchable math glyphs (`π`, `∫`, `≤`, `E = mc`).

- [x] **Milestone 7: Extensible Delimiter Vertical Assembly, Radical Overbar Continuity & Integral Op Parity**
  - [x] 7.1 Replaced heuristic delimiter assembly with Donald Knuth's canonical `tex.web` §713 top-to-bottom stack algorithm (exact touching without sub-pixel gaps).
  - [x] 7.2 Implemented authoritative `cmex10` discrete radical variants (0x70, 0x71, 0x72, 0x73) and extensible radical assembly (top 0x76, rep 0x75, bot 0x74) per `tex.web` §737.
  - [x] 7.3 Restored canonical `\nulldelimiterspace` (1.2pt) margin on fraction rules matching TeX Live ground truth.
  - [x] 7.4 Corrected integral italic correction and script placement (`\int` subscript hugs bottom hook without `ic`, superscript shifted right by `ic`, exact `sup_drop` and `sub_drop` baseline shifts).
  - [x] 7.5 Fixed Computer Modern `/ToUnicode` mapping for display `\int` (0x5A -> U+222B) and all CMEX big operators.
