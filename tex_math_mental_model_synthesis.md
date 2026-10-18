# TeX Mathematical Typesetting Mental Model: Top-to-Bottom Synthesis

**Status:** Architectural Reference  
**Scope:** Formal mapping of Donald Knuth's 3-Stage Mathematical Engine to RushTeX

---

## 1. The Big Picture: Math is Just Boxes and Glue

TeX has only three primitive building blocks for all visible output:
- **Boxes**: Enclosing rectangles with width ($w$), height ($h$), and depth ($d$).
- **Glue**: Elastic horizontal/vertical space with natural dimension, stretchability, and shrinkability.
- **Rules**: Solid filled rectangles of arbitrary width and height.

Math typesetting does not have its own special PDF renderer. Its entire function is:
$$\text{LaTeX Math Source} \longrightarrow \text{Ordinary HBox (Boxes + Glue + Rules)}$$
Once converted, the resulting HBox is handed back to normal document layout:
- **Inline Math**: Placed directly into the paragraph horizontal list, subject to Knuth-Plass line breaking.
- **Display Math**: Centered horizontally on its own line, surrounded by `\abovedisplayskip` and `\belowdisplayskip`.

---

## 2. Stage 1: The Math List (Tree of "Noads")

While parsing `$...$` or `\[...\]`, TeX does not create character boxes. It constructs a linked list of **noads** (math-specific node records).

### 2.1 The Eight Fundamental Atom Classes
Every symbol is classified into one of eight classes via its 12-bit `\mathcode`:
1. `Ord` (Ordinary: $x, 1, \alpha$)
2. `Op` (Large operator: $\sum, \int, \lim$)
3. `Bin` (Binary operator: $+ , -, \times$)
4. `Rel` (Relational operator: $=, <, \le$)
5. `Open` (Opening delimiter: $(, [, \{)
6. `Close` (Closing delimiter: ), ], \})
7. `Punct` (Punctuation: comma, semicolon)
8. `Inner` (Inner sub-formula, fraction, or radical)

### 2.2 Structural Noads
Alongside symbol noads, the parser emits structural noads:
- **Fraction Noad**: Holds numerator and denominator math lists, and rule thickness.
- **Radical Noad**: Holds radicand math list and surd delimiter descriptor.
- **Under / Over Noads**: Stacks bar rules or braces above/below sub-lists.
- **Accent Noad**: Carries nucleus math list and accent glyph code.
- **Vcenter Noad**: Vertically centers an arbitrary box around the math axis.
- **Left / Right Delimiter Noads**: Open and close variable-height delimiter groups.
- **Style Noads**: Explicit style switches (`\displaystyle`, `\textstyle`, etc.).
- **Choice Noads**: Four parallel sub-lists (`{Display}{Text}{Script}{ScriptScript}`); the active one is selected once the surrounding style is resolved.

### 2.3 Field Anatomy of Every Noad
Every standard noad contains three optional sub-fields:
- **Nucleus**: The base mathematical item (character, sub-box, or nested math list).
- **Superscript**: The exponent field (a nested math list).
- **Subscript**: The index field (a nested math list).

This recursive structure allows mathematical formulas to nest arbitrarily without special-cased data types.

---

## 3. The Driving Variable: The Style Hierarchy

