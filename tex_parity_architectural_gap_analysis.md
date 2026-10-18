# RushTeX vs. Donald Knuth's TeX: Architectural & Algorithmic Parity Gap Analysis

**Date:** September 2026  
**Document Purpose:** Complete systemic analysis of how TeX formats mathematical typography, how TeX defines and executes spacing, and the precise algorithmic and structural gaps between TeX and RushTeX.

---

## 1. How TeX Generates Symbols and Formats Math

Donald Knuth designed TeX’s mathematical typesetting engine around a strict two-phase pipeline:
1. **Parsing to Math List (MList)**: TeX parses the input token stream and generates a flat linked list of mathematical nodes called a **math list**.
2. **MList to Horizontal List (HList) Conversion**: In Chapter 26 and Appendix G of *The TeXbook*, TeX converts the math list into a concrete horizontal box list (`hlist`) by executing **22 exact transformation rules**.

### 1.1 The 16 Math Families and the 12-Bit `\mathcode`
TeX does not hardcode characters to fonts or classes in parser `match` statements. Instead, every character code ($0..255$) has a 12-bit integer `\mathcode`:
$$\text{mathcode} = (\text{Class} \ll 12) \mid (\text{Family} \ll 8) \mid \text{Character}$$

- **Class ($0..7$)**:
  - `0`: `Ord` (Ordinary — Latin/Greek letters, digits)
  - `1`: `Op` (Large operator — $\sum, \int$)
  - `2`: `Bin` (Binary operator — $+,-,\times$)
  - `3`: `Rel` (Relation — $=,<,\le$)
  - `4`: `Open` (Opening delimiter — $(,[,\\{)
  - `5`: `Close` (Closing delimiter — ),],\\})
  - `6`: `Punct` (Punctuation — comma, semicolon)
  - `7`: `Var` (Variable family — inherits family from current font)
- **Family ($0..15$)**: An index into the active font family table. Standard Plain TeX / LaTeX uses:
  - Family 0: Roman (`cmr10`)
  - Family 1: Math Italic (`cmmi10`)
  - Family 2: Math Symbols (`cmsy10`)
  - Family 3: Math Extension (`cmex10`)
  - Families 4+: Custom/AMS symbols (`msam10`, `msbm10`, bold, sans-serif, etc.)

### 1.2 The Three Font Sizes per Family (`textfont`, `scriptfont`, `scriptscriptfont`)
Each family in TeX points to **three distinct font files simultaneously**:
- `\textfont f`: Used for Display and Text styles ($10\,\text{pt}$, e.g. `cmr10`, `cmmi10`, `cmsy10`)
- `\scriptfont f`: Used for Script style ($7\,\text{pt}$, e.g. `cmr7`, `cmmi7`, `cmsy7`)
- `\scriptscriptfont f`: Used for ScriptScript style ($5\,\text{pt}$, e.g. `cmr5`, `cmmi5`, `cmsy5`)

Knuth hand-cut optical designs for the smaller sizes: strokes are proportionally thicker, x-heights are taller, and spacing parameters are wider to prevent thin strokes from disappearing or crowding. TeX never performs uniform linear geometric scaling on 10pt glyphs; it swaps the font descriptor to the optical size font.

### 1.3 How Symbols Are Stored and Generated (TFM Tables)
For every glyph, TeX queries the TeX Font Metric (TFM) file, which defines:
1. **Basic Metrics**: Width ($w$), Height ($h$), Depth ($d$), and Italic Correction ($ic$).
2. **Character Information Tag**:
   - `Tag 0`: Standard character.
   - `Tag 1`: Ligature/Kern step list (for inter-character kerning).
   - `Tag 2`: `char_list` (Discrete sizing chain: e.g. small parenthesis $\to$ `\big` $\to$ `\Big` $\to$ `\bigg` $\to$ `\Bigg`).
   - `Tag 3`: `extensible_recipe` (Extensible delimiter construction: `top`, `mid`, `bot`, `rep` glyph indices).

---

## 2. Typographic Spacing in TeX: All Types and How They Interact

In TeX, mathematical horizontal spacing is never a single hardcoded number. It is governed by a multi-layered mathematical spacing grammar.

