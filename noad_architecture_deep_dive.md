# TeX Noad Architecture & Foundational Structural Mechanics

**Status:** Foundational Architecture Note  
**Primary Reference:** Donald Knuth's `tex.web` (Sections 35–37) & *The TeXbook* (Chapter 17, 26, and Appendix G)

---

## 1. Why Donald Knuth Invented "Noads"

In TeX's ordinary paragraph builder, characters from the input stream are immediately converted into concrete **nodes**:
- `char_node`: A glyph of fixed width, height, and depth from a specific font.
- `hlist_node` / `vlist_node`: Concrete boxes containing resolved positions.
- `glue_node` / `kern_node`: Fixed or elastic spaces.

In mathematical mode (`$...$`), **this immediate conversion is physically impossible**:
1. **Style is unknown**: A letter $x$ cannot be sized as $10\,\text{pt}$, $7\,\text{pt}$, or $5\,\text{pt}$ until its position in the formula tree (display, text, numerator, superscript) is resolved.
2. **Grammar is context-dependent**: A hyphen `"-"` cannot be classified as a binary operator ($a - b$) or a unary prefix ($-x$) until its neighbors are parsed.
3. **Heights depend on children**: The height of a fraction rule, radical surd, or surrounding parentheses cannot be computed until the inner sub-formula is completely parsed and measured.
4. **Scripts require nucleus geometry**: The vertical shift and horizontal stagger of $x_i^2$ depend on the bounding box and italic correction of the nucleus $x$.

To solve this, Knuth created the concept of a **Noad** (a "node-like object", or "not-quite-a-node"). A Noad is a **structural semantic blueprint** that captures mathematical intent during parsing and postpones all physical geometry decisions until the entire formula is assembled into a **math list (MList)**.

---

## 2. Structural Memory Layout of a Noad in TeX

In `tex.web` (Section 35), Knuth defines a standard Noad as a compact, fixed 4-word structure (16 bytes on a 32-bit machine):

```
+-------------------------------------------------------------+
| Word 0: [ type: 8 bits ] [ subtype: 8 bits ] [ link: 16b ]  |
+-------------------------------------------------------------+
| Word 1: nucleus  (field: math_char | sub_box | sub_mlist)   |
+-------------------------------------------------------------+
| Word 2: supscr   (field: math_char | sub_box | sub_mlist)   |
+-------------------------------------------------------------+
| Word 3: subscr   (field: math_char | sub_box | sub_mlist)   |
+-------------------------------------------------------------+
```

### 2.1 The Eight Primary Atom Types
The `type` field classifies the Noad into one of the eight canonical spacing classes:
- `ord_noad` (0): Ordinary symbol or variable ($x, 1, \alpha$)
- `op_noad` (1): Large operator ($\sum, \int, \lim$)
- `bin_noad` (2): Binary operator ($+, -, \times, \cdot$)
- `rel_noad` (3): Relation ($=, <, \ge, \in$)
- `open_noad` (4): Opening delimiter ($(, [, \{, |$)
- `close_noad` (5): Closing delimiter ($), ], \}, |$)
- `punct_noad` (6): Punctuation (comma, semicolon)
- `inner_noad` (7): Inner sub-formula, fraction, or radical

### 2.2 The Subtype Field
Used to encode behavioral flags without growing the structure. For example, on an `op_noad`:
- `subtype = normal`: Limits placed per default (above/below in display, side-scripts in text).
- `subtype = limits`: Explicit `\limits` override (limits forced above and below).
- `subtype = no_limits`: Explicit `\nolimits` override (limits forced to side scripts).

### 2.3 The Three Universal Fields: Nucleus, Superscript, Subscript
Every standard Noad carries:
1. `nucleus`: The central mathematical object.
2. `supscr`: The superscript expression (or `empty_field`).
3. `subscr`: The subscript expression (or `empty_field`).

Each of these three fields is polymorphic (`math_type`):
- `math_char`: A single character code and font family index `(fam, char)`.
- `sub_box`: A pre-evaluated concrete box (e.g. `\hbox{...}`).
- `sub_mlist`: A pointer to an entire nested math list (e.g. `{x^2 + 1}`).
- `empty`: Nothing is present.

