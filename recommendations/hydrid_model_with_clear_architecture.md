I inspected the math files, including the OpenType parser, math layout, delimiter/variant code, calligraphy tables, vector paths, and legacy math-font mappings.

Your implementation is further along than the description in `modern_font.md` suggests, but it is currently a **hybrid OpenType/legacy system**, not yet a genuinely font-driven OpenType math pipeline.

The main findings are:

| Area                              | Current RushTeX state                         | Main deficiency                                       |
| --------------------------------- | --------------------------------------------- | ----------------------------------------------------- |
| OpenType container parsing        | Implemented                                   | Good base                                             |
| `MATH` constants                  | Dynamically parsed                            | Already substantial                                   |
| MathGlyphInfo                     | Partially parsed                              | Needs fuller coverage                                 |
| MathVariants                      | Partially dynamically parsed                  | Assembly side incomplete                              |
| Unicode `cmap`                    | **Not actually implemented in uploaded code** | Major gap                                             |
| `hmtx`                            | **Not actually decoded**                      | Major gap                                             |
| `glyf` outlines                   | **Not dynamically decoded**                   | Major gap                                             |
| CFF CharStrings                   | **Not dynamically decoded**                   | Major gap                                             |
| Calligraphic glyphs               | Hardcoded GIDs/metrics                        | Font-specific                                         |
| Vector glyphs                     | Precomputed PDF paths                         | Font-specific                                         |
| Delimiter data                    | Largely pre-extracted/hardcoded               | Not truly font-driven                                 |
| PDF OpenType embedding/subsetting | Not present in uploaded source                | Major gap, though PDF backend may be outside this ZIP |

The most important discrepancy is in `fonts/opentype.rs`. Its header says:

> parser for OpenType font tables (`MATH`, `CFF `, `cmap`, `head`, `hmtx`)

But the implementation I inspected does **not** actually contain a Unicode `cmap` resolver, an `hmtx` metric reader, a `glyf` decoder, or a CFF CharString interpreter.

What it really implements is primarily:

```text
SFNT/OpenType table directory
        │
        ├── MATH
        │    ├── MathConstants       ✓
        │    ├── MathGlyphInfo       partial ✓
        │    └── MathVariants        partial ✓
        │
        └── generic find_table()     ✓
```

So the comment currently overstates the parser's capabilities.

Your biggest architectural gap is therefore this missing operation:

```text
Unicode scalar
     │
     │ cmap
     ▼
 Glyph ID
     │
 ┌───┴───────────────┐
 ▼                   ▼
hmtx                MATH
advance          optical/layout
metrics             data
```

At present, RushTeX frequently knows the glyph ID **because you told it beforehand**, rather than because the loaded font resolved it.

For example, `calligraphy.rs` contains entries essentially of the form:

```text
Unicode U+1D4AC
glyph_id = 1256
width = 697
height = 669
...
```

That works for the particular font from which those values were extracted. But if I give RushTeX another conforming OpenType math font, glyph 1256 may mean something completely different.

The correct architecture should instead perform:

```text
U+1D4AC
    ↓
font.cmap_lookup(0x1D4AC)
    ↓
GID 1256       // only if THIS font says so
    ↓
font.h_advance(1256)
font.math_italic_correction(1256)
font.math_top_accent(1256)
```

That is probably the single most important modernization step.

There is a similar issue in `vector_paths.rs`. You have precomputed New Computer Modern paths and then dispatch on fixed GIDs:

```text
1120..1145 → script paths
1240..1265 → script paths
832        → integral
806        → summation
804        → product
...
```

This gives you modern-looking vector output, but it isn't yet general OpenType rendering.

The architecture is effectively:

```text
GID 832
  ↓
RushTeX hardcoded lookup
  ↓
pre-generated PDF Bézier commands
```

where a general font implementation would be:

```text
Unicode ∫
   ↓
cmap
   ↓
GID
   ↓
glyf/CFF
   ↓
font's actual outline
```

There is another important issue in `delimiters.rs`. You have tables such as:

```text
NEWCM_PAREN_VARIANTS
NEWCM_BRACKET_VARIANTS
NEWCM_BRACE_VARIANTS
NEWCM_RADICAL_VARIANTS
```

and manually defined assembly recipes.

That means RushTeX knows NewCM's answers, rather than asking the currently loaded font for them.

Your `opentype.rs` is already beginning to solve this correctly with `vert_variant_count()` and `vert_variant()`. I would continue that approach and eventually make the static `NEWCM_*_VARIANTS` tables unnecessary for normal operation.

Also be careful about this conceptual distinction in the assembly definitions:

```text
top_glyph: 0x239B
```

`0x239B` looks like a **Unicode code point**. An OpenType MathGlyphAssembly uses **glyph IDs**, not Unicode scalar values. Unicode U+239B and GID `0x239B` are entirely different namespaces. Once `cmap` is implemented, this distinction should be enforced strongly in the type system.

For example, I would avoid representing both as plain `u16`/`u32` everywhere. Conceptually use:

```text
UnicodeScalar(u32)
GlyphId(u16)
```

so accidentally treating U+239B as glyph 9115 becomes difficult.

Your layout layer also remains partly legacy-driven. `math_layout.rs` still emits ordinary text through things like `Face::Roman`, `Face::Symbol`, etc., while only selected cases go through `Paint::VectorGlyph`.

So you currently have two pipelines:

