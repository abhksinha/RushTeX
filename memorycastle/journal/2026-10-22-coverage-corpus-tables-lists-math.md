# Coverage corpus, first round: tables, lists, multicolumn, mathematics, two-column
Author: abhi+claude | Date: 2026-10-22
Files: `scripts/parity/corpus/`, `scripts/parity/parity.py`, `src/backend/backend_ship.rs`, `src/engine/engine_page.rs`, `src/engine/engine_ship.rs`, `src/engine/engine_align.rs`, `src/engine/engine_box.rs`, `src/engine/engine_node.rs`, `src/engine/engine_linebreak.rs`

## Context
Batch 3 of `2026-10-22-batches-1-4-plan.md`. The user asked for distinct topics over count:
tables, mathematics, enumerations, minipages, two-column layout, wrapped figures, from
CTAN documentation.

## Decision
- Documents come from CTAN source archives (`mirrors.ctan.org/<path>.zip`), each unpacked
  in its own scratch directory, screened with pdfLaTeX first (a document that errors under
  pdfLaTeX itself is dropped), then copied unchanged into `scripts/parity/corpus/`. A `.dtx`
  is typeset through a two-line `<name>.tex` driver (`\input <name>.dtx`), which is what
  `pdflatex <name>.dtx` does and keeps the harness's `*.tex` corpus rule.
- Missing packages are installed with `tlmgr --usermode install`.
- `pdftex.map` effects: `SlantFont`/`ExtendFont` are read as mapfile.c reads them (a C
  `float`, thousandths rounded half away from zero) and applied to the embedded Type 1
  program as writet1.c does (`t1_modify_fm` slant before extend, `t1_modify_italic`,
  `-Slant_<s>`/`-Extend_<e>` on `/FontName`, numbers printed as `%g` of a `float`).
  Bera's slanted shapes (`fvsro8t`) need it.
- `pdftex.map` is indexed once per document (`map_index`, first entry of a name wins, as
  mapfile.c keeps it) instead of a linear search of the whole file for every font.
- pdfTeX's `print_ignored_err`: with bit 1 of `\ignoreprimitiveerror` (latex.ltx sets 1),
  "Infinite glue shrinkage found in box being split" goes to the log only (longtable).
- The reference cache is invalidated when the TeX installation changes (the newest of each
  tree's `ls-R` and `tlpkg/texlive.tlpdb`), not only when the document changes.

- Shared fonts follow pdftex.web `pdf_init_font`: when a font is first shipped, it shares the
  PDF object of an earlier-shipped font with the same file name, and every size is named
  `/F<base>` with `/Widths` from the base's size (`font_base` in `engine_ship.rs`, one `u16`
  per font in `Page::font_base`, replacing the used-font bitset). The earlier rule (lowest
  font number owns the object) gave `s` of CMBX12 the width 443.7 instead of 443.8 when the
  14.4pt size was shipped before the 12pt one (bm, 78sp drift after the `s`).
- Alignment entries are peeked as e-TeX peeks them (etex.ch §785/§791,
  `get_x_or_protected`): a `\protected` macro is not expanded while looking for `\omit` or
  `\noalign`. LaTeX's `\{` is `\protected`; expanded early, its `\ifmmode` ran before the
  entry's `$` and gave `\char"66` in math (CMMI `f`). Fixed calc and multirow.
- §1113: `\/` after a ligature takes the italic correction of the ligature's character
  (xcolor, `\meta{stuff}` ends in the `ff` ligature).
- `clang` (§212, a horizontal list's `aux` field) is saved per nest level (`Level::clang`,
  a `u8` in existing padding). It was machine-wide: a footnote's inner paragraph reset it,
  so after `\verb` (which sets `\language` to nohyphenation in a group) the outer
  paragraph never got the language whatsit back to 0, and no later word was hyphenated.
- `\tracingparagraphs > 0` prints pdfTeX's feasible-break lines (§846 `@@n: line l.f t=`,
  §856 `@<kind> via @@n b= p= d=`, `@firstpass`/`@secondpass`) to stderr, without the
  §857 text lines, so `grep '^@'` of both diffs directly. It found the hyphenation gap.

## Rejected
- booktabs.dtx: pdfLaTeX itself stops with "Improper alphabetic constant" under the current
  kernel. array.dtx: 3 pdfLaTeX errors. threeparttable.tex: needs `miscdoc`, not in TeX
  Live.

## Measured
- ZERO on the first run, no code change: bare_conf (IEEEtran two-column), tabularx (12
  pages), multicol (38), dcolumn (4).
- ZERO after the fixes above: enumitem (24 pages; slant effect), mathtools (39; stale
  reference rebuilt after Palatino was installed), longtable (33; ignored error).

- ZERO after the font and alignment fixes: bm (21 pages), calc (17), multirow (30); with no
  change: theorem, hhline, delarray, ftnright, enumerate, verbatim, afterpage, varioref,
  thmtest, natbib (21), geometry (42), float (12).
- wrapfig-doc with `\usepackage{microtype}` removed (tikz kept) is ZERO: microtype
  (pdfTeX protrusion and font expansion) is its only gap.
- xcolor ZERO (64 pages, 11855 paths) after the ligature italic correction and `clang`.

## Open
- mathtools's earlier mismatch was a reference built before `palatino` was installed; the
  install-time stamp now catches that.
- microtype: `\lpcode`, `\rpcode`, `\efcode`, `\pdfprotrudechars`, `\pdfadjustspacing`
  and font expansion, from pdftex.web.
