# Parity precision and Phase 3 TODO

Tracking list for the follow-up after the category-wise gap sweep. Status: `[ ]` open,
`[~]` in progress, `[x]` done, `[-]` deferred (reason given).

## 1. Font metric precision

TFM values were stored rounded to 1/1000 em, which cost up to 0.005pt per value at
10pt. `\alpha` (w + ic) came out 6.440pt against TeX's 6.434pt; long formulas
accumulated 0.02–0.04pt.

- [x] Regenerate every glyph table (`math_tfm.rs`, `math_tfm_faces.rs`) from the real TFM
      fix_words. Storage stays 8 bytes per glyph (`PackedGlyphMetric`, 1/16384 em);
      lookups unpack to `GlyphMetric` in 1/1000 em as `f32`.
- [x] Audit consumers that narrowed metrics to integers (`italic_correction`,
      `char_height_depth`, table cell extents now return `f32`; PDF `/Widths` round).
- [x] Delimiter fitting and extensible repeat counts tolerate float noise
      (`DELIM_FIT_EPS`), as TeX compares exact scaled points.
- [-] CM `MathFontParameters` (sigma/xi) stay in 1/1000 em: the 64-byte struct is shared
      by all families and the residual is under 0.005pt vertically.
- [x] Math comparison: 478/479 within 0.02pt (was 423/438). The one miss is display
      `\pmod`, an artifact of measuring a display formula in an `\hbox`.

Found and fixed on the way:

- [x] `\text{…}` interword space uses the text font's fontdimen 2 at the script size
      (`MathSkip::TextSpace`) instead of 6mu.
- [x] `\ldots`/`\cdots` dot widths from the TFM tables.
- [x] `\above <dim>` and `\genfrac` thickness (Rule 15d clearances follow the thickness).
- [x] Generalized-fraction delimiters are sized to σ20/σ21 exactly (Rule 15e).
- [x] `subarray`/`\substack` `\baselineskip` from cmsy7 fontdimens 10 + 12.
- [x] Radicals: the overbar is as thick as the surd is tall and the clearance split uses
      the surd's depth (§737); fixes script-script roots.
- [x] `\lvert`, `\rvert`, `\lVert`, `\rVert` (also after `\left`/`\right`, `\bigl`…).

## 2. CM interword space

- [x] Computer Modern body text uses fontdimen 2: cmr10 333, cmbx10 383, cmti10 358
      (was 278 for every face).
- [x] Tabular cells drop leading/trailing spaces (`\ignorespaces`…`\unskip`); with the
      wider space they had pushed `\multicolumn` spans past their columns.
- [x] `\multicolumn` excess widens only the last spanned column, and the span counts the
      `\tabcolsep` padding between its columns (TeX §801).

## 3. Parity harness preamble

- [x] `scripts/differential_parity_harness.py` already loads `cancel` and `accents`
      (`\utilde`); both are now wrapped in `\IfFileExists` so a basic TeX Live without
      them no longer fails all 26 math specimens.
- [ ] Locally: `tlmgr install cancel accents` so `rule18_bottom_and_overlay` gets a true
      reference image.

## 4. Roadmap Phase 3

- [x] `center`, `flushleft`, `flushright`, `\centering`, `\raggedright`, `\raggedleft`:
      greedy natural-width breaking for ragged lines, `\topsep`/`\partopsep` merged like
      `\addvspace`, no indent after an environment closed without a blank line
      (`\@endpe`), and `\par` now ends the paragraph inside groups.
- [x] Font sizes `\tiny` … `\Huge` from `size10/11/12.clo`: group-scoped inline runs, a
      top-level declaration persists, the paragraph uses the `\baselineskip` in force at
      its `\par` (also `{\Large …\par}`), and a line with larger text falls back to
      `\lineskip`.
