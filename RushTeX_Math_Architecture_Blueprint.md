# RushTeX Mathematical Typesetting Architecture

## A broad design blueprint for pdfLaTeX-compatible mathematical geometry

**Status:** Architectural guidance based on the current RushTeX math
subsystem and the September 2026 reference manual\
**Primary goal:** Make the same supported LaTeX mathematical source
produce mathematical typesetting that is geometrically and
typographically equivalent to a clearly defined legacy LaTeX reference
profile, while preserving RushTeX's independent Rust implementation and
performance-oriented architecture.

------------------------------------------------------------------------

## 1. Executive direction

RushTeX should not pursue mathematical fidelity by manually adjusting
individual formulas, symbols, or screenshots. It should pursue fidelity
by reproducing the **general mathematical typesetting model** that
causes mature TeX engines to place every nucleus, script, rule,
delimiter, accent, operator, and matrix cell where it belongs.

The current RushTeX architecture is already moving in the right
direction. It separates:

-   mathematical syntax and parsing;
-   a semantic mathematical AST;
-   mathematical styles;
-   atom classification and inter-atom spacing;
-   geometric boxes;
-   font/symbol resolution;
-   mathematical layout;
-   rendering into the existing PDF display pipeline;
-   tests.

That structure should be preserved. The next major phase should not be
another wholesale redesign. It should be a transition from
**approximation-driven geometry** to **authoritative, rule-driven,
font-aware geometry**.

The central architectural principle is:

> **Mathematical source describes structure. The math engine resolves
> structure into exact geometry. The PDF backend only paints
> already-resolved geometry.**

The renderer should never be responsible for deciding where a
superscript belongs, how tall a parenthesis should be, how far a
numerator should sit above a fraction rule, or where an accent attaches.
Those are mathematical-layout decisions and must be completed before PDF
emission.

The long-run quality target should therefore be expressed as:

> **For a defined compatibility profile, the same mathematical source
> should produce the same mathematical structure, style transitions,
> spacing classes, font metrics, box dimensions, baseline relationships,
> delimiter choices, and visible placement as the reference TeX/LaTeX
> system, within explicitly defined tolerances.**

This is more precise than simply saying "make it look like LaTeX."

------------------------------------------------------------------------

## 2. Define exactly what "identical to LaTeX" means

Before further refinement, RushTeX should freeze a **reference
compatibility profile**.

"LaTeX" is not one immutable visual output. Mathematical appearance
depends on the engine, format, font set, packages, document class, font
size, and macro definitions. Therefore the project should define one
initial gold standard, for example:

-   pdfLaTeX;
-   a specific TeX Live release;
-   a specific base document class and options;
-   a specific default mathematical font family;
-   a defined set of supported packages;
-   a defined set of relevant TeX parameters;
-   a defined output magnification and page geometry.

Only after RushTeX can reproduce that profile closely should additional
profiles be introduced.

This avoids an impossible target such as "identical to every LaTeX
installation." Instead, RushTeX can say:

> **RushTeX Math Compatibility Profile 1 targets the mathematical layout
> behavior of a documented pdfLaTeX configuration.**

Later profiles can target other font packages or engines without
corrupting the core mathematical model.

### 2.1 Four levels of compatibility

Treat mathematical compatibility as four distinct layers.

**Layer 1 --- Source compatibility.**\
The same supported LaTeX source is accepted.

**Layer 2 --- Semantic compatibility.**\
The source is interpreted as the same mathematical structure: the same
atoms, operators, groups, scripts, fractions, radicals, delimiters,
accents, matrices, styles, and limits.

**Layer 3 --- Geometric compatibility.**\
The same structures receive equivalent dimensions and relative
positions: baselines, widths, heights, depths, clearances, rule
positions, script offsets, delimiter extents, and matrix geometry.

**Layer 4 --- Visual/PDF compatibility.**\
The resulting glyphs, rules, and page placement look equivalent when
rendered, subject to the exact font and PDF representation chosen.

RushTeX should test all four independently. A formula can pass Layer 1
while failing Layer 2; it can pass Layer 2 while having visibly wrong
geometry; and it can have correct geometry but use different glyph
outlines.

------------------------------------------------------------------------

## 3. Preserve the current subsystem separation

The current source organization is a good foundation and should remain
conceptually stable:

-   `math_parser`: mathematical syntax into semantic structure;
-   `math_ast`: formula structure and style hierarchy;
-   `math_spacing`: atom classes and inter-atom spacing;
-   `math_fonts`: symbol and mathematical-font knowledge;
-   `math_box`: compact geometric result representation;
-   `math_layout`: structure-to-geometry conversion;
-   `math_tests`: correctness and regression testing.

The most important architectural improvement is not to merge these
layers, but to make the contracts between them stronger.

### 3.1 Parser responsibility

The parser should answer:

> **What mathematical object did the author request?**

It should not answer:

> **Where should this object be drawn?**

For example, the parser may determine that a node is a fraction with a
numerator and denominator. It should not decide fraction-rule thickness,
numerator elevation, or denominator clearance.

### 3.2 Layout responsibility

The mathematical layout layer should answer:

> **Given this mathematical structure, current style, mathematical font,
> and compatibility profile, what exact geometry does it require?**

This is the center of the math engine.

### 3.3 Font responsibility