### 2.1 The Math Unit (`mu`)
- $1\,\text{em} = 18\,\text{mu}$.
- $1\,\text{mu} = \frac{1}{18}\,\text{em}$ of the font in **Family 2 (CMSY) at the current style size**.
- At $10\,\text{pt}$, $1\,\text{mu} = \frac{10}{18} \approx 0.5556\,\text{pt}$.
- At $7\,\text{pt}$, $1\,\text{mu} = \frac{7}{18} \approx 0.3889\,\text{pt}$.
- At $5\,\text{pt}$, $1\,\text{mu} = \frac{5}{18} \approx 0.2778\,\text{pt}$.

### 2.2 The Three Canonical Muskip Registers
TeX defines three parameterized glue registers for inter-atom spacing:
1. `\thinmuskip`: $3\,\text{mu}$ (at $10\,\text{pt} = 1.6667\,\text{pt}$).
2. `\medmuskip`: $4\,\text{mu} \text{ plus } 2\,\text{mu} \text{ minus } 4\,\text{mu}$ (at $10\,\text{pt} = 2.2222\,\text{pt}$).
3. `\thickmuskip`: $5\,\text{mu} \text{ plus } 5\,\text{mu}$ (at $10\,\text{pt} = 2.7778\,\text{pt}$).

### 2.3 The $8 \times 8$ Inter-Atom Spacing Matrix
Between any two adjacent math atoms in a math list, TeX inserts glue according to Appendix G Rule 19:

| Left \ Right | Ord | Op | Bin | Rel | Open | Close | Punct | Inner |
| :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- | :--- |
| **Ord** | 0 | 1 (thin) | 2 (med)* | 3 (thick)* | 0 | 0 | 0 | 1 (thin)* |
| **Op** | 1 (thin) | 1 (thin) | 0 | 3 (thick)* | 0 | 0 | 0 | 1 (thin)* |
| **Bin** | 2 (med)* | 2 (med)* | 0 | 0 | 2 (med)* | 0 | 0 | 2 (med)* |
| **Rel** | 3 (thick)* | 3 (thick)* | 0 | 0 | 3 (thick)* | 0 | 0 | 3 (thick)* |
| **Open** | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| **Close** | 0 | 1 (thin) | 2 (med)* | 3 (thick)* | 0 | 0 | 0 | 1 (thin)* |
| **Punct** | 1 (thin)* | 1 (thin)* | 0 | 1 (thin)* | 1 (thin)* | 1 (thin)* | 1 (thin)* | 1 (thin)* |
| **Inner** | 1 (thin)* | 1 (thin) | 2 (med)* | 3 (thick)* | 1 (thin)* | 0 | 1 (thin)* | 1 (thin)* |

*\* Denotes spacing that is automatically suppressed to $0\,\text{mu}$ in Script and ScriptScript styles.*

### 2.4 User-Level Explicit Spacing Commands
- `\,`: `\mskip\thinmuskip` ($+3\,\text{mu}$)
- `\:`: `\mskip\medmuskip` ($+4\,\text{mu}$)
- `\;`: `\mskip\thickmuskip` ($+5\,\text{mu}$)
- `\!`: `\mskip-\thinmuskip` ($-3\,\text{mu}$)
- `\quad`: `\hskip 1em` ($18\,\text{mu}$)
- `\qquad`: `\hskip 2em` ($36\,\text{mu}$)

### 2.5 List Algebra: Why Explicit Space ADDS to, Rather Than Overwrites, Atom Spacing
In TeX, the math list contains both atoms and explicit glue elements:
$$[\text{Op}(\int), \text{Glue}(-3\mu), \text{Ord}(f)]$$
When TeX converts this list into a horizontal list:
1. It processes $\text{Op}(\int)$ into an hbox.
2. It encounters $\text{Glue}(-3\mu)$ and inserts a glue node with width $-3\mu$.
3. It detects that $\text{Op}$ is followed by $\text{Ord}$. According to the Spacing Matrix, $\text{Op} \to \text{Ord}$ requires $\text{thinmuskip} (+3\mu)$. TeX inserts this $+3\mu$ glue node.
4. Total net spacing between $\int$ and $f$:
$$\text{Net Space} = (-3\mu) + (+3\mu) = 0\mu$$
Knuth designed this intentionally (*The TeXbook*, p. 167): writing `\int\! f` precisely cancels the automatic thin space that TeX places after an operator!