- [x] Design sizes: `OpticalMaster` selects the Computer Modern master by size with the
      ranges of `ot1lmr.fd` (equal to `ot1cmr.fd` at LaTeX's sizes): cmr5–9/12/17,
      cmbx5–9/12, cmti7–9/12, cmtt8/9/12. The face stays the same; widths, fontdimens,
      lig/kern programs, hyphenation pieces and the PDF font (`/FOn`) all resolve from
      (face, size). Math glyph runs keep their TFM fonts. Word positions at 8, 9, 12, 14.4
      and 17.28pt now match pdfLaTeX within 0.05pt.
- [x] Line breaking is TeX's `line_break` (`layout_linebreak.rs`, tex.web §813–§890) in
      scaled points: badness, fitness classes, demerits, the three passes, looseness,
      discretionaries and pattern hyphenation in pass 2. Word widths are the TFM fix_words
      through `store_scaled`; interword glue is fontdimens 2/3/4/7 of each word's font and
      size with the space factor (§1044). Lines are set with TeX's glue ratio. The hz
      breaker runs only when microtype is requested.
- [x] `\textsc` / `\scshape`: new `Face::SmallCaps` (cmcsc10 program, TFM metrics,
      lig/kern program, fontdimens 2 and 7). Base-14 families keep their upright face.
- [x] Colour: `\textcolor`, `\color` (ends in `\ignorespaces`), `\definecolor` (rgb, RGB,
      HTML, gray, cmyk), the xcolor base colours and `a!p!b` mixes. Sparse colour runs per
      paragraph; `Paint::Color` emits `rg`/`RG`; every line ends black.
- [x] Colour in table cells (reset per cell), headings/captions (flattened runs) and math
      (`MathNode::Colored` for `\color`/`\textcolor`, `\definecolor` names passed to the
      math parser; text after a coloured formula returns to the line's colour).
- [x] Cell spaces between items are kept (`\textcolor{red}{R} cell` lost its space).

Found and fixed on the way:

- [x] `Face::is_mono`/`is_bold`/`is_italic` aliased the bit masks onto faces past the six
      body faces (face 31 read as monospace).
- [x] A paragraph of only declarations before a blank line (`\noindent`, `\color{..}`)
      ended the document parse early ("unclosed \begin{document}").
- [x] A table caption that broke the page was placed from the old page's float top.

## 5. TeX-exact spacing (follow-up)

- [x] Breaks and positions: a mixed paragraph (roman, italic, `\small`, bold, `\texttt`,
      hyphenation, sentence spacing) has the same 19 lines as pdfLaTeX and every word within
      0.009pt horizontally (was 9 break differences, 1.18pt).
- [x] Italic correction after `\textit`/`\emph` unless `.` or `,` follows (`\check@icr`).
- [x] `Td` offsets are taken between rounded positions; the 0.01pt rounding drifted about
      0.01pt per line.
- [x] Hyphenation equals `\showhyphens` on all 4244 distinct words of the docs; the 14
      `\hyphenation` exceptions of `hyphen.tex` are applied.
- [x] Table cells and multi-line captions go through the TeX breaker with `\sloppy`
      (`\@arrayparboxrestore`), justified with TeX's glue setting. `\cline` takes no height
      (`\vskip-\arrayrulewidth`). Vertical rules are centred on the column boundary, as
      pdfTeX draws them. `table_paragraph_col` 84.9% to 99.6%, `table_cline` 91.3% to 99.8%.
- [x] Column widths are measured with the same exact widths the cells are set with.
- [x] A design-size master no longer embeds the unused 10pt face.
- [x] ToUnicode: OT1 quotes (`` `` `` and `''`), `¡`, `¿` and accents corrected, a separate
      typewriter map for cmtt (`{ } \ " < > _`), ligatures extract as letters.
- [ ] FontDescriptor ascent/descent of the masters (cmr9, cmtt10) differ from pdfTeX's, so
      text bounding boxes in extraction tools are off by up to 0.5pt; glyphs are not.
- [ ] Words with an explicit hyphen (`fixed-width`) are not pattern-hyphenated before
      the hyphen as TeX does for the first part.
- [ ] Section headings break at a fixed width after the number instead of `\@hangfrom`.

## 6. Unit system (found during item 2)

- [x] Layout coordinates are PDF big points while font sizes are TeX points: a 10pt font
      is drawn 10bp tall (TeX: 9.963bp), so all text is 0.375% too large against
      dimensions given in `cm`/`in`. Visible as the p{5cm} cell in `table_paragraph_col`
      wrapping "Second row with normal content" (TeX: 141.999pt in a 142.26pt column)
      and as +0.4% widths in the pagination specimens. Fix: TeX points throughout
      layout, paper sizes in pt, one `0.99626 0 0 0.99626 0 0 cm` per page and MediaBox
      in bp in the backend.
      Done: `parse_dimension_pt` uses TeX units (in, cm, mm, bp, pc, dd, cc, sp), paper
      sizes are in pt (`A4_PT`, `LETTER_PT`), and each page stream starts with
      `0.996264 0 0 0.996264 0 0 cm`. `table_paragraph_col` 27% to 85%, longtable p1 70% to
      98%, `multipage_breaks` 80% to 88–90%.

## 7. Verification and benchmark

- [x] `cargo test` (218 pass; the 4 failures need the NewCM OpenType fonts, as before),
      `cargo clippy --all-targets -- -D warnings` clean, parity harness 38/38 A,
      math 478/479 within 0.02pt.
- [x] Benchmark before/after, rushtex vs pdfLaTeX (`benchmark.sh` now runs on macOS).
- [x] `table_booktabs` 63.7% to 99.8%: booktabs.sty rule widths (.08em/.05em/.03em) and
      spacing (`\aboverulesep`, `\belowrulesep`, `\doublerulesep`, `\cmidrulekern`,
      the `\@lastruleclass` rules for adjacent rules and `\cmidrule`s).
- [x] Page number centred on the text block (`\hfil\thepage\hfil`), not the page: it was
      8pt left of pdfLaTeX.

## 8. Vertical list, floats and pages (math survey)

- [x] `\prevdepth` interline glue (§679) for paragraph lines, headings and displays; line
      height and depth come from the TFM metrics.
- [x] Display skips (§1199–1206), with the number on its own line when it does not fit
      (§1205); align/gather rows are lines with `\openup\jot`, numbered per row, `\notag`.
- [x] Float placement `h`/`t`/deferred/float page (`layout_float.rs`); `[placement]` parsed.
- [x] TeX's page builder (`layout_pagebuilder.rs`): breakpoint costs with club, widow,
      broken, `\@secpenalty`, `\postdisplaypenalty`, `\maxdepth`; glue shrinks on overfull
      pages; TOC page numbers are corrected. All 10 page breaks of the survey match.
- [x] `\maketitle` and the article TOC per article.cls.
- [x] Matrix/cases delimiters follow Rule 19 (one repeat too tall before).
- [x] `\TeX`, `\LaTeX`, `\ `, `~` as glue, text accents through `\accent`, `\usepackage{a,b}`.
- [x] No NewCMMath fallback with Computer Modern.
- [ ] Shrink of `\floatsep`/`\textfloatsep`; display glue that `\addvspace` removes before a
      heading still counts as shrink.
- [ ] `\flushbottom` (page stretch) for twoside classes.