The mathematical-font layer should answer:

> **What are the real metrics and mathematical properties of the
> selected glyphs and font?**

It should eventually become the authority for glyph advance, bounding
geometry, italic correction, accent attachment, available size variants,
extensible constructions, and mathematical constants.

### 3.4 Backend responsibility

The PDF backend should answer only:

> **How do I paint these resolved glyphs and rules efficiently and
> correctly?**

Keeping the backend ignorant of mathematical semantics is essential. It
makes the math engine testable independently of PDF serialization and
keeps future backends possible.

------------------------------------------------------------------------

## 4. Replace empirical proportions with authoritative mathematical parameters

This is the highest-priority architectural direction.

The current layout implementation contains many useful provisional
geometric approximations expressed as fractions of the current font
size. Examples include approximate axis height, glyph height/depth,
script shifts, fraction clearances, radical geometry, operator sizing,
delimiter scaling, matrix row spacing, and accent placement.

Those approximations were valuable because they allowed RushTeX to cross
the threshold from "math symbols are emitted" to "math is structurally
typeset." They should now be treated as **bootstrap values**, not as the
final compatibility model.

The next generation of the engine should obtain mathematical geometry
from three ordered sources:

1.  **Reference-compatible mathematical parameters** for the selected
    compatibility profile.
2.  **Per-font/per-glyph mathematical metrics** where the font provides
    them or RushTeX has precomputed them.
3.  **Explicit fallback rules** only where authoritative data is
    unavailable.

The fallback layer should be visible and testable. It should not
silently become the normal path.

### 4.1 Why this matters systemically

A single correct mathematical metric can improve many unrelated-looking
formulas.

Accurate script parameters improve:

-   powers;
-   indices;
-   tensor notation;
-   nested scripts;
-   scripts attached to fractions;
-   scripts attached to radicals;
-   scripts attached to delimiters;
-   scripts attached to operators.

Accurate axis and rule parameters improve:

-   fractions;
-   large operators;
-   delimiters;
-   radicals;
-   relation alignment;
-   matrix centering.

Accurate italic correction improves:

-   variable superscripts;
-   integral limits/scripts;
-   operator scripts;
-   visual spacing after slanted glyphs.

Therefore the project should prefer changes that improve an entire class
of expressions rather than changes that improve one screenshot.

------------------------------------------------------------------------

## 5. Build a real mathematical-font model

RushTeX currently resolves mathematical commands to faces and glyphs.
That is necessary, but high-fidelity mathematical typography requires a
richer concept of a **Math Font**.

A mathematical font is not merely a table from command names to glyph
codes. Architecturally, it should expose several categories of
information.

### 5.1 Global mathematical constants

The font/profile layer should make available authoritative values for
concepts such as:

-   mathematical axis height;
-   default rule thickness;
-   script and scriptscript scaling;
-   superscript shifts and minimum clearances;
-   subscript shifts and minimum clearances;
-   combined subscript/superscript gap constraints;
-   fraction numerator and denominator shifts;
-   fraction numerator and denominator minimum gaps;
-   display-style variants of fraction parameters;
-   radical vertical gaps;
-   radical rule thickness and extra ascender;
-   upper/lower limit gaps and baseline offsets;
-   minimum heights for display operators;
-   delimiter sizing parameters.

The exact internal representation is a performance decision.
Architecturally, however, these values must be centralized and
authoritative rather than scattered as unrelated constants throughout
layout logic.

### 5.2 Per-glyph mathematical information

For each relevant mathematical glyph, RushTeX should eventually know:

-   advance width;
-   actual height and depth;
-   ink bounds when needed;
-   italic correction;
-   top accent attachment;
-   whether larger variants exist;
-   what those variants are;
-   whether the glyph can be assembled from pieces;
-   assembly-piece geometry and overlap rules where applicable.

This is especially important for slanted variables, integrals, radicals,
delimiters, wide accents, and large operators.

### 5.3 Precompute rather than parse at runtime

RushTeX does not need to abandon its performance philosophy to obtain
richer font mathematics.

The preferred architecture is:

**font source → offline/build-time extraction → compact RushTeX
math-font pack → direct indexed runtime access**

This keeps expensive interpretation away from the hot compilation path.

The runtime should ideally see a compact, immutable mathematical-font
description designed for cache-efficient lookup.

### 5.4 Separate font data from compatibility rules

Do not conflate:

-   "this glyph has this italic correction" with
-   "TeX places a superscript according to this rule."

The first is font data. The second is layout policy.

Keeping them separate will make it possible to support multiple
mathematical fonts without duplicating the layout engine.

------------------------------------------------------------------------

## 6. Make the math box model the universal geometric contract

The current width/height/depth box is fundamentally the correct
abstraction and should remain central.

Every mathematical object should be reducible to a box relative to a
baseline:

-   width;
-   height above baseline;
-   depth below baseline;
-   placement of children;
-   optional auxiliary geometry required for painting.

The essential property is **composability**.

A fraction does not need to know whether its numerator is a single `x`,
a matrix, or another fraction. It asks the numerator for its box and
applies fraction rules.

A radical does not need to know what is inside it. It asks the radicand
for its box and sizes itself accordingly.

A delimiter does not need to understand a matrix. It sees the total
required extent and selects or constructs an appropriate delimiter.