---

## 3. Specialized Structural Noads

When mathematical constructs require distinct parameters, TeX extends the Noad record:

1. **Fraction Noad (`fraction_noad`, 6 words / 24 bytes)**:
   - Replaces `nucleus`, `supscr`, `subscr` with:
     - `thickness`: Fraction bar thickness (`default_code` for canonical rule thickness).
     - `numerator`: Sub-mlist pointer for numerator.
     - `denominator`: Sub-mlist pointer for denominator.
     - `left_delimiter`: Delimiter code for `\overwithdelims`.
     - `right_delimiter`: Delimiter code for `\overwithdelims`.
2. **Radical Noad (`radical_noad`, 5 words / 20 bytes)**:
   - Holds standard `nucleus`, `supscr`, `subscr`, plus:
     - `left_delimiter`: The radical surd delimiter descriptor.
3. **Accent Noad (`accent_noad`, 5 words / 20 bytes)**:
   - Holds standard `nucleus`, `supscr`, `subscr`, plus:
     - `accent_chr`: The accent character and family `(fam, char)`.
4. **Delimiter Noad (`left_noad` / `right_noad`)**:
   - Holds delimiter specification for dynamic height sizing.
5. **Choice Noad (`choice_node`)**:
   - Holds four pointers to alternative math lists (`display`, `text`, `script`, `scriptscript`).
6. **Style Noad (`style_node`)**:
   - Records explicit style transitions (`\displaystyle`, `\textstyle`).

---

## 4. How Noads Translate into Modern Rust & RushTeX

In modern Rust, Knuth's union-based `mem[]` words are cleanly and safely expressed as an **Algebraic Data Type (Enum)**:

| TeX 1982 (Pascal `mem[]` Words) | Modern Rust (`MathNode` Enum) | Memory Strategy under `Resource_Conscious_Software_Engineering.md` |
| :--- | :--- | :--- |
| Dynamic 4–6 word heap allocation (`get_node`) | Tagged enum variant | **Stack-allocated contiguous array** for formula rows; zero heap allocation on hot path. |
| Integer `math_type` tag | Enum discriminator | Compact 1-byte discriminator (`#[repr(u8)]`). |
| Pointer-based linked list (`link(p)`) | Sequential array / slice | Sequential contiguous memory (`[MathAtomRecord; 64]`) maximizing L1 cache prefetching. |
| Global `fam_fnt` array lookup | Static `.rodata` font tables | Precomputed 64-byte aligned compile-time metric tables. |

---

## 5. The Exact Transformation Lifecycle

```
[Input LaTeX: "x^2 + \frac{1}{y}"]
            ↓
   (1. PARSER: Stage 1)
   Emits Math List of Noads:
   [
     Noad(type=Ord, nucleus='x', sup='2'),
     Noad(type=Bin, nucleus='+'),
     FractionNoad(num='1', denom='y')
   ]
            ↓
   (2. CONVERTER: Stage 2 - Pass 1)
   Recursively converts each Noad to a MathBox using current Style's FontDimens:
   - Box('x') with sup Box('2') shifted by sup1 and kerned by scriptspace.
   - Box('+') from Symbol font.
   - Box(Fraction) with rule_thickness, num/denom stacked around axis_height.
            ↓
   (3. CONVERTER: Stage 2 - Pass 2)
   Iterates across the Box sequence:
   - Evaluates inter-atom spacing via 8x8 matrix (Ord → Bin gets medmuskip).
   - Resolves mu into physical points via current font's quad.
   - Normalizes unary/binary operators (Rule 5).
            ↓
   (4. DISSOLUTION: Stage 3)
   A single concrete HBox of (Boxes + Glue + Rules) is handed to document layout.
```

This confirms our foundational model: **Noads are purely structural blueprints**. All typographic beauty, spacing, clearances, and alignments are resolved in Stage 2 by querying the authoritative font dimension parameters for the active style.