### 2.6 Micro-Typography Spacing Rules
1. **Italic Correction ($ic$)**:
   - **TeX Rule 17 (Characters)**: If a nucleus is a character from a slanted font (math italic) and is followed by glue (including inter-atom space), its italic correction is appended as a kern. If followed by a subscript or superscript, $ic$ is suppressed between the nucleus and subscript, but transferred to the superscript offset.
   - **TeX Rule 13 (Operators)**: Operators have $ic$ incorporated directly into the operator box bounding width ($w = w(c) + ic(c)$). No separate $ic$ is appended after the box.
2. **Script Kerning and Clearance (TeX Rule 18)**:
   - `\scriptspace`: TeX automatically appends $0.5\,\text{pt}$ after every subscript and superscript box to prevent scripts from colliding with subsequent symbols.
   - Minimum vertical clearance: TeX enforces a minimum clearance gap between sub and sup boxes ($\sigma_5$ and $\sigma_6$ fontdimen parameters).
3. **Radical Clearance (TeX Rule 11)**:
   - Clearances ($clr$) above the radicand to the radical overbar rule are computed from font parameters, ensuring equalized optical gaps.
4. **Fraction Clearances (TeX Rule 10)**:
   - Numerator lift and denominator drop are governed by $\sigma_8$ through $\sigma_{12}$ parameters, with minimum clearances ($clr_1, clr_2$) enforced.

---

## 3. The Coding and Algorithmic Gaps in RushTeX

| Category | Real TeX / pdfLaTeX Specification | Current RushTeX Subsystem | Gap & Root Cause |
| :--- | :--- | :--- | :--- |
| **1. Pipeline** | **2-Phase Pipeline**: Parser produces an MList of atoms; Layout iterates across MList applying Appendix G Rules 1–22 in sequence. | **1-Pass AST Tree Traversal**: Layout recursively walks the syntax AST. | Context-dependent spacing and normalization (like Rule 5 and explicit glue cancellation) require complex multi-node state lookahead in recursive passes. |
| **2. Symbol Classification** | **12-bit `\mathcode` LUT**: Every character maps to `(Class, Family, Char)`. Classes are uniform and extensible. | Hardcoded `match` statements across `math_parser.rs`, `math_fonts.rs`, and `math_layout.rs`. | Inconsistent atom classification across different code paths; additions to symbols require editing 3 to 4 separate files. |
| **3. Font Sizing** | **3-Tier Family Pointers**: Each family points to `textfont` ($10\,\text{pt}$), `scriptfont` ($7\,\text{pt}$), `scriptscriptfont` ($5\,\text{pt}$) with distinct hand-designed metrics. | Partial optical sizing: Type 1 fonts embedded, but several layout routines still multiply base $10\,\text{pt}$ metrics by $0.70$ or $0.50$. | Metric drift in sub/superscript fractions, delimiters, and symbols due to linear scaling instead of reading TFM metrics of the optical font. |
| **4. Font Metric Parameters** | **22 TeX Appendix G Font Dimensions**: Reads $\sigma_1 \dots \sigma_{22}$ from `Family 2` (CMSY) and `Family 3` (CMEX). | Hardcoded fractions of font size (e.g. `0.25 * font_size`, `0.45 * font_size`) across layout functions. | Approximations create cumulative sub-millipoint errors that diverge from pdfLaTeX as formulas grow in complexity. |
| **5. Atom Spacing & Glue** | **Linear Node Stream with Additive Glue**: Explicit `\mskip` and automatic atom glue coexist additively in the horizontal list. | Direct cursor increments during AST layout: spaces previously caused inter-atom spacing to be skipped. | Explicit spacing (`\!`, `\,`) could override or corrupt inter-atom spacing rather than compositing with it. |
| **6. Delimiter Assembly** | **Discrete Sizing & Extensible Recipes**: Queries TFM `char_list` and `extensible_recipe` (top, mid, bot, rep) to construct tall brackets. | Discrete sizes implemented for CMEX, but fallback delimiters or custom heights sometimes rely on geometric heuristics. | Visual distortion and non-canonical stroke weights in tall matrix or case delimiters. |

---

## 4. Algorithmic Roadmap to Full Parity

To achieve true, verifiable, and architectural parity without "hit-and-trial" adjustments:

1. **Establish a Single 12-Bit `\mathcode` Lookup Table (L1 Cache-Resident)**:
   - A 256-element array `[u16; 256]` (512 bytes) fitting entirely within L2/L1 cache.
   - Centralizes Class (3 bits), Family (4 bits), and Char Code (8 bits).
   - Eliminates all hardcoded symbol `match` branches in the parser and layout engine.

2. **Formalize the 22 Appendix G Font Parameter Table (`FontDimen22`)**:
   - Extract the 22 canonical font dimension constants directly from `cmsy10.tfm` and `cmex10.tfm` into a packed, 64-byte aligned structure:
     `num1, num2, num3, denom1, denom2, sup1, sup2, sup3, sub1, sub2, sup_drop, sub_drop, axis_height, default_rule_thickness, etc.`
   - Replace all `font_size * 0.xx` heuristics with these exact parameters.

3. **Complete 3-Tier Optical Font Metric Lookups**:
   - Ensure all sub-formula and script measurements query `resolve_optical_face` to retrieve exact metrics from `CMR7/5`, `CMMI7/5`, and `CMSY7/5`.

4. **Flatten AST to 64-Byte Aligned Math List for Row Layout**:
   - Convert row expressions into a flat sequence of packed 16-byte `MathAtomRecord` structures before laying out.
   - Execute Rule 5 normalization and Rule 19 spacing calculations as an array pass over contiguous memory, guaranteeing both TeX algorithmic parity and optimal L1 CPU cache vectorization.

---

## 5. Resource-Conscious Software Engineering Compliance Matrix

Every element of the mathematical parity roadmap must strictly conform to the engineering principles defined in `Resource_Conscious_Software_Engineering.md`:

| Resource-Conscious Principle | TeX Legacy Model (What to Avoid) | RushTeX Implementation (Strict Resource Rule) |
| :--- | :--- | :--- |
| **1. Eliminate Work Before Optimizing** (§1) | Multiple dynamic AST passes with repeated string matching and defensive re-validation. | Single-pass parsing directly into packed atom codes; layout traverses contiguous records with zero redundant parsing. |
| **2. Allocate Only Necessary Memory** (§2) | Dynamic linked-list node allocation (`get_node`) on every atom, glue, and script. | **Zero heap allocation** on hot math layout paths. Bounded 64-slot stack buffers for formula rows ($\le 1\,\text{KB}$). |
| **3. Keep Working Set Small** (§6) | Deep pointer structures scattered across global memory heap (`mem[]`). | Entire active formula layout state fits inside **L1 CPU Cache** (32 KB limit). Zero TLB misses. |
| **4. Contiguous and Predictable Access** (§7) | Pointer chasing through linked lists (`link(p)` and `info(p)`). | Sequential array traversal over contiguous `[MathAtomRecord; 64]` buffers. Maximizes hardware prefetcher hit rate. |
| **5. Separate Hot from Cold Data** (§8) | Node structures containing debug strings, token histories, and source locations mixed with dimensions. | **Hot path**: packed `(x, y, w, h, d, ic)` geometry. **Cold path**: diagnostics, error formatting, and macro definitions stored outside layout structures. |
| **6. Smallest Appropriate Representation** (§10) | Multi-word integers and pointer structures for character properties. | **12-bit packed `\mathcode` in `u16`**: 3-bit class, 4-bit family, 8-bit glyph index. Atom kinds stored in `u8`. |
| **7. Precompute Stable Knowledge** (§12) | Dynamic metric loading, TFM parsing, and runtime spacing rule calculations. | **Compile-time `.rodata` tables**: 64-byte aligned TFM glyph tables, 64-byte branchless spacing matrix, 512-byte static `\mathcode` LUT. |
| **8. Specialize the Common Case** (§16) | Uniformly heavy linked-list allocation even for simple two-symbol formulas ($x+y$). | Fast path: Formulas with $\le 64$ atoms evaluated directly on stack. General fallback: Bounded arena vector for pathological formulas. |
| **9. Phase-Specific Representations** (§17) | Monolithic AST retaining syntax tokens through to rendering. | Phase 1 (Parser) emits AST $\to$ Phase 2 (Layout) flattens to linear geometric records $\to$ Phase 3 (Backend) paints cached glyph runs. Tokens discarded immediately. |
| **10. Bound Resource Growth** (§23) | Unbounded recursion and unconstrained heap growth on deeply nested formulas. | Bounded stack arrays with explicit capacity checks; graceful fallback without panics or memory blowup. |