That compositional rule is what makes arbitrary nesting manageable.

### 6.1 Distinguish logical and ink geometry when needed

For mature mathematical typography, one geometric rectangle may
eventually be insufficient.

There are two concepts:

**Logical/advance geometry** controls how neighboring objects are
positioned.

**Ink geometry** describes where the visible glyph actually extends.

For many symbols these are close. For slanted glyphs, accents, large
operators, and unusual symbols they can differ materially.

RushTeX does not necessarily need to enlarge every hot-path box
immediately. A cold or optional metric lookup can provide ink
information where required. The architectural principle is simply that
the engine must not assume advance width and visible bounds are always
identical.

### 6.2 Geometry must be deterministic

The same formula, profile, font, and configuration should produce the
same resolved mathematical geometry across:

-   native builds;
-   WASM;
-   debug/release modes;
-   supported operating systems.

Whether the internal representation remains floating point or later uses
fixed-point units is an implementation choice that should be decided by
measurement. The architectural requirement is deterministic output
within the compatibility contract.

------------------------------------------------------------------------

## 7. Treat mathematical style as a state machine, not a font-size multiplier

The current eight-style model---display, text, script, scriptscript, and
cramped forms---is an important foundation and should be preserved.

The next step is to make style affect **all relevant mathematical
rules**, not merely scale.

Style controls or influences:

-   font selection/size;
-   script transitions;
-   fraction child styles;
-   radical behavior;
-   large-operator variants;
-   limit placement;
-   spacing suppression;
-   clearances;
-   delimiter choices;
-   cramped superscript behavior.

The architecture should therefore think:

> **Style is a mathematical-layout state.**

not:

> **Style is a number multiplied by font size.**

### 7.1 Contextual style propagation

Every compound mathematical object should have an explicit rule for the
styles of its children.

Examples:

-   fraction numerator;
-   fraction denominator;
-   superscript;
-   subscript;
-   radical degree;
-   operator limits;
-   matrix cells;
-   nested groups.

This should be centralized enough that compatibility behavior is
auditable. A future contributor should be able to inspect a
style-transition specification and understand how every structure
changes style.

### 7.2 Cramped styles must have visible meaning

Cramped styles exist because vertical relationships change in certain
contexts. They should not merely be alternate enum values that happen to
share most constants.

As fidelity improves, crampedness should influence the specific
placements for which the reference system distinguishes cramped and
non-cramped behavior.

------------------------------------------------------------------------

## 8. Atom classification and spacing should become a complete compatibility layer

RushTeX already has the classic eight atom classes:

-   ordinary;
-   operator;
-   binary;
-   relation;
-   opening;
-   closing;
-   punctuation;
-   inner.

That is the correct broad model.

The next architectural objective is not a larger spacing table; it is
**correct atom-list normalization and context-sensitive classification
before spacing is applied**.

### 8.1 Classification precedes spacing

The engine should conceptually perform:

**semantic math tree → effective atom sequence → context corrections →
spacing decisions → box composition**

A symbol's nominal class is not always its effective class in context.

The canonical example is a minus sign that may behave as a binary
operator in one position and as a unary/ordinary construct in another.
Similar contextual effects occur around openings, relations, operators,
and boundaries.

Therefore a lookup table can remain extremely fast, but the inputs to
that table must already represent the correct effective atom classes.

### 8.2 Spacing must be style-aware

Some mathematical spaces disappear or change in script styles. RushTeX
already recognizes this direction. It should be treated as part of the
formal compatibility model, not an optimization detail.

### 8.3 Explicit spaces and semantic spacing

LaTeX math includes author-requested spacing commands as well as
automatic atom spacing. These should remain distinct concepts.

Automatic spacing comes from mathematical grammar/classification.

Explicit spacing comes from the source and should survive even where
automatic spacing would not occur.

------------------------------------------------------------------------

## 9. Scripts: make them constraint-driven

Superscripts and subscripts are among the most visible tests of
mathematical quality.

The final architecture should not think in terms of one fixed
"superscript offset" or "subscript offset." It should think in terms of
**candidate positions plus required constraints**.

Relevant constraints include:

-   minimum superscript elevation;
-   minimum superscript bottom clearance;
-   maximum allowed interaction with the base;
-   minimum subscript lowering;
-   minimum subscript top clearance;
-   baseline/drop constraints;
-   minimum gap when both scripts are present;
-   italic correction and glyph-specific attachment behavior;
-   style/crampedness;
-   special treatment of operators and integrals where required.

This transforms script placement from visual nudging into a stable
geometric system.

### 9.1 The compatibility principle

If `x^2`, `f^2`, `\int^b`, and `(\frac ab)^2` require different apparent
placements, RushTeX should not store four exceptions. Their differences
should emerge from:

-   different base metrics;
-   italic correction;
-   different base height/depth;
-   different atom/operator type;
-   different style;
-   common script constraints.

That is the architecture that scales.

------------------------------------------------------------------------

## 10. Fractions: center them on the mathematical axis using constraints

Fractions should be treated as a complete vertical construction, not as
"numerator, horizontal line, denominator."

The architecture should resolve:

1.  child styles;
2.  numerator and denominator boxes;
3.  fraction width;
4.  rule thickness;
5.  rule position relative to the mathematical axis;
6.  numerator shift;
7.  denominator shift;
8.  minimum clearances;
9.  horizontal centering;
10. final height and depth.

