# testmath: text logos lose italic correction, `$..$` inside `\text`
Author: abhi+claude | Date: 2026-10-19
Files: `src/frontend/frontend_parser.rs`, `src/math/math_parser.rs`, `src/math/math_ast.rs`, `src/math/math_layout.rs`

## Context
Resumed from `memorycastle/journal/2026-10-19-testmath-compiles.md`, open item 1 (`\AmS` hyphen 0.75pt late), then the next page-1 drift.

## Decision
- `tex_logo` wraps every logo in `text_hbox` (an unshifted `MathNode::Raise`). The logos are text: an hbox adds no italic correction, so cmsy `S` (.075em) no longer widens `\AmS`. Same for `\textit{\TeX}`'s X.
- New `MathNode::Formula`: `$...$` inside `\text{...}` (`parse_text_arg` splits on unescaped `$`, helpers `text_words`, `unescaped_dollar`). Layout: text style (script styles kept, uncramped), `face_override` cleared, trailing italic correction inside the box, Ord atom. Before, `$G$` was printed literally.

## Rejected
- Changing the CMEX ToUnicode for wide accents. `miss.py` reports `\widehat` glyphs as unmatched because the ToUnicode maps them to combining U+0302, which PyMuPDF snaps to the previous glyph. Rendering is identical to pdfLaTeX (checked at 300dpi), so it is a measurement artefact, not a defect.

## Measured
- `testmath` 5.2% → 5.4% of glyphs within 0.1pt. Page 1: 602 → 7 unmatched (all 7 are the wide-accent artefact). Page 2: 471/1590 unmatched, of which the missing running head is the first line; the body already matches.
- `sample2e`, `small2e`, `lppl`: 100%. Tests 234 pass plus the 4 known NewCM failures; clippy clean.

## Open
- **Running heads** (`\pagestyle{headings}`, `\markboth`, `\thispagestyle{plain}` from `\maketitle`) are next. They need cmsl10 (`\slshape`), and `Face` is full: 32 values, `used_faces: u32`, and `PackedWordAttr` packs the face in 5 bits next to a 3-bit `WordKind` (8 kinds). The same limit blocks cmbxti10. Needs the user's decision on how to grow `Face`.
- Parity tools: new `miss.py` matches glyphs by position (any char) and groups unmatched reference glyphs by line; better than the zip-based `chardiff.py`, which cascades after one extra glyph.

## Decision (continued, same day)
- `Face` grows to 64 values (user's decision): `Face::Slanted` = cmsl10 (`\slshape`, `\textsl`), `used_faces`/face masks are `u64`, `PackedWordAttr` is `u16` (6 face bits). This reverses the "widening `Face`" rejection in `memorycastle/journal/2026-10-18-design-sizes-colour-booktabs.md` for shapes only: design sizes stay masters, shapes become faces. cmbxsl10/cmsltt10 are not bundled: bold and typewriter win over the slant (`Face::with_bold`, `with_mono`, `FontDecl::apply`).
- Font assets come from scratchpad `fontgen/gen_one.py` (reproduces cmcsc10's CFF, metrics, FixMetrics and lig/kern byte-exact).
- Italic correction after text font commands uses the last glyph after ligatures (`word_italic_correction_pt`), and the test is latex.ltx's `\maybe@ic@`: outer font slant 0 (`has_slant`), whatever the inner font.
- Running heads: `PageStyle` (empty/plain/headings/myheadings), `\thispagestyle`, `\maketitle` → plain. Marks are `Block::VMark(Some(MarkData))`; `PageMarks` gives `\rightmark` = first right mark on the page, else the top mark. `decorate_page` paints head/foot when the page is retired (new_page and the finish paths), replacing the deferred page numbering. Head baseline is `\headsep` (25pt) above the text block.
- Expander implements `\lowercase`, `\uppercase`, `\MakeUppercase`, `\MakeLowercase` (`change_case`).
- `\ldots`/`\cdots` in Computer Modern are three Punct atoms (cmmi `.`, cmsy `\cdot`) at the style's optical size, thin space apart except in script styles.
- amsthm theorems are trivlists: the expander wraps them in `rushtextheorem`/`rushtextheoremhalf` (remark style), parsed as `Block::Aligned` with `TrivlistKind::Theorem`; layout adds `\@beginparpenalty`/`\@endparpenalty` (-51) and `\topsep` (half for remark) without `\partopsep`, keeping `\parindent`.

## Measured (continued)
- `testmath` 5.4% → 9.0% (41 vs 46 pages). Running head on page 2 within 0.02pt. Minimal theorem test matches pdfLaTeX.
- `sample2e`, `small2e`, `lppl` stay 100%. Tests 234 pass + 4 known NewCM failures.
- Latency: manual 5.5ms, testmath 7.2ms median (process spawn included).

## Open (continued)
- `\sectionmark`/`\subsectionmark` under `headings` (needs uppercased section titles); twoside even heads (`\leftmark`); marks in horizontal mode.
- cmbxsl10 and cmbxti10 masters.
