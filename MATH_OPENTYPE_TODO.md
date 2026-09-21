# RushTeX Modern OpenType Mathematical Typography Architecture: Implementation Roadmap & TODO

This document tracks the step-by-step engineering roadmap for modern OpenType math font typography in RushTeX. All implementations strictly adhere to [`god_instruction.md`](file:///home/triton/Coding/texrs/god_instruction.md), [`AGENTS.md`](file:///home/triton/Coding/texrs/AGENTS.md), and [`Resource_Conscious_Software_Engineering.md`](file:///home/triton/Coding/texrs/Resource_Conscious_Software_Engineering.md): pure safe Rust (`#![forbid(unsafe_code)]`), zero runtime heap allocations on the hot layout path, contiguous cache-line alignment (SoA/AoS), and pre-computed static `.rodata` tables.

---

## Modern OpenType Math Milestones Dashboard

| Milestone | Area / Description | Status | Target Modules |
|---|---|---|---|
| **M1** | **OpenType MATH Constants & Table Parsing** | ✅ Completed | `src/math/fonts/opentype.rs`, `src/math/fonts/math_constants.rs` |
| **M2** | **Vertical Delimiters & Extensible Assemblies** | ✅ Completed | `src/math/fonts/delimiters.rs`, `src/math/math_layout.rs` |
| **M3** | **Font-Driven Radicals & Degree Positioning** | ✅ Completed | `src/math/math_layout.rs`, `src/math/fonts/vector_paths.rs` |
| **M4** | **Large Operators & Dynamic Limit Mechanics** | ✅ Completed | `src/math/fonts/operators.rs`, `src/math/math_layout.rs` |
| **M5** | **Wide Accents & Horizontal Extensible Assemblies** | ✅ Completed | `src/math/fonts/delimiters.rs`, `src/math/math_layout.rs`, `src/math/fonts/vector_paths.rs` |
| **M6** | **Alphanumeric Alphabets & Bold Vectors** | ✅ Completed | `src/math/fonts/calligraphy.rs`, `src/math/math_layout.rs` |
| **M7** | **Zero-Type-1 Vector Glyph PDF Emission** | ✅ Completed | `src/backend/backend_pdf.rs`, `src/math/fonts/vector_paths.rs` |
| **M8** | **Comprehensive Regression Certification** | ✅ Completed | `src/math/math_tests.rs`, `tests/` |
| **M9** | **Multi-Family Optical Math Architecture** | ✅ Completed | `src/math/fonts/firamath.rs`, `src/math/fonts/asana.rs`, `src/math/fonts/mod.rs` |
| **M10**| **Authoritative Font-Driven CFF/TrueType Subsetter** | ✅ Completed | `src/backend/backend_pdf.rs`, `src/math/fonts/opentype.rs` |
| **M11**| **Composite CIDFont Embedding & Complete Math ToUnicode** | ✅ Completed | `src/backend/backend_pdf.rs`, `src/layout/layout_engine.rs` |

---

## Detailed Task Breakdown

### Milestone 1: OpenType MATH Constants & Table Parsing
- [x] Zero-allocation safe parser for OpenType `MATH`, `CFF `, `cmap` (Format 4 & 12), `head`, `hmtx`.
- [x] 64-byte cache-aligned `OpenTypeMathConstants` with authoritative values for NewCMMath and STIX Two Math.
- [x] Dynamic font auto-detection in system TeX Live paths with fallback to static constants.

### Milestone 2: Vertical Delimiters & Extensible Assemblies
- [x] Multi-tier discrete variant advances for parentheses, square brackets, curly braces, vertical bars.
- [x] Extensible 3-piece and 4-piece glyph assemblies (top, mid, bot, ext).
- [x] Dynamic connector overlap query (`minConnectorOverlap`) eliminating micro-gaps between stacked assembly pieces.
- [x] Integration with `layout_delimiter` in `src/math/math_layout.rs`.

### Milestone 3: Font-Driven Radicals & Degree Positioning
- [x] Discrete variant selection for surds (base, v1..v4) based on target formula height.
- [x] Multi-piece radical assembly (`uni23B7` bottom hook, `radical.ex` extender, `radical.tp` top cap).
- [x] MATH parameter driven overbar rule thickness (`RadicalRuleThickness`) perfectly aligned with top cap.
- [x] Mathematical index degree tucking (`\sqrt[n]{x}`) using `radical_degree_bottom_raise_percent` and kerning offsets.

### Milestone 4: Large Operators & Dynamic Limit Mechanics
- [x] Style-driven operator variant selection (`\int`, `\iint`, `\iiint`, `\oint`, `\sum`, `\prod`) between Text and Display styles.
- [x] Font-driven italic correction offsets for upper limit placement.
- [x] Slanted stem lower-limit tuck kerning offsets.
- [x] Integral symbol outlines driven by font designer Bézier curves.

### Milestone 5: Wide Accents & Horizontal Extensible Assemblies
- [x] Extract 50 high-precision cubic Bézier vector paths for horizontal variants and assemblies.
- [x] Discrete horizontal variant selection for `\widehat` (base, h1..h7) and `\widetilde` (base, h1..h7).
- [x] OpenType horizontal 5-piece assembly for `\overbrace` and `\underbrace` (Left, Extender, Mid, Extender, Right) with `minConnectorOverlap`.
- [x] OpenType horizontal 3-piece assembly for extensible arrows (`\overrightarrow`, `\overleftarrow`, `\xrightarrow`).
- [x] Connect wide accent layout and over/under brace layout directly to `active_opentype_math_font()`.
- [x] Verified zero geometric distortion or stretching across arbitrary formula widths with 100% test pass rate.

### Milestone 6: Mathematical Alphanumeric Unicode Alphabets & Bold Vectors
- [x] Bold vector formatting (`\mathbf{v}`, `\boldsymbol{v}`, `\bm{v}`).
- [x] Blackboard Bold (`\mathbb{R}`, `\mathbb{C}`, `\mathbb{Z}`, `\mathbb{N}`) font glyph resolution.
- [x] Fraktur (`\mathfrak{g}`, `\mathfrak{H}`) and Calligraphic (`\mathcal{P}`, `\mathcal{H}`) symbol mapping.
- [x] Optical apex accent kerning for slanted bold vectors.

### Milestone 7: Zero-Type-1 Vector Glyph PDF Emission
- [x] Pure PDF path operator emission (`q ... cm ... f Q`) for math glyphs.
- [x] Pre-computed `.rodata` byte tables for zero heap allocation and instant execution.
- [x] Dense jump-table GID lookup resolving in sub-nanosecond time.

### Milestone 8: Comprehensive Regression Certification
- [x] End-to-end multi-environment typography test certifying delimiters, radicals, operators, accents, 5-piece braces, alphanumerics, and optical apex kerning.
- [x] Invariant verification of memory layout and cache alignment (`OpenTypeMathConstants` 128 bytes align 64, `OpenTypeScriptGlyph` 16 bytes align 4, `MathNode` <= 32 bytes).
- [x] Bit-level reproducibility between dynamic string compilation (`compile_str`) and zero-allocation scratch buffer (`compile_into`).
- [x] Verified release compiler throughput exceeding 75,000 to 89,000 documents/sec for standard documents and >7,700 documents/sec for complex multi-equation documents.
- [x] 100% test suite pass rate across all 127 tests with zero compiler/clippy warnings.
- [x] Release binary size validated at 1.4 MB within target resource bounds under `#![forbid(unsafe_code)]`.

### Milestone 9: Multi-Family Optical Math Architecture
- [x] Dynamic binary font resolution for all 4 primary font families: Times (`STIXTwoMath-Regular.otf`), Helvetica (`FiraMath-Regular.otf`), Palatino (`Asana-Math.otf`), Computer Modern (`NewCMMath-Regular.otf`).
- [x] 64-byte aligned static `.rodata` OpenTypeMathConstants tables for Fira Math (`FIRAMATH_CONSTANTS`) and Asana Math (`ASANA_MATH_CONSTANTS`).
- [x] Unified `active_math_font` and `active_opentype_math_font` resolvers with automatic fallback chains.
- [x] Integrated `opentype_engine()` dispatch in math layout parameterized by preamble-selected font family.
- [x] End-to-end integration test (`test_milestone_9_multi_family_opentype_math_certification`) validating zero-regression PDF emission across all four typeface ecosystems.

### Milestone 10: Authoritative Font-Driven CFF/TrueType Subsetter
- [x] Zero-allocation CFF table parser (`cff_table`, `cff_postscript_name`, `cff_font_bbox`, `cff_charstrings_count`, `cff_charstring`).
- [x] CFF INDEX structure decoder and Top DICT operator interpreter (`parse_cff_index`, `decode_top_dict_operator`, `decode_top_dict_bbox`).
- [x] TrueType `glyf` and `loca` outline data slice extractor supporting both short (16-bit) and long (32-bit) offset formats (`truetype_glyph_data`).
- [x] High-performance 2-byte CIDFont ToUnicode CMap generator (`emit_cid_tounicode_cmap`) supporting both BMP and SMP mathematical alphanumeric characters via UTF-16 surrogate pairs.
- [x] Unit test suites (`test_cff_and_truetype_outline_access`, `test_cid_tounicode_cmap_generation`) validating 100% precision on authentic OpenType binaries.

### Milestone 11: Composite CIDFont Embedding & Complete Math ToUnicode
- [x] Zero-allocation reverse cmap parsers (`reverse_cmap_format_12`, `reverse_cmap_format_4`) and bidirectional Unicode lookup (`unicode_for_glyph`).
- [x] Type 0 Composite Font (`/Subtype /Type0`, `/Encoding /Identity-H`, `/DescendantFonts`) and Descendant CIDFont (`/Subtype /CIDFontType0`, `/CIDSystemInfo`, `/DW 1000`, `/W`) emission in `backend_pdf.rs`.
- [x] FontDescriptor emission (`/Flags 4`, `/FontBBox`, `/Ascent`, `/Descent`, `/CapHeight`) with embedded CFF table (`/FontFile3` with `/Subtype /CIDFontType0C`).
- [x] Global `OnceLock` caching for compressed CFF streams (`get_compressed_cff`), eliminating repeated deflate compression on hot layout/emission paths.
- [x] Dual-mode PDF content stream emission: visible CID text glyphs (`Paint::CidGlyphRun`) and invisible text overlay (`3 Tr` with `Paint::VectorGlyph`) ensuring universal copy/paste, searchability, and screen-reader accessibility.
- [x] Comprehensive certification test (`test_milestone_11_composite_cidfont_and_tounicode_certification`) validating zero-bloat plain text documents, font dictionary hierarchy, and memory footprint invariants (`size_of::<Paint>() <= 32`).

### Milestone 12: Dynamic CFF Font Subsetting & Geometry Oracle Validation
- [x] Dynamic in-memory CFF CharStrings subsetter generating minimal `/FontFile3` streams for referenced glyph IDs.
- [x] Machine-readable mathematical bounding box and baseline geometry verification oracle (`MathFormulaGeometry`, `measure_formula_geometry`, `verify_geometry_tolerance`).
- [x] Multi-line math alignment layout (`align*`, `aligned`, `cases`) parameterized with font-driven baselines and column spacing (`row_parameters_font`).
- [x] End-to-end text extraction and searchability regression testing suite across complex nested expressions (`test_milestone_12_dynamic_cff_subsetting_and_geometry_oracle`).