Display and non-display styles may require different parameters.

Nested fractions should require no special-case code. If the box
contract and style propagation are correct, nesting should emerge
naturally.

### 10.1 Fractions are a key calibration primitive

Fractions are ideal for validating the entire vertical system because
they exercise:

-   style transitions;
-   baselines;
-   axis;
-   rule thickness;
-   vertical clearances;
-   child dimensions;
-   horizontal centering.

A high-quality fraction subsystem will improve a large portion of
scientific mathematics.

------------------------------------------------------------------------

## 11. Radicals: treat the radical sign and overbar as one extensible construction

The current RushTeX radical implementation has already moved beyond a
simple fixed glyph. The next direction is to generalize it.

A radical consists conceptually of:

-   a radicand box;
-   required vertical clearance;
-   a rule/overbar;
-   a radical glyph or extensible assembly;
-   optional degree/index;
-   attachment/overlap geometry;
-   final combined box.

The radical glyph and overbar must visually connect according to
font/profile data, not according to one hard-coded outline proportion
tied to one font.

### 11.1 Remove font-outline assumptions from generic layout

Any geometry that is specifically derived from one Adobe glyph's outline
should eventually move into font-specific mathematical data.

The generic radical algorithm should ask:

> "Give me a radical construction that satisfies this required extent."

It should not know that a particular font's radical junction occurs at a
particular percentage of glyph size.

This separation is crucial if RushTeX later supports Computer Modern,
Latin Modern Math, Palatino-derived math, or other families faithfully.

------------------------------------------------------------------------

## 12. Build one general extensible-glyph subsystem

This should become one of the major architectural milestones.

Many mathematical features share the same fundamental problem:

> **A symbol must grow to match the geometry of its content.**

Examples include:

-   parentheses;
-   square brackets;
-   braces;
-   vertical bars;
-   angle brackets;
-   radicals;
-   some arrows;
-   wide hats and tildes;
-   over/under braces;
-   large operators in some systems.

Do not solve these as unrelated layout tricks.

The general subsystem should support:

1.  ordinary glyph;
2.  sequence of predesigned larger variants;
3.  extensible assembly from pieces;
4.  minimum overlap rules;
5.  target extent;
6.  axis-aware placement;
7.  final metrics.

### 12.1 Variant selection

For moderate required sizes, select the smallest available variant that
satisfies the target.

### 12.2 Assembly

For sizes beyond available variants, construct the symbol from
top/middle/bottom/extender pieces as appropriate.

### 12.3 Font ownership

The font layer describes which variants and pieces exist.

The layout layer decides what target size is required.

The extensible-glyph subsystem connects the two.

This will eliminate a large class of special cases and make deeply
nested expressions robust.

------------------------------------------------------------------------

## 13. Large operators and limits need their own typographic policy

Summation, product, integral, limit-like operators, and related
constructs should not be treated as ordinary glyphs with scripts.

The layout architecture should distinguish:

-   operator identity/class;
-   display versus text style;
-   available display-sized variants;
-   side scripts versus above/below limits;
-   operator-specific italic correction;
-   upper/lower limit gaps;
-   centering behavior;
-   integral-specific placement where reference behavior differs.

The same source operator may behave differently in inline and display
mathematics. That behavior belongs to the compatibility/layout layer.

### 13.1 Do not generalize too aggressively

A useful abstraction should not erase real typographic differences.

For example, integral script placement is visually and metrically
different from centered limits on a summation operator. The architecture
should share common machinery while allowing operator categories to
apply different placement policies.

------------------------------------------------------------------------

## 14. Accents should use attachment geometry, not geometric centering alone

Accents are now visible and structurally functional in RushTeX. Their
next stage is font-aware optical placement.

The architecture should distinguish:

-   narrow accents over a single nucleus;
-   glyph-specific accent attachment;
-   wide accents over compound nuclei;
-   dot/double-dot constructions;
-   vector arrows;
-   bars;
-   extensible accents.

A visually centered accent is not always mathematically centered over
the glyph's typographic shape.

Where available, use a per-glyph top-accent attachment position. Where
unavailable, use a documented fallback.

### 14.1 Wide accents belong with extensible glyphs

`\widehat`, `\widetilde`, over-arrows, and similar constructs should
eventually share variant/assembly infrastructure with delimiters rather
than being implemented as arbitrary scaled ordinary glyphs.

------------------------------------------------------------------------

## 15. Matrices and arrays must be content-sensitive grids

The current matrix support is a strong structural improvement, but the
final architecture should avoid globally fixed row height.

A matrix should be measured in two dimensions.

For every cell, determine:

-   width;
-   height;
-   depth;
-   alignment behavior.

Then determine:

-   each column's required width;
-   each row's required height above its row baseline;
-   each row's required depth below its row baseline;
-   inter-column spacing;
-   inter-row spacing;
-   total matrix extent;
-   axis-relative vertical placement.

Only after the matrix box is known should delimiters be selected.

### 15.1 Why fixed rows fail

A row containing `x` and a row containing a display-like fraction do not
require the same vertical geometry.

Content-sensitive row metrics allow:

-   fractions inside matrices;
-   radicals inside matrices;
-   nested matrices;
-   scripts;
-   tall operators;
-   arrays and cases;
-   mixed row complexity.