```text
                 MathNode
                    │
          ┌─────────┴─────────┐
          ▼                   ▼
    legacy Face/text     selected modern glyph
          │                   │
     GlyphRun            VectorGlyph
```

The long-term architecture should converge toward:

```text
MathNode
   ↓
Unicode/math semantic resolution
   ↓
selected OpenType math font
   ↓
GlyphId
   ↓
metrics + MATH
   ↓
positioned glyph
   ↓
PDF embedded-font glyph
```

There is one thing I would **not** prioritize first: writing a complete Bézier/CharString rendering engine.

The more important order is:

**First:** implement robust `cmap` Format 4 and Format 12 lookup. Format 12 matters especially for mathematical alphanumeric characters above U+FFFF, such as U+1D400–U+1D7FF.

**Second:** implement `head`/`maxp`/`hhea`/`hmtx` metric access, including `unitsPerEm`, glyph count, number of horizontal metrics, advance width and LSB.

**Third:** replace hardcoded Unicode→GID and width tables with those dynamic queries.

**Fourth:** complete dynamic `MATH` variants and `MathGlyphAssembly`, including part records, connector lengths, extender flags and `MinConnectorOverlap`.

**Fifth:** implement the PDF font embedding/subsetting layer. I cannot conclusively audit that portion because the uploaded ZIP contains the `math` source tree, not the complete PDF backend.

**Sixth:** add `glyf`/`loca` parsing for TrueType and CFF/CFF2 support where required by your subsetter or vector-outline fallback.

So the central issue is no longer that RushTeX "doesn't understand OpenType MATH." It clearly does to a meaningful degree.

The more precise diagnosis is:

$$
\boxed{\text{RushTeX understands parts of OpenType MATH, but does not yet treat the loaded font itself as the authoritative glyph database.}}
$$

That is why you still need NewCM-specific constants, hardcoded glyph IDs, pre-extracted paths and legacy TeX/Adobe mappings.

Once **`Unicode → cmap → GID → hmtx/MATH → positioned GID → subsetted embedded font`** becomes the primary path, most of those hardcoded compatibility layers can become fallbacks rather than the foundation.

For RushTeX, I would choose a **hybrid architecture internally, with OpenType as the primary path**.

Do not make the two systems equal. Architect it as:

```text
                    Math input
                        │
                        ▼
                 Semantic layout
                        │
                Glyph resolution
                        │
             ┌──────────┴──────────┐
             ▼                     ▼
       OpenType path          Legacy fallback
        [PRIMARY]              [COMPATIBILITY]
             │                     │
     cmap → GID              OT1/OML/OMS/OMX
     hmtx metrics            legacy metrics
     MATH tables             hardcoded tables
     variants/assembly       CM/TeX assembly
             │                     │
             └──────────┬──────────┘
                        ▼
                 Positioned Glyphs
                        │
                        ▼
                  PDF backend
```

The reason is practical. OpenType-only would give you a cleaner architecture, Unicode-native math, interchangeable modern math fonts, proper `MATH` variants/assemblies, and far fewer font-specific hacks. But immediately deleting the existing legacy machinery would throw away working code and make regression testing much harder.

The key design decision is therefore: **new features must not depend on the legacy path**.

For example, your desired API should conceptually become:

```text
font.glyph_for_unicode(U+222B)
        ↓
      GID 832

font.advance_width(832)
font.math_italic_correction(832)
font.vertical_variants(832)
font.glyph_assembly(832)
```

The layout engine should not care whether that information ultimately came from OpenType or a compatibility provider.

I would introduce a font abstraction roughly like:

```text
MathFont
 ├── glyph_for_unicode()
 ├── glyph_metrics()
 ├── math_constants()
 ├── italic_correction()
 ├── top_accent_attachment()
 ├── vertical_variants()
 ├── horizontal_variants()
 └── glyph_assembly()
```

Then implement:

```text
OpenTypeMathFont
       │
       ├── cmap
       ├── hmtx
       ├── MATH
       └── glyf/CFF

LegacyTexMathFont
       │
       ├── OT1/OML/OMS/OMX
       ├── TFM-like metrics
       └── existing hardcoded assembly
```

That prevents code like this from spreading throughout the layout engine:

```text
if modern_font {
    ...
} else if cm_font {
    ...
} else if symbol_font {
    ...
}
```

Instead, layout asks the font abstraction for a glyph and doesn't need to know how it was obtained.

Eventually, your architecture can naturally evolve to:

```text
                    MathFont
                       │
          ┌────────────┴────────────┐
          ▼                         ▼
   OpenTypeMathFont          LegacyTexMathFont
      95% usage                  fallback
```

and perhaps years later:

```text
                    MathFont
                       │
                       ▼
                OpenTypeMathFont

Legacy support → optional compatibility module
```

One additional recommendation from the source you showed: **stop expanding `vector_paths.rs`, `calligraphy.rs`, and NewCM-specific GID tables now.** They are useful as transitional infrastructure, but every additional hardcoded NewCM glyph increases migration work.

Your immediate milestone should instead be:

$$
\boxed{\texttt{Unicode} \rightarrow \texttt{cmap} \rightarrow
\texttt{GID} \rightarrow \texttt{hmtx/MATH}}
$$

Once that works, test the exact same equation with New Computer Modern Math, STIX Two Math, and another OpenType math font. If changing the font file changes the typography correctly **without changing RushTeX source code**, you have crossed the important architectural boundary.

So: **hybrid for compatibility; OpenType-first by design.** Do not build a permanent 50/50 hybrid.
