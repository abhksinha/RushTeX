# RushTeX Mathematical Master Parity Plan: The Authoritative Data Pipeline

**Status:** Master Architectural & Data Extraction Blueprint  
**Guiding Engineering Philosophy:** [`Resource_Conscious_Software_Engineering.md`](file:///home/triton/Coding/texrs/Resource_Conscious_Software_Engineering.md) and [`RushTeX_Math_Architecture_Blueprint.md`](file:///home/triton/Coding/texrs/RushTeX_Math_Architecture_Blueprint.md)

---

## 1. Executive Objective

To achieve sub-millipoint, bit-for-bit geometric and visual parity with TeX Live / pdfLaTeX without trial-and-error visual debugging, by extracting the complete mathematical ground-truth from Donald Knuth's canonical source materials and compiling it directly into zero-allocation, L1/L2 cache-resident static structures in Rust.

---

## 2. The Four Pillars of Ground-Truth Data

We have assembled the canonical primary sources directly within the environment:

1. **Donald Knuth's `tex.web` (Downloaded)**:
   - Stored in [`compatiblityt_with_legacy/tex.web`](file:///home/triton/Coding/texrs/compatiblityt_with_legacy/tex.web).
   - Authoritative Pascal algorithms: Section 36 (`mlist_to_hlist`), Section 37 (`clean_box`, `fetch`), Section 38 (`make_over`, `make_fraction`), Section 39 (`make_radical`), Section 40 (`make_scripts`), and Section 41 (`make_math_accent`).
2. **Knuth's `plain.tex` (System TeX Live)**:
   - Located at `/usr/share/texlive/texmf-dist/tex/plain/base/plain.tex`.
   - Contains all canonical `\mathcode`, `\mathchardef`, and `\delcode` integer assignments across all standard symbols, letters, and operators.
3. **TeX Live Canonical TFM Font Metric Files (System TeX Live)**:
   - Located in `/usr/share/texlive/texmf-dist/fonts/tfm/public/cm/` and `amsfonts/`:
     - Roman: `cmr10.tfm`, `cmr7.tfm`, `cmr5.tfm`
     - Math Italic: `cmmi10.tfm`, `cmmi7.tfm`, `cmmi5.tfm`
     - Math Symbol: `cmsy10.tfm`, `cmsy7.tfm`, `cmsy5.tfm`
     - Math Extension: `cmex10.tfm`
     - AMS Symbols: `msam10.tfm`, `msbm10.tfm`
4. **TeX TFM Decompiler Tool (`tftopl`)**:
   - Available natively in the shell to decompile binary TFM files into exact Property List (PL) human-readable and machine-parseable representations.

---

## 3. The Extraction & Compilation Pipeline ("Hit Jackpot in a Go")

We formally separate the offline extraction pipeline from the runtime cache representation:

```
[tex.web / plain.tex] ───┐
                         ├──→ [Offline Ingestion: Full TeX 2^20 Fixed-Point]
[Canonical TFM Files] ───┘           │ (Checksum & checksum cross-checks)
                                     ↓
                         [Offline Verification against Ground Truth]
                                     ↓
                         [Runtime Quantization into Packed Cache Structs]
                                     │
                                     ├── 1. Static Mathcode LUT (512 bytes, L1-resident)
                                     ├── 2. FontDimen22 Table (176 bytes, L1-resident)
                                     ├── 3. Complete 9 Optical Tables (< 10 KB total)
                                     ├── 4. Delimiter Recipes (packed 4 bytes)
                                     └── 5. 64-byte Aligned TFM Metrics
```

### Phase 1: The Unified 12-Bit `\mathcode` Table
- Extracted directly from `plain.tex` lines 49–78 and 744–820.
- Packed into a 256-entry static array: `pub static MATHCODE_TABLE: [u16; 256]`.
- Memory size: **512 bytes** (fits within 8 cache lines of L1 data cache).
- Bit breakdown:
  - Bits 0..7: Character code ($0..255$)
  - Bits 8..11: Math Family ($0..15$)
  - Bits 12..14: Atom Class ($0..7$, matching `MathAtomKind`)
- **Resource Rule**: Eliminates dozens of scattered parser/layout `match` blocks and replaces them with a single L1-resident array indexing operation.

### Phase 2: The Canonical 22-Parameter Appendix G Table (`FontDimen22`)
- Extracted directly from `cmsy10.tfm` (parameters 8–22) and `cmex10.tfm` (parameters 8–13).
- Packed into an aligned structure matching CPU cache-line boundaries:
  - `slant`, `space`, `stretch`, `shrink`, `x_height`, `quad`, `num1`, `num2`, `num3`, `denom1`, `denom2`, `sup1`, `sup2`, `sup3`, `sub1`, `sub2`, `sup_drop`, `sub_drop`, `delim1`, `delim2`, `axis_height`, `default_rule_thickness`.
- **Resource Rule**: Replaces every heuristic floating-point multiplier (`font_size * 0.xx`) with authoritative constants.

### Phase 3: The Complete 3-Tier Optical Metric Tables (< 10 KB total)
- Full offline extraction of exact width, height, depth, and italic correction for all optical sizes:
  - Roman: $10\,\text{pt}, 7\,\text{pt}, 5\,\text{pt}$ (`cmr10`, `cmr7`, `cmr5`)
  - Math Italic: $10\,\text{pt}, 7\,\text{pt}, 5\,\text{pt}$ (`cmmi10`, `cmmi7`, `cmmi5`)
  - Math Symbol: $10\,\text{pt}, 7\,\text{pt}, 5\,\text{pt}$ (`cmsy10`, `cmsy7`, `cmsy5`)
- Stored as compact, 64-byte aligned tables in `.rodata` ($128 \times 8\,\text{bytes} = 1\,\text{KB}$ per face).
- Total storage for all 9 tables is $< 10\,\text{KB}$—zero cache penalty, fits easily in L2 cache, eliminating all lossy $0.70\times/0.50\times$ scaling.
- **Resource Rule**: Zero dynamic memory allocation, zero disk I/O, zero runtime decompression.

### Phase 4: Delimiter Chains and Extensible Recipes
- Extracted directly from `cmex10.tfm` `char_list` and `extensible_recipe` fields.
- Discrete sizing chains for parentheses, brackets, braces, and radicals.
- Exact assembly recipes: `(top, mid, bot, rep)` packed into 4 bytes per delimiter.
- **Resource Rule**: Compact 4-byte packed structs; eliminates ad-hoc delimiter geometry.

### Phase 5: Numeric Model & Automated Differential Verification
- **Numeric Model Awareness**: TeX operates in integer scaled points ($1\,\text{sp} = 1/65536\,\text{pt}$). RushTeX targets strict geometric sub-millipoint parity ($\pm 0.005\,\text{pt}$), recognizing where integer glue rounding differs from float arithmetic.
- **Automated Ground-Truth Pipeline**: Instead of hand-authoring expected test values, an automated extraction script runs the formula corpus through real `pdflatex`, parses the exact box/glyph/rule hierarchy from TeX's log/DVI output, and generates reference `MathGeometryTrace` records.
- **Scope Partitioning**: A strict contract for the "classical core" (held to byte-for-byte Appendix G parity) clearly partitioned from RushTeX superset extensions.

---

## 4. Strict Resource-Conscious Guarantees

Under [`Resource_Conscious_Software_Engineering.md`](file:///home/triton/Coding/texrs/Resource_Conscious_Software_Engineering.md):
1. **Zero Heap Allocations on Math Hot Path**: Layout runs entirely within stack buffers and scratch space.
2. **L1 Data Cache Residency**: Active lookup tables (mathcodes, spacing matrix, fontdimens) occupy $< 2\,\text{KB}$ combined.
3. **Sequential Traversal**: Flat atom sequences replace recursive tree traversals and pointer-chasing lists.
4. **Precomputed Knowledge**: All symbol semantics, character codes, and metrics are compile-time constants in `.rodata`.