TeX defines eight styles:
1. $\text{Display} \ (D)$ and $\text{Cramped Display} \ (D')$
2. $\text{Text} \ (T)$ and $\text{Cramped Text} \ (T')$
3. $\text{Script} \ (S)$ and $\text{Cramped Script} \ (S')$
4. $\text{ScriptScript} \ (SS)$ and $\text{Cramped ScriptScript} \ (SS')$

### 3.1 What "Cramped" Means
"Cramped" styles suppress the extra vertical elevation of superscripts. For example, in a fraction denominator or inside a square root, superscripts use lower shifts ($sup_3$ instead of $sup_1$) so formulas do not become excessively tall.

### 3.2 The Appendix G Style Transition Table
- Numerator of fraction in style $S$: transitions to $S$ (or $S'$ if parent was cramped).
- Denominator of fraction in style $S$: transitions to cramped $S'$.
- Superscript of style $S$: transitions to the script tier of $S$ (uncramped unless parent was cramped).
- Subscript of style $S$: transitions to the script tier of $S$ (always cramped).
- Nested scripts bottom out at `ScriptScript`—TeX never scales below 5pt.

---

## 4. Fonts: 16 Families $\times$ 3 Optical Sizes

Every math family ($0..15$) points to three distinct fonts simultaneously:
- `\textfont f`: Active in Display and Text styles ($10\,\text{pt}$)
- `\scriptfont f`: Active in Script style ($7\,\text{pt}$)
- `\scriptscriptfont f`: Active in ScriptScript style ($5\,\text{pt}$)

**The Crucial Insight**:
TeX never multiplies $10\,\text{pt}$ glyph metrics by $0.70$ or $0.50$. The current style *selects a completely different font file* (`cmmi7`, `cmsy7`, `cmmi5`, `cmsy5`) designed with proportionally thicker stems, wider tracking, and distinct metrics.

### 4.1 The 22 Font Dimension Parameters ($\sigma_1 \dots \sigma_{22}$)
Families 2 (`cmsy`) and 3 (`cmex`) provide the 22 canonical mathematical parameters:
- $\sigma_1 \dots \sigma_7$: `slant`, `space`, `stretch`, `shrink`, `x_height`, `quad`, `num1`
- $\sigma_8 \dots \sigma_{14}$: `num2`, `num3`, `denom1`, `denom2`, `sup1`, `sup2`, `sup3`
- $\sigma_{15} \dots \sigma_{22}$: `sub1`, `sub2`, `sup_drop`, `sub_drop`, `delim1`, `delim2`, `axis_height`, `default_rule_thickness`

All vertical lifts, clearances, rule thicknesses, and gaps in Stage 2 pull from these parameters **for the font active at the current style**.

---

## 5. Stage 2: `mlist_to_hlist` (The Core Conversion)

Execution occurs in two distinct passes over the math list:

### Pass One: Recursive Box Conversion
- **Symbol / Character Noads**: Converted to character boxes. TeX Rule 18 evaluates trial vertical shifts ($sup_{1..3}, sub_{1..2}$), checks clearance against nucleus height/depth, enforces minimum gap between sub and sup, and appends `\scriptspace` ($0.5\,\text{pt}$).
- **Fraction Noads**: Recursively builds numerator and denominator boxes at their respective sub-styles, places a rule of `default_rule_thickness`, and centers the assembly vertically across `axis_height`.
- **Radical Noads**: Selects the smallest surd from `char_list` or builds an extensible surd (`top/mid/bot/rep`), enforces Rule 11 clearances ($clr$), and draws the overbar.
- **Left / Right Delimiters**: Measures maximum height and depth between delimiters, queries `char_list` chains, falls back to extensible assembly, and centers across the axis.
- **Big Operators**: Swaps to larger variant glyphs in display style, stacks limits above/below using `big_op_spacing1..5`, and incorporates italic correction directly into box width.
- **Choice Noads**: The active style is now known; the three unused branches are discarded.

### Pass Two: Inter-Atom Glue Insertion
- Iterates left-to-right through the boxed list.
- Queries the $8 \times 8$ spacing matrix for adjacent atom pairs.
- Reclassifies `Bin` to `Ord` if at the beginning of a list or preceded by `Bin`, `Rel`, `Open`, or `Op` (TeX Rule 5).
- Converts `mu` to physical points using the active font's `quad` ($18\,\text{mu} = 1\,\text{quad}$).
- Suppresses `Bin` and `Rel` spacing to $0\,\text{mu}$ in Script and ScriptScript styles.
- Composites user-level explicit glues (`\!`, `\,`, `\quad`) additively with atom spacing.

---

## 6. Mapping to RushTeX's Architecture

The existing RushTeX code boundaries align remarkably well with this model:
- `math_parser.rs` $\longleftrightarrow$ **Stage 1 (Math List generation)**
- `math_layout.rs` / `math_fonts.rs` $\longleftrightarrow$ **Stage 2, Pass One (Noad to Box conversion)**
- `math_spacing.rs` $\longleftrightarrow$ **Stage 2, Pass Two (Inter-atom glue resolution)**
- `layout_engine.rs` $\longleftrightarrow$ **Stage 3 (Dissolving HBox into paragraph/page)**

The discrepancies were never structural flaws in RushTeX's architecture. They were parameter and font provenance gaps inside Pass One and Pass Two:
1. Approximating metrics via $0.70\times/0.50\times$ scale factors instead of querying the optical font file.
2. Approximating vertical clearances via fractions of font size instead of reading the 22 $\sigma$ parameters.
3. Bypassing inter-atom spacing when explicit `Space` nodes were present in Pass Two.