### 15.2 Generalize from matrix to alignment grid

Long term, a common grid/alignment model can support:

-   `matrix`;
-   `pmatrix`;
-   `bmatrix`;
-   `Bmatrix`;
-   `vmatrix`;
-   `Vmatrix`;
-   `array`;
-   `cases`;
-   aligned mathematical structures.

The delimiter is then a wrapper around the measured grid, not part of
the grid algorithm itself.

------------------------------------------------------------------------

## 16. Separate mathematical semantics from glyph encoding

The current symbol layer uses existing PDF/font encodings. That is
practical, but the mathematical core should not conceptually identify a
mathematical symbol with an Adobe byte code.

The semantic layer should know:

> "This is mathematical alpha, a relation, a summation operator, a
> partial derivative symbol..."

The font layer should know:

> "In this selected font resource, that semantic symbol maps to this
> glyph/code and these metrics."

This separation will make it much easier to:

-   add additional fonts;
-   support Unicode-oriented input;
-   support modern OpenType mathematical fonts;
-   preserve semantic identity for accessibility;
-   change PDF font encoding without touching parser/layout semantics.

------------------------------------------------------------------------

## 17. Create a formal compatibility specification inside the project

Do not let the intended behavior live only in code comments or
screenshots.

Create a document such as:

`docs/math-compatibility.md`

It should state, at a conceptual level:

-   reference engine/profile;
-   supported mathematical constructs;
-   atom classes;
-   style-transition rules;
-   spacing behavior;
-   script placement policy;
-   fraction policy;
-   radical policy;
-   delimiter policy;
-   operator/limit policy;
-   accent policy;
-   matrix/grid policy;
-   font-metric sources;
-   fallback policy;
-   accepted tolerances;
-   known deviations.

This document becomes the contract for contributors.

A contributor should not need to infer whether RushTeX wants "something
attractive" or "pdfLaTeX-compatible behavior." The project should say
explicitly which is required.

------------------------------------------------------------------------

## 18. Build a reference mathematics corpus

The next major infrastructure investment should be a dedicated
mathematical compatibility corpus.

The current manual is useful as an integration demonstration, but it
should not be the primary mathematical regression suite.

Create many small, deliberately isolated specimens grouped by concept.

### 18.1 Core categories

The corpus should include at least:

-   ordinary variables and digits;
-   Greek letters;
-   unary/binary ambiguity;
-   binary operators;
-   relations;
-   punctuation;
-   named operators/functions;
-   opening/closing atoms;
-   explicit spaces;
-   superscripts;
-   subscripts;
-   simultaneous sub/superscripts;
-   nested scripts;
-   fractions;
-   nested fractions;
-   radicals;
-   indexed radicals;
-   sums/products;
-   integrals;
-   limits;
-   delimiters;
-   nested delimiters;
-   accents;
-   wide accents;
-   vectors;
-   matrices;
-   arrays;
-   cases;
-   mixed constructs;
-   inline math;
-   display math;
-   math embedded in tables;
-   math near line/page boundaries.

### 18.2 Combinatorial tests

After primitives are correct, combine them aggressively.

A mature math engine is defined less by whether `\frac{a}{b}` works than
by whether a fraction containing a sum containing scripted fractions
inside dynamically sized delimiters works without any special-case
intervention.

### 18.3 Pathological tests

Include deliberately extreme cases:

-   deeply nested fractions;
-   many script levels;
-   very tall matrices;
-   extremely wide accents;
-   delimiters around tall nested structures;
-   empty groups;
-   unusual combinations of atom classes;
-   operators with both scripts;
-   radicals containing matrices;
-   matrices containing radicals and fractions.

The goal is not only visual quality. Pathological cases expose
architectural coupling.

------------------------------------------------------------------------

## 19. Compare geometry, not only screenshots

Visual diffing is valuable but insufficient.

RushTeX should develop a **mathematical geometry regression
representation**.

For a formula, the test system should be able to record conceptual
information such as:

-   final formula width/height/depth;
-   each child box;
-   baseline positions;
-   script attachment coordinates;
-   fraction-rule coordinates and thickness;
-   radical-rule coordinates;
-   selected delimiter variant/assembly and extent;
-   operator and limit boxes;
-   accent position;
-   matrix row/column metrics;
-   effective atom classes;
-   inserted automatic spacing.

This provides a semantic/geometric explanation for every visible
difference.

### 19.1 Why this matters

A screenshot may tell you:

> "The superscript looks too high."

A geometry trace can tell you:

> "The superscript baseline is 0.38 pt higher than the reference because
> the minimum-bottom constraint is using the fallback font-size ratio
> instead of the profile metric."

That turns visual tuning into engineering.

------------------------------------------------------------------------

## 20. Use the reference engine as an oracle, not as an implementation template

RushTeX is an independent implementation. Preserve that.

The compatibility process should ask:

1.  What source was provided?
2.  What visible/geometric behavior does the reference produce?
3.  Which documented mathematical rule or metric explains it?
4.  How should RushTeX express that rule in its own architecture?
5.  Does the fix generalize across the corpus?

Avoid building a growing collection of "make this screenshot match"
exceptions.

A good change should usually improve an entire class of formulas.

### 20.1 Prefer documented behavior

