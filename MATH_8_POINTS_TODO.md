# RushTeX Mathematical Typesetting Architecture: The 8 Points Roadmap

This document outlines the complete architectural roadmap for full TeX Live and pdfLaTeX mathematical typesetting parity in RushTeX.

---

## 1. Syntax Parsing & Semantic AST
- [x] Math mode delimiters: Inline (`$...$`, `\(...\)`) and display (`\[...\]`, `$$...$$`, `\begin{equation}`).
- [x] Core AST nodes: Atoms, Operators, Scripts (Sub/Super), Fractions, Radicals, Delimiters, Accents, Matrices/Grids, Spacers.
- [x] Multi-style branch selection: `\mathchoice{D}{T}{S}{SS}` (TeX.web §1171).
- [x] Atom class coercion: `\mathord`, `\mathop`, `\mathbin`, `\mathrel`, `\mathopen`, `\mathclose`, `\mathpunct`, `\mathinner`.

## 2. The 8-Style State Machine
- [x] The 8 TeX styles: Display ($D$), Cramped Display ($D'$), Text ($T$), Cramped Text ($T'$), Script ($S$), Cramped Script ($S'$), ScriptScript ($SS$), Cramped ScriptScript ($SS'$).
- [x] Style propagation: Fractions (num in $T$, denom in cramped $T'$), radicals (cramped), scripts ($S$ / $SS$).
- [x] Explicit style scoping: `\displaystyle`, `\textstyle`, `\scriptstyle`, `\scriptscriptstyle`.
- [x] Dynamic branch evaluation in `\mathchoice` based on active style.

## 3. Atom Classification & Spacing Matrix (Appendix G Rule 5)
- [x] 8 Atom classes: `Ord`, `Op`, `Bin`, `Rel`, `Open`, `Close`, `Punct`, `Inner`.
- [x] Complete $8 \times 8$ spacing matrix ($0$, $3\,\text{mu}$, $4\,\text{mu}$, $5\,\text{mu}$) for Display/Text and Script styles.
- [x] Binary atom normalization: Leading/trailing `Bin` converted to `Ord`.
- [x] Coerced atom integration: Dynamic atom kind overrides directly driving the Rule 5 spacing engine.

## 4. Authoritative Font Metrics & Mathematical Constants
- [x] 22 global font dimensions ($\sigma_1 \dots \sigma_{22}$) from `cmsy10` / math parameters.
- [x] Exact 8-byte static `GlyphMetric` structs packed for L1 cache residency (CMSY10, CMEX10, CMMI10, CMR10, CMBX10, CMTI10).
- [x] 100% CMEX10 operator metrics and delimiter chains.
- [x] Canonical `skewchar` (char 127) kerning tables from `cmmi10.tfm` for optical apex alignment.

## 5. Geometric Construction Rules (Appendix G Rules 10–21)
- [x] Rule 10: Fractions and binomials with axis centering, elevation shifts, and clearance guarantees.
- [x] Rule 11: Radicals and surds with discrete sizing tiers (0x70–0x73) and extensible assemblies (0x74–0x76).
- [x] Rule 11b: Degree indexing ($\sqrt[n]{x}$) tucked into the radical hook.
- [x] Rule 12: Math accents (dots, vectors, wide accents `\widehat`, `\widetilde`).
- [x] Rule 12b: Authoritative `skewchar` optical accent positioning over slanted italic letters.
- [x] Rule 13: Large operators centered on the mathematical axis with italic correction limit overhang.
- [x] Rule 18: Simultaneous sub/superscripts with collision prevention and `sub_drop`/`sup_drop`.
- [x] Rule 20 & 21: Overline, underline, and `\vcenter` vertical centering.

## 6. Delimiters & Extensible Assemblies (Rule 19)
- [x] Automatic delimiter sizing (`\left` / `\right`) targeting formula height.
- [x] Discrete variant chains in `CMEX10` (parens, brackets, braces, floors, ceilings, angles, slashes).
- [x] Extensible 4-piece glyph stacks (parens, brackets, braces, floors, ceilings, vertical bars).
- [x] Extensible single (`|`) and double (`\|`) vertical bars using CMEX modules 0x0C and 0x0D.

## 7. Content-Sensitive Multiline Grids & Matrices
- [x] Alignment environments: `matrix`, `pmatrix`, `bmatrix`, `vmatrix`, `Vmatrix`, `cases`.
- [x] AMS multiline environments: `align`, `align*`, `gather`, `flalign`, `split`.
- [x] Column width computation and relation alignment (`&`).
- [x] Equation numbering and margin clearance (`\tag`, `\label`, `\ref`).
- [x] `smallmatrix` inline compact matrix environment.

## 8. Display Math Integration & PDF Generation
- [x] Equation centering ($d = \frac{1}{2}(\text{line\_w} - \text{eq\_w})$).
- [x] Contextual display skips (`abovedisplayskip`, `belowdisplayskip`).
- [x] Embedded Type 1 font programs (CMR, CMMI, CMSY, CMEX, MSAM, MSBM).
- [x] Full ToUnicode CMap searchability for all math symbols and operators.
- [x] Preceding line width threshold detection for short display skips (`abovedisplayshortskip`).
