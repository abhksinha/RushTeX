Modern fonts are essentially **structured binary databases**. A `.ttf` or `.otf` file contains multiple tables, each storing a different kind of information.

For example, suppose a font contains the glyph \(A\). Conceptually, the font contains something like:

```text
FONT FILE
│
├── Character mapping
│     Unicode U+0041 → glyph #37
│
├── Glyph #37
│     ├── contour 1
│     │     point (120, 0)
│     │     point (500, 700)
│     │     ...
│     └── contour 2
│           ...
│
├── Metrics
│     advance width = 650
│     left bearing = 20
│
├── Kerning/layout
│     A followed by V → adjust spacing
│
└── Other metadata
      font name, copyright, etc.
```

The actual file is binary rather than human-readable text.

For a TrueType font, several important tables are:

* `cmap` — maps Unicode characters to glyph IDs.
* `glyf` — stores glyph outlines.
* `loca` — tells where each glyph's data occurs in `glyf`.
* `hmtx` — horizontal widths and side bearings.
* `head` — global font information.
* `maxp` — number of glyphs and related limits.
* `name` — font name, family, designer, etc.
* `GSUB` — glyph substitutions, such as ligatures.
* `GPOS` — precise glyph positioning.
* `MATH` — additional information needed for mathematical typography.

For example, the `cmap` might effectively say:

```text
U+0041  → glyph 37       A
U+0042  → glyph 38       B
U+222B  → glyph 1260     ∫
U+2211  → glyph 1302     ∑
```

Then glyph 1260 contains the actual geometry of \(\int\).

The geometry itself is stored in **font coordinate units**, not pixels. A font might define, for example,

$$
1000\text{ units}=1\text{ em}.
$$

A glyph might contain points approximately like:

```text
Point 0: (250, -200)
Point 1: (180,  100)
Point 2: (300,  500)
Point 3: (400,  750)
...
```

Some points are on the contour; others are Bézier control points. Connecting these according to the TrueType rules produces the outline.

Then the renderer scales those coordinates according to the requested size. Therefore the same stored glyph can become

$$
\small \int
\qquad
\large \int
\qquad
\Huge \int
$$

without storing three bitmap images.

Math fonts add another particularly interesting layer. The `MATH` table can tell the engine things such as superscript positioning, fraction-bar thickness, radical positioning, italic correction, accent attachment, and which larger glyphs should be used for extensible symbols.

So for

$$
\left(\frac{x^2}{y}\right)
$$

the font contains not merely pictures of \(x,2,y,(,)\). It also provides mathematical typography information that helps the engine construct the complete expression correctly.

In short:

**A modern font is a binary collection of tables containing character→glyph mappings, vector outlines, dimensions, positioning rules, substitutions, and— for math fonts—special mathematical layout information.**

The next useful level would be to take an **actual `.ttf` file and decode one glyph byte-by-byte**, showing how the coordinates and Bézier control points for something like `A` or \(\int\) are physically represented in the file.