Where behavior is documented in public specifications or
mathematical-font data, use that as the design basis.

Where exact legacy behavior must be established empirically, document
the experiment and the resulting compatibility rule.

This is especially important for long-term open-source maintainability.

------------------------------------------------------------------------

## 21. Establish tolerances and a compatibility scorecard

"Looks identical" should eventually become measurable.

Define tolerances separately for:

-   baseline positions;
-   glyph origins;
-   box dimensions;
-   rule thickness;
-   rule coordinates;
-   delimiter extent;
-   matrix column alignment;
-   matrix row baseline;
-   overall formula dimensions.

Some differences may be exactly reproducible; others may require a small
numeric tolerance due to representation or font/PDF differences.

### 21.1 Do not collapse everything into one score

Track categories independently:

-   parse success;
-   semantic equivalence;
-   geometry equivalence;
-   visual equivalence;
-   unsupported command count;
-   known font differences.

A single "97% compatible" number can hide serious failures.

------------------------------------------------------------------------

## 22. Add a mathematical debug/inspection mode

This is one of the highest-leverage development tools RushTeX can add.

A debug mode should be able to visualize or dump:

-   box boundaries;
-   baselines;
-   mathematical axis;
-   height/depth extents;
-   atom classes;
-   inserted spaces;
-   script attachment points;
-   fraction rule and clearances;
-   radical target extent;
-   delimiter target extent;
-   matrix row baselines and column boundaries;
-   accent attachment points.

The output can be a debug PDF, a textual geometry dump, or both.

The key is that contributors should be able to see **why** an expression
was positioned as it was.

This is particularly important in an open-source project. It lets a
contributor diagnose a layout problem without learning the entire
engine.

------------------------------------------------------------------------

## 23. Keep correctness ahead of micro-optimization during this phase

RushTeX's broader performance work is already unusually aggressive. The
math subsystem should now be allowed to become correct before it is made
maximally compact.

The order should be:

1.  establish the correct mathematical rule;
2.  validate it against the reference corpus;
3.  stabilize the behavior;
4.  profile;
5.  optimize representation and lookup;
6.  re-run the entire compatibility corpus.

Do not choose a weaker mathematical model merely because it fits a
smaller structure.

At the same time, do not abandon RushTeX's performance philosophy. The
eventual goal is:

> **Correct rules represented in cache-efficient data.**

not:

> **Approximate rules represented extremely efficiently.**

### 23.1 Protect performance with budgets

Once a compatibility milestone is correct, establish performance budgets
so future contributions cannot accidentally introduce pathological
allocation or repeated font parsing.

Correctness and performance should become independent regression
dimensions.

------------------------------------------------------------------------

## 24. Keep hot-path and cold-path data separate

The current project is strongly cache-conscious. Preserve that by
distinguishing data needed for nearly every atom from data needed only
by special constructs.

Hot information may include:

-   atom class;
-   style;
-   basic advance metrics;
-   common spacing parameters.

Cold/specialized information may include:

-   extensible-glyph assembly recipes;
-   rare accent variants;
-   unusual operator data;
-   detailed ink bounds;
-   large delimiter piece lists.

Do not force every ordinary `x + y` to load all the data needed for a
giant brace assembly.

This is how RushTeX can gain typographic sophistication without turning
the common path into a heavyweight font engine.

------------------------------------------------------------------------

## 25. Make fallback behavior explicit

Not every font will have perfect mathematical metadata.

The architecture should define a hierarchy:

1.  exact profile/font metric;
2.  compatible precomputed metric;
3.  documented generic fallback;
4.  diagnostic if acceptable fidelity cannot be guaranteed.

Fallbacks should be deterministic and centralized.

Avoid scattering expressions such as "0.2 × font size" throughout
unrelated layout branches. If a proportional fallback is necessary, it
should come from one named fallback profile so that it can be audited,
tuned, and eventually replaced.

------------------------------------------------------------------------

## 26. Treat math fonts as installable compatibility assets

This also fits RushTeX's future modular distribution model.

The core engine can remain small while mathematical font/profile packs
contain:

-   glyph mappings;
-   metrics;
-   mathematical constants;
-   variants;
-   assemblies;
-   accent data;
-   compatibility-specific tables.

Conceptually:

**RushTeX core + mathematical layout rules + selected math-font/profile
pack**

This is preferable to baking every future font and compatibility mode
into one monolithic executable.

It also works well for WASM: the browser can load the mathematical pack
only when required and cache it.

------------------------------------------------------------------------

## 27. Preserve a clean boundary between legacy compatibility and future RushTeX extensions

RushTeX may eventually introduce cleaner native mathematical syntax or
new features. Do not let those ambitions destabilize the compatibility
engine.

Both legacy LaTeX syntax and future RushTeX-native syntax should ideally
lower into the same mathematical intermediate representation.

Conceptually:

**LaTeX math frontend → Math IR ← future RushTeX math frontend**

Then:

**Math IR → compatibility/style/font layout → resolved geometry → PDF**

This allows RushTeX to innovate at the syntax level while maintaining
one high-quality mathematical compositor.

------------------------------------------------------------------------

## 28. Accessibility and semantic preservation should be planned now

Visual equivalence is the immediate goal, but the semantic AST gives
RushTeX an opportunity legacy PDF workflows often struggle with.

