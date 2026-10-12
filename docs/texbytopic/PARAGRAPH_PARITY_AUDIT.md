# TeX by Topic (Eijkhout): Paragraph Construction Audit & Parity Specification

This document details the exact specifications, algorithmic invariants, and mathematical formulas extracted from **Chapters 15–20 of Victor Eijkhout's *TeX by Topic: A Systematizer's Reference***, mapped directly to RushTeX's implementation in [`src/layout/layout_hz.rs`](file:///home/triton/Coding/rushtex/src/layout/layout_hz.rs) and [`src/layout/layout_engine.rs`](file:///home/triton/Coding/rushtex/src/layout/layout_engine.rs).

---

## 1. Chapter Summary & Core Primitives

| Chapter | Topic | Key Primitives / Parameters | Status in RushTeX |
| :--- | :--- | :--- | :--- |
| **Ch 15** | Baseline Distances | `\baselineskip`, `\lineskip`, `\lineskiplimit` | Implemented (fixed leading & minimum gap) |
| **Ch 16** | Paragraph Start | `\parindent`, `\everypar`, `\indent`, `\noindent` | Implemented (`parindent`, heading suppress, `\noindent`) |
| **Ch 17** | Paragraph End | `\par`, `\parfillskip`, `\break`, `\endgraf` | Implemented (last-line terminal glue & flush) |
| **Ch 18** | Paragraph Shape | `\hangindent`, `\hangafter`, `\parshape`, `\hsize` | `\hangindent`/`\hangafter` supported; `\parshape` matrix pending |
| **Ch 19** | Line Breaking | 3 passes, fitness classes, demerit formulas, `\tolerance` | Single-pass DP + font expansion (`hz`); full 3-pass & demerits needed |
| **Ch 20** | Spacing | Space factor `\sfcode`, `\spaceskip`, `\xspaceskip` | Uniform space width; space factor adjustment pending |

---

## 2. Invariant Specifications from *TeX by Topic*

### 2.1 Baseline Distances (Chapter 15)
- **Desired distance**: `\baselineskip` ($B$).
- **Actual distance between boxes**: $\Delta = B - \text{depth}(box_1) - \text{height}(box_2)$.
- **Rule**:
  $$\text{Interline Glue} = \begin{cases} \Delta & \text{if } \Delta \ge \text{\textbackslash lineskiplimit} \\ \text{\textbackslash lineskip} & \text{if } \Delta < \text{\textbackslash lineskiplimit} \end{cases}$$
- **RushTeX Status**: Preserved across all paragraph lines and multi-page flows.

### 2.2 Paragraph Shape (Chapter 18)
- **Hanging Indentation**:
  - `\hangindent = x`, `\hangafter = n`.
  - If $n \ge 0$: lines $1 \dots n$ have width `\hsize`; lines $n+1 \dots$ have width `\hsize - |x|` (indented by $x$ on left if $x > 0$, or on right if $x < 0$).
  - If $n < 0$: lines $1 \dots |n|$ are indented; subsequent lines have full `\hsize`.
- **General Shape (`\parshape`)**:
  - Matrix of $N$ lines: $[(i_1, w_1), (i_2, w_2), \dots, (i_N, w_N)]$.
  - Line $k > N$ reuses $(i_N, w_N)$.

### 2.3 Line Breaking & Demerits (Chapter 19)
- **Badness ($b$)**:
  $$b = \begin{cases} 100 \times (|r|)^3 & \text{if } |r| \le 1 \\ 10000 & \text{if } |r| > 1 \end{cases}$$
- **Demerits ($d$)**:
  $$d = \begin{cases} (\text{\textbackslash linepenalty} + b)^2 + p^2 & \text{if } p \ge 0 \\ (\text{\textbackslash linepenalty} + b)^2 - p^2 & \text{if } -10000 < p < 0 \\ (\text{\textbackslash linepenalty} + b)^2 & \text{if } p \le -10000 \end{cases}$$
- **Additional Demerits**:
  - Plus `\doublehyphendemerits` if current line and preceding line both end in a hyphen.
  - Plus `\finalhyphendemerits` if the second-to-last line of the paragraph ends in a hyphen.
  - Plus `\adjdemerits` if adjacent lines belong to incompatible fitness classes (difference in class index $> 1$).
- **Four Fitness Classes**:
  - Class 0 (Tight): $r < -0.5$
  - Class 1 (Decent): $-0.5 \le r \le 0.5$
  - Class 2 (Loose): $0.5 < r \le 1.0$
  - Class 3 (Very Loose): $r > 1.0$
- **Three-Pass Engine Architecture**:
  1. **Pass 1**: `pretolerance` (default 100). No hyphenation attempted. If total demerits $< \infty$, done.
  2. **Pass 2**: `tolerance` (default 200). Hyphenation dictionary (Knuth-Liang) enabled.
  3. **Pass 3**: Only triggered if `emergencystretch > 0` and Pass 2 fails. Adds `emergencystretch` to the stretchability of every line.

### 2.4 Spacing & Space Factors (Chapter 20)
- Character space factor $f$ (`\sfcode`):
  - Uppercase letters ($A..Z$): $f = 999$.
  - Lowercase letters ($a..z$): $f = 1000$.
  - Punctuation: `.`, `?`, `!` have $f = 3000$; `:` has $f = 2000$; `;` has $f = 1500$; `,` has $f = 1250$.
- **Glue adjustment from $f$**:
  - When $f \ge 2000$: Space width expands by font extra space (`\fontdimen 7`), and stretch expands by factor $f / 1000$.
  - When `\frenchspacing` is active, all `\sfcode` values are reset to 1000.

---

## 3. The Paragraph Parity Ladder

1. **Step 1A: Space Factor Table (`\sfcode`) & Punctuation Spacing**
   - Add zero-cost L1-cached ASCII array for space factors (1000 default, 3000 for periods, 1250 for commas).
   - Multiply inter-word glue stretch/shrink by space factor.
2. **Step 1B: Exact Demerit & Fitness Class Invariants**
   - Incorporate the 4 fitness classes into DAG node states.
   - Add `\adjdemerits`, `\doublehyphendemerits`, and `\finalhyphendemerits` penalties to DAG edges.
3. **Step 1C: Knuth-Liang Hyphenation Discretionary Integration**
   - Add hyphenation break opportunities with `\hyphenpenalty` (default 50) and `\exhyphenpenalty` (default 50).
4. **Step 1D: `\parshape` Multi-Line Matrix Support**
   - Allow generalized line width and indent tuples per paragraph.
