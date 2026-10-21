# pdfTeX character protrusion and font expansion (microtype) at zero shift
Author: abhi+claude | Date: 2026-10-22
Files: `src/engine/engine_microtype.rs`, `src/engine/engine_mod.rs`, `src/engine/engine_chars.rs`, `src/engine/engine_cond.rs`, `src/engine/engine_mlist.rs`, `src/engine/engine_noad.rs`, `src/engine/engine_pack.rs`, `src/engine/engine_linebreak.rs`, `src/engine/engine_font.rs`, `src/engine/engine_ship.rs`, `src/backend/backend_pdftex.rs`, `src/backend/backend_scaled.rs`, `src/engine/engine_value.rs`, `src/engine/engine_box.rs`, `src/engine/engine_node.rs`, `src/engine/engine_native.rs`, `src/engine/engine_run.rs`

## Context
wrapfig-doc (the last of 39 coverage documents off zero) loads `microtype`, which under pdfTeX turns on `\pdfprotrudechars=2`, `\pdfadjustspacing=2` and `\pdffontexpand ... autoexpand`. Without microtype it was already ZERO.

## Decision
- New module `engine_microtype.rs` holds everything: `Micro` (per-font `\lpcode`/`\rpcode`/`\efcode` tables made on first use; expansion links `blink`, stretch/shrink copies, step, auto), the primitives (`\lpcode`, `\rpcode`, `\efcode`, `\pdffontexpand`), protrusion helpers (`cp_skipable`, `protchar_left`/`protchar_right`, `char_pw`, `margin_kern`) and expansion (`get_kern`, `char_flex`, `kern_flex`, `subst_font`, `expand_line`, `expand_pars`). New primitives go through `Meaning::Native`, as `PRIMS` (u8) is full.
- An expanded font is a real `Font` entry: same `Rc` metrics, `expand: i16` thousandths; `Font::dims` scales width and italic correction with `round_xn_over_d(v, 1000+e, 1000)`, `get_kern` scales kerns. All metric lookups go through `Font::dims(c)` (11 call sites changed), so nothing copies width tables.
- Margin kerns are `Kind::Kern` with `sub` `MARGIN_LEFT`=4 / `MARGIN_RIGHT`=5; `\unkern`, `\lastkern` ignore them. `\leftmarginkern`/`\rightmarginkern` print `0pt` without a margin kern.
- Protrusion level 1 in `post_line_break`; level 2 as `Breaker::total_pw` added to `shortfall` in `try_break` (pdftex.web 25192).
- Expansion: line `Widths` grew from 6 to 8 components (font stretch/shrink) only meaningful with `\pdfadjustspacing>1`; the `widths(list, i)` closure takes the list and index so a font kern sees its neighbours (equivalent to pdfTeX's `prev_char_p` / `link(prev_char_p)=p` test). `check_expand_pars` is a single pre-scan per paragraph (`expand_pars`), since mixed steps or limits are an error in pdfTeX anyway. The shortfall formula is ported verbatim. `hpack`'s `cal_expand_ratio`/`subst_ex_font` is `expand_line`, called before packing each line.
- PDF: an auto-expanded copy shares its base's font object and `/F` name (`font_base` resolves `blink`), and is drawn with `Tm` `(1000+e)/1000 0 0 1 x y`; `TextState` keeps `tm_a` and ports `pdf_set_text_pos`, `pdf_begin_string` and `adv_char_width` arithmetic exactly (reset at `BT`).
- `zero_glue` identity: pdfTeX's `cp_skipable` skips glue only when `glue_ptr = zero_glue`. A zero glue is `zero_glue` iff it came from an internal glue (registers/params; §1229 `trap_zero_glue` makes every assigned zero glue `zero_glue`). `scan_glue_spec` reports a new spec for scanned dimensions, negation, `\glueexpr`/`\muexpr` and pdfTeX conversions; `\hskip`/`\vskip` mark such zero glue with `font = 1`. Found through KOMA's `\sectionlinesformat` (`\hskip\glueexpr 0pt`), which stops protrusion of the section number.

## Rejected
- Precomputing a per-paragraph flex array for kerns: the neighbour test in the closure is free and moves no data.
- Margin-kern variation (`cal_margin_kern_var`): an auto-expanded copy shares its base's `quad` and lp/rp codes, so the variation is always 0; left out with a comment. Revisit only if non-auto (TFM-loaded) expansion is added.

## Measured
- Fixtures (scratchpad `mt/`): prot1 (L1) 152/152, prot2 (L2) 271/271, exp1 (L1) 199/199 with content streams byte-identical including `Tm`, exp2 (L2 + protrusion) 415/415 with `\tracingparagraphs` identical to pdfTeX's log, KOMA section fixture 60/60.
- wrapfig-doc: 2560/9938 glyphs before (primitive surface only), 7903 with protrusion, 9917 with expansion, 9938/9938 ZERO after the `zero_glue` fix.

## Open
- `fix_expand_value` is still unused (only needed for non-auto expanded fonts).
- Non-auto expansion (loading `cmr10+20.tfm`) and `\pdfprotrudechars` with VF fonts are not covered by any corpus document.