Do not discard mathematical semantic identity too early.

Preserve enough information that future output layers can support:

-   searchable/copyable symbols;
-   Unicode mappings;
-   tagged PDF;
-   alternative mathematical representations;
-   accessibility metadata;
-   MathML export or interoperability.

This does not need to block geometric compatibility work. It simply
means the architecture should avoid reducing mathematics to anonymous
glyph bytes before semantic information has served its purpose.

------------------------------------------------------------------------

## 29. Open-source contribution boundaries for math

Because RushTeX is intended to become an open-source project, organize
mathematical work by stable responsibilities.

Potential ownership areas include:

-   math syntax/parser compatibility;
-   atom classification and spacing;
-   style propagation;
-   math-font metrics;
-   scripts;
-   fractions;
-   radicals;
-   extensible glyphs;
-   large operators;
-   accents;
-   matrices/alignment;
-   reference corpus;
-   geometry comparison tools;
-   performance profiling.

Every contribution that changes geometry should ideally provide:

-   a minimal reference case;
-   a regression case;
-   evidence of the relevant general rule;
-   results across affected corpus categories;
-   performance impact where meaningful.

The project should discourage unexplained visual constants and
formula-specific patches.

------------------------------------------------------------------------

## 30. Recommended architectural milestones

### Milestone A --- Freeze the reference profile

Document exactly which pdfLaTeX/TeX Live/font configuration RushTeX is
initially trying to match.

**Exit criterion:** every comparison has an unambiguous reference.

### Milestone B --- Authoritative mathematical metrics

Centralize global math constants and per-glyph mathematical information.
Move font-specific outline assumptions out of generic layout.

**Exit criterion:** ordinary geometry no longer depends primarily on
scattered font-size proportions.

### Milestone C --- Exact style and atom behavior

Complete style transitions, cramped behavior, atom
normalization/reclassification, and automatic spacing rules.

**Exit criterion:** simple formulas match the reference structurally and
in horizontal rhythm.

### Milestone D --- Scripts and fractions

Make script placement and fraction construction constraint-driven and
reference-calibrated.

**Exit criterion:** large script/fraction corpus matches within defined
geometric tolerances.

### Milestone E --- Extensible constructions

Build general variants/assembly machinery and migrate delimiters and
radicals to it.

**Exit criterion:** deeply nested formulas choose correct-sized
constructions without construct-specific scaling hacks.

### Milestone F --- Operators and accents

Complete display operators, integral behavior, limits, italic
correction, accent attachment, and wide accents.

**Exit criterion:** calculus/physics corpus has reference-quality
geometry.

### Milestone G --- Content-sensitive grids

Upgrade matrices/arrays to per-row height/depth and per-column
measurement, then wrap them with general delimiters.

**Exit criterion:** mixed-complexity matrices remain stable and
reference-compatible.

### Milestone H --- Automated geometric differential testing

Produce reference and RushTeX outputs plus machine-readable geometry
comparisons.

**Exit criterion:** a geometry regression is detected automatically
before merge.

### Milestone I --- Compatibility expansion

Only after the geometry model is stable, expand aggressively into
additional AMS/LaTeX constructs.

**Exit criterion:** new syntax mostly lowers into existing compositional
primitives rather than requiring new layout hacks.

### Milestone J --- Performance consolidation

Profile the now-correct engine, pack data, separate hot/cold paths,
eliminate avoidable allocation, and restore/extend performance budgets.

**Exit criterion:** mathematical fidelity is preserved while the
implementation returns to RushTeX-level performance expectations.

------------------------------------------------------------------------

## 31. What RushTeX should avoid

### Avoid equation-specific nudges

Never solve "this particular integral looks wrong" by adding a special
coordinate adjustment unless the reference behavior itself genuinely
defines a special operator rule.

### Avoid scattered magic ratios

Temporary proportional constants are acceptable during development, but
final behavior should come from named font/profile parameters or
documented fallback policy.

### Avoid parser/layout coupling

Syntax recognition should not decide geometric placement.

### Avoid PDF/layout coupling

PDF emission should not decide mathematical semantics or geometry.

### Avoid assuming one font's outlines are universal

Adobe Symbol geometry, Computer Modern geometry, and modern OpenType
math geometry are not interchangeable.

### Avoid fixed matrix row geometry

Rows should respond to their contents.

### Avoid scaling delimiters indefinitely

Use designed variants and assemblies when the target profile/font
supports them.

### Avoid optimizing before the compatibility rule is known

A fast approximation can become expensive technical debt.

### Avoid measuring success only with the manual

The manual is an integration showcase, not a comprehensive compatibility
corpus.

### Avoid claiming universal "LaTeX-identical" output too early

Tie compatibility claims to the documented profile and tested corpus.

------------------------------------------------------------------------

## 32. A useful mental model for the entire subsystem

The mature RushTeX mathematical pipeline should conceptually look like
this:

``` text
LaTeX mathematical source
          |
          v
Syntax parsing
          |
          v
Semantic Math IR / AST
          |
          v
Atom normalization + semantic classification
          |
          v
Math style propagation
          |
          +-----------------------------+
          |                             |
          v                             v
Compatibility rules              Math-font/profile data
          |                      - constants
          |                      - glyph metrics
          |                      - italic corrections
          |                      - accent attachments
          |                      - variants/assemblies
          |                             |
          +--------------+--------------+
                         |
                         v
                 Mathematical layout
                  /   /   |   \   \
             scripts frac rad  ops grid ...
                         |
                         v
              Resolved mathematical boxes
               + positioned glyph/rule ops
                         |
                         v
                 Document/page layout
                         |
                         v
                    PDF backend
```

The most important boundary is between **resolved mathematical
geometry** and the PDF backend.

By the time a formula reaches the backend, the backend should not need
to know what a fraction, superscript, matrix, or radical is.

------------------------------------------------------------------------

## 33. The core design principles

If the team remembers only a short set of rules, use these:

1.  **Match rules, not screenshots.**
2.  **Define one exact reference profile before claiming identity.**
3.  **Preserve semantic mathematics until layout is complete.**
4.  **Make fonts provide metrics; do not guess metrics globally.**
5.  **Make every mathematical structure return composable box
    geometry.**
6.  **Treat style as layout state, not merely scale.**
7.  **Normalize atom classes before spacing.**
8.  **Place scripts by constraints and glyph metrics.**
9.  **Build fractions around the mathematical axis and authoritative
    clearances.**
10. **Unify scalable symbols under one variants/assembly system.**
11. **Make matrices content-sensitive two-dimensional layouts.**
12. **Keep font-specific knowledge out of generic layout algorithms.**
13. **Use pdfLaTeX as a compatibility oracle, not as RushTeX's internal
    architecture.**
14. **Compare machine-readable geometry in addition to rendered
    pixels.**
15. **Make fallbacks explicit and centralized.**
16. **Correctness first; optimize the proven model afterward.**
17. **Keep hot common data separate from cold specialized math data.**
18. **Require every visual fix to explain the general rule it
    implements.**
19. **Test combinations and pathological nesting, not only isolated
    primitives.**
20. **Make compatibility measurable and reproducible.**

------------------------------------------------------------------------

## 34. Final direction for the current RushTeX codebase

The present RushTeX math subsystem should be regarded as a **successful
structural prototype entering its precision phase**.

It already has several important foundations:

-   a semantic AST rather than direct PDF emission from syntax;
-   all eight TeX-style math states, including cramped variants;
-   explicit atom classes;
-   a compact spacing table;
-   width/height/depth boxes;
-   structural nodes for scripts, fractions, radicals, big operators,
    delimiters, matrices, and accents;
-   separate measurement/rendering entry points;
-   unit/integration tests;
-   a PDF output path that can already demonstrate substantial
    mathematical structure.

The primary architectural weakness is that many final placements still
depend on empirical font-size proportions and font-specific assumptions
embedded in generic layout. That is why the latest output is
structurally much better but still requires refinement before it can be
considered reference-equivalent.

Therefore:

> **Do not replace the current architecture. Deepen it.**

The next version of RushTeX math should transform the existing layout
layer from a collection of good geometric approximations into a
**parameterized mathematical compositor** whose behavior is driven by a
documented TeX-compatibility profile and authoritative mathematical-font
data.

Once that transformation is complete, the project should be able to
improve mathematical fidelity systematically:

-   one script rule fixes thousands of powers and indices;
-   one italic-correction model fixes many slanted bases and integrals;
-   one fraction model fixes arbitrary nested fractions;
-   one extensible-glyph engine fixes delimiters, radicals, and wide
    constructions;
-   one content-sensitive grid model fixes the entire matrix/array
    family;
-   one reference corpus prevents regressions across all of them.

That is the path toward the characteristic beauty of mature TeX
mathematical composition without giving up what makes RushTeX distinct:
a clean independent implementation, modern Rust architecture,
deterministic in-memory processing, modularity, compact data, and
performance-conscious execution.

------------------------------------------------------------------------

## 35. Definition of success

The math subsystem should be considered mature for the initial
compatibility profile when all of the following are true:

-   supported LaTeX mathematical source parses without source changes;
-   semantic interpretation agrees with the reference;
-   style transitions agree with the reference;
-   atom classification and spacing agree with the reference;
-   glyph metrics come from authoritative font/profile data;
-   scripts satisfy reference-compatible placement rules;
-   fractions agree in rule position, thickness, clearance, and child
    styles;
-   radicals use correct clearance and extensible construction;
-   delimiters select or assemble correct sizes;
-   large operators and limits follow reference-compatible policies;
-   accents use correct attachment/extension behavior;
-   matrices and arrays derive geometry from actual cell contents;
-   deeply nested combinations remain compositional;
-   geometry comparisons pass documented tolerances;
-   rendered comparisons show no systematic visual discrepancy;
-   performance remains within the project's stated budgets;
-   no compatibility fix depends on formula-specific coordinates.

At that point, RushTeX will not merely be "drawing math that looks
similar to LaTeX." It will have a coherent mathematical typesetting
architecture capable of **producing LaTeX-compatible geometry by
construction**.

------------------------------------------------------------------------

### Project note

This document intentionally describes **architectural direction rather
than Rust implementation details**. Concrete data structures, packing
strategies, lookup representations, and algorithms should remain free to
evolve as long as they preserve the contracts above. This gives RushTeX
room to pursue its cache-local, compact, native/WASM-friendly
implementation while making mathematical fidelity a first-class
invariant.
